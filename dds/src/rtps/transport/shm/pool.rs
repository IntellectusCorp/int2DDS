//! Payload pool laid out inside a shared segment, allocated buddy-style.
//!
//! The pool is one power-of-two region split into blocks of `min_block << order`.
//! Only the owning participant allocates; its free lists live in process heap.
//! Every other process touches nothing but its own bit in `SlotMeta::refs`,
//! indexed by registry slot. A block goes Free -> Writing (owner bit set) ->
//! Ready, and back to Free once every bit is clear.
//!
//! Shared memory holds one `SlotMeta` per node of the buddy tree. Handing out
//! a block bumps the generation of its subtree and its ancestors, so a stale
//! descriptor for any overlapping block is rejected at `claim`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::rtps::transport::shm::slot::SlotRef;
use crate::rtps::transport::shm::{LayoutError, SHARED_ALIGN};

const POOL_MAGIC: u64 = 0x494E_5432_5A43_504C; // "INT2ZCPL"

pub(crate) const SLOT_FREE: u32 = 0;
pub(crate) const SLOT_WRITING: u32 = 1;
pub(crate) const SLOT_READY: u32 = 2;

const PAGE: u64 = 4096;
/// Leaves are capped so the per-node metadata stays within 1 MiB.
const MAX_ORDER: u32 = 13;
/// One minimum block. The floor is technical, not a recommendation.
pub(crate) const MIN_POOL_SIZE: u64 = PAGE;
pub(crate) const MAX_POOL_SIZE: u64 = 16 << 30;
/// The smallest pool the tests exercise: 16 leaves, enough to split and merge.
#[cfg(test)]
pub(crate) const TEST_POOL_SIZE: u64 = 64 * 1024;

/// How long a Writing block must have sat with its lease dropped before
/// `acquire` takes it back.
pub(crate) const STALE_LEASE_AFTER: Duration = Duration::from_secs(1);

pub(crate) fn valid_pool_size(size: u64) -> bool {
    size.is_power_of_two() && (MIN_POOL_SIZE..=MAX_POOL_SIZE).contains(&size)
}

#[repr(C, align(64))]
pub(crate) struct PoolHeader {
    pub magic: AtomicU64,
    pub pool_size: u64,
    pub meta_offset: u64,
    pub data_offset: u64,
    pub min_block: u32,
    pub max_order: u32,
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

#[derive(Clone, Copy)]
struct Geometry {
    pool_size: u64,
    min_block: u64,
    max_order: u32,
    meta_offset: u64,
    data_offset: u64,
}

impl Geometry {
    fn new(pool_size: u64) -> Result<Geometry, LayoutError> {
        if !valid_pool_size(pool_size) {
            return Err(LayoutError::BadConfig);
        }
        let min_block = (pool_size >> MAX_ORDER).max(PAGE);
        let max_order = (pool_size / min_block).trailing_zeros();
        let nodes = (2u64 << max_order) - 1;
        let meta_offset = align_up(std::mem::size_of::<PoolHeader>() as u64, SHARED_ALIGN);
        let data_offset =
            align_up(meta_offset + nodes * std::mem::size_of::<SlotMeta>() as u64, PAGE);
        Ok(Geometry { pool_size, min_block, max_order, meta_offset, data_offset })
    }

    fn nodes(&self) -> usize {
        (2usize << self.max_order) - 1
    }

    fn block_count(&self, order: u16) -> u32 {
        1u32 << (self.max_order - order as u32)
    }

    fn block_size(&self, order: u16) -> u64 {
        self.min_block << order
    }

    /// Nodes are numbered top-down: the whole pool is node 0, each order below
    /// continues where the previous one ended.
    fn node_id(&self, order: u16, index: u32) -> Option<usize> {
        if order as u32 > self.max_order || index >= self.block_count(order) {
            return None;
        }
        Some((self.block_count(order) - 1 + index) as usize)
    }

