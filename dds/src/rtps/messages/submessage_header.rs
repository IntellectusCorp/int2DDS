//! RTPS submessage header structure.
//!
//! This module defines the `SubmessageHeader` that precedes each submessage in
//! an RTPS message, containing the submessage ID, flags, and length.

use speedy::{Context, Readable, Writable, Writer};

use crate::rtps::messages::submessage_id::SubmessageId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Readable)]
pub(crate) struct SubmessageHeader {
    submessage_id: SubmessageId,
    flags: u8,
    submessage_length: u16,
}

#[allow(dead_code)]
impl SubmessageHeader {
    pub(crate) fn new(submessage_id: SubmessageId, flags: u8, submessage_length: u16) -> Self {
        Self { submessage_id, flags, submessage_length }
    }

    pub(crate) fn submessage_id(&self) -> SubmessageId {
        self.submessage_id
    }

    pub(crate) fn flags(&self) -> u8 {
        self.flags
    }

    pub(crate) fn submessage_length(&self) -> u16 {
        self.submessage_length
    }

    pub(crate) fn submessage_flag(&self) -> u8 {
        self.flags
    }

    // E=0 means big-endian, E=1 means little-endian.
    pub(crate) fn endianness_flag(&self) -> Option<speedy::Endianness> {
        if self.submessage_id == SubmessageId::ACKNACK
            || self.submessage_id == SubmessageId::DATA
            || self.submessage_id == SubmessageId::DATA_FRAG
            || self.submessage_id == SubmessageId::GAP
            || self.submessage_id == SubmessageId::HEARTBEAT
            || self.submessage_id == SubmessageId::HEARTBEAT_FRAG
            || self.submessage_id == SubmessageId::INFO_DST
            || self.submessage_id == SubmessageId::INFO_REPLY
            || self.submessage_id == SubmessageId::INFO_SRC
            || self.submessage_id == SubmessageId::INFO_TS
            || self.submessage_id == SubmessageId::PAD
            || self.submessage_id == SubmessageId::NACK_FRAG
            || self.submessage_id == SubmessageId::INFO_REPLY_IP4
        {
            if (self.flags & 0x01) != 0 {
                Some(speedy::Endianness::LittleEndian)
            } else {
                Some(speedy::Endianness::BigEndian)
            }
        } else {
            None
        }
    }

    // 9.4.5.6 HeartBeat Submessage
    //ACK / 1: reader does not require a response from the writer.
    //ACK / 0: writer must respond to the AckNack message
    //HEARTBEAT / 1: Writer does not require a response from the Reader..
    //HEARTBEAT / 0: Reader must respond to the HeartBeat message.
    pub(crate) fn final_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::ACKNACK
            || self.submessage_id == SubmessageId::HEARTBEAT
        {
            Some(self.flags & 0x02 != 0)
        } else {
            None
        }
    }

    // 9.4.5.3 Data Submessage
    pub(crate) fn inline_qos_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::DATA || self.submessage_id == SubmessageId::DATA_FRAG
        {
            Some(self.flags & 0x02 != 0)
        } else {
            None
        }
    }

    // 9.4.5.3 Data Submessage
    pub(crate) fn data_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::DATA {
            Some(self.flags & 0x04 != 0)
        } else {
            None
        }
    }

    // 9.4.5.3 Data Submessage
    //9.4.5.4 DataFrag Submessage
    pub(crate) fn key_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::DATA {
            Some(self.flags & 0x08 != 0)
        } else if self.submessage_id == SubmessageId::DATA_FRAG {
            Some(self.flags & 0x04 != 0)
        } else {
            None
        }
    }

    // 9.4.5.6 HeartBeat Submessage
    //L=1 means the DDS DataReader associated with the RTPS Reader should refresh the ‘manual’ liveliness of the DDS DataWriter associated with the RTPS Writer of the message
    pub(crate) fn liveliness_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::HEARTBEAT {
            Some(self.flags & 0x04 != 0)
        } else {
            None
        }
    }

    //9.4.5.9 InfoReply Submessage
    //M=1 means the InfoReply also includes a multicastLocatorList.
    pub(crate) fn multicast_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::INFO_REPLY
            || self.submessage_id == SubmessageId::INFO_REPLY_IP4
        {
            Some(self.flags & 0x02 != 0)
        } else {
            None
        }
    }

    //9.4.5.11 InfoTimestamp Submessage
    // I=0 means the InfoTimestamp also includes a timestamp.
    //I=1 means subsequent Submessages should not be considered to have a valid timestamp.
    pub(crate) fn invalidate_flag(&self) -> Option<bool> {
        if self.submessage_id == SubmessageId::INFO_TS {
            Some(self.flags & 0x02 != 0)
        } else {
            None
        }
    }
}

impl<C: Context> Writable<C> for SubmessageHeader {
    #[inline]
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.submessage_id)?;
        writer.write_value(&self.flags)?;

        match self.endianness_flag() {
            Some(endian) => {
                // matching via writer.context().endianness() panics
                if endian == speedy::Endianness::LittleEndian {
                    writer.write_u8(self.submessage_length as u8)?;
                    writer.write_u8((self.submessage_length >> 8) as u8)?;
                } else if endian == speedy::Endianness::BigEndian {
                    writer.write_u8((self.submessage_length >> 8) as u8)?;
                    writer.write_u8(self.submessage_length as u8)?;
                } else {
                    todo!()
                }
            }
            None => {
                todo!()
            }
        };

        Ok(())
    }
}
