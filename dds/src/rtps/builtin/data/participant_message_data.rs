//! Participant message data for liveliness assertions.
//!
//! This module defines participant message types used for manual and automatic
//! liveliness assertion in the WLP (Writer Liveliness Protocol). Participants
//! send these messages to maintain liveliness state.

#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    infrastructure::qos_policy::LivelinessQosPolicyKind,
    rtps::common::{guid::GuidPrefix, types::SerializedData},
    topic::type_support::DdsType,
};

#[derive(DdsType, Copy, PartialEq, Eq)]
#[dds_type(crate_path = "crate", no_default, no_partialeq)]
pub(crate) struct ParticipantMessageDataKind([u8; 4]);

impl ParticipantMessageDataKind {
    pub(crate) const UNKNOWN: Self = Self([0; 4]);
    pub(crate) const AUTOMATIC_LIVELINESS_UPDATE: Self = Self([0x00, 0x00, 0x00, 0x01]);
    pub(crate) const MANUAL_LIVELINESS_UPDATE: Self = Self([0x00, 0x00, 0x00, 0x02]);

    pub(crate) fn is_vendor_specific(&self) -> bool {
        self.0[0] & 0x80 != 0
    }

    pub(crate) fn is_standard(&self) -> bool {
        self.0[0] & 0x80 == 0
    }
}

impl From<LivelinessQosPolicyKind> for ParticipantMessageDataKind {
    fn from(value: LivelinessQosPolicyKind) -> Self {
        match value {
            LivelinessQosPolicyKind::Automatic => Self::AUTOMATIC_LIVELINESS_UPDATE,
            LivelinessQosPolicyKind::ManualByParticipant => Self::MANUAL_LIVELINESS_UPDATE,
            _ => Self::UNKNOWN,
        }
    }
}

#[derive(DdsType)]
#[dds_type(crate_path = "crate", no_default)]
pub(crate) struct ParticipantMessageData {
    participant_guid_prefix: GuidPrefix,
    kind: ParticipantMessageDataKind,
    data: Vec<u8>,
}

impl ParticipantMessageData {
    pub(crate) fn new(participant_guid_prefix: GuidPrefix, kind: LivelinessQosPolicyKind) -> Self {
        Self { participant_guid_prefix, kind: kind.into(), data: Vec::new() }
    }

    pub(crate) fn participant_guid_prefix(&self) -> GuidPrefix {
        self.participant_guid_prefix
    }

    pub(crate) fn kind(&self) -> ParticipantMessageDataKind {
        self.kind
    }

    pub(crate) fn data(&self) -> Vec<u8> {
        self.data.clone()
    }

    pub(crate) fn set_participant_guid_prefix(&mut self, prefix: GuidPrefix) {
        self.participant_guid_prefix = prefix;
    }

    pub(crate) fn set_kind(&mut self, kind: ParticipantMessageDataKind) {
        self.kind = kind;
    }

    pub(crate) fn set_data(&mut self, data: Vec<u8>) {
        self.data = data;
    }

    pub(crate) fn with_data(mut self, data: Vec<u8>) -> Self {
        self.data = data;
        self
    }

    pub(crate) fn is_automatic_liveliness(&self) -> bool {
        self.kind == ParticipantMessageDataKind::AUTOMATIC_LIVELINESS_UPDATE
    }

    pub(crate) fn is_manual_liveliness(&self) -> bool {
        self.kind == ParticipantMessageDataKind::MANUAL_LIVELINESS_UPDATE
    }

    pub(crate) fn from_serialized_data(data: &[u8]) -> Result<Self, String> {
        let bytes = data;

        // Minimum size: 4 (header) + 12 (GuidPrefix) + 4 (kind) + 4 (data length) = 24 bytes
        if bytes.len() < 24 {
            return Err(format!(
                "ParticipantMessageData requires at least 24 bytes, got {}",
                bytes.len()
            ));
        }

        let mut pos = 0;

        // Parse encapsulation header (4 bytes)
        // Encapsulation kind is always in big-endian byte order
        let encap_kind = u16::from_be_bytes([bytes[pos], bytes[pos + 1]]);
        let little_endian = match encap_kind {
            0x0000 => false, // CDR_BE
            0x0001 => true,  // CDR_LE
            _ => {
                return Err(format!(
                    "ParticipantMessageData: invalid encapsulation kind 0x{:04x}",
                    encap_kind
                ));
            }
        };
        pos += 2;

        // Encapsulation options (2 bytes) - skip
        pos += 2;

        // Parse GuidPrefix (12 bytes)
        let mut guid_prefix_bytes = [0u8; 12];
        guid_prefix_bytes.copy_from_slice(&bytes[pos..pos + 12]);
        let participant_guid_prefix = GuidPrefix::from(guid_prefix_bytes);
        pos += 12;

        // Parse kind (4 bytes)
        let mut kind_bytes = [0u8; 4];
        kind_bytes.copy_from_slice(&bytes[pos..pos + 4]);
        let kind = ParticipantMessageDataKind(kind_bytes);
        pos += 4;

        // Parse data sequence length (4 bytes, using detected endianness)
        if bytes.len() < pos + 4 {
            return Err(format!(
                "ParticipantMessageData: not enough bytes for data length at position {}",
                pos
            ));
        }
        let data_length = if little_endian {
            u32::from_le_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
        } else {
            u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
        } as usize;
        pos += 4;

        // Check if we have enough bytes for the data
        if bytes.len() < pos + data_length {
            return Err(format!(
                "ParticipantMessageData: data field requires {} bytes, but only {} available",
                data_length,
                bytes.len() - pos
            ));
        }

        // Parse data
        let data = bytes[pos..pos + data_length].to_vec();

        Ok(Self { participant_guid_prefix, kind, data })
    }

    pub(crate) fn to_serialized_data(&self) -> SerializedData {
        let mut serialized = Vec::new();

        // Encapsulation header (4 bytes)
        // Encapsulation kind: CDR_LE (0x0001) - always in big-endian byte order
        serialized.extend_from_slice(&0x0001u16.to_be_bytes());
        // Encapsulation options: 0x0000 - always in big-endian byte order
        serialized.extend_from_slice(&0x0000u16.to_be_bytes());

        // Serialize GuidPrefix (12 bytes)
        serialized.extend_from_slice(&self.participant_guid_prefix);

        // Serialize kind (4 bytes)
        serialized.extend_from_slice(&self.kind.0);

        // Serialize data sequence length (4 bytes, little-endian)
        let data_len = self.data.len() as u32;
        serialized.extend_from_slice(&data_len.to_le_bytes());

        // Serialize data
        serialized.extend_from_slice(&self.data);

        // Add padding to 4-byte alignment if needed
        let padding_needed = (4 - (serialized.len() % 4)) % 4;
        serialized.extend(std::iter::repeat_n(0x00, padding_needed));

        SerializedData::from(serialized)
    }
}
