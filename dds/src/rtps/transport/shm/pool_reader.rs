//! Reader-side slot access. A reader only ever sets and clears its own bit.

use std::sync::atomic::Ordering;
use std::sync::Mutex;

use crate::rtps::transport::shm::pool::{Pool, SLOT_READY};
use crate::rtps::transport::shm::slot_ref::SlotRef;

/// Must reach `PoolReader::release`. Dropping one without it strands the local
/// count above zero, and from then on nothing can lower this reader's shared
/// bit -- the owner's `acquire` CAS fails on that slot for the rest of the
/// process's life.
pub(crate) struct ClaimedSlot {
    class: u16,
    index: u32,
    len: u32,
}

pub(crate) struct PoolReader {
    pool: Pool,
    my_bit: u64,
    owner_slot: u16,
    owner_epoch_low: u16,
    /// Per-slot claim count, one Vec per class. The shared bit moves only on
    /// the 0 -> 1 and 1 -> 0 transitions, so several local readers holding one
    /// sample cost two shared-memory writes in total, not two per reader.
    local: Mutex<Vec<Vec<u32>>>,
}

impl PoolReader {
    /// `owner_slot` / `owner_epoch_low` identify the segment this reader is
    /// attached to; `claim` accepts only descriptors minted by that owner.
    pub(crate) fn new(
        pool: Pool,
        my_slot: u32,
        owner_slot: u16,
        owner_epoch_low: u16,
    ) -> PoolReader {
        let local =
            (0..pool.class_count()).map(|c| vec![0u32; pool.slot_count(c) as usize]).collect();
        PoolReader {
            pool,
            my_bit: 1u64 << my_slot,
            owner_slot,
            owner_epoch_low,
            local: Mutex::new(local),
        }
    }

    pub(crate) fn pool(&self) -> &Pool {
        &self.pool
    }

    pub(crate) fn claim(&self, r: &SlotRef) -> Option<ClaimedSlot> {
        // 0. owner identity, before anything else: `generation` restarts at 0
        // in a fresh segment, so a descriptor left over from a previous
        // incarnation of this slot id would otherwise validate on generation
        // alone and point into an unrelated participant's pool.
        if r.owner_slot != self.owner_slot || r.owner_epoch_low != self.owner_epoch_low {
            return None;
        }
        // 1. structural
        let meta = self.pool.meta(r.class, r.index)?;
        if r.len > self.pool.slot_size(r.class) {
            return None;
        }
        // 2. state
        if meta.state.load(Ordering::Acquire) != SLOT_READY
            || meta.generation.load(Ordering::Acquire) != r.generation
        {
            return None;
        }
        // 3. claim. The local count and the shared bit move under one lock: a
        // thread that takes the count 1 -> 2 must not proceed before the thread
        // that took it 0 -> 1 has set the bit, or it would read the slot with
        // no bit standing for it.
        let mut local = self.local.lock().unwrap_or_else(|e| e.into_inner());
        let n = &mut local[r.class as usize][r.index as usize];
        *n += 1;
        if *n == 1 {
            meta.refs.fetch_or(self.my_bit, Ordering::AcqRel);
        }
        // 4. re-check after claiming
        if meta.generation.load(Ordering::Acquire) != r.generation {
            *n -= 1;
            if *n == 0 {
                meta.refs.fetch_and(!self.my_bit, Ordering::AcqRel);
            }
            return None;
        }
        drop(local);
        Some(ClaimedSlot { class: r.class, index: r.index, len: r.len })
    }

    /// Both parameters share `'a` on purpose: elision would bind the result to
    /// `&self` alone, letting a caller keep the slice across `release(c)` — which
    /// consumes `c` — and read a slot the owner has already recycled.
    pub(crate) fn bytes<'a>(&'a self, c: &'a ClaimedSlot) -> &'a [u8] {
        let ptr = self.pool.data(c.class, c.index).expect("claim validated the range");
        // Safety: `claim` bounded `index` and `len`, and this reader's bit in
        // `refs` keeps the owner from recycling the slot while `c` is alive.
        unsafe { std::slice::from_raw_parts(ptr, c.len as usize) }
    }

