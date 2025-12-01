use chrono::Utc;
use std::{net::Ipv4Addr, str::FromStr, sync::Arc};

use crate::{
    rtps::{
        common::{
            entity_id::EntityId,
            locator::Locator,
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
        transport::{get_transport_type, port_manager::PortManager, TransportType},
    },
    serialize::pl_cdr::{discovery_helpers, RtpsMessageBuilder},
};

#[derive(Debug, Clone)]
pub(crate) struct SpdpMessage {
    rtps_message: Arc<RtpsMessage>,
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

    fn create_info_ts_submessage() -> Submessage {
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
    ) -> RtpsResult<Submessage> {
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

        data.add_serialized_data(Self::create_serialized_data(participant)?);
        let length = data.octets_to_next_header();

        let submessage_body: SubmessageBody = SubmessageBody::Data(data);

        let data_submessage = Submessage {
            header: SubmessageHeader::new(SubmessageId::DATA, data_header_flag.flag, length),
            body: submessage_body,
        };
        Ok(data_submessage)
    }

    fn create_serialized_data(participant: Arc<Participant>) -> RtpsResult<SerializedData> {
        let domain_id = participant.domain_id();
        let participant_guid = participant.guid();

        let (vendor_id, entity_name) = {
            let local_participant_proxy_data = participant.local_participant_proxy_data();
            (
                local_participant_proxy_data.vendor_id(),
                Some(local_participant_proxy_data.entity_name().to_string()),
            )
        };

        let mut locators = Vec::new();

        // Get transport type from environment variable
        let transport_type = get_transport_type();
        let participant_ip = Ipv4Addr::from_str(&participant.working_ip()).unwrap();

        // Create locators based on transport type
        match transport_type {
            TransportType::TCP => {
                // Use TCP locators for TCP transport
                let metatraffic = Locator::from_tcp_v4(
                    participant_ip,
                    PortManager::get_discovery_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );
                let default = Locator::from_tcp_v4(
                    participant_ip,
                    PortManager::get_user_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );
                locators.push((
                    ParameterId::PidMetatrafficUnicastLocator,
                    metatraffic.kind(),
                    metatraffic.port(),
                    metatraffic.address,
                ));
                locators.push((
                    ParameterId::PidDefaultUnicastLocator,
                    default.kind(),
                    default.port(),
                    default.address,
                ));
            }
            TransportType::UDP => {
                // Use UDP locators for UDP transport (default)
                let metatraffic = Locator::from_ip_v4_addr_and_port(
                    &participant_ip,
                    PortManager::get_discovery_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );
                let default = Locator::from_ip_v4_addr_and_port(
                    &participant_ip,
                    PortManager::get_user_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );
                locators.push((
                    ParameterId::PidMetatrafficUnicastLocator,
                    metatraffic.kind(),
                    metatraffic.port(),
                    metatraffic.address,
                ));
                locators.push((
                    ParameterId::PidDefaultUnicastLocator,
                    default.kind(),
                    default.port(),
                    default.address,
                ));
            }
            TransportType::Hybrid => {
                // Hybrid mode: Include BOTH UDP and TCP locators
                // UDP locators
                let udp_metatraffic = Locator::from_ip_v4_addr_and_port(
                    &participant_ip,
                    PortManager::get_discovery_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );
                let udp_default = Locator::from_ip_v4_addr_and_port(
                    &participant_ip,
                    PortManager::get_user_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );

                // TCP locators
                let tcp_metatraffic = Locator::from_tcp_v4(
                    participant_ip,
                    PortManager::get_discovery_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );
                let tcp_default = Locator::from_tcp_v4(
                    participant_ip,
                    PortManager::get_user_traffic_unicast_port(
                        participant.domain_id(),
                        participant.participant_id(),
                    ) as u32,
                );

                // Add all four locators (UDP + TCP)
                locators.push((
                    ParameterId::PidMetatrafficUnicastLocator,
                    udp_metatraffic.kind(),
                    udp_metatraffic.port(),
                    udp_metatraffic.address,
                ));
                locators.push((
                    ParameterId::PidDefaultUnicastLocator,
                    udp_default.kind(),
                    udp_default.port(),
                    udp_default.address,
                ));
                locators.push((
                    ParameterId::PidMetatrafficUnicastLocator,
                    tcp_metatraffic.kind(),
                    tcp_metatraffic.port(),
                    tcp_metatraffic.address,
                ));
                locators.push((
                    ParameterId::PidDefaultUnicastLocator,
                    tcp_default.kind(),
                    tcp_default.port(),
                    tcp_default.address,
                ));
            }
        }

        match discovery_helpers::create_spdp_participant_message(
            domain_id as u32,
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
                    domain_id as u32,
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

    pub(crate) fn rtps_message(&self) -> Arc<RtpsMessage> {
        self.rtps_message.clone()
    }
}
