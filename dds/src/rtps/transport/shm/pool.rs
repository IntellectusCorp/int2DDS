//! Slot pool laid out inside a shared segment.
//!
//! Only the owning participant allocates. Other processes touch nothing but
//! their own bit in `SlotMeta::refs`, so no free list lives in shared memory.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::rtps::transport::shm::layout::LayoutError;

pub(crate) const MAX_CLASSES: usize = 4;
pub(crate) const POOL_MAGIC: u64 = 0x494E_5432_5A43_504C; // "INT2ZCPL"

pub(crate) const SLOT_FREE: u32 = 0;
pub(crate) const SLOT_WRITING: u32 = 1;
pub(crate) const SLOT_READY: u32 = 2;

const CACHE_LINE: u64 = 64;
const PAGE: u64 = 4096;

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
    /// Class geometry comes from configuration, so a bad set is an error the
    /// caller falls back on, never a panic that takes the participant down.
    pub(crate) fn new(classes: &[(u32, u32)]) -> Result<PoolLayout, LayoutError> {
        if classes.is_empty() || classes.len() > MAX_CLASSES {
            return Err(LayoutError::BadConfig);
        }
        // `class_for` picks the first fit, so sizes must be strictly ascending.
        if !classes.windows(2).all(|w| w[0].0 < w[1].0) {
            return Err(LayoutError::BadConfig);
        }
        let mut cursor = align_up(std::mem::size_of::<PoolHeader>() as u64, CACHE_LINE);
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

// Safety: the pointer addresses a shared mapping whose geometry is written
// once before publication. `SlotLease::bytes_mut` does hand out a non-atomic
// `&mut [u8]` into slot data, but the `SLOT_WRITING` state plus the
// owner-only bit in `refs` (see the module doc) gate that: a slice is only
// live while the slot is in that state and only the leasing side touches it,
// so no other party may read or write the same bytes concurrently.
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
    /// `base` must point to a region previously initialized by `init` that is
    /// at least `region_size` bytes and stays mapped while the pool is used.
    ///
    /// The returned pool's `meta`/`data` are safe to call because this function
    /// rejects any geometry whose regions fall outside `region_size`.
    pub(crate) unsafe fn attach(base: *mut u8, region_size: u64) -> Result<Pool, LayoutError> {
        // The header itself must fit before anything reads through it.
        if region_size < std::mem::size_of::<PoolHeader>() as u64 {
            return Err(LayoutError::SizeMismatch);
        }
        // Gate first, through a raw projection: see `SegmentView::attach`.
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
        // Geometry comes from shared memory. Bound every region before the
        // accessors do pointer math with it.
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
            if !within {
                return Err(LayoutError::BadOffset);
            }
            // `meta` casts `meta_offset` to `&SlotMeta`, which is `align(64)`:
            // a merely-contained offset can still be misaligned, which is UB.
            if !c.meta_offset.is_multiple_of(CACHE_LINE)
                || !c.data_offset.is_multiple_of(CACHE_LINE)
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
        // Safety: `desc` proved the class exists and `index < slot_count`, and
        // `attach`/`init` proved the meta array fits inside the region.
        unsafe {
            let base = self.base.add(desc.meta_offset as usize) as *const SlotMeta;
            Some(&*base.add(index as usize))
        }
    }

    pub(crate) fn data(&self, class: u16, index: u32) -> Option<*mut u8> {
        let desc = self.desc(class, index)?;
        // Widen before multiplying: `slot_size * index` in u32 wraps silently in
        // release builds and would produce an in-bounds-looking wrong pointer.
        let offset = desc.data_offset as usize + desc.slot_size as usize * index as usize;
        // Safety: same bounds as `meta`, plus the data region's own size check.
        unsafe { Some(self.base.add(offset)) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    fn layout() -> PoolLayout {
        PoolLayout::new(&[(64, 4), (1024, 2)]).unwrap()
    }

    fn pool_region(layout: &PoolLayout) -> AlignedRegion {
        AlignedRegion::new(layout.total_size() as usize)
    }

    #[test]
    fn class_for_rounds_up_to_smallest_fit() {
        let l = layout();
        let buf = pool_region(&l);
        let pool = unsafe { Pool::init(buf.ptr(), &l) };
        assert_eq!(pool.class_for(1), Some(0));
        assert_eq!(pool.class_for(64), Some(0));
        assert_eq!(pool.class_for(65), Some(1));
        assert_eq!(pool.class_for(1024), Some(1));
        assert_eq!(pool.class_for(1025), None);
    }

    #[test]
    fn init_marks_every_slot_free() {
        let l = layout();
        let buf = pool_region(&l);
        let pool = unsafe { Pool::init(buf.ptr(), &l) };
        for class in 0..pool.class_count() {
            for index in 0..pool.slot_count(class) {
                let meta = pool.meta(class, index).unwrap();
                assert_eq!(meta.state.load(Ordering::Relaxed), SLOT_FREE);
                assert_eq!(meta.refs.load(Ordering::Relaxed), 0);
            }
        }
    }

    #[test]
    fn out_of_range_access_returns_none() {
        let l = layout();
        let buf = pool_region(&l);
        let pool = unsafe { Pool::init(buf.ptr(), &l) };
        assert!(pool.meta(0, 4).is_none());
        assert!(pool.meta(9, 0).is_none());
        assert!(pool.data(1, 2).is_none());
    }

    #[test]
    fn data_regions_do_not_overlap() {
        let l = layout();
        let buf = pool_region(&l);
        let pool = unsafe { Pool::init(buf.ptr(), &l) };
        let a = pool.data(0, 0).unwrap();
        let b = pool.data(0, 1).unwrap();
        assert_eq!(unsafe { b.offset_from(a) }, 64);
        let c = pool.data(1, 0).unwrap();
        assert!(c >= unsafe { a.add(64 * 4) });
    }

    #[test]
    fn attach_before_init_reports_not_ready() {
        let l = layout();
        let buf = pool_region(&l);
        let err = unsafe { Pool::attach(buf.ptr(), l.total_size()) }.unwrap_err();
        assert_eq!(err, LayoutError::NotReady);
    }

    #[test]
    fn attach_rejects_region_smaller_than_the_header() {
        let l = layout();
        let buf = pool_region(&l);
        unsafe { Pool::init(buf.ptr(), &l) };
        let err = unsafe { Pool::attach(buf.ptr(), 8) }.unwrap_err();
        assert_eq!(err, LayoutError::SizeMismatch);
    }

    #[test]
    fn attach_rejects_geometry_beyond_the_region() {
        let l = layout();
        let buf = pool_region(&l);
        unsafe { Pool::init(buf.ptr(), &l) };
        // PoolHeader is 128 bytes. Use 128 to pass header check but fail geometry check.
        let err = unsafe { Pool::attach(buf.ptr(), 128) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);
    }

    #[test]
    fn attach_rejects_misaligned_geometry() {
        let l = layout();
        let buf = pool_region(&l);
        unsafe { Pool::init(buf.ptr(), &l) };
        // Nudge a published offset off its cache line, as a stale or corrupt
        // writer could. It still lands inside the region.
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
    fn bad_class_configuration_is_an_error_not_a_panic() {
        // `PoolLayout` has no `Debug`, so match instead of `unwrap_err`.
        let bad = |c: &[(u32, u32)]| matches!(PoolLayout::new(c), Err(LayoutError::BadConfig));
        assert!(bad(&[]), "empty");
        assert!(bad(&[(64, 1), (64, 1)]), "equal sizes are not ascending");
        assert!(bad(&[(4, 1), (3, 1)]), "descending sizes");
        let too_many: Vec<(u32, u32)> =
            (0..MAX_CLASSES as u32 + 1).map(|i| ((i + 1) * 64, 1)).collect();
        assert!(bad(&too_many), "more classes than MAX_CLASSES");
    }

    #[test]
    fn attach_reads_back_the_same_geometry() {
        let l = layout();
        let buf = pool_region(&l);
        unsafe { Pool::init(buf.ptr(), &l) };
        let pool = unsafe { Pool::attach(buf.ptr(), l.total_size()) }.unwrap();
        assert_eq!(pool.class_count(), 2);
        assert_eq!(pool.slot_size(1), 1024);
        assert_eq!(pool.slot_count(0), 4);
    }
}
