//! Payload slot pool laid out inside a shared segment.
//!
//! Only the owning participant allocates; its free queue lives in process
//! heap. Every other process touches nothing but its own bit in
//! `SlotMeta::refs`, indexed by registry slot. A slot goes Free -> Writing
//! (owner bit set) -> Ready, and back to Free once every bit is clear.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::rtps::transport::shm::slot::SlotRef;
use crate::rtps::transport::shm::{LayoutError, SHARED_ALIGN};

pub(crate) const MAX_CLASSES: usize = 4;
const POOL_MAGIC: u64 = 0x494E_5432_5A43_504C; // "INT2ZCPL"

pub(crate) const SLOT_FREE: u32 = 0;
pub(crate) const SLOT_WRITING: u32 = 1;
pub(crate) const SLOT_READY: u32 = 2;

const PAGE: u64 = 4096;

/// How long a Writing slot must have sat with its lease dropped before
/// `acquire` takes it back.
pub(crate) const STALE_LEASE_AFTER: Duration = Duration::from_secs(1);

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub(crate) struct ClassDesc {
    pub slot_size: u32,
    pub slot_count: u32,
    pub meta_offset: u64,
    pub data_offset: u64,
}

#[repr(C, align(64))]
pub(crate) struct PoolHeader {
    pub magic: AtomicU64,
    pub class_count: u32,
    pub _pad: u32,
    pub classes: [ClassDesc; MAX_CLASSES],
}

#[repr(C, align(64))]
pub(crate) struct SlotMeta {
    pub state: AtomicU32,
    pub generation: AtomicU32,
    pub len: AtomicU32,
    pub _pad: u32,
    pub refs: AtomicU64,
}

// Read out of the mapping by every peer, so its size is wire format.
const _: () = assert!(std::mem::size_of::<SlotMeta>() == 64);

pub(crate) struct PoolLayout {
    classes: Vec<ClassDesc>,
    total: u64,
}

const fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
}

impl PoolLayout {
    /// Sizes must be strictly ascending: `class_for` picks the first fit.
    pub(crate) fn new(classes: &[(u32, u32)]) -> Result<PoolLayout, LayoutError> {
        if classes.is_empty() || classes.len() > MAX_CLASSES {
            return Err(LayoutError::BadConfig);
        }
        if !classes.windows(2).all(|w| w[0].0 < w[1].0) {
            return Err(LayoutError::BadConfig);
        }
        let mut cursor = align_up(std::mem::size_of::<PoolHeader>() as u64, SHARED_ALIGN);
        let mut descs = Vec::with_capacity(classes.len());
        for &(slot_size, slot_count) in classes {
            let meta_offset = cursor;
            cursor += std::mem::size_of::<SlotMeta>() as u64 * slot_count as u64;
            cursor = align_up(cursor, PAGE);
            let data_offset = cursor;
            cursor += slot_size as u64 * slot_count as u64;
            cursor = align_up(cursor, PAGE);
            descs.push(ClassDesc { slot_size, slot_count, meta_offset, data_offset });
        }
        Ok(PoolLayout { classes: descs, total: cursor })
    }

    pub(crate) fn total_size(&self) -> u64 {
        self.total
    }
}

#[derive(Debug)]
pub(crate) struct Pool {
    base: *mut u8,
    classes: Vec<ClassDesc>,
}

// Safety: geometry is written once before publication. The only non-atomic
// access to slot data is `SlotLease::bytes_mut`, gated by the Writing state
// and the owner-only bit.
unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}

impl Pool {
    /// # Safety
    /// `base` must point to a zeroed writable region of `layout.total_size()` bytes.
    pub(crate) unsafe fn init(base: *mut u8, layout: &PoolLayout) -> Pool {
        let header = base as *mut PoolHeader;
        std::ptr::addr_of_mut!((*header).class_count).write(layout.classes.len() as u32);
        for (i, desc) in layout.classes.iter().enumerate() {
            std::ptr::addr_of_mut!((*header).classes[i]).write(*desc);
        }
        for desc in &layout.classes {
            for index in 0..desc.slot_count {
                let meta = base.add(desc.meta_offset as usize) as *mut SlotMeta;
                let meta = &*meta.add(index as usize);
                meta.state.store(SLOT_FREE, Ordering::Relaxed);
                meta.generation.store(0, Ordering::Relaxed);
                meta.len.store(0, Ordering::Relaxed);
                meta.refs.store(0, Ordering::Relaxed);
            }
        }
        (*header).magic.store(POOL_MAGIC, Ordering::Release);
        Pool { base, classes: layout.classes.clone() }
    }

