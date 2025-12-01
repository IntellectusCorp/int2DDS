//! Entity kind classification for RTPS entities.
//!
//! This module defines `EntityKind` which classifies RTPS entities by type:
//! builtin vs user-defined, reader vs writer, and keyed vs keyless. The EntityKind
//! is the last byte of an EntityId as defined in Table 9.1 of the RTPS specification.

use speedy::{Readable, Writable};

// Table 9.1 - entityKind octet of an EntityId_t
// #[repr(u8)] setting does not work and causes padding, changed to this approach
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Readable, Writable, Hash)]
pub struct EntityKind(pub u8);
impl EntityKind {
    pub const UNKNOWN_USER_DEFINED: Self = Self(0x00);

    pub const USER_DEFINED_UNKNOWN: Self = Self(0x00);
    pub const BUILT_IN_UNKNOWN: Self = Self(0xC0);
    pub const BUILT_IN_PARTICIPANT: Self = Self(0xC1);

    pub const USER_DEFINED_WRITER_WITH_KEY: Self = Self(0x02);
    pub const BUILT_IN_WRITER_WITH_KEY: Self = Self(0xC2);
    pub const USER_DEFINED_WRITER_NO_KEY: Self = Self(0x03);
    pub const BUILT_IN_WRITER_NO_KEY: Self = Self(0xC3);

    pub const USER_DEFINED_READER_WITH_KEY: Self = Self(0x07);
    pub const BUILT_IN_READER_WITH_KEY: Self = Self(0xC7);
    pub const USER_DEFINED_READER_NO_KEY: Self = Self(0x04);
    pub const BUILT_IN_READER_NO_KEY: Self = Self(0xC4);

    pub const USER_DEFINED_WRITER_GROUP: Self = Self(0x08);
    pub const BUILT_IN_WRITER_GROUP: Self = Self(0xC8);
    pub const USER_DEFINED_READER_GROUP: Self = Self(0x09);
    pub const BUILT_IN_READER_GROUP: Self = Self(0xC9);

    pub fn is_valid(&self) -> bool {
        self.0 >= 0xc0
    }

    pub fn is_reader(&self) -> bool {
        let e = self.0 & 0x0F;
        e == 0x04 || e == 0x07 || e == 0x09
    }

    pub fn is_writer(&self) -> bool {
        let e = self.0 & 0x0F;
        e == 0x02 || e == 0x03 || e == 0x08
    }

    pub fn is_built_in(&self) -> bool {
        (self.0 & 0xF0) == 0xC0
    }

    pub fn is_user_defined(&self) -> bool {
        (self.0 & 0xF0) == 0x00
    }

    pub fn is_with_key(&self) -> bool {
        (self.0 & 0x0F) == 0x02 || (self.0 & 0x0F) == 0x07
    }
}
impl TryFrom<u8> for EntityKind {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        let entity_kind = EntityKind(value);
        if entity_kind.is_valid() {
            Ok(entity_kind)
        } else {
            Err("Invalid EntityKind value")
        }
    }
}

impl From<EntityKind> for u8 {
    fn from(entity_kind: EntityKind) -> Self {
        entity_kind.0
    }
}
