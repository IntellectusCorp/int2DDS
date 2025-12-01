//! RTPS message header extension (RTPS 2.5+).
//!
//! This module defines the HeaderExtension structure introduced in RTPS version 2.5
//! (section 8.3.7). The header extension appears after the main header and provides
//! additional message metadata including message length, timestamp, and checksums.

use crate::rtps::common::{
    checksum::Checksum,
    parameters::ParameterList,
    time::Timestamp,
    types::{MessageLength, UExtension4, WExtension8},
};

// 8.3.7 The RTPS HeaderExtension
// Concept introduced in RTPS version 2.5. Appears after the Header.

pub(crate) struct HeaderExtension {
    message_length: MessageLength,
    rtps_send_timestamp: Timestamp,
    u_extension4: UExtension4,
    w_extension8: WExtension8,
    message_checksum: Checksum,
    parameters: ParameterList,
}

#[allow(dead_code)]
impl HeaderExtension {
    pub(crate) fn message_length(&self) -> MessageLength {
        self.message_length
    }

    pub(crate) fn rtps_send_timestamp(&self) -> Timestamp {
        self.rtps_send_timestamp
    }

    pub(crate) fn u_extension4(&self) -> UExtension4 {
        self.u_extension4
    }

    pub(crate) fn w_extension8(&self) -> WExtension8 {
        self.w_extension8
    }

    pub(crate) fn message_checksum(&self) -> Checksum {
        self.message_checksum
    }

    pub(crate) fn parameters(&self) -> ParameterList {
        self.parameters.clone()
    }

    // 8.3.7.3 Validity
    pub(crate) fn is_valid(&self) -> bool {
        todo!()
    }
}
