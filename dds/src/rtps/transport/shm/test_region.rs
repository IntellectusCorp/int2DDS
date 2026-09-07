//! 64-byte aligned scratch region for unit tests.
//!
//! Real segments come from mmap / MapViewOfFile and are page aligned, so the
//! `align(64)` shared structs are always well aligned there. `Vec<u8>` gives no
//! such guarantee, and building a reference to an under-aligned struct is UB.

use std::alloc::{alloc_zeroed, dealloc, Layout};

pub(crate) struct AlignedRegion {
    ptr: *mut u8,
    layout: Layout,
}

impl AlignedRegion {
    pub(crate) fn new(size: usize) -> AlignedRegion {
        let layout = Layout::from_size_align(size, 64).expect("valid layout");
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "allocation failed");
        AlignedRegion { ptr, layout }
    }

    pub(crate) fn ptr(&self) -> *mut u8 {
        self.ptr
    }
}

impl Drop for AlignedRegion {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}