    fn total_size(&self) -> u64 {
        self.data_offset + self.pool_size
    }
}

pub(crate) struct PoolLayout {
    geometry: Geometry,
}

const fn align_up(v: u64, a: u64) -> u64 {
    (v + a - 1) & !(a - 1)
}

impl PoolLayout {
    /// `pool_size` is configuration: a power of two between `MIN_POOL_SIZE`
    /// and `MAX_POOL_SIZE`, or an error the caller falls back on.
    pub(crate) fn new(pool_size: u64) -> Result<PoolLayout, LayoutError> {
        Ok(PoolLayout { geometry: Geometry::new(pool_size)? })
    }

    pub(crate) fn total_size(&self) -> u64 {
        self.geometry.total_size()
    }
}

#[derive(Debug)]
pub(crate) struct Pool {
    base: *mut u8,
    pool_size: u64,
    min_block: u64,
    max_order: u32,
    meta_offset: u64,
    data_offset: u64,
}

// Safety: geometry is written once before publication. The only non-atomic
// access to block data is `SlotLease::bytes_mut`, gated by the Writing state
// and the owner-only bit.
unsafe impl Send for Pool {}
unsafe impl Sync for Pool {}

impl Pool {
    fn from_geometry(base: *mut u8, g: Geometry) -> Pool {
        Pool {
            base,
            pool_size: g.pool_size,
            min_block: g.min_block,
            max_order: g.max_order,
            meta_offset: g.meta_offset,
            data_offset: g.data_offset,
        }
    }

    fn geometry(&self) -> Geometry {
        Geometry {
            pool_size: self.pool_size,
            min_block: self.min_block,
            max_order: self.max_order,
            meta_offset: self.meta_offset,
            data_offset: self.data_offset,
        }
    }

    /// # Safety
    /// `base` must point to a zeroed writable region of `layout.total_size()` bytes.
    pub(crate) unsafe fn init(base: *mut u8, layout: &PoolLayout) -> Pool {
        let g = layout.geometry;
        let header = base as *mut PoolHeader;
        std::ptr::addr_of_mut!((*header).pool_size).write(g.pool_size);
        std::ptr::addr_of_mut!((*header).meta_offset).write(g.meta_offset);
        std::ptr::addr_of_mut!((*header).data_offset).write(g.data_offset);
        std::ptr::addr_of_mut!((*header).min_block).write(g.min_block as u32);
        std::ptr::addr_of_mut!((*header).max_order).write(g.max_order);
        let metas = base.add(g.meta_offset as usize) as *mut SlotMeta;
        for node in 0..g.nodes() {
            let meta = &*metas.add(node);
            meta.state.store(SLOT_FREE, Ordering::Relaxed);
            meta.generation.store(0, Ordering::Relaxed);
            meta.len.store(0, Ordering::Relaxed);
            meta.refs.store(0, Ordering::Relaxed);
        }
        (*header).magic.store(POOL_MAGIC, Ordering::Release);
        Pool::from_geometry(base, g)
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
        // Geometry comes from shared memory: recompute it from `pool_size`
        // and accept only what matches, then bound it against the region.
        let g = Geometry::new(header.pool_size).map_err(|_| LayoutError::BadVersion)?;
        if header.min_block as u64 != g.min_block
            || header.max_order != g.max_order
            || header.meta_offset != g.meta_offset
            || header.data_offset != g.data_offset
            || g.total_size() > region_size
        {
            return Err(LayoutError::BadOffset);
        }
        Ok(Pool::from_geometry(base, g))
    }

    pub(crate) fn max_order(&self) -> u16 {
        self.max_order as u16
    }

    pub(crate) fn block_count(&self, order: u16) -> u32 {
        self.geometry().block_count(order)
    }

    pub(crate) fn block_size(&self, order: u16) -> u64 {
        self.geometry().block_size(order)
    }

    /// The smallest order whose block holds `len` bytes; `None` when even the
    /// whole pool does not.
    pub(crate) fn order_for(&self, len: usize) -> Option<u16> {
        let len = len as u64;
        if len > self.pool_size {
            return None;
        }
        let blocks = len.div_ceil(self.min_block).max(1).next_power_of_two();
        Some(blocks.trailing_zeros() as u16)
    }

