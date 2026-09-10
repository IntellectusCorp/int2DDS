//! Bounded MPSC queue of fixed-size cells. Each cell carries its own sequence
//! marker, so producers never wait on a global commit order and a consumer can
//! only observe a cell after its writer published it.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::rtps::transport::shm::layout::LayoutError;

pub(crate) const RING_INLINE: usize = 240;
pub(crate) const SPILL_NONE: u32 = u32::MAX;
pub(crate) const RING_MAGIC: u64 = 0x494E_5432_5A43_524E; // "INT2ZCRN"

#[repr(C, align(64))]
pub(crate) struct RingEntry {
    pub seq: AtomicU64,
    pub len: u32,
    pub spill: u32,
    pub inline: [u8; RING_INLINE],
}

// Cells are addressed by index across processes, so the stride is wire format.
const _: () = assert!(std::mem::size_of::<RingEntry>() == 256);

#[repr(C, align(64))]
pub(crate) struct RingHeader {
    pub magic: AtomicU64,
    pub enqueue_pos: AtomicU64,
    pub dequeue_pos: AtomicU64,
    pub capacity: u32,
    pub waiters: AtomicU32,
    pub futex_word: AtomicU32,
    pub _pad: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RingError {
    Full,
    /// The cell was reclaimed by `recover_if_wedged` while this producer was
    /// preempted. The payload was not published.
    Preempted,
}

pub(crate) struct Ring {
    base: *mut u8,
    capacity: u64,
    mask: u64,
}

// Safety: the pointer addresses a shared mapping whose geometry is written
// once before publication. `push` writes non-atomic cell fields (`len`,
// `spill`, `inline`) and `pop` reads them, but each cell's `seq` marker gates
// both:
// a producer only touches a cell it has just claimed via CAS on `enqueue_pos`,
// and a consumer only reads a cell after `seq` proves that write is published
// and before any producer may claim it again.
unsafe impl Send for Ring {}
unsafe impl Sync for Ring {}

const RING_HEADER_SIZE: u64 = std::mem::size_of::<RingHeader>() as u64;

impl Ring {
    pub(crate) fn size_for(capacity: u32) -> u64 {
        RING_HEADER_SIZE + std::mem::size_of::<RingEntry>() as u64 * capacity as u64
    }

    /// # Safety
    /// `base` must point to a zeroed writable region of `size_for(capacity)` bytes.
    pub(crate) unsafe fn init(base: *mut u8, capacity: u32) -> Result<Ring, LayoutError> {
        // Capacity is configuration, so a bad value is an error the caller
        // falls back on, never a panic. Zero is not a power of two either.
        if !capacity.is_power_of_two() {
            return Err(LayoutError::BadConfig);
        }
        let header = base as *mut RingHeader;
        std::ptr::addr_of_mut!((*header).capacity).write(capacity);
        (*header).enqueue_pos.store(0, Ordering::Relaxed);
        (*header).dequeue_pos.store(0, Ordering::Relaxed);
        (*header).waiters.store(0, Ordering::Relaxed);
        (*header).futex_word.store(0, Ordering::Relaxed);
        let ring = Ring { base, capacity: capacity as u64, mask: capacity as u64 - 1 };
        for i in 0..capacity as u64 {
            ring.seq_of(i).store(i, Ordering::Relaxed);
        }
        (*header).magic.store(RING_MAGIC, Ordering::Release);
        Ok(ring)
    }