    pub(crate) fn release(&self, c: ClaimedSlot) {
        let mut local = self.local.lock().unwrap_or_else(|e| e.into_inner());
        let n = &mut local[c.class as usize][c.index as usize];
        debug_assert!(*n > 0, "release without a matching claim");
        // Saturating, not wrapping: an unmatched release used to be harmless
        // because the clear was idempotent. Wrapping to u32::MAX would pin the
        // slot for the process's whole life.
        *n = n.saturating_sub(1);
        if *n == 0 {
            if let Some(meta) = self.pool.meta(c.class, c.index) {
                meta.refs.fetch_and(!self.my_bit, Ordering::AcqRel);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::pool::PoolLayout;
    use crate::rtps::transport::shm::pool_owner::PoolOwner;
    use crate::rtps::transport::shm::test_region::AlignedRegion;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    struct Fixture {
        _region: AlignedRegion,
        owner: PoolOwner,
        reader: PoolReader,
    }

    fn fixture() -> Fixture {
        let layout = PoolLayout::new(&[(64, 2)]).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        let base = region.ptr();
        let owner = PoolOwner::new(unsafe { Pool::init(base, &layout) }, 3, 1, Arc::new(()));
        let reader =
            PoolReader::new(unsafe { Pool::attach(base, layout.total_size()) }.unwrap(), 5, 3, 1);
        Fixture { _region: region, owner, reader }
    }

    #[test]
    fn claim_exposes_committed_bytes() {
        let mut f = fixture();
        let mut lease = f.owner.acquire(8).unwrap();
        lease.bytes_mut()[..3].copy_from_slice(b"xyz");
        let r = f.owner.commit(lease, 3);
        let claimed = f.reader.claim(&r).unwrap();
        assert_eq!(f.reader.bytes(&claimed), b"xyz");
        let meta = f.reader.pool().meta(r.class, r.index).unwrap();
        assert_eq!(meta.refs.load(Ordering::Acquire) & (1 << 5), 1 << 5);
    }

    #[test]
    fn claim_rejects_stale_generation() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let mut r = f.owner.commit(lease, 1);
        r.generation = r.generation.wrapping_sub(1);
        assert!(f.reader.claim(&r).is_none());
    }

    #[test]
    fn claim_rejects_a_descriptor_from_another_incarnation() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let r = f.owner.commit(lease, 1);
        assert!(f.reader.claim(&r).is_some());

        let mut stale_epoch = r;
        stale_epoch.owner_epoch_low = r.owner_epoch_low.wrapping_sub(1);
        assert!(f.reader.claim(&stale_epoch).is_none(), "stale epoch must be rejected");

        let mut other_owner = r;
        other_owner.owner_slot = r.owner_slot + 1;
        assert!(f.reader.claim(&other_owner).is_none(), "other owner must be rejected");
    }

    #[test]
    fn claim_rejects_out_of_range_descriptor() {
        let f = fixture();
        let r = SlotRef {
            owner_slot: 3,
            owner_epoch_low: 1,
            class: 0,
            index: 99,
            generation: 1,
            len: 1,
        };
        assert!(f.reader.claim(&r).is_none());
    }

    #[test]
    fn claim_rejects_length_beyond_slot_size() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let mut r = f.owner.commit(lease, 1);
        r.len = 65;
        assert!(f.reader.claim(&r).is_none());
    }

