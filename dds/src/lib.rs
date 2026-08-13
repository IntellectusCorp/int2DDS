#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("../../README.md")]

#[cfg(test)]
extern crate self as int2dds;

pub mod dcps;
#[doc(hidden)]
pub use crate::publication::data_writer::DataWriterBase;
#[doc(hidden)]
pub use crate::subscription::data_reader::DataReaderBase;
// Re-exported so derive-macro output can name these via the crate_path prefix
// instead of requiring every downstream crate to depend on them directly.
#[doc(hidden)]
pub use bytes;
#[doc(hidden)]
pub use dcps::*;
#[doc(hidden)]
pub use int2dds_cdr::impl_primitive_serialization;
#[doc(hidden)]
pub use log;
#[doc(hidden)]
pub use speedy;
pub mod common;
pub mod config;
pub mod route_gateway;
pub mod rtps;
#[doc(hidden)]
pub mod serialize;
pub mod xtypes;
#[doc(hidden)]
pub use dcps::topic::{DdsType, FieldAccessor};
#[doc(hidden)]
pub mod utils;

extern crate md5;

/// Test utilities for generating unique domain IDs to prevent test interference
#[cfg(test)]
pub mod test_utils {
    use std::sync::atomic::{AtomicI32, Ordering};

    // Start from 100 to avoid conflicts with hardcoded domain IDs (0-99)
    // Domain ID valid range: 0-232
    static TEST_DOMAIN_ID_COUNTER: AtomicI32 = AtomicI32::new(100);

    /// Returns a unique domain ID for test isolation.
    /// Each call returns a different value, ensuring tests don't interfere with each other.
    pub fn unique_domain_id() -> i32 {
        TEST_DOMAIN_ID_COUNTER.fetch_add(1, Ordering::SeqCst)
    }
}
