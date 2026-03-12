#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("../../README.md")]

#[cfg(test)]
extern crate self as int2dds;

pub mod dcps;
#[doc(hidden)]
pub use crate::publication::data_writer::DataWriterBase;
#[doc(hidden)]
pub use crate::subscription::data_reader::DataReaderBase;
#[doc(hidden)]
pub use dcps::*;
pub mod common;
pub mod config;
pub mod rtps;
#[doc(hidden)]
pub mod serialize;
pub mod xtypes;
#[doc(hidden)]
pub use dcps::topic::{DdsType, FieldAccessor};
#[doc(hidden)]
pub use int2dds_derive::DdsType as DeriveDdsType;
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
