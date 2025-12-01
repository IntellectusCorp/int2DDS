//! GUID - Globally Unique Identifier for RTPS entities.
//!
//! This module defines `Guid` which uniquely identifies RTPS entities across the entire
//! DDS domain. A GUID consists of a GuidPrefix (12 bytes, unique per participant) and
//! an EntityId (4 bytes, unique within the participant).

use speedy::{Readable, Writable};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::entity_kind::EntityKind;

// guidPrefix[0] = vendorId[0] && guidPrefix[1] = vendorId[1]
pub const GUIDPREFIX_UNKNOWN: GuidPrefix = [0x00; 12];
pub type GuidPrefix = [u8; 12];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Readable, Writable, Hash)]
pub struct Guid {
    prefix: GuidPrefix,  // [u8; 12]
    entity_id: EntityId, // [u8; 4]
}

impl Guid {
    pub const UNKNOWN: Guid = Guid { prefix: GUIDPREFIX_UNKNOWN, entity_id: EntityId::UNKNOWN };
    pub fn new(prefix: GuidPrefix, entity_id: EntityId) -> Self {
        Self { prefix, entity_id }
    }

    pub fn prefix(&self) -> GuidPrefix {
        self.prefix
    }

    pub fn entity_id(&self) -> EntityId {
        self.entity_id
    }
    pub fn entity_kind(&self) -> EntityKind {
        self.entity_id.entity_kind
    }

    pub fn to_bytes(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];

        bytes[0..12].copy_from_slice(&self.prefix);

        let entity_bytes = self.entity_id.to_bytes();
        bytes[12..16].copy_from_slice(&entity_bytes);

        bytes
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        let mut prefix = [0u8; 12];
        prefix.copy_from_slice(&bytes[0..12]);

        let mut entity_bytes = [0u8; 4];
        entity_bytes.copy_from_slice(&bytes[12..16]);

        Guid { prefix, entity_id: EntityId::from_bytes(entity_bytes) }
    }

    pub fn generate_unique_guid_prefix() -> GuidPrefix {
        let mut prefix = [0u8; 12];

        // Vendor ID (INT2)
        prefix[0] = 0x01;
        prefix[1] = 0x19;

        // Generate 10 bytes by hashing time + PID + address
        let mut hasher = DefaultHasher::new();

        // Current time
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        now.hash(&mut hasher);

        // Process ID
        std::process::id().hash(&mut hasher);

        // Hash stack address
        let local = 0u8;
        (&local as *const u8 as usize).hash(&mut hasher);

        // Hash → 64 bits + partial timestamp → total 10 bytes generated
        let hash = hasher.finish();
        for i in 0..8 {
            prefix[i + 2] = ((hash >> (i * 8)) & 0xFF) as u8;
        }

        // Remaining 2 bytes use high bits of timestamp
        let ts_high = (now >> 56) as u16;
        prefix[10] = (ts_high & 0xFF) as u8;
        prefix[11] = (ts_high >> 8) as u8;

        prefix
    }

    pub fn guid_prefix_to_string(prefix: &GuidPrefix) -> String {
        prefix.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(":")
    }
}

pub type EntityName = String;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
struct EntityIdSet {
    entity_ids: Vec<EntityId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupDigest([u8; 4]);
impl GroupDigest {
    pub fn new(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    pub fn from_entity_ids() -> Self {
        todo!()
        // DDSI-RTPS v2.5 9.3.2.5 GroupDigest_t (p.150)
    }
}

#[cfg(test)]
mod tests {
    use crate::rtps::common::entity_kind::EntityKind;

    use super::*;

    #[test]
    fn test_entity_kind_to_bytes() {
        // Test that EntityKind values are correctly represented as bytes
        assert_eq!(EntityKind::BUILT_IN_PARTICIPANT.0, 0xC1);
        assert_eq!(EntityKind::USER_DEFINED_WRITER_WITH_KEY.0, 0x02);
        assert_eq!(EntityKind::BUILT_IN_READER_NO_KEY.0, 0xC4);
    }

    #[test]
    fn test_entity_id_to_from_bytes() {
        // Create an EntityId with some test values
        let entity_id = EntityId {
            entity_key: [0x01, 0x02, 0x03],
            entity_kind: EntityKind::BUILT_IN_PARTICIPANT,
        };

        // Convert to bytes
        let bytes = entity_id.to_bytes();

        // Verify byte representation
        assert_eq!(bytes, [0x01, 0x02, 0x03, 0xC1]);

        // Convert back from bytes
        let reconstructed = EntityId::from_bytes(bytes);

        // Verify the reconstructed object equals the original
        assert_eq!(entity_id.entity_key, reconstructed.entity_key);
        assert_eq!(entity_id.entity_kind, reconstructed.entity_kind);
        assert_eq!(entity_id, reconstructed);
    }

    #[test]
    fn test_guid_to_from_bytes() {
        // Create a test GuidPrefix (12 bytes)
        let prefix: GuidPrefix =
            [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C];

        // Create a test EntityId
        let entity_id = EntityId {
            entity_key: [0x0D, 0x0E, 0x0F],
            entity_kind: EntityKind::BUILT_IN_WRITER_NO_KEY,
        };

        // Create a GUID
        let guid = Guid::new(prefix, entity_id);

        // Convert to bytes
        let bytes = guid.to_bytes();

        // Expected byte representation (prefix followed by entity_id bytes)
        let expected = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E,
            0x0F, 0xC3,
        ];

        // Verify byte representation
        assert_eq!(bytes, expected);

        // Convert back from bytes
        let reconstructed = Guid::from_bytes(bytes);

