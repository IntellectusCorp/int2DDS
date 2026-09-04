use log::trace;

use crate::rtps::{
    common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
    entities::entity::Entity as _,
    logic::common::ParticipantAccessor,
    messages::{
        header::Header,
        message_receiver::{MessageReceiver, TypedSubmessage},
        submessages::{data::Data, data_frag::DataFrag},
    },
};

/// Receive path for datagrams read from a user data multicast socket.
///
/// A group datagram names no destination: it carries no INFO_DST and an unknown
/// readerId, so the matched readers cannot be narrowed from its contents. The
/// group it arrived on is what narrows them, and it is known only because the
/// socket it was read from is joined to that one group.
pub(crate) trait MulticastMessageProcessor: ParticipantAccessor {
    fn handle_multicast_rtps_message(
        &mut self,
        message_receiver: MessageReceiver,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        // Other vendors may still address a group datagram to one participant.
        if message_receiver.has_dst_submessage()
            && !message_receiver.is_dst_me(participant.guid().prefix())
        {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidDestinationGuid,
                "INFO_DST is not me",
            ));
        }

        let rtps_header = *message_receiver.rtps_message_header().unwrap();
        let submessages = message_receiver.parse_submessages();

        for submessage in submessages {
            match submessage {
                TypedSubmessage::Data(_header, data) => {
                    self.handle_multicast_data_message(&rtps_header, data, &message_receiver)?;
                }
                TypedSubmessage::DataFrag(_header, data_frag) => {
                    self.handle_multicast_datafrag_message(
                        &rtps_header,
                        data_frag,
                        &message_receiver,
                    )?;
                }
                // Samples are the only thing ever multicast. Everything else
                // belongs to a conversation with one endpoint, and honouring it
                // here would let an unaddressed datagram drive that state.
                _ => trace!("[UserMulticast] Ignoring a non-sample submessage on a group socket"),
            }
        }

        Ok(())
    }

    fn handle_multicast_data_message(
        &mut self,
        rtps_header: &Header,
        data: &Data,
        message_receiver: &MessageReceiver,
    ) -> RtpsResult<()>;

    fn handle_multicast_datafrag_message(
        &mut self,
        rtps_header: &Header,
        data_frag: &DataFrag,
        message_receiver: &MessageReceiver,
    ) -> RtpsResult<()>;
}