    fn node_meta(&self, node: usize) -> &SlotMeta {
        unsafe {
            let metas = self.base.add(self.meta_offset as usize) as *const SlotMeta;
            &*metas.add(node)
        }
    }

    pub(crate) fn meta(&self, order: u16, index: u32) -> Option<&SlotMeta> {
        Some(self.node_meta(self.geometry().node_id(order, index)?))
    }

    pub(crate) fn data(&self, order: u16, index: u32) -> Option<*mut u8> {
        let g = self.geometry();
        g.node_id(order, index)?;
        let offset = g.data_offset + g.block_size(order) * index as u64;
        unsafe { Some(self.base.add(offset as usize)) }
    }

    /// Every node overlapping block `(order, index)`: the block itself, its
    /// descendants, and its ancestors.
    fn overlapping(&self, order: u16, index: u32) -> impl Iterator<Item = &SlotMeta> {
        let g = self.geometry();
        let below = (0..=order).flat_map(move |o| {
            let span = 1u32 << (order - o);
            (index * span..(index + 1) * span).map(move |i| (o, i))
        });
        let above =
            (order as u32 + 1..=g.max_order).map(move |o| (o as u16, index >> (o - order as u32)));
        below.chain(above).filter_map(move |(o, i)| g.node_id(o, i)).map(move |n| self.node_meta(n))
    }
}

pub(crate) struct SlotLease {
    pub(crate) order: u16,
    pub(crate) index: u32,
    ptr: *mut u8,
    size: usize,
    /// Alive while the caller holds the lease; `reclaim_stale_leases` only
    /// recovers entries whose lease was dropped without `commit` or `abort`.
    _live: Arc<()>,
    /// Keeps the mapping alive: a lease outlives its guard and may cross threads.
    _map: Arc<dyn Send + Sync>,
}

// Safety: `acquire` put the block in Writing with only the owner bit set, so
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
    /// Free block indices per order.
    free: Vec<VecDeque<u32>>,
    leased: Vec<(u16, u32, Instant, Weak<()>)>,
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
        let mut free: Vec<VecDeque<u32>> =
            (0..=pool.max_order()).map(|_| VecDeque::new()).collect();
        free[pool.max_order() as usize].push_back(0);
        PoolOwner {
            own_bit: 1u64 << own_slot,
            own_slot,
            own_epoch,
            free,
            leased: Vec::new(),
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
        let order = self.pool.order_for(len)?;
        // A refused block is held aside until the search ends, so the next
        // `take_block` splits a larger block instead of handing it back.
        let mut refused = Vec::new();
        let mut taken = None;
        for _ in 0..self.pool.block_count(order) {
            let Some(index) = self.take_block(order) else { break };
            if self.claim(order, index) {
                taken = Some(index);
                break;
            }
            refused.push(index);
        }
        self.free[order as usize].extend(refused);
        let index = taken?;
        let ptr = self.pool.data(order, index).expect("taken from the free lists");
        let size = self.pool.block_size(order) as usize;
        let live = Arc::new(());
        self.leased.push((order, index, Instant::now(), Arc::downgrade(&live)));
        Some(SlotLease { order, index, ptr, size, _live: live, _map: Arc::clone(&self.map) })
    }

    /// A free block of `order`, splitting a larger one when none is free.
    fn take_block(&mut self, order: u16) -> Option<u32> {
        if let Some(index) = self.free[order as usize].pop_front() {
            return Some(index);
        }
        if order >= self.pool.max_order() {
            return None;
        }
        let parent = self.take_block(order + 1)?;
        self.free[order as usize].push_back(parent * 2 + 1);
        Some(parent * 2)
    }

