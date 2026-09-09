#![allow(clippy::needless_doctest_main)]
#![doc = include_str!("../../README.md")]

#[cfg(test)]
extern crate self as int2dds;

pub mod dcps;
#[doc(hidden)]
pub use crate::publication::data_writer::DataWriterBase;
#[doc(hidden)]
pub use crate::subscription::data_reader::DataReaderBase;
// Re-exported so derive-macro output can name `bytes::Bytes` via the int2dds path.
#[doc(hidden)]
pub use bytes;
#[doc(hidden)]
pub use dcps::*;
pub mod common;
pub mod config;
pub mod route_gateway;
pub mod rtps;
// Re-export so the ffi crate can name the SEDP discovery event type
// (rtps::entities::participant are pub(crate)).
pub use crate::rtps::entities::participant::EndpointDiscoveryEvent;
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

    // 7400 + 250 * 232 + 11 = 65411, the last port mapping that fits a 16 bit UDP port.
    const LAST_DOMAIN_ID: i32 = 232;

    static TEST_DOMAIN_ID_COUNTER: AtomicI32 = AtomicI32::new(0);

    /// Returns a domain ID for test isolation, cycling through 0..=LAST_DOMAIN_ID.
    /// Consecutive calls differ, so tests that overlap in time never share a domain.
    pub fn unique_domain_id() -> i32 {
        TEST_DOMAIN_ID_COUNTER.fetch_add(1, Ordering::SeqCst) % (LAST_DOMAIN_ID + 1)
    }
}
