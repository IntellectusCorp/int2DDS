//! Reader-side slot access. A reader only ever sets and clears its own bit.

use std::sync::atomic::Ordering;

use crate::rtps::transport::shm::pool::{Pool, SLOT_READY};
use crate::rtps::transport::shm::slot_ref::SlotRef;

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
        PoolReader { pool, my_bit: 1u64 << my_slot, owner_slot, owner_epoch_low }
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
        // 3. claim
        meta.refs.fetch_or(self.my_bit, Ordering::AcqRel);
        // 4. re-check after claiming
        if meta.generation.load(Ordering::Acquire) != r.generation {
            meta.refs.fetch_and(!self.my_bit, Ordering::AcqRel);
            return None;
        }
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
        if let Some(meta) = self.pool.meta(c.class, c.index) {
            meta.refs.fetch_and(!self.my_bit, Ordering::AcqRel);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::pool::PoolLayout;
    use crate::rtps::transport::shm::pool_owner::PoolOwner;
    use crate::rtps::transport::shm::test_region::AlignedRegion;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    struct Fixture {
        _region: AlignedRegion,
        owner: PoolOwner,
        reader: PoolReader,
    }

    fn fixture() -> Fixture {
        let layout = PoolLayout::new(&[(64, 2)]).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        let base = region.ptr();
        let owner = PoolOwner::new(unsafe { Pool::init(base, &layout) }, 3, 1);
        let reader =
            PoolReader::new(unsafe { Pool::attach(base, layout.total_size()) }.unwrap(), 5, 3, 1);
        Fixture { _region: region, owner, reader }
    }

    #[test]
    fn claim_exposes_committed_bytes() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        f.owner.slot_mut(&lease)[..3].copy_from_slice(b"xyz");
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
        let _a = f.owner.acquire(8).unwrap();
        let _b = f.owner.acquire(8).unwrap();
        assert!(f.owner.acquire(8).is_none());
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(f.owner.reclaim_stale_leases(Duration::from_millis(10)), 2);
        assert!(f.owner.acquire(8).is_some());
    }
}
