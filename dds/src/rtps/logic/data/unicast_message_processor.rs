use std::{
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

use log::trace;

use crate::rtps::{
    common::{
        guid::Guid,
        locator::Locator,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
    },
    entities::entity::Entity as _,
    logic::data::participant_message_processor::ParticipantAccessor,
    messages::{
        header::Header,
        message_receiver::{MessageReceiver, TypedSubmessage},
        submessage_header::SubmessageHeader,
        submessages::{
            ack_nack::AckNack, data::Data, data_frag::DataFrag, gap::Gap, heartbeat::Heartbeat,
            nack_frag::NackFrag,
        },
    },
};

pub(crate) trait UnicastMessageSender: ParticipantAccessor {
    fn send_message_to_network<T>(buffer: &[u8], locators: T) -> RtpsResult<()>
    where
        T: IntoIterator<Item = Locator>;
}

pub(crate) trait UnicastMessageHandler: ParticipantAccessor {
    fn handle_rtps_message(&mut self, message_receiver: MessageReceiver) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        if message_receiver.has_dst_submessage() {
            let local_guid_prefix = participant.guid().prefix();
            if !message_receiver.is_dst_me(local_guid_prefix) {
                return Err(RtpsError::new(
                    RtpsErrorCode::InvalidDestinationGuid,
                    "INFO_DST is not me",
                ));
            }
        }

        let rtps_header = *message_receiver.rtps_message_header().unwrap();
        let submessages = message_receiver.parse_submessages();

        for submessage in submessages {
            match submessage {
                TypedSubmessage::Data(header, data) => {
                    self.handle_data_message(&rtps_header, &header, &data)?;
                }
                TypedSubmessage::Heartbeat(header, heartbeat) => {
                    self.handle_heartbeat_message(&rtps_header, &header, &heartbeat)?;
                }
                TypedSubmessage::AckNack(header, acknack) => {
                    self.handle_acknack_message(&rtps_header, &header, &acknack)?;
                }
                TypedSubmessage::DataFrag(header, data_frag) => {
                    self.handle_datafrag_message(&rtps_header, &header, &data_frag)?;
                }
                TypedSubmessage::NackFrag(header, nack_frag) => {
                    self.handle_nackfrag_message(&rtps_header, &header, &nack_frag)?;
                }
                TypedSubmessage::Gap(header, gap) => {
                    self.handle_gap_message(&rtps_header, &header, &gap)?;
                }
            }
        }

        Ok(())
    }

    fn handle_data_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        data: &Data,
    ) -> RtpsResult<()>;

    fn handle_heartbeat_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        heartbeat: &Heartbeat,
    ) -> RtpsResult<()>;

    fn handle_acknack_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        acknack: &AckNack,
    ) -> RtpsResult<()>;

    fn handle_datafrag_message(
        &mut self,
        _rtps_header: &Header,
        _submessage_header: &SubmessageHeader,
        _data_frag: &DataFrag,
    ) -> RtpsResult<()> {
        trace!("Unsupported message type: DataFrag");
        Ok(())
    }

    fn handle_nackfrag_message(
        &mut self,
        _rtps_header: &Header,
        _submessage_header: &SubmessageHeader,
        _nack_frag: &NackFrag,
    ) -> RtpsResult<()> {
        trace!("Unsupported message type: NackFrag");
        Ok(())
    }

    fn handle_gap_message(
        &mut self,
        _rtps_header: &Header,
        _submessage_header: &SubmessageHeader,
        _gap: &Gap,
    ) -> RtpsResult<()> {
        trace!("Unsupported message type: Gap");
        Ok(())
    }
}