    #[test]
    fn claim_rejects_a_slot_still_being_written() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let generation = f
            .owner
            .pool()
            .meta(lease.class, lease.index)
            .unwrap()
            .generation
            .load(Ordering::Acquire);
        let r = SlotRef {
            owner_slot: 3,
            owner_epoch_low: 1,
            class: lease.class,
            index: lease.index,
            generation,
            len: 1,
        };
        assert!(f.reader.claim(&r).is_none(), "slot is still in Writing state");
    }

    #[test]
    fn release_lets_the_owner_reuse_the_slot() {
        let mut f = fixture();
        let a = f.owner.acquire(8).unwrap();
        let b = f.owner.acquire(8).unwrap();
        let ra = f.owner.commit(a, 1);
        f.owner.commit(b, 1);
        let claimed = f.reader.claim(&ra).unwrap();
        f.owner.release_own(ra.class, ra.index);
        assert!(f.owner.acquire(8).is_none());
        f.reader.release(claimed);
        assert!(f.owner.acquire(8).is_some());
    }

    #[test]
    fn reclaim_participant_clears_only_that_bit() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let r = f.owner.commit(lease, 1);
        let _claimed = f.reader.claim(&r).unwrap();
        assert_eq!(f.owner.reclaim_participant(5), 1);
        let meta = f.reader.pool().meta(r.class, r.index).unwrap();
        assert_eq!(meta.refs.load(Ordering::Acquire), 1 << 3);
    }

    #[test]
    fn reclaim_stale_leases_recovers_abandoned_writes() {
        let mut f = fixture();
        // Dropped without `commit` or `abort` -- what a panicking writer leaves.
        let _ = f.owner.acquire(8).unwrap();
        let _ = f.owner.acquire(8).unwrap();
        assert!(f.owner.acquire(8).is_none());
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(f.owner.reclaim_stale_leases(Duration::from_millis(10)), 2);
        assert!(f.owner.acquire(8).is_some());
    }

    #[test]
    fn two_claims_of_one_slot_hold_the_bit_until_both_release() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let r = f.owner.commit(lease, 3);
        let first = f.reader.claim(&r).unwrap();
        let second = f.reader.claim(&r).unwrap();
        let meta = f.reader.pool().meta(r.class, r.index).unwrap();

        f.reader.release(first);
        assert_eq!(
            meta.refs.load(Ordering::Acquire) & (1 << 5),
            1 << 5,
            "the bit must survive while a second claim is still held"
        );

        f.reader.release(second);
        assert_eq!(meta.refs.load(Ordering::Acquire) & (1 << 5), 0);
    }

    /// The production topology: `PeerMap` hands every local reader the same
    /// `Arc<PeerSegment>`, so they share one `PoolReader` and one bit. The
    /// shared bit must stand for the whole time any of them holds a claim.
    #[test]
    fn threads_sharing_one_reader_never_hold_a_slot_without_the_bit() {
        const SLOT_SIZE: usize = 1024;
        const SLOTS: u32 = 2;
        const OWNER_SLOT: u32 = 0;
        const READER_SLOT: u32 = 5;
        const THREADS: u32 = 3;
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
        // Safety: same region, already initialized by `Pool::init`.
        let reader = PoolReader::new(
            unsafe { Pool::attach(region.ptr(), layout.total_size()) }.unwrap(),
            READER_SLOT,
            OWNER_SLOT as u16,
            1,
        );

        let latest: Mutex<Option<SlotRef>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let claimed = AtomicU64::new(0);
        let my_bit = 1u64 << READER_SLOT;

        std::thread::scope(|s| {
            for _ in 0..THREADS {
                let (reader, latest, stop, claimed) = (&reader, &latest, &stop, &claimed);
                s.spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        let Some(r) = *latest.lock().unwrap() else {
                            std::hint::spin_loop();
                            continue;
                        };
                        let Some(c) = reader.claim(&r) else {
                            std::hint::spin_loop();
                            continue;
                        };
                        // The bit must already stand when `claim` returns. It is
                        // set only on the 0 -> 1 transition, so a thread that
                        // took the count 1 -> 2 is relying on another thread
                        // having set it under the same lock.
                        let held = reader
                            .pool()
                            .meta(r.class, r.index)
                            .unwrap()
                            .refs
                            .load(Ordering::Acquire)
                            & my_bit
                            != 0;
                        reader.release(c);
                        assert!(held, "claim returned with no bit standing for it");
                        claimed.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }

            let deadline = Instant::now() + run_for;
            while Instant::now() < deadline {
                let Some(lease) = owner.acquire(SLOT_SIZE) else {
                    std::hint::spin_loop();
                    continue;
                };
                let r = owner.commit(lease, SLOT_SIZE as u32);
                *latest.lock().unwrap() = Some(r);
                owner.release_own(r.class, r.index);
            }
            stop.store(true, Ordering::Relaxed);
        });

        assert!(claimed.load(Ordering::Relaxed) > 0, "no reader ever claimed");
    }
}