    /// # Safety
    /// `base` must point to a region initialized by `init` that is at least
    /// `region_size` bytes and stays mapped while the pool is used.
    pub(crate) unsafe fn attach(base: *mut u8, region_size: u64) -> Result<Pool, LayoutError> {
        if region_size < std::mem::size_of::<PoolHeader>() as u64 {
            return Err(LayoutError::SizeMismatch);
        }
        let magic = &*std::ptr::addr_of!((*(base as *const PoolHeader)).magic);
        if magic.load(Ordering::Acquire) != POOL_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const PoolHeader);
        let count = header.class_count as usize;
        if count == 0 || count > MAX_CLASSES {
            return Err(LayoutError::BadVersion);
        }
        let classes = header.classes[..count].to_vec();
        // Geometry comes from shared memory: bound and align-check every
        // region before the accessors do pointer math with it.
        for c in &classes {
            let meta_len =
                (std::mem::size_of::<SlotMeta>() as u64).checked_mul(c.slot_count.into());
            let data_len = (c.slot_size as u64).checked_mul(c.slot_count.into());
            let within = match (meta_len, data_len) {
                (Some(ml), Some(dl)) => c
                    .meta_offset
                    .checked_add(ml)
                    .zip(c.data_offset.checked_add(dl))
                    .is_some_and(|(m, d)| m <= region_size && d <= region_size),
                _ => false,
            };
            if !within
                || !c.meta_offset.is_multiple_of(SHARED_ALIGN)
                || !c.data_offset.is_multiple_of(SHARED_ALIGN)
            {
                return Err(LayoutError::BadOffset);
            }
        }
        Ok(Pool { base, classes })
    }

    pub(crate) fn class_count(&self) -> u16 {
        self.classes.len() as u16
    }

    pub(crate) fn class_for(&self, len: usize) -> Option<u16> {
        self.classes.iter().position(|c| c.slot_size as usize >= len).map(|i| i as u16)
    }

    pub(crate) fn slot_size(&self, class: u16) -> u32 {
        self.classes[class as usize].slot_size
    }

    pub(crate) fn slot_count(&self, class: u16) -> u32 {
        self.classes[class as usize].slot_count
    }

    fn desc(&self, class: u16, index: u32) -> Option<&ClassDesc> {
        let desc = self.classes.get(class as usize)?;
        (index < desc.slot_count).then_some(desc)
    }

    pub(crate) fn meta(&self, class: u16, index: u32) -> Option<&SlotMeta> {
        let desc = self.desc(class, index)?;
        unsafe {
            let base = self.base.add(desc.meta_offset as usize) as *const SlotMeta;
            Some(&*base.add(index as usize))
        }
    }

    pub(crate) fn data(&self, class: u16, index: u32) -> Option<*mut u8> {
        let desc = self.desc(class, index)?;
        // Widen before multiplying: `slot_size * index` wraps in u32.
        let offset = desc.data_offset as usize + desc.slot_size as usize * index as usize;
        unsafe { Some(self.base.add(offset)) }
    }
}

pub(crate) struct SlotLease {
    pub(crate) class: u16,
    pub(crate) index: u32,
    ptr: *mut u8,
    size: usize,
    /// Alive while the caller holds the lease; `reclaim_stale_leases` only
    /// recovers entries whose lease was dropped without `commit` or `abort`.
    _live: Arc<()>,
    /// Keeps the mapping alive: a lease outlives its guard and may cross threads.
    _map: Arc<dyn Send + Sync>,
}

// Safety: `acquire` put the slot in Writing with only the owner bit set, so
// these bytes are reachable by no one else until `commit` or `abort`.
unsafe impl Send for SlotLease {}

