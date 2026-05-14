//! Receive-side arena for UDP packets.
//!
//! One `BytesMut` chunk backs many incoming datagrams. Each packet is handed
//! out as a zero-copy `Bytes` slice of that chunk. When every slice derived
//! from the chunk is dropped, the chunk's underlying allocation is freed in
//! one shot by `bytes`' internal `Arc<Shared>`. This replaces the previous
//! per-packet `Bytes::copy_from_slice`, which malloc/free/madvise-churned
//! the heap once per datagram.

use bytes::{Bytes, BytesMut};

// Max UDP datagram payload this process accepts. Matches `MAX_MESSAGE_SIZE`
// in `udp_listener`.
pub(crate) const MAX_UDP_PACKET_BYTES: usize = 64 * 1024;

// Default chunk size. Picked to hold roughly one 1MB DDS sample worth of
// DATAFRAG packets before forcing a new chunk.
pub(crate) const DEFAULT_RECV_ARENA_CHUNK_BYTES: usize = 1024 * 1024;

#[derive(Debug)]
pub(crate) struct RecvArena {
    current: BytesMut,
    chunk_size: usize,
    max_packet: usize,
    slot_reserved: bool,
    slot_start: usize,
}

impl RecvArena {
    // `chunk_size` is clamped to `>= max_packet` so a single datagram always fits.
    pub(crate) fn new(chunk_size: usize, max_packet: usize) -> Self {
        let chunk_size = chunk_size.max(max_packet);
        Self {
            current: BytesMut::with_capacity(chunk_size),
            chunk_size,
            max_packet,
            slot_reserved: false,
            slot_start: 0,
        }
    }

    // Reserve a writable slot sized for the largest possible datagram.
    // Returns a zero-initialized mutable slice to hand to `recv_from`.
    pub(crate) fn reserve_packet_slot(&mut self) -> &mut [u8] {
        // Roll back any previous reservation so slots can't stack up.
        self.release_unused_slot();

        // Reclaim in place every iteration: BytesMut::reserve rewinds the
        // cursor to the chunk start when all frozen slices were dropped.
        self.current.reserve(self.chunk_size);

        // Mark the new slot as reserved and zero-fill it. The zeroing is needed to
        // prevent uninitialized memory from being exposed if the caller commits a shorter packet.
        self.slot_start = self.current.len();
        self.slot_reserved = true;
        self.current.resize(self.slot_start + self.max_packet, 0);
        &mut self.current[self.slot_start..]
    }

    // Freeze the actually received bytes as a zero-copy `Bytes` slice.
    // `actual_len` is clamped to the slot size so an out-of-range caller
    // value can't corrupt arena state.
    pub(crate) fn commit_received_packet(&mut self, actual_len: usize) -> Bytes {
        if !self.slot_reserved {
            return Bytes::new();
        }
        let actual_len = actual_len.min(self.max_packet);

        // Drop the unused zero-filled tail of the slot so the next reservation
        // reuses that capacity.
        let new_len = self.slot_start + actual_len;
        self.current.truncate(new_len);
        self.slot_reserved = false;

        self.current.split_to(actual_len).freeze()
    }