    /// # Safety
    /// `base` must point to a region previously initialized by `init` that is
    /// at least `region_size` bytes and stays mapped while the ring is used.
    ///
    /// The returned ring's `entry_ptr` stays inside the region because this
    /// function rejects any capacity whose entry array would not fit.
    pub(crate) unsafe fn attach(base: *mut u8, region_size: u64) -> Result<Ring, LayoutError> {
        // The header itself must fit before anything reads through it.
        if region_size < RING_HEADER_SIZE {
            return Err(LayoutError::SizeMismatch);
        }
        // Gate first, through a raw projection: see `SegmentView::attach`.
        let magic = &*std::ptr::addr_of!((*(base as *const RingHeader)).magic);
        if magic.load(Ordering::Acquire) != RING_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const RingHeader);
        let capacity = header.capacity as u64;
        if capacity == 0 || !capacity.is_power_of_two() {
            return Err(LayoutError::BadVersion);
        }
        // `capacity` comes from shared memory and feeds `entry_ptr`'s pointer
        // math. Without this bound a corrupt value indexes past the mapping.
        if Ring::size_for(header.capacity) > region_size {
            return Err(LayoutError::BadOffset);
        }
        Ok(Ring { base, capacity, mask: capacity - 1 })
    }

    fn header(&self) -> &RingHeader {
        // Safety: `init`/`attach` established the pointer and alignment. The
        // header's non-atomic fields are written once before publication; the
        // counters are only ever touched through their atomics.
        unsafe { &*(self.base as *const RingHeader) }
    }

    fn entry_ptr(&self, pos: u64) -> *mut RingEntry {
        // Safety: `mask` came from a power-of-two capacity validated at attach,
        // so the index is inside the entry array `size_for` reserved.
        unsafe {
            let entries = self.base.add(RING_HEADER_SIZE as usize) as *mut RingEntry;
            entries.add((pos & self.mask) as usize)
        }
    }

    /// The cell's publication marker, reached without forming a reference to
    /// the cell. A producer may still be writing the non-atomic body, and a
    /// whole-struct `&RingEntry` would assert that nobody is.
    fn seq_of(&self, pos: u64) -> &AtomicU64 {
        // Safety: same bounds as `entry_ptr`; the projection touches only the
        // atomic field, which is sound to alias.
        unsafe { &*std::ptr::addr_of!((*self.entry_ptr(pos)).seq) }
    }

    pub(crate) fn push(&self, payload: &[u8], spill: u32) -> Result<(), RingError> {
        if payload.len() > RING_INLINE {
            return Err(RingError::Full);
        }
        let header = self.header();
        let pos = loop {
            let pos = header.enqueue_pos.load(Ordering::Relaxed);
            let seq = self.seq_of(pos).load(Ordering::Acquire);
            let dif = seq as i64 - pos as i64;
            if dif == 0 {
                if header
                    .enqueue_pos
                    .compare_exchange_weak(pos, pos + 1, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    break pos;
                }
            } else if dif < 0 {
                return Err(RingError::Full);
            }
            std::hint::spin_loop();
        };

        // Safety: the CAS above claimed this cell exclusively until the
        // publishing store below, so no other producer and no consumer may
        // touch its body.
        unsafe {
            let e = self.entry_ptr(pos);
            std::ptr::addr_of_mut!((*e).len).write(payload.len() as u32);
            std::ptr::addr_of_mut!((*e).spill).write(spill);
            std::ptr::copy_nonoverlapping(
                payload.as_ptr(),
                std::ptr::addr_of_mut!((*e).inline) as *mut u8,
                payload.len(),
            );
        }
        // A compare_exchange, not a store: `recover_if_wedged` may have freed
        // this cell while this producer was preempted between the claim and
        // here. Overwriting its marker would close the cell to consumer and
        // producer alike, for good. Losing the CAS means the cell is no longer
        // ours, so only this sample is lost.
        if self
            .seq_of(pos)
            .compare_exchange(pos, pos + 1, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            return Err(RingError::Preempted);
        }
        Ok(())
    }

