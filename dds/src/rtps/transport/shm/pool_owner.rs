//! Owner-side slot allocation. The free queue lives in process heap, never in
//! shared memory: readers may only clear their own bit in `SlotMeta::refs`.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use crate::rtps::transport::shm::pool::{Pool, SLOT_READY, SLOT_WRITING};
use crate::rtps::transport::shm::slot_ref::SlotRef;

pub(crate) struct SlotLease {
    pub(crate) class: u16,
    pub(crate) index: u32,
    ptr: *mut u8,
    size: usize,
    /// Alive for as long as the caller holds the lease. `reclaim_stale_leases`
    /// recovers only entries whose lease was dropped without `commit` or
    /// `abort` -- a panic, never a slow serializer.
    _live: Arc<()>,
    /// Keeps the mapping `ptr` points into alive. The borrow checker used to do
    /// this: the old `slot_mut` borrowed the owner, which owned the mapping. A
    /// lease now outlives its guard and can cross threads, so it holds the
    /// mapping itself.
    _map: Arc<dyn Send + Sync>,
}

// Safety: `acquire` put the slot in Writing with only the owner's bit set, and
// nothing clears that before `commit` or `abort`, so these bytes are reachable
// by no one else. Moving the lease moves that exclusive right with it.
unsafe impl Send for SlotLease {}

impl SlotLease {
    /// `&mut self` is what forbids two live slices into one slot; the pool
    /// mutex used to serve that role and no longer needs to be held here.
    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        // Safety: see the `Send` note above.
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) }
    }
}

pub(crate) struct PoolOwner {
    pool: Pool,
    own_slot: u32,
    own_bit: u64,
    own_epoch: u64,
    free: Vec<VecDeque<u32>>,
    leased: Vec<Vec<(u32, Instant, Weak<()>)>>,
    map: Arc<dyn Send + Sync>,
}

impl PoolOwner {
    /// `map` must own the mapping `pool` points into: every `SlotLease` clones
    /// it, and that clone is the only thing keeping the mapping alive once the
    /// lease outlives its guard. Tests over a stack region pass a placeholder
    /// because the region already outlives them.
    pub(crate) fn new(
        pool: Pool,
        own_slot: u32,
        own_epoch: u64,
        map: Arc<dyn Send + Sync>,
    ) -> PoolOwner {
        let classes = pool.class_count() as usize;
        let free = (0..classes)
            .map(|c| (0..pool.slot_count(c as u16)).collect::<VecDeque<u32>>())
            .collect();
        PoolOwner {
            own_bit: 1u64 << own_slot,
            own_slot,
            own_epoch,
            free,
            leased: vec![Vec::new(); classes],
            map,
            pool,
        }
    }

    pub(crate) fn pool(&self) -> &Pool {
        &self.pool
    }

    pub(crate) fn acquire(&mut self, len: usize) -> Option<SlotLease> {
        let class = self.pool.class_for(len)?;
        let queue_len = self.free[class as usize].len();
        for _ in 0..queue_len {
            let index = self.free[class as usize].pop_front()?;
            let meta = self.pool.meta(class, index)?;
            // Cheap early-out, not a guarantee: a reader may set its bit right
            // after this load, which is why the claim below is an RMW.
            if meta.refs.load(Ordering::Acquire) != 0 {
                self.free[class as usize].push_back(index);
                continue;
            }
            // The bump must precede the CAS. After it, a reader's step-4
            // re-check rejects this sample; doing the CAS first would let a
            // reader claim, re-read the still-old generation, accept, and then
            // watch this owner overwrite the slot under it.
            meta.generation.fetch_add(1, Ordering::AcqRel);
            // A CAS, never a blind store: the reader's `fetch_or` and this
            // write share one modification order on `refs`. If the reader won,
            // the CAS fails and the slot is left alone; if this won, the
            // reader's `fetch_or` reads from it and so observes the bumped
            // generation at step 4 and backs out. A blind store instead erases
            // the reader's bit while it holds a slice into the slot.
            //
            // A failed CAS leaves the generation bumped for nothing, so readers
            // mid-claim on that sample back out too. That costs one sample,
            // never the slot.
            //
            // The store also used to clear a dead reader's residual bits. A CAS
            // does not, so a dead reader's bit blocks the slot until
            // `reclaim_participant` clears it -- that is what that path is for.
            if meta
                .refs
                .compare_exchange(0, self.own_bit, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                self.free[class as usize].push_back(index);
                continue;
            }
            meta.len.store(0, Ordering::Relaxed);
            meta.state.store(SLOT_WRITING, Ordering::Release);
            let ptr = self.pool.data(class, index).expect("index came from the free queue");
            let size = self.pool.slot_size(class) as usize;
            let live = Arc::new(());
            self.leased[class as usize].push((index, Instant::now(), Arc::downgrade(&live)));
            return Some(SlotLease {
                class,
                index,
                ptr,
                size,
                _live: live,
                _map: Arc::clone(&self.map),
            });
        }
        None
    }

