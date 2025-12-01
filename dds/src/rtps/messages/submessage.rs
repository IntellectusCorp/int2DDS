use bytes::Bytes;
use speedy::{Context, Error, Readable, Writable, Writer};

use crate::rtps::{
    common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
    messages::{
        message_receiver::MessageReceiver,
        submessage_body::SubmessageBody,
        submessage_header::SubmessageHeader,
        submessage_id::SubmessageId,
        submessages::{
            ack_nack::AckNack,
            data::Data,
            data_frag::DataFrag,
            gap::Gap,
            heartbeat::Heartbeat,
            heartbeat_frag::HeartbeatFrag,
            info::{InfoDestination, InfoReply, InfoReplyIp4, InfoSource, InfoTimestamp},
            nack_frag::NackFrag,
            pad::Pad,
        },
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Submessage {
    pub(crate) header: SubmessageHeader,
    pub(crate) body: SubmessageBody,
}

impl Submessage {
    pub(crate) fn read_from_buffer(
        message_receiver: &mut MessageReceiver,
        all_submessages_bytes: &mut Bytes,
    ) -> RtpsResult<Option<Self>> {
        let map_speedy_err = |p: Error| RtpsError::new(RtpsErrorCode::Io, p.to_string());

        let submessage_header =
            SubmessageHeader::read_from_buffer(all_submessages_bytes).map_err(map_speedy_err)?;
        // log::debug!(
        //     "submessage_header: {:?} size: {}",
        //     submessage_header,
        //     submessage_header.submessage_length()
        // );

        // Length of current submessage
        // let curr_submessage_len = 4 + submessage_header.submessage_length() as usize;

        // Split the current submessage slice, move the pointer of all_submessages_bytes to the start of the next submessage
        // let mut curr_submessage_bytes = all_submessages_bytes.split_to(curr_submessage_len);

        // Handle RTPS 2.5 case where submessageLength == 0 - start
        // In case submessageLength==0, the Submessage is the last Submessage in the Message and extends up to the end of the Message
        // OpenDDS uses the "submessageLength omission" pattern
        let submessage_len = submessage_header.submessage_length() as usize;

        let actual_body = if submessage_len == 0 {
            all_submessages_bytes.len() - 4 // Everything except the header
        } else {
            submessage_len
        };

        let mut curr_submessage_bytes = all_submessages_bytes.split_to(4 + actual_body);
        // Handle RTPS 2.5 case where submessageLength == 0 - end

        // Separate header and body from current submessage
        let mut submessage_body_bytes = curr_submessage_bytes.split_off(4);

        let submessage_body = match submessage_header.submessage_id() {
            SubmessageId::ACKNACK => {
                let submessage_body =
                    AckNack::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::AckNack(submessage_body))
            }
            SubmessageId::DATA => {
                let submessage_body =
                    Data::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::Data(submessage_body))
            }
            SubmessageId::DATA_FRAG => {
                let submessage_body =
                    DataFrag::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::DataFrag(submessage_body))
            }
            SubmessageId::GAP => {
                let submessage_body = Gap::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::Gap(submessage_body))
            }
            SubmessageId::HEARTBEAT => {
                let submessage_body =
                    Heartbeat::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::Heartbeat(submessage_body))
            }
            SubmessageId::HEARTBEAT_FRAG => {
                let submessage_body =
                    HeartbeatFrag::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::HeartbeatFrag(submessage_body))
            }
            SubmessageId::INFO_DST => {
                let submessage_body = InfoDestination::read_from_buffer_with_ctx(
                    submessage_header.endianness_flag().ok_or_else(|| {
                        RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None)
                    })?,
                    &mut submessage_body_bytes,
                )
                .map_err(map_speedy_err)?;
                // Change in state of Receiver
                message_receiver.from_destination(&submessage_body);
                Some(SubmessageBody::InfoDestination(submessage_body))
            }
            SubmessageId::INFO_REPLY => {
                let submessage_body =
                    InfoReply::deserialize(&submessage_body_bytes, &submessage_header)?;
                // Change in state of Receiver
                message_receiver.from_reply(&submessage_header, &submessage_body);
                Some(SubmessageBody::InfoReply(submessage_body))
            }
            SubmessageId::INFO_REPLY_IP4 => {
                let submessage_body =
                    InfoReplyIp4::deserialize(&submessage_body_bytes, &submessage_header)?;
                // Change in state of Receiver
                message_receiver.from_reply_ip4(&submessage_header, &submessage_body);
                Some(SubmessageBody::InfoReplyIp4(submessage_body))
            }
            SubmessageId::INFO_SRC => {
                let submessage_body = InfoSource::read_from_buffer_with_ctx(
                    submessage_header.endianness_flag().ok_or_else(|| {
                        RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None)
                    })?,
                    &mut submessage_body_bytes,
                )
                .map_err(map_speedy_err)?;
                // Change in state of Receiver
                message_receiver.from_source(&submessage_body);
                Some(SubmessageBody::InfoSource(submessage_body))
            }
            SubmessageId::INFO_TS => {
                let submessage_body = InfoTimestamp::read_from_buffer_with_ctx(
                    submessage_header.endianness_flag().ok_or_else(|| {
                        RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None)
                    })?,
                    &mut submessage_body_bytes,
                )
                .map_err(map_speedy_err)?;
                // Change in state of Receiver
                message_receiver.from_timestamp(&submessage_header, &submessage_body);
                Some(SubmessageBody::InfoTimestamp(submessage_body))
            }
            SubmessageId::NACK_FRAG => {
                let submessage_body =
                    NackFrag::deserialize(&submessage_body_bytes, &submessage_header)?;
                Some(SubmessageBody::NackFrag(submessage_body))
            }
            SubmessageId::PAD => {
                let submessage_body =
                    Pad::read_from_buffer(&submessage_body_bytes).map_err(map_speedy_err)?; // Reads nothing
                Some(SubmessageBody::Pad(submessage_body))
            }
            _ => {
                // println!("Unknown submessage ID : {:?}", submessage_header.submessage_id());
                None
            }
        };

        if let Some(body) = submessage_body {
            Ok(Some(Submessage { header: submessage_header, body }))
        } else {
            Ok(None)
        }
    }
}

impl<C: Context> Writable<C> for Submessage {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        let Submessage { header, body, .. } = self;
        writer.write_value(header)?;
        writer.write_value(body)?;

        Ok(())
    }
}
