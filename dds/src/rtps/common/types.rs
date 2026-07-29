//! Common RTPS type definitions and vendor identification.
//!
//! This module provides common RTPS types including vendor IDs, protocol versions,
//! and shared data structures. Vendor IDs identify DDS implementation vendors as
//! registered with the DDS Foundation.

use std::sync::Arc;

use bytes::Bytes;

use super::{
    guid::{GroupDigest, Guid},
    parameters::ParameterList,
    sequence::SequenceNumber,
};
use speedy::{Readable, Writable};

// https://www.dds-foundation.org/dds-rtps-vendor-and-product-ids/
pub const VENDORID_UNKNOWN: VendorId = [0x00, 0x00];
pub const VENDORID_RTI_CONNEXT: VendorId = [0x01, 0x01];
pub const VENDORID_OPENSLICE: VendorId = [0x01, 0x02];
pub const VENDORID_OPENDDS: VendorId = [0x01, 0x03];
pub const VENDORID_INTERCOM: VendorId = [0x01, 0x04];
pub const VENDORID_COREDX: VendorId = [0x01, 0x05];
pub const VENDORID_RTI_CONNEXT_MICRO: VendorId = [0x01, 0x06];
pub const VENDORID_VORTEX_CAFE: VendorId = [0x01, 0x0A];
pub const VENDORID_VORTEX_LITE: VendorId = [0x01, 0x0D];
pub const VENDORID_EPROSIMA: VendorId = [0x01, 0x0F];
pub const VENDORID_CYCLONE: VendorId = [0x01, 0x10];
pub const VENDORID_GURUM: VendorId = [0x01, 0x11];
pub const VENDORID_RUST: VendorId = [0x01, 0x12];
pub const VENDORID_ZRDDS: VendorId = [0x01, 0x13];
pub const VENDORID_DUST: VendorId = [0x01, 0x14];
pub const VENDORID_SAFE: VendorId = [0x01, 0x15];
pub const VENDORID_FEDERATED_DESIGNS: VendorId = [0x01, 0x16];
pub const VENDORID_ROCKET: VendorId = [0x01, 0x17];
pub const VENDORID_BELL: VendorId = [0x01, 0x18];
pub const VENDORID_INT2: VendorId = [0x01, 0x19];
pub type VendorId = [u8; 2];

pub const PROTOCOL_RTPS: ProtocolId = *b"RTPS";
pub type ProtocolId = [u8; 4];
pub type DomainId = u32;
pub type ParticipantId = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Readable, Writable)]
pub struct ProtocolVersion {
    pub major: u8,
    pub minor: u8,
}

impl ProtocolVersion {
    pub const PROTOCOLVERSION: Self = Self::PROTOCOLVERSION_2_5;

    pub fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    pub const PROTOCOLVERSION_2_5: Self = Self { major: 2, minor: 5 };
}
impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

pub type MessageLength = u32;
pub const MESSAGE_LENGTH_INVALID: MessageLength = 0;
pub const RTPS_HEADER_LENGTH: MessageLength = 20;

pub type StatusInfo = [u8; 4];

pub type SubmessageFlag = u8;

#[derive(Debug, Clone)]
pub struct OriginalWriterInfo {
    pub original_writer_guid: Guid,
    pub original_writer_sn: SequenceNumber,
    pub original_writer_qos: ParameterList,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct WriterGroupInfo {
    writer_set: GroupDigest,
}

pub type Count = u32;

pub type UExtension4 = [u8; 4];
pub type WExtension8 = [u8; 8];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicKind {
    NoKey,
    WithKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Alive,
    AliveFiltered,
    NotAliveDisposed,
    NotAliveUnregistered,
    NotAliveDisposedUnregistered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeCount {
    high: i32,
    low: u32,
}
impl ChangeCount {
    // The 64-bit count is obtained using the formula:
    // change_count = low + high * 2^(32)
    pub fn to_i64(&self) -> i64 {
        // high.wrapping_shl(32) == high << 32
        (self.high as i64).wrapping_shl(32) | (self.low as i64)
    }
}
impl std::fmt::Display for ChangeCount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_i64())
    }
}

pub type SerializedData = Arc<[u8]>;
pub type SerializedDataFragment = Arc<[u8]>;

/// Payload carried by DATA / DATA_FRAG submessages.
///
/// On the send path, large payloads are already stored in a `CacheChange`
/// and only need to be written into the wire buffer; we borrow the slice
/// for the duration of serialization.
///
/// On the receive path, payloads are parsed from the socket buffer as
/// `Owned(Bytes)` so they share the original allocation via refcount
/// without copying. `Bytes::slice(range)` returns a zero-copy sub-slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubmessagePayload<'a> {
    Owned(Bytes),
    Borrowed(&'a [u8]),
}

impl<'a> SubmessagePayload<'a> {
    pub(crate) fn as_slice(&self) -> &[u8] {
        match self {
            SubmessagePayload::Owned(data) => data,
            SubmessagePayload::Borrowed(data) => data,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.as_slice().len()
    }
}

impl Default for SubmessagePayload<'_> {
    fn default() -> Self {
        SubmessagePayload::Owned(Bytes::new())
    }
}

impl From<Bytes> for SubmessagePayload<'_> {
    fn from(data: Bytes) -> Self {
        SubmessagePayload::Owned(data)
    }
}

impl<'a> From<&'a [u8]> for SubmessagePayload<'a> {
    fn from(data: &'a [u8]) -> Self {
        SubmessagePayload::Borrowed(data)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct GroupInfo<'a> {
    pub group_entity_id: u32,
    pub group_data: &'a [u8],
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wrapping count comparison: `(current.wrapping_sub(previous) as i32) <= 0`
    #[test]
    fn count_wrapping_comparison_boundary_values() {
        // Basic: newer count should pass
        let is_old = |current: Count, previous: Count| -> bool {
            (current.wrapping_sub(previous) as i32) <= 0
        };

        // Same count: old (duplicate)
        assert!(is_old(5, 5));

        // Simple increment: not old
        assert!(!is_old(6, 5));

        // Simple decrement: old
        assert!(is_old(4, 5));

        // Wrap around u32::MAX: not old (MAX -> 0 is forward by 1)
        assert!(!is_old(0, u32::MAX));

        // Wrap around u32::MAX: not old (MAX -> 1 is forward by 2)
        assert!(!is_old(1, u32::MAX));

        // Reverse wrap: old (0 -> MAX is backward by 1)
        assert!(is_old(u32::MAX, 0));

        // Half-range boundary: exactly i32::MAX apart: not old
        assert!(!is_old(i32::MAX as u32, 0));

        // Half-range boundary: i32::MAX + 1 apart: old (ambiguous, treated as backward).
        // Safe because consecutive comparisons never differ by more than 2^31 (RFC 1982).
        assert!(is_old(i32::MAX as u32 + 1, 0));

        // Large gap forward near wrap
        assert!(!is_old(u32::MAX - 1, u32::MAX - 5));

        // Both near MAX, current behind
        assert!(is_old(u32::MAX - 5, u32::MAX - 1));

        // Wrap: previous near MAX, current small
        assert!(!is_old(3, u32::MAX - 2));

        // Wrap: previous small, current near MAX: old (backward)
        assert!(is_old(u32::MAX - 2, 3));
    }
}