    /// Bump the generation of every overlapping node, then require every one
    /// of them unreferenced, then take the block. A reader mid-claim on any
    /// overlapping block re-checks its generation after setting its bit and
    /// backs out; a bit that stays (a dead reader) refuses the block.
    fn claim(&mut self, order: u16, index: u32) -> bool {
        for meta in self.pool.overlapping(order, index) {
            meta.generation.fetch_add(1, Ordering::AcqRel);
        }
        if self.pool.overlapping(order, index).any(|m| m.refs.load(Ordering::Acquire) != 0) {
            return false;
        }
        let meta = self.pool.meta(order, index).expect("in range");
        if meta.refs.compare_exchange(0, self.own_bit, Ordering::AcqRel, Ordering::Acquire).is_err()
        {
            return false;
        }
        meta.len.store(0, Ordering::Relaxed);
        meta.state.store(SLOT_WRITING, Ordering::Release);
        true
    }

    pub(crate) fn commit(&mut self, lease: SlotLease, len: u32) -> SlotRef {
        let generation = {
            let meta = self.pool.meta(lease.order, lease.index).expect("lease is in range");
            meta.len.store(len, Ordering::Release);
            meta.state.store(SLOT_READY, Ordering::Release);
            meta.generation.load(Ordering::Acquire)
        };
        self.drop_lease(lease.order, lease.index);
        SlotRef {
            owner_slot: self.own_slot as u16,
            owner_epoch_low: self.own_epoch as u16,
            order: lease.order,
            index: lease.index,
            generation,
            len,
        }
    }

    pub(crate) fn abort(&mut self, lease: SlotLease) {
        self.release_own(lease.order, lease.index);
        self.drop_lease(lease.order, lease.index);
    }

    /// The block must not have a live lease. Merges with its buddy while that
    /// buddy is free too.
    pub(crate) fn release_own(&mut self, mut order: u16, mut index: u32) {
        if let Some(meta) = self.pool.meta(order, index) {
            meta.refs.fetch_and(!self.own_bit, Ordering::AcqRel);
        }
        while order < self.pool.max_order() {
            let buddy = index ^ 1;
            let Some(pos) = self.free[order as usize].iter().position(|&i| i == buddy) else {
                break;
            };
            self.free[order as usize].remove(pos);
            order += 1;
            index /= 2;
        }
        self.free[order as usize].push_back(index);
    }

    fn drop_lease(&mut self, order: u16, index: u32) {
        self.leased.retain(|(o, i, _, _)| (*o, *i) != (order, index));
    }

    /// Clear a dead participant's bit across every node.
    pub(crate) fn reclaim_participant(&mut self, dead_slot: u32) -> usize {
        let dead_bit = 1u64 << dead_slot;
        let mut cleared = 0;
        for node in 0..self.pool.geometry().nodes() {
            let prev = self.pool.node_meta(node).refs.fetch_and(!dead_bit, Ordering::AcqRel);
            if prev & dead_bit != 0 {
                cleared += 1;
            }
        }
        cleared
    }

    /// Recover blocks left in Writing by a lease dropped without `commit` or
    /// `abort`. A lease still in the caller's hands is never touched.
    pub(crate) fn reclaim_stale_leases(&mut self, older_than: Duration) -> usize {
        let now = Instant::now();
        let stale: Vec<(u16, u32)> = self
            .leased
            .iter()
            .filter(|(_, _, at, live)| {
                live.strong_count() == 0 && now.duration_since(*at) >= older_than
            })
            .map(|(o, i, _, _)| (*o, *i))
            .collect();
        // `strong_count` is relaxed; order the abandoned writer's stores
        // before the next owner's.
        if !stale.is_empty() {
            std::sync::atomic::fence(Ordering::Acquire);
        }
        for (order, index) in &stale {
            self.release_own(*order, *index);
            self.drop_lease(*order, *index);
        }
        stale.len()
    }
}

/// Must reach `PoolReader::release`, or this reader's bit stays set on the
/// block for the rest of the process's life.
pub(crate) struct ClaimedSlot {
    order: u16,
    index: u32,
    len: u32,
}

pub(crate) struct PoolReader {
    pool: Pool,
    my_bit: u64,
    owner_slot: u16,
    owner_epoch_low: u16,
    /// Per-node claim count. The shared bit moves only on 0 -> 1 and 1 -> 0.
    local: Mutex<Vec<u32>>,
}

