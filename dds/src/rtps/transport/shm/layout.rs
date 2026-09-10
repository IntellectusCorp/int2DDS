//! Shared segment header and offset layout.
//!
//! The magic word doubles as the initialization gate: it is written last with
//! Release ordering, so a process that attaches mid-initialization sees zero
//! and backs off instead of reading a half-written header.

use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const SEGMENT_MAGIC: u64 = 0x494E_5432_5A43_5347; // "INT2ZCSG"
pub(crate) const SEGMENT_VERSION: u32 = 1;

/// Every shared struct in this module is `#[repr(C, align(64))]`; an offset
/// that is not a multiple of this yields a misaligned reference, which is UB.
pub(crate) const SHARED_ALIGN: u64 = 64;

#[repr(C, align(64))]
pub(crate) struct SegmentHeader {
    pub magic: AtomicU64,
    pub segment_size: u64,
    pub owner_epoch: u64,
    pub ring_offset: u64,
    pub pool_offset: u64,
    pub version: u32,
    pub owner_slot: u32,
}

// The first thing a peer reads out of the mapping, so its size is wire format.
const _: () = assert!(std::mem::size_of::<SegmentHeader>() == 64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutError {
    NotReady,
    BadVersion,
    SizeMismatch,
    BadOffset,
    BadConfig,
}

#[derive(Debug)]
pub(crate) struct SegmentView {
    base: *mut u8,
}

// Safety: the pointer addresses a shared mapping whose header is written once
// before publication and read-only afterwards.
unsafe impl Send for SegmentView {}
unsafe impl Sync for SegmentView {}

impl SegmentView {
    /// # Safety
    /// `base` must point to a writable region of at least `size` bytes that no
    /// other process has initialized yet.
    pub(crate) unsafe fn init(
        base: *mut u8,
        size: u64,
        owner_slot: u32,
        owner_epoch: u64,
        ring_offset: u64,
        pool_offset: u64,
    ) -> SegmentView {
        let header = base as *mut SegmentHeader;
        std::ptr::addr_of_mut!((*header).segment_size).write(size);
        std::ptr::addr_of_mut!((*header).owner_epoch).write(owner_epoch);
        std::ptr::addr_of_mut!((*header).ring_offset).write(ring_offset);
        std::ptr::addr_of_mut!((*header).pool_offset).write(pool_offset);
        std::ptr::addr_of_mut!((*header).version).write(SEGMENT_VERSION);
        std::ptr::addr_of_mut!((*header).owner_slot).write(owner_slot);
        (*header).magic.store(SEGMENT_MAGIC, Ordering::Release);
        SegmentView { base }
    }

    /// # Safety
    /// `base` must point to a readable region of at least `mapped_size` bytes
    /// that stays mapped for as long as the returned view is used.
    ///
    /// The returned view's `ring_ptr`/`pool_ptr` are safe to call because this
    /// function rejects any header whose offsets fall outside `mapped_size`.
    pub(crate) unsafe fn attach(
        base: *mut u8,
        mapped_size: u64,
    ) -> Result<SegmentView, LayoutError> {
        // Read the gate through a raw projection. Until it reads as initialized
        // another process may still be writing the non-atomic fields, and a
        // struct-wide reference would assert that nobody is.
        let magic = &*std::ptr::addr_of!((*(base as *const SegmentHeader)).magic);
        if magic.load(Ordering::Acquire) != SEGMENT_MAGIC {
            return Err(LayoutError::NotReady);
        }
        let header = &*(base as *const SegmentHeader);
        if header.version != SEGMENT_VERSION {
            return Err(LayoutError::BadVersion);
        }
        if header.segment_size != mapped_size {
            return Err(LayoutError::SizeMismatch);
        }
        // Offsets come from shared memory: bound them before any pointer math,
        // or `ring_ptr`/`pool_ptr` would reach outside the mapping.
        let least = std::mem::size_of::<SegmentHeader>() as u64;
        if header.ring_offset < least
            || header.pool_offset < least
            || header.ring_offset >= mapped_size
            || header.pool_offset >= mapped_size
        {
            return Err(LayoutError::BadOffset);
        }
        // Containment is not enough: `ring_ptr`/`pool_ptr` are cast to
        // `align(64)` structs, so an offset like 65 would build a misaligned
        // reference rather than merely a wrong one.
        if !header.ring_offset.is_multiple_of(SHARED_ALIGN)
            || !header.pool_offset.is_multiple_of(SHARED_ALIGN)
        {
            return Err(LayoutError::BadOffset);
        }
        Ok(SegmentView { base })
    }

    pub(crate) fn header(&self) -> &SegmentHeader {
        unsafe { &*(self.base as *const SegmentHeader) }
    }

    pub(crate) fn ring_ptr(&self) -> *mut u8 {
        unsafe { self.base.add(self.header().ring_offset as usize) }
    }

    pub(crate) fn pool_ptr(&self) -> *mut u8 {
        unsafe { self.base.add(self.header().pool_offset as usize) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::transport::shm::test_region::AlignedRegion;

    #[test]
    fn attach_before_init_reports_not_ready() {
        let r = AlignedRegion::new(4096);
        let err = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap_err();
        assert_eq!(err, LayoutError::NotReady);
    }

    #[test]
    fn attach_after_init_exposes_offsets() {
        let r = AlignedRegion::new(4096);
        unsafe { SegmentView::init(r.ptr(), 4096, 7, 3, 64, 1024) };
        let view = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap();
        assert_eq!(view.header().owner_slot, 7);
        assert_eq!(view.header().owner_epoch, 3);
        assert_eq!(view.ring_ptr(), unsafe { r.ptr().add(64) });
        assert_eq!(view.pool_ptr(), unsafe { r.ptr().add(1024) });
    }

    #[test]
    fn attach_rejects_size_mismatch() {
        let r = AlignedRegion::new(4096);
        unsafe { SegmentView::init(r.ptr(), 4096, 0, 1, 64, 1024) };
        let err = unsafe { SegmentView::attach(r.ptr(), 2048) }.unwrap_err();
        assert_eq!(err, LayoutError::SizeMismatch);
    }

    #[test]
    fn attach_rejects_offsets_outside_the_mapping() {
        let r = AlignedRegion::new(4096);
        unsafe { SegmentView::init(r.ptr(), 4096, 0, 1, 64, 8192) };
        let err = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);
    }

    #[test]
    fn attach_rejects_misaligned_offsets() {
        let r = AlignedRegion::new(4096);
        unsafe { SegmentView::init(r.ptr(), 4096, 0, 1, 64, 1025) };
        let err = unsafe { SegmentView::attach(r.ptr(), 4096) }.unwrap_err();
        assert_eq!(err, LayoutError::BadOffset);
    }
}
