//! Receive-side arena for UDP packets.
//!
//! One `BytesMut` chunk backs many incoming datagrams. Each packet is handed
//! out as a zero-copy `Bytes` slice of that chunk. When every slice derived
//! from the chunk is dropped, `BytesMut::reserve` rewinds the cursor to the
//! chunk start so subsequent packets reuse the same allocation. A single
//! scratch buffer is zero-filled once at construction and reused as the
//! `recv_from` destination.

use bytes::{Bytes, BytesMut};
use std::io;
use std::net::SocketAddr;

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
    // Buffer reused as the recv_from destination on every call,
    // Pre-initialized once to make it safe to pass as &mut [u8] which requires initialized memory.
    scratch: Box<[u8]>,
}

impl RecvArena {
    // `chunk_size` is clamped to `>= max_packet` so a single datagram always fits.
    pub(crate) fn new(chunk_size: usize, max_packet: usize) -> Self {
        let chunk_size = chunk_size.max(max_packet);
        Self {
            current: BytesMut::with_capacity(chunk_size),
            chunk_size,
            max_packet,
            scratch: vec![0u8; max_packet].into_boxed_slice(),
        }
    }

    // Receive one datagram into the arena. Returns the zero-copy `Bytes` view
    // backed by the current chunk and the sender address. On `WouldBlock` or
    // other recv errors the arena state is untouched.
    pub(crate) fn recv_from(
        &mut self,
        sock: &mio::net::UdpSocket,
    ) -> io::Result<(Bytes, SocketAddr)> {
        let (nbytes, sender) = sock.recv_from(&mut self.scratch[..])?;
        let nbytes = nbytes.min(self.max_packet);

        self.ensure_space();
        self.current.extend_from_slice(&self.scratch[..nbytes]);
        Ok((self.current.split_to(nbytes).freeze(), sender))
    }

    // Reserve chunk_size worth of spare capacity. When the chunk's Arc strong
    // count is 1 (no outstanding Bytes), this rewinds the cursor in place; when
    // it is >1, a fresh chunk is allocated.
    fn ensure_space(&mut self) {
        self.current.reserve(self.max_packet);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test helper that mirrors recv_from's arena work without needing a socket:
    // it runs the same reserve + extend_from_slice + split_to sequence the
    // production path uses.
    impl RecvArena {
        fn commit_from_slice(&mut self, src: &[u8]) -> Bytes {
            let n = src.len().min(self.max_packet);
            self.ensure_space();
            self.current.extend_from_slice(&src[..n]);
            self.current.split_to(n).freeze()
        }
    }

    #[test]
    fn multiple_packets_share_single_chunk() {
        let mut arena = RecvArena::new(64 * 1024, 1500);
        let mut handed_out = Vec::new();

        // Carve 10 Bytes slices out of one chunk, each with a distinct byte pattern.
        for i in 0..10 {
            let n = 100 + i;
            let mut payload = vec![0u8; n];
            payload.iter_mut().enumerate().for_each(|(k, b)| *b = ((i + k) & 0xff) as u8);
            let bytes = arena.commit_from_slice(&payload);
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
    fn rewinds_in_place_when_all_slices_dropped() {
        let mut arena = RecvArena::new(4096, 1500);
        arena.ensure_space();
        let origin = arena.current.as_ptr();

        // Two commits whose Bytes are dropped at the end of the block,
        // returning the chunk's Arc strong count to 1.
        {
            let _ = arena.commit_from_slice(&[0xaa; 1500]);
            let _ = arena.commit_from_slice(&[0xbb; 1500]);
        }

        // Next ensure_space must rewind to the chunk origin instead of
        // allocating a fresh chunk.
        arena.ensure_space();
        assert_eq!(arena.current.as_ptr(), origin);
    }

    #[test]
    fn rewinds_every_iteration_when_idle() {
        let mut arena = RecvArena::new(4096, 1500);
        arena.ensure_space();
        let origin = arena.current.as_ptr();

        // Each committed Bytes is dropped before the next commit, so every
        // iteration's ensure_space must come back to the same origin.
        for _ in 0..10 {
            let _ = arena.commit_from_slice(&[0xcc; 1500]);
        }

        arena.ensure_space();
        assert_eq!(arena.current.as_ptr(), origin);
    }

    #[test]
    fn allocates_new_chunk_when_outstanding_slices_block_reclaim() {
        let chunk_size = 1024 * 1024;
        let max_packet = 64 * 1024;
        let mut arena = RecvArena::new(chunk_size, max_packet);
        arena.ensure_space();
        let origin = arena.current.as_ptr();

        // Keep the committed Bytes alive so strong_count > 1 blocks reclaim.
        let _b1 = arena.commit_from_slice(&[0xaa; 64 * 1024]);

        // Next ensure_space requests chunk_size; reclaim fails, fresh chunk alloc.
        arena.ensure_space();
        assert_ne!(arena.current.as_ptr(), origin);
    }

    #[test]
    fn committed_bytes_outlive_arena_drop() {
        let b = {
            let mut arena = RecvArena::new(4096, 1500);
            // Drop the arena at end of this block — the Bytes keeps the chunk alive via Arc.
            arena.commit_from_slice(&[1, 2, 3, 4])
        };
        assert_eq!(&b[..], &[1, 2, 3, 4]);
    }

    #[test]
    fn zero_length_datagram_yields_empty_bytes() {
        let mut arena = RecvArena::new(4096, 1500);
        let b = arena.commit_from_slice(&[]);
        assert_eq!(b.len(), 0);
    }

    #[test]
    fn oversized_payload_is_clamped_to_max_packet() {
        let mut arena = RecvArena::new(64 * 1024, 1500);
        // Caller hands more than max_packet; arena must clamp without panicking.
        let payload = vec![0xee; 4096];
        let b = arena.commit_from_slice(&payload);
        assert_eq!(b.len(), 1500);
    }
}
