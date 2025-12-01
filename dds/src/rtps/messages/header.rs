//! RTPS message header structure.
//!
//! This module defines the RTPS message `Header` as specified in section 8.3.6
//! of the RTPS specification. The header identifies the protocol, version, vendor,
//! and source participant of each RTPS message.

// 8.3.6 The RTPS Header
use speedy::{Readable, Writable};

use crate::rtps::common::{
    guid::GuidPrefix,
    types::{ProtocolId, ProtocolVersion, VendorId, PROTOCOL_RTPS, VENDORID_INT2},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Readable, Writable)]
pub(crate) struct Header {
    protocol: ProtocolId,
    version: ProtocolVersion,
    vendor_id: VendorId,
    guid_prefix: GuidPrefix,
}

#[allow(dead_code)]
impl Header {
    pub(crate) fn new(guid: GuidPrefix) -> Self {
        Self {
            protocol: PROTOCOL_RTPS,
            version: ProtocolVersion::PROTOCOLVERSION,
            vendor_id: VENDORID_INT2,
            guid_prefix: guid,
        }
    }

    pub(crate) fn protocol(&self) -> ProtocolId {
        self.protocol
    }

    pub(crate) fn version(&self) -> ProtocolVersion {
        self.version
    }

    pub(crate) fn vendor_id(&self) -> VendorId {
        self.vendor_id
    }

    pub(crate) fn guid_prefix(&self) -> GuidPrefix {
        self.guid_prefix
    }

    pub(crate) fn is_valid(&self) -> bool {
        // 8.3.6.3
        // (): When the message has fewer octets than required to contain the full header.
        // () && (self.protocol == PROTOCOL_RTPS) && (self.version > PROTOCOLVERSION)
        // self.size == RTPS_HEADER_LENGTH
        self.protocol == PROTOCOL_RTPS
            && self.version.major <= ProtocolVersion::PROTOCOLVERSION.major
    }
}