    pub(crate) fn commit(&mut self, lease: SlotLease, len: u32) -> SlotRef {
        let generation = {
            let meta = self.pool.meta(lease.class, lease.index).expect("lease is in range");
            meta.len.store(len, Ordering::Release);
            meta.state.store(SLOT_READY, Ordering::Release);
            meta.generation.load(Ordering::Acquire)
        };
        self.drop_lease(lease.class, lease.index);
        SlotRef {
            owner_slot: self.own_slot as u16,
            owner_epoch_low: self.own_epoch as u16,
            class: lease.class,
            index: lease.index,
            generation,
            len,
        }
    }

    pub(crate) fn abort(&mut self, lease: SlotLease) {
        self.release_own(lease.class, lease.index);
        self.drop_lease(lease.class, lease.index);
    }

    /// The slot must not have a live lease: this hands it back to the free
    /// queue, and the next `acquire` would issue a second lease for it.
    pub(crate) fn release_own(&mut self, class: u16, index: u32) {
        if let Some(meta) = self.pool.meta(class, index) {
            meta.refs.fetch_and(!self.own_bit, Ordering::AcqRel);
        }
        self.free[class as usize].push_back(index);
    }

    fn drop_lease(&mut self, class: u16, index: u32) {
        self.leased[class as usize].retain(|(i, _, _)| *i != index);
    }

    /// Clear a dead participant's bit across every slot this owner holds.
    pub(crate) fn reclaim_participant(&mut self, dead_slot: u32) -> usize {
        let dead_bit = 1u64 << dead_slot;
        let mut cleared = 0;
        for class in 0..self.pool.class_count() {
            for index in 0..self.pool.slot_count(class) {
                let meta = self.pool.meta(class, index).expect("in range");
                let prev = meta.refs.fetch_and(!dead_bit, Ordering::AcqRel);
                if prev & dead_bit != 0 {
                    cleared += 1;
                }
            }
        }
        cleared
    }

    /// Recover slots left in Writing state by a loan that was never committed
    /// or aborted. Safe because an uncommitted slot is referenced by nobody.
    pub(crate) fn reclaim_stale_leases(&mut self, older_than: Duration) -> usize {
        let now = Instant::now();
        let mut recovered = 0;
        for class in 0..self.free.len() {
            let stale: Vec<u32> = self.leased[class]
                .iter()
                // A lease still in the caller's hands is not abandoned, however
                // old: 6.2 hands its slice to user serialization code, which may
                // run arbitrarily long. Only a lease dropped without `commit` or
                // `abort` is recoverable.
                .filter(|(_, at, live)| {
                    live.strong_count() == 0 && now.duration_since(*at) >= older_than
                })
                .map(|(i, _, _)| *i)
                .collect();
            // `Weak::strong_count` is a relaxed load. Pair it with an acquire
            // fence so the abandoned writer's stores to the slot happen-before
            // the next owner's stores to it.
            if !stale.is_empty() {
                std::sync::atomic::fence(Ordering::Acquire);
            }
            for index in stale {
                self.release_own(class as u16, index);
                self.drop_lease(class as u16, index);
                recovered += 1;
            }
        }
        recovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::pool::{PoolLayout, SLOT_READY, SLOT_WRITING};
    use crate::rtps::transport::shm::pool_reader::PoolReader;
    use crate::rtps::transport::shm::test_region::AlignedRegion;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;

    /// The region must outlive the owner, so callers bind both.
    fn owner() -> (AlignedRegion, PoolOwner) {
        let layout = PoolLayout::new(&[(64, 2)]).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        let pool = unsafe { Pool::init(region.ptr(), &layout) };
        // `region` outlives the owner lexically, so the mapping a lease would
        // hold is not `region` itself -- any placeholder does.
        (region, PoolOwner::new(pool, 3, 1, Arc::new(())))
    }

    #[test]
    fn acquire_marks_writing_and_bumps_generation() {
        let (_region, mut o) = owner();
        let lease = o.acquire(10).unwrap();
        let meta = o.pool().meta(lease.class, lease.index).unwrap();
        assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_WRITING);
        assert_eq!(meta.generation.load(Ordering::Relaxed), 1);
        assert_eq!(meta.refs.load(Ordering::Relaxed), 1 << 3);
    }

