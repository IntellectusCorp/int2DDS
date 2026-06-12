//! Entity identifiers for RTPS entities within a participant.
//!
//! This module defines `EntityId` which uniquely identifies RTPS entities (readers, writers)
//! within a participant. Each EntityId consists of a 3-byte key and an EntityKind byte that
//! indicates the entity type (builtin vs user-defined, reader vs writer, keyed vs keyless).

use crate::dcps::topic::type_support::DdsType;
use crate::rtps::common::entity_kind::EntityKind;

#[derive(DdsType, PartialEq, Copy, Eq, PartialOrd, Ord, Hash)]
#[dds_type(crate_path = "crate", no_partialeq)]
pub struct EntityId {
    pub entity_key: [u8; 3],
    pub entity_kind: EntityKind, //u8
}
impl EntityId {
    pub const UNKNOWN: Self =
        Self { entity_key: [0x00; 3], entity_kind: EntityKind::UNKNOWN_USER_DEFINED };
    // Table 9.2 - EntityId_t values fully predefined by the RTPS Protocol
    pub const PARTICIPANT: Self =
        Self { entity_key: [00, 00, 0x01], entity_kind: EntityKind::BUILT_IN_PARTICIPANT };
    pub const SEDP_BUILTIN_TOPICS_WRITER: Self =
        Self { entity_key: [00, 00, 0x02], entity_kind: EntityKind::BUILT_IN_WRITER_WITH_KEY };
    pub const SEDP_BUILTIN_TOPICS_READER: Self =
        Self { entity_key: [00, 00, 0x02], entity_kind: EntityKind::BUILT_IN_READER_WITH_KEY };
    pub const SEDP_BUILTIN_PUBLICATIONS_WRITER: Self =
        Self { entity_key: [00, 00, 0x03], entity_kind: EntityKind::BUILT_IN_WRITER_WITH_KEY };
    pub const SEDP_BUILTIN_PUBLICATIONS_READER: Self =
        Self { entity_key: [00, 00, 0x03], entity_kind: EntityKind::BUILT_IN_READER_WITH_KEY };
    pub const SEDP_BUILTIN_SUBSCRIPTIONS_WRITER: Self =
        Self { entity_key: [00, 00, 0x04], entity_kind: EntityKind::BUILT_IN_WRITER_WITH_KEY };
    pub const SEDP_BUILTIN_SUBSCRIPTIONS_READER: Self =
        Self { entity_key: [00, 00, 0x04], entity_kind: EntityKind::BUILT_IN_READER_WITH_KEY };
    pub const SPDP_BUILTIN_PARTICIPANT_WRITER: Self =
        Self { entity_key: [00, 0x01, 00], entity_kind: EntityKind::BUILT_IN_WRITER_WITH_KEY };
    pub const SPDP_BUILTIN_PARTICIPANT_READER: Self =
        Self { entity_key: [00, 0x01, 00], entity_kind: EntityKind::BUILT_IN_READER_WITH_KEY };
    pub const P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER: Self =
        Self { entity_key: [00, 0x02, 00], entity_kind: EntityKind::BUILT_IN_WRITER_WITH_KEY };
    pub const P2P_BUILTIN_PARTICIPANT_MESSAGE_READER: Self =
        Self { entity_key: [00, 0x02, 00], entity_kind: EntityKind::BUILT_IN_READER_WITH_KEY };

    // DDS-XTypes 1.3 Table 21 - TypeLookup service builtin endpoints (keyless).
    pub const TYPE_LOOKUP_REQUEST_WRITER: Self =
        Self { entity_key: [0x00, 0x03, 0x00], entity_kind: EntityKind::BUILT_IN_WRITER_NO_KEY };
    pub const TYPE_LOOKUP_REQUEST_READER: Self =
        Self { entity_key: [0x00, 0x03, 0x00], entity_kind: EntityKind::BUILT_IN_READER_NO_KEY };
    pub const TYPE_LOOKUP_REPLY_WRITER: Self =
        Self { entity_key: [0x00, 0x03, 0x01], entity_kind: EntityKind::BUILT_IN_WRITER_NO_KEY };
    pub const TYPE_LOOKUP_REPLY_READER: Self =
        Self { entity_key: [0x00, 0x03, 0x01], entity_kind: EntityKind::BUILT_IN_READER_NO_KEY };