impl PoolReader {
    /// `claim` accepts only descriptors minted by `owner_slot` at `owner_epoch_low`.
    pub(crate) fn new(
        pool: Pool,
        my_slot: u32,
        owner_slot: u16,
        owner_epoch_low: u16,
    ) -> PoolReader {
        let local = vec![0u32; pool.geometry().nodes()];
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
        let node = self.pool.geometry().node_id(r.order, r.index)?;
        let meta = self.pool.node_meta(node);
        if r.len as u64 > self.pool.block_size(r.order) {
            return None;
        }
        if meta.state.load(Ordering::Acquire) != SLOT_READY
            || meta.generation.load(Ordering::Acquire) != r.generation
        {
            return None;
        }
        // Count and bit move under one lock, so a second claimer never reads
        // the block before the first has set the bit.
        let mut local = self.local.lock().unwrap_or_else(|e| e.into_inner());
        let n = &mut local[node];
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
        Some(ClaimedSlot { order: r.order, index: r.index, len: r.len })
    }

    /// Both parameters share `'a` so the slice cannot outlive `release(c)`.
    pub(crate) fn bytes<'a>(&'a self, c: &'a ClaimedSlot) -> &'a [u8] {
        let ptr = self.pool.data(c.order, c.index).expect("claim validated the range");
        unsafe { std::slice::from_raw_parts(ptr, c.len as usize) }
    }