        // Verify reconstructed object equals original
        assert_eq!(guid.prefix, reconstructed.prefix);
        assert_eq!(guid.entity_id.entity_key, reconstructed.entity_id.entity_key);
        assert_eq!(guid.entity_id.entity_kind, reconstructed.entity_id.entity_kind);
        assert_eq!(guid, reconstructed);
    }

    #[test]
    fn test_entity_kind_methods() {
        // Test is_valid() - should return true for built-in entities (0xC0-0xFF)
        assert!(EntityKind::BUILT_IN_UNKNOWN.is_valid());
        assert!(EntityKind::BUILT_IN_PARTICIPANT.is_valid());
        assert!(EntityKind(0xFF).is_valid());
        assert!(!EntityKind::USER_DEFINED_WRITER_WITH_KEY.is_valid());
        assert!(!EntityKind(0x99).is_valid());

        // Test is_reader()
        assert!(EntityKind::USER_DEFINED_READER_NO_KEY.is_reader());
        assert!(EntityKind::USER_DEFINED_READER_WITH_KEY.is_reader());
        assert!(EntityKind::USER_DEFINED_READER_GROUP.is_reader());
        assert!(EntityKind::BUILT_IN_READER_NO_KEY.is_reader());
        assert!(EntityKind::BUILT_IN_READER_WITH_KEY.is_reader());
        assert!(EntityKind::BUILT_IN_READER_GROUP.is_reader());
        assert!(!EntityKind::USER_DEFINED_WRITER_WITH_KEY.is_reader());
        assert!(!EntityKind::BUILT_IN_PARTICIPANT.is_reader());

        // Test is_writer()
        assert!(EntityKind::USER_DEFINED_WRITER_WITH_KEY.is_writer());
        assert!(EntityKind::USER_DEFINED_WRITER_NO_KEY.is_writer());
        assert!(EntityKind::USER_DEFINED_WRITER_GROUP.is_writer());
        assert!(EntityKind::BUILT_IN_WRITER_WITH_KEY.is_writer());
        assert!(EntityKind::BUILT_IN_WRITER_NO_KEY.is_writer());
        assert!(EntityKind::BUILT_IN_WRITER_GROUP.is_writer());
        assert!(!EntityKind::USER_DEFINED_READER_NO_KEY.is_writer());
        assert!(!EntityKind::BUILT_IN_PARTICIPANT.is_writer());

        // Test is_built_in()
        assert!(EntityKind::BUILT_IN_UNKNOWN.is_built_in());
        assert!(EntityKind::BUILT_IN_PARTICIPANT.is_built_in());
        assert!(EntityKind::BUILT_IN_READER_GROUP.is_built_in());
        assert!(!EntityKind::USER_DEFINED_WRITER_GROUP.is_built_in());
        assert!(!EntityKind::USER_DEFINED_UNKNOWN.is_built_in());

        // Test is_user_defined()
        assert!(EntityKind::USER_DEFINED_UNKNOWN.is_user_defined());
        assert!(EntityKind::USER_DEFINED_WRITER_WITH_KEY.is_user_defined());
        assert!(EntityKind::USER_DEFINED_READER_NO_KEY.is_user_defined());
        assert!(!EntityKind::BUILT_IN_PARTICIPANT.is_user_defined());
        assert!(!EntityKind::BUILT_IN_READER_GROUP.is_user_defined());

        // Test is_with_key()
        assert!(EntityKind::USER_DEFINED_WRITER_WITH_KEY.is_with_key());
        assert!(EntityKind::BUILT_IN_WRITER_WITH_KEY.is_with_key());
        assert!(EntityKind::USER_DEFINED_READER_WITH_KEY.is_with_key());
        assert!(EntityKind::BUILT_IN_READER_WITH_KEY.is_with_key());
        assert!(!EntityKind::USER_DEFINED_WRITER_NO_KEY.is_with_key());
        assert!(!EntityKind::BUILT_IN_READER_NO_KEY.is_with_key());
        assert!(!EntityKind::USER_DEFINED_WRITER_GROUP.is_with_key());
    }

    #[test]
    fn test_edge_cases() {
        // Test EntityId with all zeros
        let zero_entity_id = EntityId { entity_key: [0, 0, 0], entity_kind: EntityKind(0) };
        let bytes = zero_entity_id.to_bytes();
        assert_eq!(bytes, [0, 0, 0, 0]);
        let reconstructed = EntityId::from_bytes(bytes);
        assert_eq!(zero_entity_id, reconstructed);

        // Test EntityId with all 0xFF
        let max_entity_id =
            EntityId { entity_key: [0xFF, 0xFF, 0xFF], entity_kind: EntityKind(0xFF) };
        let bytes = max_entity_id.to_bytes();
        assert_eq!(bytes, [0xFF, 0xFF, 0xFF, 0xFF]);
        let reconstructed = EntityId::from_bytes(bytes);
        assert_eq!(max_entity_id, reconstructed);

        // Test GUID with all zeros
        let zero_prefix = [0u8; 12];
        let zero_guid = Guid::new(zero_prefix, zero_entity_id);
        let bytes = zero_guid.to_bytes();
        assert_eq!(bytes, [0; 16]);
        let reconstructed = Guid::from_bytes(bytes);
        assert_eq!(zero_guid, reconstructed);

        // Test GUID with alternating pattern
        let alt_prefix = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
        let alt_entity_id =
            EntityId { entity_key: [0xAA, 0x55, 0xAA], entity_kind: EntityKind(0x55) };
        let alt_guid = Guid::new(alt_prefix, alt_entity_id);
        let bytes = alt_guid.to_bytes();
        assert_eq!(
            bytes,
            [
                0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55,
                0xAA, 0x55
            ]
        );
        let reconstructed = Guid::from_bytes(bytes);
        assert_eq!(alt_guid, reconstructed);
    }
}