    /// `&mut self` no longer means exactly one thread ever calls this: a
    /// caller behind a `Mutex<Ring>` may hand it to several threads, one at a
    /// time. That keeps the ring's own invariants intact -- `&mut` still rules
    /// out two calls racing on `dequeue_pos` -- but with more than one
    /// consumer the wakeup does not scale with them: `Notifier::signal` wakes
    /// one sleeper, so a second consumer waiting at the same time sleeps until
    /// its own timeout instead of being woken.
    pub(crate) fn pop(&mut self, out: &mut [u8; RING_INLINE]) -> Option<(u32, u32)> {
        let header = self.header();
        let pos = header.dequeue_pos.load(Ordering::Relaxed);
        if self.seq_of(pos).load(Ordering::Acquire) != pos + 1 {
            self.recover_if_wedged(pos);
            return None;
        }
        // Safety: the marker above synchronizes with the producer's publishing
        // store, so the body is fully written and no producer may claim this
        // cell again until the freeing store below.
        let (len, spill) = unsafe {
            let raw = self.entry_ptr(pos);
            let len = std::ptr::addr_of!((*raw).len).read();
            let spill = std::ptr::addr_of!((*raw).spill).read();
            let len_clamped = (len as usize).min(RING_INLINE);
            std::ptr::copy_nonoverlapping(
                std::ptr::addr_of!((*raw).inline) as *const u8,
                out.as_mut_ptr(),
                len_clamped,
            );
            (len_clamped as u32, spill)
        };
        self.seq_of(pos).store(pos + self.capacity, Ordering::Release);
        header.dequeue_pos.store(pos + 1, Ordering::Release);
        Some((len, spill))
    }

    /// A producer that claimed this cell and died before its publishing store
    /// leaves `seq == pos` for good: the consumer never advances, later cells
    /// stay unreachable, and every push fails once `enqueue_pos` laps. Free the
    /// cell only on the strongest evidence available -- a full lap has been
    /// claimed since, so the claimer did not publish while every other producer
    /// filled the ring. Freeing the cell is a CAS, not the plain store a normal
    /// `pop` makes: `pop` has already seen `seq == pos + 1` and so knows the
    /// producer finished, while this pass only inferred it.
    ///
    /// Two wedged cells in a row take two passes: after the first is freed the
    /// lap condition no longer holds, so the second waits for one more push.
    fn recover_if_wedged(&mut self, pos: u64) {
        let header = self.header();
        if self.seq_of(pos).load(Ordering::Acquire) != pos {
            return;
        }
        if header.enqueue_pos.load(Ordering::Acquire).wrapping_sub(pos) < self.capacity {
            return;
        }
        // A compare_exchange for the same reason the publishing store is one:
        // the producer this pass declared gone may publish between the guards
        // above and here. Losing the CAS means it did, so leave the cell to the
        // next `pop`, which delivers it by the normal path. A plain store here
        // would drop a sample `push` already reported as delivered.
        if self
            .seq_of(pos)
            .compare_exchange(pos, pos + self.capacity, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            return;
        }
        header.dequeue_pos.store(pos + 1, Ordering::Release);
    }

    pub(crate) fn futex_ptr(&self) -> *const AtomicU32 {
        &self.header().futex_word as *const AtomicU32
    }

    pub(crate) fn waiters(&self) -> &AtomicU32 {
        &self.header().waiters
    }

    /// Whether the consumer would find nothing to pop right now. This does not
    /// recover a wedged cell -- only `pop` does -- so a consumer that waits on
    /// this alone must bound its wait.
    pub(crate) fn is_empty(&self) -> bool {
        let pos = self.header().dequeue_pos.load(Ordering::Relaxed);
        self.seq_of(pos).load(Ordering::Acquire) != pos + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    fn ring(capacity: u32) -> (AlignedRegion, Ring) {
        let region = AlignedRegion::new(Ring::size_for(capacity) as usize);
        let r = unsafe { Ring::init(region.ptr(), capacity) }.unwrap();
        (region, r)
    }

    #[test]
    fn pop_on_empty_ring_returns_none() {
        let (_g, mut r) = ring(4);
        let mut out = [0u8; RING_INLINE];
        assert!(r.pop(&mut out).is_none());
    }

    #[test]
    fn push_then_pop_round_trips() {
        let (_g, mut r) = ring(4);
        r.push(b"hello", 7).unwrap();
        let mut out = [0u8; RING_INLINE];
        let (len, spill) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"hello");
        assert_eq!(spill, 7);
        assert!(r.pop(&mut out).is_none());
    }