    pub(crate) fn release(&self, c: ClaimedSlot) {
        let Some(node) = self.pool.geometry().node_id(c.order, c.index) else { return };
        let mut local = self.local.lock().unwrap_or_else(|e| e.into_inner());
        let n = &mut local[node];
        debug_assert!(*n > 0, "release without a matching claim");
        *n = n.saturating_sub(1);
        if *n == 0 {
            self.pool.node_meta(node).refs.fetch_and(!self.my_bit, Ordering::AcqRel);
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
    /// 16 leaves of 4 KiB.
    const SMALL_POOL: u64 = TEST_POOL_SIZE;
    const WHOLE: usize = SMALL_POOL as usize;

    struct Fixture {
        _region: AlignedRegion,
        owner: PoolOwner,
        reader: PoolReader,
    }

    fn fixture() -> Fixture {
        let layout = PoolLayout::new(SMALL_POOL).unwrap();
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

    #[test]
    fn layout_rejects_a_bad_pool_size() {
        let bad = |s: u64| matches!(PoolLayout::new(s), Err(LayoutError::BadConfig));
        assert!(bad(0));
        assert!(bad(MIN_POOL_SIZE / 2));
        assert!(bad(TEST_POOL_SIZE + 1), "not a power of two");
        assert!(bad(MAX_POOL_SIZE * 2));
        assert!(PoolLayout::new(MIN_POOL_SIZE).is_ok(), "a single block is a pool");
        assert!(PoolLayout::new(32 << 20).is_ok());
    }

    #[test]
    fn geometry_caps_the_leaves_and_orders_pick_the_smallest_fit() {
        let pool = fixture().reader;
        let pool = pool.pool();
        assert_eq!(pool.max_order(), 4, "64 KiB / 4 KiB = 16 leaves");
        assert_eq!(pool.block_size(0), 4096);
        assert_eq!(pool.order_for(1), Some(0));
        assert_eq!(pool.order_for(4096), Some(0));
        assert_eq!(pool.order_for(4097), Some(1));
        assert_eq!(pool.order_for(WHOLE), Some(4));
        assert_eq!(pool.order_for(WHOLE + 1), None);
        assert!(pool.meta(0, 16).is_none());
        assert!(pool.meta(5, 0).is_none());
        assert!(pool.data(4, 1).is_none());

        let big = Geometry::new(1 << 30).unwrap();
        assert_eq!(big.max_order, MAX_ORDER, "a large pool grows its blocks, not its tree");
        assert_eq!(big.min_block, (1 << 30) >> MAX_ORDER);
    }

    #[test]
    fn attach_rejects_a_header_it_cannot_trust() {
        let l = PoolLayout::new(SMALL_POOL).unwrap();
        let buf = AlignedRegion::new(l.total_size() as usize);
        let err = unsafe { Pool::attach(buf.ptr(), l.total_size()) }.unwrap_err();
        assert_eq!(err, LayoutError::NotReady);

        unsafe { Pool::init(buf.ptr(), &l) };
        let err = unsafe { Pool::attach(buf.ptr(), 8) }.unwrap_err();
        assert_eq!(err, LayoutError::SizeMismatch);
        let err = unsafe { Pool::attach(buf.ptr(), l.total_size() - 1) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset, "geometry past the region");

        // A published offset that does not match the recomputed geometry.
        unsafe {
            let field = std::ptr::addr_of_mut!((*(buf.ptr() as *mut PoolHeader)).data_offset);
            field.write(field.read() + PAGE);
        }
        let err = unsafe { Pool::attach(buf.ptr(), l.total_size()) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);
    }

    #[test]
    fn acquire_commit_claim_release_round_trips() {
        let mut f = fixture();
        assert!(f.owner.acquire(WHOLE + 1).is_none(), "larger than the pool");

        let mut lease = f.owner.acquire(10).unwrap();
        assert_eq!(lease.order, 0);
        let meta = f.reader.pool().meta(lease.order, lease.index).unwrap();
        assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_WRITING);
        assert_eq!(meta.refs.load(Ordering::Relaxed), 1 << OWNER_SLOT);

        lease.bytes_mut()[..3].copy_from_slice(b"abc");
        let r = f.owner.commit(lease, 3);
        assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_READY);
        assert_eq!((r.owner_slot, r.len), (OWNER_SLOT as u16, 3));
        assert_eq!(r.generation, meta.generation.load(Ordering::Relaxed));

        let claimed = f.reader.claim(&r).unwrap();
        assert_eq!(f.reader.bytes(&claimed), b"abc");
        assert_ne!(meta.refs.load(Ordering::Acquire) & (1 << READER_SLOT), 0);

        // The owner drops its bit; the reader's claim alone keeps the block
        // and everything overlapping it out of the pool.
        f.owner.release_own(r.order, r.index);
        assert!(f.owner.acquire(WHOLE).is_none(), "the whole pool overlaps it");
        f.reader.release(claimed);
        assert!(f.owner.acquire(WHOLE).is_some());
    }

    #[test]
    fn blocks_split_on_demand_and_merge_on_release() {
        let mut f = fixture();
        let a = f.owner.acquire(1).unwrap();
        let b = f.owner.acquire(1).unwrap();
        assert_eq!((a.order, b.order), (0, 0));
        assert_eq!(a.index ^ b.index, 1, "buddies come out of one split");
        let big = f.owner.acquire(WHOLE / 2).unwrap();
        assert_eq!(big.order, 3);
        assert!(f.owner.acquire(WHOLE / 2).is_none(), "the other half is split up");

        f.owner.abort(a);
        f.owner.abort(b);
        f.owner.abort(big);
        assert!(f.owner.acquire(WHOLE).is_some(), "everything merged back");
    }

    #[test]
    fn a_reader_holding_a_leaf_keeps_every_block_over_it_out_of_use() {
        let mut f = fixture();
        let lease = f.owner.acquire(1).unwrap();
        let r = f.owner.commit(lease, 1);
        let claimed = f.reader.claim(&r).unwrap();
        f.owner.release_own(r.order, r.index);

        // The other 15 leaves are usable; nothing spanning the claimed one is.
        let taken: Vec<SlotLease> = (0..15).map(|_| f.owner.acquire(1).unwrap()).collect();
        assert!(f.owner.acquire(1).is_none());
        for t in taken {
            f.owner.abort(t);
        }
        let away = f.owner.acquire(4097).expect("an order-1 block away from the leaf");
        f.owner.abort(away);
        assert!(f.owner.acquire(WHOLE).is_none());

        f.reader.release(claimed);
        assert!(f.owner.acquire(WHOLE).is_some());
    }

    #[test]
    fn claim_rejects_a_descriptor_that_does_not_match_the_block() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let generation = f
            .reader
            .pool()
            .meta(lease.order, lease.index)
            .unwrap()
            .generation
            .load(Ordering::Acquire);
        let writing = SlotRef {
            owner_slot: OWNER_SLOT as u16,
            owner_epoch_low: 1,
            order: lease.order,
            index: lease.index,
            generation,
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
        bad.len = 4097;
        assert!(f.reader.claim(&bad).is_none(), "longer than the block");
    }

    #[test]
    fn a_stale_descriptor_for_a_block_that_was_split_or_merged_is_rejected() {
        let mut f = fixture();
        // Mint the whole pool, release it, then split it for a small sample.
        let lease = f.owner.acquire(WHOLE).unwrap();
        let whole = f.owner.commit(lease, 1);
        f.owner.release_own(whole.order, whole.index);
        let small = f.owner.acquire(1).unwrap();
        assert!(f.reader.claim(&whole).is_none(), "the parent's generation moved");

        // The other way round: a leaf minted, released, merged into a bigger block.
        let leaf = f.owner.commit(small, 1);
        f.owner.release_own(leaf.order, leaf.index);
        let _big = f.owner.acquire(WHOLE).unwrap();
        assert!(f.reader.claim(&leaf).is_none(), "the leaf's generation moved");
    }

    #[test]
    fn two_claims_of_one_block_hold_the_bit_until_both_release() {
        let mut f = fixture();
        let lease = f.owner.acquire(8).unwrap();
        let r = f.owner.commit(lease, 3);
        let first = f.reader.claim(&r).unwrap();
        let second = f.reader.claim(&r).unwrap();
        let meta = f.reader.pool().meta(r.order, r.index).unwrap();

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
        let meta = f.reader.pool().meta(r.order, r.index).unwrap();
        assert_eq!(meta.refs.load(Ordering::Acquire), 1 << OWNER_SLOT);
    }

    #[test]
    fn a_dropped_lease_is_reclaimed_and_a_live_one_is_not() {
        let mut f = fixture();
        let mut live = f.owner.acquire(WHOLE / 2).unwrap();
        live.bytes_mut()[0] = 1;
        let dropped = f.owner.acquire(WHOLE / 2).unwrap();
        drop(dropped);
        assert!(f.owner.acquire(1).is_none(), "the pool is exhausted");

        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(f.owner.reclaim_stale_leases(Duration::from_millis(10)), 1);
        assert!(f.owner.acquire(WHOLE / 2).is_some(), "the abandoned half comes back");
        drop(live);
    }

    #[test]
    fn a_lease_keeps_the_mapping_alive() {
        let layout = PoolLayout::new(SMALL_POOL).unwrap();
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

    /// Readers share one `PoolReader`, as `PeerMap` hands every local reader
    /// the same `PeerSegment`. A claimed block must never be rewritten under
    /// them, and its bit must stand whenever `claim` returns.
    #[test]
    fn concurrent_readers_never_see_a_block_recycled_under_them() {
        const SAMPLE: usize = 1024;
        const THREADS: u32 = 3;
        let run_for = Duration::from_millis(1500);

        let Fixture { _region, mut owner, reader } = fixture();
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
                            .meta(r.order, r.index)
                            .unwrap()
                            .refs
                            .load(Ordering::Acquire)
                            & my_bit
                            != 0;
                        reader.release(c);
                        assert!(
                            intact,
                            "block rewritten while claimed at generation {}",
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
                let Some(mut lease) = owner.acquire(SAMPLE) else {
                    std::hint::spin_loop();
                    continue;
                };
                let generation = owner
                    .pool()
                    .meta(lease.order, lease.index)
                    .unwrap()
                    .generation
                    .load(Ordering::Acquire);
                write_pattern(&mut lease.bytes_mut()[..SAMPLE], generation);
                let r = owner.commit(lease, SAMPLE as u32);
                *latest.lock().unwrap() = Some(r);
                owner.release_own(r.order, r.index);
                published += 1;
            }
            stop.store(true, Ordering::Relaxed);
            assert!(published > 1000, "too few publications to be meaningful: {published}");
        });

        assert!(claimed.load(Ordering::Relaxed) > 0, "readers never claimed anything");
    }
}
