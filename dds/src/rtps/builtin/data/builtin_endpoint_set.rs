//! Builtin endpoint set flags for discovery protocol capabilities.
//!
//! This module defines bitflags indicating which builtin discovery endpoints
//! a participant supports (SPDP, SEDP participant/publication/subscription
//! announcers and detectors).

#![allow(dead_code)]
#![allow(unused_variables)]

use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct BuiltinEndpointFlag: u32 {
        const DISC_BUILTIN_ENDPOINT_PARTICIPANT_ANNOUNCER                = 1 << 0;
        const DISC_BUILTIN_ENDPOINT_PARTICIPANT_DETECTOR                 = 1 << 1;
        const DISC_BUILTIN_ENDPOINT_PUBLICATIONS_ANNOUNCER               = 1 << 2;
        const DISC_BUILTIN_ENDPOINT_PUBLICATIONS_DETECTOR                = 1 << 3;
        const DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_ANNOUNCER              = 1 << 4;
        const DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_DETECTOR               = 1 << 5;

        // Deprecated bits ??optionally include with comment
        const DISC_BUILTIN_ENDPOINT_PARTICIPANT_PROXY_ANNOUNCER          = 1 << 6;
        const DISC_BUILTIN_ENDPOINT_PARTICIPANT_PROXY_DETECTOR           = 1 << 7;
        const DISC_BUILTIN_ENDPOINT_PARTICIPANT_STATE_ANNOUNCER          = 1 << 8;
        const DISC_BUILTIN_ENDPOINT_PARTICIPANT_STATE_DETECTOR           = 1 << 9;

        const BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_WRITER           = 1 << 10;
        const BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_READER           = 1 << 11;

        // DDS-XTypes 1.3 Table 22 - TypeLookup service builtin endpoint bits.
        const BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_WRITER    = 1 << 12;
        const BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_READER    = 1 << 13;
        const BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_WRITER      = 1 << 14;
        const BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_READER      = 1 << 15;

        const DISC_BUILTIN_ENDPOINT_TOPICS_ANNOUNCER                     = 1 << 28;
        const DISC_BUILTIN_ENDPOINT_TOPICS_DETECTOR                      = 1 << 29;
    }

}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuiltinEndpointSet {
    bitmask: u32,
}

impl BuiltinEndpointSet {
    /// Create an initially empty EndpointSet
    pub(crate) fn new() -> Self {
        let mut bitmask = 0;
        bitmask |=
            BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_WRITER.bits();
        bitmask |=
            BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REQUEST_DATA_READER.bits();
        bitmask |=
            BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_WRITER.bits();
        bitmask |=
            BuiltinEndpointFlag::BUILTIN_ENDPOINT_TYPELOOKUP_SERVICE_REPLY_DATA_READER.bits();
        Self { bitmask }
    }
    /// Create from bitmask value
    pub(crate) fn from_bits(bits: u32) -> Self {
        Self { bitmask: bits }
    }

    /// Return the internal bitmask value
    pub(crate) fn bits(&self) -> u32 {
        self.bitmask
    }

    /// Add an endpoint
    pub(crate) fn add(&mut self, flag: BuiltinEndpointFlag) {
        self.bitmask |= flag.bits();
    }

    /// Check if endpoint is included
    pub(crate) fn contains(&self, flag: BuiltinEndpointFlag) -> bool {
        self.bitmask & flag.bits() != 0
    }

    /// Check if empty
    pub(crate) fn is_empty(&self) -> bool {
        self.bitmask == 0
    }

    pub(crate) fn empty() -> Self {
        Self { bitmask: 0 }
    }
}
impl std::fmt::Display for BuiltinEndpointSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Reuse the bitflags-derived Debug to expand the bitmask into flag names.
        write!(f, "{:?}", BuiltinEndpointFlag::from_bits_truncate(self.bitmask))
    }
}

impl Default for BuiltinEndpointSet {
    fn default() -> Self {
        Self::new()
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct BuiltinEndpointQos: u32 {
        const BEST_EFFORT_PARTICIPANT_MESSAGE_DATA_READER = 1 << 0;
    }
}
