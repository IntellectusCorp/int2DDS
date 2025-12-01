//! RTPS submessage body enumeration.
//!
//! This module defines the `SubmessageBody` enum representing all possible
//! RTPS submessage types including DATA, ACKNACK, HEARTBEAT, GAP, INFO
//! submessages, and fragmentation submessages.

use speedy::{Context, Readable, Reader, Writable, Writer};

use crate::rtps::messages::submessages::{
    ack_nack::AckNack,
    data::Data,
    data_frag::DataFrag,
    gap::Gap,
    heartbeat::Heartbeat,
    heartbeat_frag::HeartbeatFrag,
    info::{InfoDestination, InfoReply, InfoReplyIp4, InfoSource, InfoTimestamp},
    nack_frag::NackFrag,
    pad::Pad,
};
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SubmessageBody {
    AckNack(AckNack),
    Data(Data),
    DataFrag(DataFrag),
    Gap(Gap),
    Heartbeat(Heartbeat),
    HeartbeatFrag(HeartbeatFrag),
    InfoDestination(InfoDestination),
    InfoReply(InfoReply),
    InfoReplyIp4(InfoReplyIp4),
    InfoSource(InfoSource),
    InfoTimestamp(InfoTimestamp),
    NackFrag(NackFrag),
    Pad(Pad),
}

impl<C: Context> Writable<C> for SubmessageBody {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let _ = match self {
            SubmessageBody::AckNack(m) => writer.write_value(&m),
            SubmessageBody::Data(m) => writer.write_value(&m),
            SubmessageBody::DataFrag(m) => writer.write_value(&m),
            SubmessageBody::Gap(m) => writer.write_value(&m),
            SubmessageBody::Heartbeat(m) => writer.write_value(&m),
            SubmessageBody::HeartbeatFrag(m) => writer.write_value(&m),
            SubmessageBody::InfoDestination(m) => writer.write_value(&m),
            SubmessageBody::InfoReply(m) => writer.write_value(&m),
            SubmessageBody::InfoReplyIp4(m) => writer.write_value(&m),
            SubmessageBody::InfoSource(m) => writer.write_value(&m),
            SubmessageBody::InfoTimestamp(m) => writer.write_value(&m),
            SubmessageBody::NackFrag(m) => writer.write_value(&m),
            SubmessageBody::Pad(m) => writer.write_value(&m),
        };
        Ok(())
    }
}

impl<'a, C: Context> Readable<'a, C> for SubmessageBody {
    fn read_from<R: Reader<'a, C>>(reader: &mut R) -> Result<Self, C::Error> {
        let body = reader.read_value::<SubmessageBody>()?;
        Ok(body)
    }
}
