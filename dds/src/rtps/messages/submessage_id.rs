//! Submessage ID constants for RTPS submessage types.
//!
//! This module defines submessage identifiers for all RTPS submessage types
//! including DATA, ACKNACK, HEARTBEAT, GAP, and INFO submessages as specified
//! in the RTPS protocol.

use speedy::{Readable, Writable};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Readable, Writable)]
pub(crate) struct SubmessageId(u8);

#[allow(dead_code)]
impl SubmessageId {
    pub(crate) const PAD: Self = Self(0x01);
    pub(crate) const ACKNACK: Self = Self(0x06);
    pub(crate) const HEARTBEAT: Self = Self(0x07);
    pub(crate) const GAP: Self = Self(0x08);
    pub(crate) const INFO_TS: Self = Self(0x09);
    pub(crate) const INFO_SRC: Self = Self(0x0C);
    pub(crate) const INFO_REPLY_IP4: Self = Self(0x0D);
    pub(crate) const INFO_DST: Self = Self(0x0E);
    pub(crate) const INFO_REPLY: Self = Self(0x0F);
    pub(crate) const NACK_FRAG: Self = Self(0x12);
    pub(crate) const HEARTBEAT_FRAG: Self = Self(0x13);
    pub(crate) const DATA: Self = Self(0x15);
    pub(crate) const DATA_FRAG: Self = Self(0x16);

    pub(crate) fn as_u8(&self) -> u8 {
        self.0
    }

    pub(crate) fn new(value: u8) -> Self {
        Self(value)
    }
}