    #[test]
    fn commit_publishes_length_and_returns_matching_ref() {
        let (_region, mut o) = owner();
        let mut lease = o.acquire(10).unwrap();
        lease.bytes_mut()[..3].copy_from_slice(b"abc");
        let r = o.commit(lease, 3);
        let meta = o.pool().meta(r.class, r.index).unwrap();
        assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_READY);
        assert_eq!(meta.len.load(Ordering::Relaxed), 3);
        assert_eq!(r.generation, meta.generation.load(Ordering::Relaxed));
        assert_eq!(r.owner_slot, 3);
        assert_eq!(r.len, 3);
    }

    #[test]
    fn abort_returns_the_slot_for_reuse() {
        let (_region, mut o) = owner();
        let first = o.acquire(10).unwrap();
        let (c, i) = (first.class, first.index);
        o.abort(first);
        let second = o.acquire(10).unwrap();
        let third = o.acquire(10).unwrap();
        assert!([second.index, third.index].contains(&i));
        assert_eq!(second.class, c);
    }

    #[test]
    fn acquire_fails_when_every_slot_is_referenced() {
        let (_region, mut o) = owner();
        let a = o.acquire(10).unwrap();
        let b = o.acquire(10).unwrap();
        o.commit(a, 1);
        o.commit(b, 1);
        assert!(o.acquire(10).is_none());
    }

    #[test]
    fn release_own_frees_the_slot() {
        let (_region, mut o) = owner();
        let a = o.acquire(10).unwrap();
        let b = o.acquire(10).unwrap();
        let r = o.commit(a, 1);
        o.commit(b, 1);
        o.release_own(r.class, r.index);
        assert!(o.acquire(10).is_some());
    }

    #[test]
    fn acquire_skips_a_slot_a_reader_still_holds() {
        let (_region, mut o) = owner();
        let a = o.acquire(10).unwrap();
        let b = o.acquire(10).unwrap();
        let ra = o.commit(a, 1);
        o.commit(b, 1);

        // A reader claims the slot, then the owner drops its own bit.
        let reader_bit = 1u64 << 5;
        o.pool().meta(ra.class, ra.index).unwrap().refs.fetch_or(reader_bit, Ordering::AcqRel);
        o.release_own(ra.class, ra.index);
        assert!(o.acquire(10).is_none(), "still referenced by the reader");

        o.pool().meta(ra.class, ra.index).unwrap().refs.fetch_and(!reader_bit, Ordering::AcqRel);
        assert!(o.acquire(10).is_some());
    }

    #[test]
    fn oversized_request_is_rejected() {
        let (_region, mut o) = owner();
        assert!(o.acquire(65).is_none());
    }

    fn write_pattern(buf: &mut [u8], generation: u32) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (generation as u8) ^ (i as u8);
        }
    }

    fn matches_pattern(buf: &[u8], generation: u32) -> bool {
        buf.iter().enumerate().all(|(i, b)| *b == (generation as u8) ^ (i as u8))
    }

    /// Regression test for the blind `refs.store` this owner used to do when
    /// claiming a slot. With a store, a reader could pass all four validation
    /// steps and then have its bit erased and the slot overwritten under it;
    /// the assertions below are exactly those two symptoms.
    #[test]
    fn concurrent_readers_never_see_a_slot_recycled_under_them() {
        const SLOT_SIZE: usize = 1024;
        const SLOTS: u32 = 2;
        const OWNER_SLOT: u32 = 0;
        const READERS: u32 = 3;
        const FIRST_READER_SLOT: u32 = 5;
        let run_for = Duration::from_millis(1500);

        let layout = PoolLayout::new(&[(SLOT_SIZE as u32, SLOTS)]).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        // Safety: `region` is a zeroed, 64-aligned block of exactly
        // `total_size()` bytes that outlives every handle below.
        let mut owner = PoolOwner::new(
            unsafe { Pool::init(region.ptr(), &layout) },
            OWNER_SLOT,
            1,
            Arc::new(()),
        );
        let readers: Vec<(u32, PoolReader)> = (0..READERS)
            .map(|i| {
                // Safety: same region, already initialized by `Pool::init`.
                let pool = unsafe { Pool::attach(region.ptr(), layout.total_size()) }.unwrap();
                (
                    FIRST_READER_SLOT + i,
                    PoolReader::new(pool, FIRST_READER_SLOT + i, OWNER_SLOT as u16, 1),
                )
            })
            .collect();

        let latest: Mutex<Option<SlotRef>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let claimed = AtomicU64::new(0);

        std::thread::scope(|s| {
            for (my_slot, reader) in readers {
                let (latest, stop, claimed) = (&latest, &stop, &claimed);
                s.spawn(move || {
                    let my_bit = 1u64 << my_slot;
                    while !stop.load(Ordering::Relaxed) {
                        let Some(r) = *latest.lock().unwrap() else {
                            std::hint::spin_loop();
                            continue;
                        };
                        let Some(c) = reader.claim(&r) else {
                            std::hint::spin_loop();
                            continue;
                        };
                        let intact = matches_pattern(reader.bytes(&c), r.generation);
                        // The owner must not have cleared this reader's bit:
                        // only `release` and `reclaim_participant` may, and
                        // neither runs here.
                        let held = reader
                            .pool()
                            .meta(r.class, r.index)
                            .unwrap()
                            .refs
                            .load(Ordering::Acquire)
                            & my_bit
                            != 0;
                        reader.release(c);
                        assert!(
                            intact,
                            "slot rewritten while claimed at generation {}",
                            r.generation
                        );
                        assert!(held, "owner erased a live reader's bit");
                        claimed.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }

            let deadline = Instant::now() + run_for;
            let mut published = 0u64;
            while Instant::now() < deadline {
                let Some(mut lease) = owner.acquire(SLOT_SIZE) else {
                    std::hint::spin_loop();
                    continue;
                };
                let generation = owner
                    .pool()
                    .meta(lease.class, lease.index)
                    .unwrap()
                    .generation
                    .load(Ordering::Acquire);
                write_pattern(&mut lease.bytes_mut()[..SLOT_SIZE], generation);
                let r = owner.commit(lease, SLOT_SIZE as u32);
                *latest.lock().unwrap() = Some(r);
                // Drop the owner bit so the slot can be recycled while readers
                // may still hold it -- the contended case this test is about.
                owner.release_own(r.class, r.index);
                published += 1;
            }
            stop.store(true, Ordering::Relaxed);
            assert!(published > 1000, "too few publications to be meaningful: {published}");
        });

        assert!(claimed.load(Ordering::Relaxed) > 0, "readers never claimed anything");
    }

    #[test]
    fn a_live_lease_survives_the_sweep_and_a_dropped_one_does_not() {
        let (_region, mut o) = owner();
        let mut lease = o.acquire(10).unwrap();
        lease.bytes_mut()[0] = 1;
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(
            o.reclaim_stale_leases(Duration::from_millis(10)),
            0,
            "a lease still in the caller's hands is not abandoned, however old"
        );

        drop(lease);
        assert_eq!(o.reclaim_stale_leases(Duration::from_millis(10)), 1);
    }

    #[test]
    fn a_lease_keeps_the_mapping_alive() {
        let layout = PoolLayout::new(&[(64, 2)]).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        let map: Arc<dyn Send + Sync> = Arc::new(());
        // Safety: `region` is a zeroed, 64-aligned block of exactly
        // `total_size()` bytes that outlives every handle below.
        let mut o =
            PoolOwner::new(unsafe { Pool::init(region.ptr(), &layout) }, 3, 1, Arc::clone(&map));
        assert_eq!(Arc::strong_count(&map), 2);

        let lease = o.acquire(8).unwrap();
        assert_eq!(
            Arc::strong_count(&map),
            3,
            "a lease must hold the mapping its pointer addresses"
        );

        drop(lease);
        assert_eq!(Arc::strong_count(&map), 2);
    }
}
