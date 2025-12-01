//! Common RTPS type definitions and vendor identification.
//!
//! This module provides common RTPS types including vendor IDs, protocol versions,
//! and shared data structures. Vendor IDs identify DDS implementation vendors as
//! registered with the DDS Foundation.

use std::sync::Arc;

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

pub type Count = i32;

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

pub type SerializedData = Arc<[u8]>;
pub type SerializedDataFragment = Arc<[u8]>;

#[derive(Debug, Clone, PartialEq, Eq, Readable, Writable)]
pub struct GroupInfo<'a> {
    pub group_entity_id: u32,
    pub group_data: &'a [u8],
}