    #[test]
    fn push_rejects_oversized_payload() {
        let (_g, r) = ring(4);
        let big = vec![0u8; RING_INLINE + 1];
        assert_eq!(r.push(&big, SPILL_NONE), Err(RingError::Full));
    }

    #[test]
    fn full_ring_reports_full_and_never_overwrites() {
        let (_g, mut r) = ring(2);
        r.push(b"a", SPILL_NONE).unwrap();
        r.push(b"b", SPILL_NONE).unwrap();
        assert_eq!(r.push(b"c", SPILL_NONE), Err(RingError::Full));
        let mut out = [0u8; RING_INLINE];
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"a");
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"b");
    }

    #[test]
    fn cells_are_reusable_after_draining() {
        let (_g, mut r) = ring(2);
        let mut out = [0u8; RING_INLINE];
        for i in 0..10u8 {
            r.push(&[i], SPILL_NONE).unwrap();
            let (len, _) = r.pop(&mut out).unwrap();
            assert_eq!(&out[..len as usize], &[i]);
        }
    }

    #[test]
    fn attach_reads_back_a_usable_ring() {
        let region = AlignedRegion::new(Ring::size_for(4) as usize);
        unsafe { Ring::init(region.ptr(), 4) }.unwrap();
        let mut r = unsafe { Ring::attach(region.ptr(), Ring::size_for(4)) }.unwrap();
        r.push(b"z", SPILL_NONE).unwrap();
        let mut out = [0u8; RING_INLINE];
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"z");
    }

    #[test]
    fn attach_rejects_capacity_larger_than_the_region() {
        let region = AlignedRegion::new(Ring::size_for(4) as usize);
        unsafe { Ring::init(region.ptr(), 4) }.unwrap();
        // A corrupt capacity: still a power of two, but its entry array runs
        // far past the mapping.
        unsafe {
            std::ptr::addr_of_mut!((*(region.ptr() as *mut RingHeader)).capacity).write(1 << 20);
        }
        // `Ring` has no `Debug`, so match instead of `unwrap_err`.
        let got = unsafe { Ring::attach(region.ptr(), Ring::size_for(4)) };
        assert!(matches!(got, Err(LayoutError::BadOffset)));
    }

    #[test]
    fn init_rejects_a_capacity_that_is_not_a_power_of_two() {
        let region = AlignedRegion::new(Ring::size_for(8) as usize);
        for capacity in [0u32, 3, 100] {
            let got = unsafe { Ring::init(region.ptr(), capacity) };
            assert!(matches!(got, Err(LayoutError::BadConfig)), "capacity {capacity}");
        }
    }

    #[test]
    fn concurrent_producers_lose_nothing() {
        const PRODUCERS: usize = 4;
        const PER_PRODUCER: usize = 500;
        let region = AlignedRegion::new(Ring::size_for(64) as usize);
        // Two handles over one region, as owner and peers really hold them:
        // producers share `&produce`, the single consumer owns `consume`.
        let produce = unsafe { Ring::init(region.ptr(), 64) }.unwrap();
        let mut consume = unsafe { Ring::attach(region.ptr(), Ring::size_for(64)) }.unwrap();

        let mut seen = vec![0usize; PRODUCERS];
        let mut out = [0u8; RING_INLINE];
        let total = PRODUCERS * PER_PRODUCER;

        std::thread::scope(|s| {
            for p in 0..PRODUCERS {
                let produce = &produce;
                s.spawn(move || {
                    for n in 0..PER_PRODUCER {
                        let msg = [p as u8, (n & 0xFF) as u8, (n >> 8) as u8];
                        while produce.push(&msg, SPILL_NONE).is_err() {
                            std::hint::spin_loop();
                        }
                    }
                });
            }

            let mut got = 0;
            // Bounded so a lost entry fails the test instead of hanging CI.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while got < total {
                if let Some((len, _)) = consume.pop(&mut out) {
                    assert_eq!(len, 3);
                    seen[out[0] as usize] += 1;
                    got += 1;
                } else if std::time::Instant::now() > deadline {
                    panic!("timed out after {got} of {total} entries");
                }
            }
        });

        assert_eq!(seen, vec![PER_PRODUCER; PRODUCERS]);
        // Both handles are declared after `region`, so they drop first and
        // never outlive the memory they point into.
    }

    #[test]
    fn a_cell_claimed_and_never_published_is_freed_after_a_full_lap() {
        let (_region, mut r) = ring(4);

        // A producer claimed cell 0 and died before its publishing store: the
        // CAS moved `enqueue_pos`, `seq` never moved.
        r.header().enqueue_pos.store(1, Ordering::Release);

        // Three live producers then fill the rest of the lap.
        for i in 0..3u8 {
            r.push(&[i], SPILL_NONE).unwrap();
        }
        assert_eq!(r.header().enqueue_pos.load(Ordering::Acquire), 4);

        let mut out = [0u8; RING_INLINE];
        assert!(r.pop(&mut out).is_none(), "the wedged cell yields nothing");
        let (len, _) = r.pop(&mut out).expect("the cell behind it must become reachable");
        assert_eq!(&out[..len as usize], &[0u8]);

        // The freed cell must be claimable again at its next position.
        while r.pop(&mut out).is_some() {}
        r.push(b"after", SPILL_NONE).unwrap();
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"after");
    }

    #[test]
    fn a_late_publish_after_recovery_loses_only_its_own_sample() {
        let (_region, mut r) = ring(4);

        // A producer claims cell 0 and stalls before publishing.
        r.header().enqueue_pos.store(1, Ordering::Release);
        for i in 0..3u8 {
            r.push(&[i], SPILL_NONE).unwrap();
        }

        let mut out = [0u8; RING_INLINE];
        assert!(r.pop(&mut out).is_none(), "the recovery pass yields nothing");

        // The stalled producer finally publishes. Its cell is no longer its own.
        assert_eq!(
            r.seq_of(0).compare_exchange(0, 1, Ordering::Release, Ordering::Relaxed),
            Err(4),
            "the freed cell must refuse the late publish"
        );

        // Everything behind it still drains, and the ring still takes writes.
        for i in 0..3u8 {
            let (len, _) = r.pop(&mut out).expect("published cells stay reachable");
            assert_eq!(&out[..len as usize], &[i]);
        }
        r.push(b"after", SPILL_NONE).unwrap();
        let (len, _) = r.pop(&mut out).unwrap();
        assert_eq!(&out[..len as usize], b"after");
    }

    #[test]
    fn a_cell_published_a_lap_late_is_still_popped() {
        let (_region, mut r) = ring(4);

        // A producer claims cell 0; three others fill the lap behind it.
        r.header().enqueue_pos.store(1, Ordering::Release);
        for i in 0..3u8 {
            r.push(&[i], SPILL_NONE).unwrap();
        }

        // It publishes after all, before any recovery pass runs. Recovery must
        // then leave the cell alone -- its first guard already sees `pos + 1`.
        r.seq_of(0).store(1, Ordering::Release);

        let mut out = [0u8; RING_INLINE];
        let (len, _) = r.pop(&mut out).expect("a published cell is never recovered away");
        assert_eq!(len, 0, "cell 0 was claimed but its body was never written");
        for i in 0..3u8 {
            let (len, _) = r.pop(&mut out).expect("the rest still drains");
            assert_eq!(&out[..len as usize], &[i]);
        }
    }
}