    pub fn new<T>(entity_key: [u8; 3], entity_kind: T) -> Self
    where
        T: TryInto<EntityKind>,
        // The Error type of the TryInto trait implemented by T must implement Debug.
        <T as TryInto<EntityKind>>::Error: std::fmt::Debug,
    {
        let entity_kind = entity_kind.try_into().expect("Invalid entity kind");

        let entity_id = Self { entity_key, entity_kind };

        if entity_id.is_reserved() {
            panic!("Attempt to create a reserved EntityId");
        }

        entity_id
    }

    pub fn is_reserved(&self) -> bool {
        // DDS-Security 1.1 check
        if self.is_dds_security_reserved() {
            return true;
        }

        // DDS-XTypes check
        if self.is_dds_xtypes_reserved() {
            return true;
        }

        // Protocol 2.2 Deprecated EntityId check
        if self.is_protocol_v2_2_deprecated() {
            return true;
        }

        false
    }

    // 9.3.1.3.1 EntityIds Reserved by other Specifications
    fn is_dds_security_reserved(&self) -> bool {
        // EntityIds with entityKey in range {ff, 00, 00} - {ff, ff, ff}
        // and entityKind in range 0xc0-0xff (inclusive)
        if self.entity_key[0] == 0xff && self.entity_kind.is_valid() {
            return true;
        }

        // {00, 02, 01} with kinds c3 or c4
        if self.entity_key == [0x00, 0x02, 0x01]
            && (self.entity_kind == EntityKind::BUILT_IN_WRITER_NO_KEY
                || self.entity_kind == EntityKind::BUILT_IN_READER_NO_KEY)
        {
            return true;
        }

        false
    }

    // 9.3.1.3.1 EntityIds Reserved by other Specifications
    fn is_dds_xtypes_reserved(&self) -> bool {
        // {00, 03, 00} and {00, 03, 01} with kinds c3 or c4
        if (self.entity_key == [0x00, 0x03, 0x00] || self.entity_key == [0x00, 0x03, 0x01])
            && (self.entity_kind == EntityKind::BUILT_IN_WRITER_NO_KEY
                || self.entity_kind == EntityKind::BUILT_IN_READER_NO_KEY)
        {
            return true;
        }

        false
    }

    // 9.3.1.4 Deprecated EntityIds in version 2.2 of the Protocol
    fn is_protocol_v2_2_deprecated(&self) -> bool {
        // Additional methods may need to be considered depending on support for pre-2.2 versions
        // The 2.0 documentation also states these are not supported... (?)
        let deprecated_combinations = [
            // writerApplications
            ([0x00, 0x00, 0x01], EntityKind::BUILT_IN_WRITER_WITH_KEY),
            // readerApplications
            ([0x00, 0x00, 0x01], EntityKind::BUILT_IN_READER_WITH_KEY),
            // writerClients
            ([0x00, 0x00, 0x05], EntityKind::BUILT_IN_WRITER_WITH_KEY),
            // readerClients
            ([0x00, 0x00, 0x05], EntityKind::BUILT_IN_READER_WITH_KEY),
            // writerServices
            ([0x00, 0x00, 0x06], EntityKind::BUILT_IN_WRITER_WITH_KEY),
            // readerServices
            ([0x00, 0x00, 0x06], EntityKind::BUILT_IN_READER_WITH_KEY),
            // writerManagers
            ([0x00, 0x00, 0x07], EntityKind::BUILT_IN_WRITER_WITH_KEY),
            // readerManagers
            ([0x00, 0x00, 0x07], EntityKind::BUILT_IN_READER_WITH_KEY),
            // writerApplicationsSelf
            ([0x00, 0x00, 0x08], EntityKind::BUILT_IN_WRITER_WITH_KEY),
        ];

        for (key, kind) in &deprecated_combinations {
            if &self.entity_key == key && self.entity_kind.0 == kind.0 {
                return true;
            }
        }

        false
    }

    pub fn entity_key(&self) -> [u8; 3] {
        self.entity_key
    }
    pub fn entity_kind(&self) -> EntityKind {
        self.entity_kind
    }

    pub fn to_bytes(&self) -> [u8; 4] {
        let mut bytes = [0u8; 4];
        bytes[0..3].copy_from_slice(&self.entity_key);
        bytes[3] = self.entity_kind.0;
        bytes
    }

    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        let mut key = [0u8; 3];
        key.copy_from_slice(&bytes[0..3]);

        EntityId { entity_key: key, entity_kind: EntityKind(bytes[3]) }
    }
}
