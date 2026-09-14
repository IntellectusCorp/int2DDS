//! Shared-memory transport and its zero-copy data plane.
//!
//! A participant owns one segment: a payload pool plus an MPSC ring that
//! carries slot descriptors. A writer serializes into a slot and sends the
//! descriptor; the reader claims the slot and reads it in place. Discovery
//! stays on UDP, and so does any sample no slot could take.

pub(crate) mod platform;
pub(crate) mod pool;
pub(crate) mod registry;
pub(crate) mod ring;
pub(crate) mod runtime;
pub(crate) mod segment;
pub(crate) mod shm_transport_plugin;
pub(crate) mod slot;

#[cfg(test)]
mod integration_test;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayoutError {
    NotReady,
    BadVersion,
    SizeMismatch,
    BadOffset,
    BadConfig,
}

/// Every shared struct is `#[repr(C, align(64))]`; an offset that is not a
/// multiple of this would build a misaligned reference, which is UB.
pub(crate) const SHARED_ALIGN: u64 = 64;

/// 64-byte aligned scratch region standing in for a mapping in unit tests.
#[cfg(test)]
pub(crate) mod test_region {
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
}
