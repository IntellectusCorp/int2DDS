//! Submessage header flags for RTPS submessages.
//!
//! This module defines the flag types used in submessage headers to indicate
//! endianness, final flag, inline QoS presence, data/key flag, and other
//! submessage-specific options.

use crate::rtps::messages::submessage_id::SubmessageId;

#[allow(dead_code)]
pub(crate) enum SubmessageFlagType {
    EndiannessFlag,
    FinalFlag,
    InlineQosFlag,
    DataFlag,
    KeyFlag,
    LivelinessFlag,
    InfoReplyFlag,
    InvalidateFlag,
}

pub(crate) struct SubmessageHeaderFlag {
    pub(crate) flag: u8,
}

impl Default for SubmessageHeaderFlag {
    fn default() -> Self {
        Self::new()
    }
}

impl SubmessageHeaderFlag {
    pub(crate) fn new() -> Self {
        Self { flag: 0x0 }
    }

    pub(crate) fn add_flag(
        &mut self,
        submessage_flag: SubmessageFlagType,
        submessage_id: SubmessageId,
    ) {
        match submessage_flag {
            SubmessageFlagType::EndiannessFlag => match submessage_id {
                SubmessageId::ACKNACK => self.flag |= 0x01,
                SubmessageId::DATA => self.flag |= 0x01,
                SubmessageId::DATA_FRAG => self.flag |= 0x01,
                SubmessageId::GAP => self.flag |= 0x01,
                SubmessageId::HEARTBEAT => self.flag |= 0x01,
                SubmessageId::HEARTBEAT_FRAG => self.flag |= 0x01,
                SubmessageId::INFO_DST => self.flag |= 0x01,
                SubmessageId::INFO_REPLY => self.flag |= 0x01,
                SubmessageId::INFO_SRC => self.flag |= 0x01,
                SubmessageId::INFO_TS => self.flag |= 0x01,
                SubmessageId::PAD => self.flag |= 0x01,
                SubmessageId::NACK_FRAG => self.flag |= 0x01,
                SubmessageId::INFO_REPLY_IP4 => self.flag |= 0x01,
                _ => {}
            },
            SubmessageFlagType::FinalFlag => match submessage_id {
                SubmessageId::ACKNACK => self.flag |= 0x02,
                SubmessageId::HEARTBEAT => self.flag |= 0x02,
                _ => {}
            },
            SubmessageFlagType::InlineQosFlag => match submessage_id {
                SubmessageId::DATA => self.flag |= 0x02,
                SubmessageId::DATA_FRAG => self.flag |= 0x02,
                _ => {}
            },
            SubmessageFlagType::DataFlag => {
                if submessage_id == SubmessageId::DATA {
                    self.flag |= 0x04
                }
            }
            SubmessageFlagType::KeyFlag => match submessage_id {
                SubmessageId::DATA => self.flag |= 0x08,
                SubmessageId::DATA_FRAG => self.flag |= 0x04,
                _ => {}
            },
            SubmessageFlagType::LivelinessFlag => {
                if submessage_id == SubmessageId::HEARTBEAT {
                    self.flag |= 0x04
                }
            }
            SubmessageFlagType::InfoReplyFlag => match submessage_id {
                SubmessageId::INFO_REPLY => self.flag |= 0x02,
                SubmessageId::INFO_REPLY_IP4 => self.flag |= 0x02,
                _ => {}
            },
            SubmessageFlagType::InvalidateFlag => {
                if submessage_id == SubmessageId::INFO_TS {
                    self.flag |= 0x02
                }
            }
        }
    }
}