    // Discard a reserved-but-unused slot (e.g. on `WouldBlock` / recv error).
    // Without this, idle polls would accumulate zero-filled dead tails.
    pub(crate) fn release_unused_slot(&mut self) {
        if self.slot_reserved {
            self.current.truncate(self.slot_start);
            self.slot_reserved = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiple_packets_share_single_chunk() {
        let mut arena = RecvArena::new(64 * 1024, 1500);
        let mut handed_out = Vec::new();

        // Carve 10 Bytes slices out of one chunk, each with a distinct byte pattern.
        for i in 0..10 {
            let slot = arena.reserve_packet_slot();
            let n = 100 + i;
            slot[..n].iter_mut().enumerate().for_each(|(k, b)| *b = ((i + k) & 0xff) as u8);
            let bytes = arena.commit_received_packet(n);
            assert_eq!(bytes.len(), n);
            handed_out.push((bytes, i, n));
        }

        // Each committed Bytes must still point at its own untouched pattern.
        for (bytes, i, n) in &handed_out {
            for (k, b) in bytes.iter().enumerate().take(*n) {
                assert_eq!(*b, ((*i + k) & 0xff) as u8);
            }
        }
    }

    #[test]
    fn new_chunk_when_remaining_capacity_too_small() {
        // 4KB chunk can only fit two 1500B slots; the third must rotate to a fresh chunk.
        let mut arena = RecvArena::new(4096, 1500);
        let _b1 = {
            let s = arena.reserve_packet_slot();
            s[0] = 0xaa;
            arena.commit_received_packet(1500)
        };
        let _b2 = {
            let s = arena.reserve_packet_slot();
            s[0] = 0xbb;
            arena.commit_received_packet(1500)
        };
        // Snapshot the chunk pointer before the reservation that should trigger rotation.
        let ptr_before = arena.current.as_ptr();
        let _b3 = {
            let s = arena.reserve_packet_slot();
            s[0] = 0xcc;
            arena.commit_received_packet(1500)
        };
        let ptr_after = arena.current.as_ptr();
        // A different backing pointer proves a new chunk was allocated.
        assert_ne!(ptr_before, ptr_after, "a new chunk should have been allocated");
    }

    #[test]
    fn release_unused_slot_rolls_back_len() {
        let mut arena = RecvArena::new(64 * 1024, 1500);
        // Reserve a slot, then release it without committing (WouldBlock case).
        let _ = arena.reserve_packet_slot();
        let len_before_release = arena.current.len();
        arena.release_unused_slot();
        // Release must shrink len back and clear the reserved flag.
        assert!(arena.current.len() < len_before_release);
        assert!(!arena.slot_reserved);

        // Arena must still be usable for the next reservation.
        let slot = arena.reserve_packet_slot();
        assert_eq!(slot.len(), 1500);
    }

    #[test]
    fn committed_bytes_outlive_arena_drop() {
        let b = {
            let mut arena = RecvArena::new(4096, 1500);
            let s = arena.reserve_packet_slot();
            s[0..4].copy_from_slice(&[1, 2, 3, 4]);
            // Drop the arena at end of this block — the Bytes keeps the chunk alive via Arc.
            arena.commit_received_packet(4)
        };
        assert_eq!(&b[..], &[1, 2, 3, 4]);
    }

    #[test]
    fn rewinds_in_place_when_all_slices_dropped() {
        let mut arena = RecvArena::new(4096, 1500);
        let origin = arena.current.as_ptr();

        // Two commits whose Bytes are dropped at the end of the block,
        // returning the chunk's Arc strong count to 1.
        {
            let s = arena.reserve_packet_slot();
            s[0] = 0xaa;
            let _ = arena.commit_received_packet(1500);
            let s = arena.reserve_packet_slot();
            s[0] = 0xbb;
            let _ = arena.commit_received_packet(1500);
        }

        // Next reservation must rewind to the chunk origin instead of
        // allocating a fresh chunk.
        let _ = arena.reserve_packet_slot();
        assert_eq!(arena.current.as_ptr(), origin);
    }

    #[test]
    fn rewinds_every_iteration_when_idle() {
        let mut arena = RecvArena::new(4096, 1500);
        let origin = arena.current.as_ptr();

        // Each committed Bytes is dropped before the next reserve, so every
        // iteration's reservation must come back to the same origin.
        for _ in 0..10 {
            let s = arena.reserve_packet_slot();
            s[0] = 0xcc;
            let _ = arena.commit_received_packet(1500);
        }

        let _ = arena.reserve_packet_slot();
        assert_eq!(arena.current.as_ptr(), origin);
    }

    #[test]
    fn allocates_new_chunk_when_outstanding_slices_block_reclaim() {
        let chunk_size = 1024 * 1024;
        let max_packet = 64 * 1024;
        let mut arena = RecvArena::new(chunk_size, max_packet);
        let origin = arena.current.as_ptr();

        // Keep the committed Bytes alive so strong_count > 1 blocks reclaim.
        let _b1 = {
            let s = arena.reserve_packet_slot();
            s[0] = 0xaa;
            arena.commit_received_packet(max_packet)
        };

        // Next reserve requests chunk_size; reclaim fails, fresh chunk alloc.
        let _ = arena.reserve_packet_slot();
        assert_ne!(arena.current.as_ptr(), origin);
    }
}
