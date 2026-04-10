use chrono::Utc;
use std::sync::Arc;

use crate::{
    rtps::{
        common::{
            entity_id::EntityId,
            parameters::{ParameterId, ParameterList},
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            types::*,
        },
        entities::{entity::Entity, participant::Participant},
        messages::{
            header::Header,
            rtps_messages::RtpsMessage,
            submessage::Submessage,
            submessage_body::SubmessageBody,
            submessage_header::SubmessageHeader,
            submessage_header_flag::{SubmessageFlagType, SubmessageHeaderFlag},
            submessage_id::SubmessageId,
            submessages::{data::Data, info::InfoTimestamp},
        },
    },
    serialize::pl_cdr::{discovery_helpers, RtpsMessageBuilder},
};

#[derive(Debug, Clone)]
pub(crate) struct SpdpMessage {
    rtps_message: Arc<RtpsMessage<'static>>,
}

impl SpdpMessage {
    pub(crate) fn new(
        participant: Arc<Participant>,
        inline_qos_list: Option<ParameterList>,
    ) -> RtpsResult<Self> {
        let participant_guid = participant.guid();

        let mut rtps_message = RtpsMessage::new(Header::new(participant_guid.prefix()));

        rtps_message.add_submessage(Self::create_info_ts_submessage());
        rtps_message.add_submessage(Self::create_data_submessage(
            participant,
            SequenceNumber::new(0, 1),
            inline_qos_list,
        )?);

        Ok(Self { rtps_message: Arc::new(rtps_message) })
    }

    fn create_info_ts_submessage() -> Submessage<'static> {
        let mut info_ts_header_flag = SubmessageHeaderFlag::new();
        info_ts_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::INFO_TS);
        let info_ts_data = InfoTimestamp::new(Utc::now());

        Submessage {
            header: SubmessageHeader::new(
                SubmessageId::INFO_TS,
                info_ts_header_flag.flag,
                info_ts_data.length(),
            ),
            body: SubmessageBody::InfoTimestamp(info_ts_data),
        }
    }

    fn create_data_submessage(
        participant: Arc<Participant>,
        sequence_number: SequenceNumber,
        inline_qos_list: Option<ParameterList>,
    ) -> RtpsResult<Submessage<'static>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::DATA);
        data_header_flag.add_flag(SubmessageFlagType::DataFlag, SubmessageId::DATA);

        let mut data = Data::new(
            EntityId::SPDP_BUILTIN_PARTICIPANT_READER,
            EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
            sequence_number,
        );

        if let Some(inline_qos_list) = inline_qos_list {
            data_header_flag.add_flag(SubmessageFlagType::InlineQosFlag, SubmessageId::DATA);
            data.set_inline_qos_list(inline_qos_list);
        }

        data.add_serialized_data(SubmessagePayload::Owned(Self::create_serialized_data(
            participant,
        )?));
        let length = data.octets_to_next_header();

        let submessage_body: SubmessageBody<'static> = SubmessageBody::Data(data);

        let data_submessage = Submessage {
            header: SubmessageHeader::new(SubmessageId::DATA, data_header_flag.flag, length),
            body: submessage_body,
        };
        Ok(data_submessage)
    }

    fn create_serialized_data(participant: Arc<Participant>) -> RtpsResult<SerializedData> {
        let domain_id = participant.domain_id();
        let participant_guid = participant.guid();
        let local_participant_proxy_data = participant.local_participant_proxy_data();

        let vendor_id = local_participant_proxy_data.vendor_id();
        let entity_name = Some(local_participant_proxy_data.entity_name().to_string());

        // Use pre-computed locators from local_participant_proxy_data
        let mut locators = Vec::with_capacity(
            local_participant_proxy_data.metatraffic_unicast_locator_list().len()
                + local_participant_proxy_data.default_unicast_locator_list().len(),
        );

        for locator in local_participant_proxy_data.metatraffic_unicast_locator_list() {
            locators.push((
                ParameterId::PidMetatrafficUnicastLocator,
                locator.kind(),
                locator.port(),
                locator.address,
            ));
        }

        for locator in local_participant_proxy_data.default_unicast_locator_list() {
            locators.push((
                ParameterId::PidDefaultUnicastLocator,
                locator.kind(),
                locator.port(),
                locator.address,
            ));
        }

        match discovery_helpers::create_spdp_participant_message(
            domain_id,
            participant_guid,
            vendor_id,
            entity_name,
            locators,
        ) {
            Ok(bytes) => Ok(Arc::from(bytes)),
            Err(e) => {
                log::error!("Error creating SPDP serialized data: {}", e);

                // Fallback to simple builder if discovery_helpers fails
                match RtpsMessageBuilder::build_spdp_participant_data(
                    domain_id,
                    participant_guid,
                    vendor_id,
                    true,
                ) {
                    Ok(bytes) => Ok(Arc::from(bytes)),
                    Err(fallback_err) => Err(RtpsError::new(
                        RtpsErrorCode::SerializationError,
                        format!("Fallback SPDP creation also failed: {}", fallback_err),
                    )),
                }
            }
        }
    }

    pub(crate) fn rtps_message(&self) -> Arc<RtpsMessage<'static>> {
        self.rtps_message.clone()
    }
}