impl SlotLease {
    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
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
    /// `map` must own the mapping `pool` points into; every lease clones it.
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
        if let Some(lease) = self.try_acquire(len) {
            return Some(lease);
        }
        if self.reclaim_stale_leases(STALE_LEASE_AFTER) == 0 {
            return None;
        }
        self.try_acquire(len)
    }

    fn try_acquire(&mut self, len: usize) -> Option<SlotLease> {
        let class = self.pool.class_for(len)?;
        let queue_len = self.free[class as usize].len();
        for _ in 0..queue_len {
            let index = self.free[class as usize].pop_front()?;
            let meta = self.pool.meta(class, index)?;
            if meta.refs.load(Ordering::Acquire) != 0 {
                self.free[class as usize].push_back(index);
                continue;
            }
            // Bump before the CAS so a reader mid-claim re-checks against the
            // new generation and backs out. A CAS, never a blind store: the
            // reader's `fetch_or` and this write share one modification order,
            // so whichever lands second sees the other.
            meta.generation.fetch_add(1, Ordering::AcqRel);
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

    /// The slot must not have a live lease.
    pub(crate) fn release_own(&mut self, class: u16, index: u32) {
        if let Some(meta) = self.pool.meta(class, index) {
            meta.refs.fetch_and(!self.own_bit, Ordering::AcqRel);
        }
        self.free[class as usize].push_back(index);
    }

    fn drop_lease(&mut self, class: u16, index: u32) {
        self.leased[class as usize].retain(|(i, _, _)| *i != index);
    }

    /// Clear a dead participant's bit across every slot.
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

    /// Recover slots left in Writing by a lease dropped without `commit` or
    /// `abort`. A lease still in the caller's hands is never touched.
    pub(crate) fn reclaim_stale_leases(&mut self, older_than: Duration) -> usize {
        let now = Instant::now();
        let mut recovered = 0;
        for class in 0..self.free.len() {
            let stale: Vec<u32> = self.leased[class]
                .iter()
                .filter(|(_, at, live)| {
                    live.strong_count() == 0 && now.duration_since(*at) >= older_than
                })
                .map(|(i, _, _)| *i)
                .collect();
            // `strong_count` is relaxed; order the abandoned writer's stores
            // before the next owner's.
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

/// Must reach `PoolReader::release`, or this reader's bit stays set on the
/// slot for the rest of the process's life.
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
    /// Per-slot claim count. The shared bit moves only on 0 -> 1 and 1 -> 0.
    local: Mutex<Vec<Vec<u32>>>,
}

impl PoolReader {
    /// `claim` accepts only descriptors minted by `owner_slot` at `owner_epoch_low`.
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
        // Owner identity first: generation restarts at 0 in a fresh segment.
        if r.owner_slot != self.owner_slot || r.owner_epoch_low != self.owner_epoch_low {
            return None;
        }
        let meta = self.pool.meta(r.class, r.index)?;
        if r.len > self.pool.slot_size(r.class) {
            return None;
        }
        if meta.state.load(Ordering::Acquire) != SLOT_READY
            || meta.generation.load(Ordering::Acquire) != r.generation
        {
            return None;
        }
        // Count and bit move under one lock, so a second claimer never reads
        // the slot before the first has set the bit.
        let mut local = self.local.lock().unwrap_or_else(|e| e.into_inner());
        let n = &mut local[r.class as usize][r.index as usize];
        *n += 1;
        if *n == 1 {
            meta.refs.fetch_or(self.my_bit, Ordering::AcqRel);
        }
        // Re-check after claiming: the owner may have recycled in between.
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

    /// Both parameters share `'a` so the slice cannot outlive `release(c)`.
    pub(crate) fn bytes<'a>(&'a self, c: &'a ClaimedSlot) -> &'a [u8] {
        let ptr = self.pool.data(c.class, c.index).expect("claim validated the range");
        unsafe { std::slice::from_raw_parts(ptr, c.len as usize) }
    }

    pub(crate) fn release(&self, c: ClaimedSlot) {
        let mut local = self.local.lock().unwrap_or_else(|e| e.into_inner());
        let n = &mut local[c.class as usize][c.index as usize];
        debug_assert!(*n > 0, "release without a matching claim");
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
    use crate::rtps::transport::shm::test_region::AlignedRegion;
    use std::sync::atomic::AtomicBool;

    const OWNER_SLOT: u32 = 3;
    const READER_SLOT: u32 = 5;

    struct Fixture {
        _region: AlignedRegion,
        owner: PoolOwner,
        reader: PoolReader,
    }

    fn fixture_with(classes: &[(u32, u32)]) -> Fixture {
        let layout = PoolLayout::new(classes).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        let base = region.ptr();
        let owner =
            PoolOwner::new(unsafe { Pool::init(base, &layout) }, OWNER_SLOT, 1, Arc::new(()));
        let reader = PoolReader::new(
            unsafe { Pool::attach(base, layout.total_size()) }.unwrap(),
            READER_SLOT,
            OWNER_SLOT as u16,
            1,
        );
        Fixture { _region: region, owner, reader }
    }

    fn fixture() -> Fixture {
        fixture_with(&[(64, 2)])
    }

    #[test]
    fn layout_rejects_a_bad_class_configuration() {
        let bad = |c: &[(u32, u32)]| matches!(PoolLayout::new(c), Err(LayoutError::BadConfig));
        assert!(bad(&[]));
        assert!(bad(&[(64, 1), (64, 1)]), "equal sizes are not ascending");
        assert!(bad(&[(4, 1), (3, 1)]));
        let too_many: Vec<(u32, u32)> =
            (0..MAX_CLASSES as u32 + 1).map(|i| ((i + 1) * 64, 1)).collect();
        assert!(bad(&too_many));
    }

    #[test]
    fn class_for_picks_the_smallest_fit_and_accessors_bound_their_input() {
        let f = fixture_with(&[(64, 4), (1024, 2)]);
        let pool = f.reader.pool();
        assert_eq!(pool.class_for(1), Some(0));
        assert_eq!(pool.class_for(64), Some(0));
        assert_eq!(pool.class_for(65), Some(1));
        assert_eq!(pool.class_for(1025), None);
        assert!(pool.meta(0, 4).is_none());
        assert!(pool.meta(9, 0).is_none());
        assert!(pool.data(1, 2).is_none());
        for class in 0..pool.class_count() {
            for index in 0..pool.slot_count(class) {
                let meta = pool.meta(class, index).unwrap();
                assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_FREE);
                assert_eq!(meta.refs.load(Ordering::Relaxed), 0);
            }
        }
    }

    #[test]
    fn attach_rejects_a_header_it_cannot_trust() {
        let l = PoolLayout::new(&[(64, 4), (1024, 2)]).unwrap();
        let buf = AlignedRegion::new(l.total_size() as usize);
        let err = unsafe { Pool::attach(buf.ptr(), l.total_size()) }.unwrap_err();
        assert_eq!(err, LayoutError::NotReady);

        unsafe { Pool::init(buf.ptr(), &l) };
        let err = unsafe { Pool::attach(buf.ptr(), 8) }.unwrap_err();
        assert_eq!(err, LayoutError::SizeMismatch);
        let err = unsafe { Pool::attach(buf.ptr(), 128) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset, "geometry past the region");

        // A published offset nudged off its cache line, still inside the region.
        unsafe {
            let field = std::ptr::addr_of_mut!((*(buf.ptr() as *mut PoolHeader)).classes[0])
                .cast::<ClassDesc>();
            let mut desc = field.read();
            desc.meta_offset += 1;
            field.write(desc);
        }
        let err = unsafe { Pool::attach(buf.ptr(), l.total_size()) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);
    }

    #[test]
    fn acquire_commit_claim_release_round_trips() {
        let mut f = fixture();
        assert!(f.owner.acquire(65).is_none(), "larger than any class");

        let mut lease = f.owner.acquire(10).unwrap();
        let meta = f.reader.pool().meta(lease.class, lease.index).unwrap();
        assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_WRITING);
        assert_eq!(meta.generation.load(Ordering::Relaxed), 1);
        assert_eq!(meta.refs.load(Ordering::Relaxed), 1 << OWNER_SLOT);

        lease.bytes_mut()[..3].copy_from_slice(b"abc");
        let r = f.owner.commit(lease, 3);
        assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_READY);
        assert_eq!((r.owner_slot, r.len, r.generation), (OWNER_SLOT as u16, 3, 1));

        let claimed = f.reader.claim(&r).unwrap();
        assert_eq!(f.reader.bytes(&claimed), b"abc");
        assert_ne!(meta.refs.load(Ordering::Acquire) & (1 << READER_SLOT), 0);

        // The owner drops its bit; the reader's claim alone keeps the slot.
        f.owner.release_own(r.class, r.index);
        let _other = f.owner.acquire(8).unwrap();
        assert!(f.owner.acquire(8).is_none(), "still referenced by the reader");
        f.reader.release(claimed);
        assert!(f.owner.acquire(8).is_some());
    }

    #[test]
    fn abort_returns_the_slot_for_reuse() {
        let mut f = fixture();
        let first = f.owner.acquire(10).unwrap();
        let (c, i) = (first.class, first.index);
        f.owner.abort(first);
        let second = f.owner.acquire(10).unwrap();
        let third = f.owner.acquire(10).unwrap();
        assert!([second.index, third.index].contains(&i));
        assert_eq!(second.class, c);
    }

    #[test]
    fn claim_rejects_a_descriptor_that_does_not_match_the_slot() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let writing = SlotRef {
            owner_slot: OWNER_SLOT as u16,
            owner_epoch_low: 1,
            class: lease.class,
            index: lease.index,
            generation: 1,
            len: 1,
        };
        assert!(f.reader.claim(&writing).is_none(), "still Writing");
        let r = f.owner.commit(lease, 1);
        assert!(f.reader.claim(&r).is_some());

        let mut bad = r;
        bad.generation = r.generation.wrapping_sub(1);
        assert!(f.reader.claim(&bad).is_none(), "stale generation");
        let mut bad = r;
        bad.owner_epoch_low = r.owner_epoch_low.wrapping_sub(1);
        assert!(f.reader.claim(&bad).is_none(), "another incarnation");
        let mut bad = r;
        bad.owner_slot += 1;
        assert!(f.reader.claim(&bad).is_none(), "another owner");
        let mut bad = r;
        bad.index = 99;
        assert!(f.reader.claim(&bad).is_none(), "out of range");
        let mut bad = r;
        bad.len = 65;
        assert!(f.reader.claim(&bad).is_none(), "longer than the slot");
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
        assert_ne!(meta.refs.load(Ordering::Acquire) & (1 << READER_SLOT), 0);
        f.reader.release(second);
        assert_eq!(meta.refs.load(Ordering::Acquire) & (1 << READER_SLOT), 0);
    }

    #[test]
    fn reclaim_participant_clears_only_that_bit() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let r = f.owner.commit(lease, 1);
        let _claimed = f.reader.claim(&r).unwrap();
        assert_eq!(f.owner.reclaim_participant(READER_SLOT), 1);
        let meta = f.reader.pool().meta(r.class, r.index).unwrap();
        assert_eq!(meta.refs.load(Ordering::Acquire), 1 << OWNER_SLOT);
    }

    #[test]
    fn a_dropped_lease_is_reclaimed_and_a_live_one_is_not() {
        let mut f = fixture();
        let mut live = f.owner.acquire(10).unwrap();
        live.bytes_mut()[0] = 1;
        let dropped = f.owner.acquire(10).unwrap();
        drop(dropped);
        assert!(f.owner.acquire(10).is_none(), "the class is exhausted");

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(f.owner.reclaim_stale_leases(Duration::from_millis(10)), 1);
        assert!(f.owner.acquire(10).is_some(), "the abandoned slot comes back");
        drop(live);
    }

    #[test]
    fn a_lease_keeps_the_mapping_alive() {
        let layout = PoolLayout::new(&[(64, 2)]).unwrap();
        let region = AlignedRegion::new(layout.total_size() as usize);
        let map: Arc<dyn Send + Sync> = Arc::new(());
        let mut o =
            PoolOwner::new(unsafe { Pool::init(region.ptr(), &layout) }, 3, 1, Arc::clone(&map));
        assert_eq!(Arc::strong_count(&map), 2);
        let lease = o.acquire(8).unwrap();
        assert_eq!(Arc::strong_count(&map), 3);
        drop(lease);
        assert_eq!(Arc::strong_count(&map), 2);
    }

    fn write_pattern(buf: &mut [u8], generation: u32) {
        for (i, b) in buf.iter_mut().enumerate() {
            *b = (generation as u8) ^ (i as u8);
        }
    }

    fn matches_pattern(buf: &[u8], generation: u32) -> bool {
        buf.iter().enumerate().all(|(i, b)| *b == (generation as u8) ^ (i as u8))
    }

    /// Regression for the blind `refs.store` the owner used to do: a reader
    /// could pass every check and then have its bit erased and the slot
    /// overwritten under it. Threads share one reader, as `PeerMap` hands
    /// every local reader the same `PeerSegment`.
    #[test]
    fn concurrent_readers_never_see_a_slot_recycled_under_them() {
        const SLOT_SIZE: usize = 1024;
        const THREADS: u32 = 3;
        let run_for = Duration::from_millis(1500);

        let f = fixture_with(&[(SLOT_SIZE as u32, 2)]);
        let Fixture { _region, mut owner, reader } = f;
        let my_bit = 1u64 << READER_SLOT;

        let latest: Mutex<Option<SlotRef>> = Mutex::new(None);
        let stop = AtomicBool::new(false);
        let claimed = AtomicU64::new(0);

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
                        let intact = matches_pattern(reader.bytes(&c), r.generation);
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
                        assert!(held, "claim returned with no bit standing for it");
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
                owner.release_own(r.class, r.index);
                published += 1;
            }
            stop.store(true, Ordering::Relaxed);
            assert!(published > 1000, "too few publications to be meaningful: {published}");
        });

        assert!(claimed.load(Ordering::Relaxed) > 0, "readers never claimed anything");
    }
}
