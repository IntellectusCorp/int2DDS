//! RTPS message reception and processing.
//!
//! This module handles receiving RTPS messages from the network, parsing them,
//! and dispatching submessages to appropriate readers and writers for processing.

use super::{
    rtps_messages::RtpsMessage,
    submessages::info::{InfoDestination, InfoReply, InfoSource, InfoTimestamp},
};
use crate::{
    rtps::common::parameters::{ParameterValue, PlCdrParameter as Parameter},
    rtps::common::types::DomainId,
    rtps::{
        builtin::data::{
            builtin_endpoint_set::BuiltinEndpointSet,
            spdp_discovered_participant_data::SPDPDiscoveredParticipantData,
        },
        common::{
            entity_id::EntityId,
            guid::{GuidPrefix, GUIDPREFIX_UNKNOWN},
            locator::{
                Locator, LOCATOR_ADDRESS_INVALID, LOCATOR_INVALID, LOCATOR_KIND_UDP_V4,
                LOCATOR_PORT_INVALID,
            },
            parameters::{ParameterId, ParameterId as CommonParameterId, ParameterList},
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            time::{RtpsDuration, RtpsTime},
            types::{ProtocolVersion, VendorId, RTPS_HEADER_LENGTH, VENDORID_UNKNOWN},
        },
        messages::{
            header::Header,
            submessage::Submessage,
            submessage_body::SubmessageBody,
            submessage_data_participant::deserialize_submessage_participant_data,
            submessage_header::SubmessageHeader,
            submessages::{
                ack_nack::AckNack, data::Data, data_frag::DataFrag, gap::Gap, heartbeat::Heartbeat,
                info::InfoReplyIp4, nack_frag::NackFrag,
            },
        },
    },
    serialize::pl_cdr::parse_discovery_data,
};
use bytes::Bytes;
use core::net::SocketAddr;
use log::{debug, error, warn};
use speedy::Readable;
use std::sync::Arc;

/// Enum representing parsed RTPS submessage (including header and body)
#[derive(Debug)]
pub enum TypedSubmessage<'a> {
    Heartbeat(&'a SubmessageHeader, &'a Heartbeat),
    AckNack(&'a SubmessageHeader, &'a AckNack),
    Data(&'a SubmessageHeader, &'a Data<'static>),
    DataFrag(&'a SubmessageHeader, &'a DataFrag<'static>),
    NackFrag(&'a SubmessageHeader, &'a NackFrag),
    Gap(&'a SubmessageHeader, &'a Gap),
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct MessageReceiver {
    source_version: ProtocolVersion,
    source_vendor_id: VendorId,
    source_guid_prefix: GuidPrefix,
    dest_guid_prefix: GuidPrefix,
    participant_guid_prefix: GuidPrefix,
    unicast_reply_locator_list: Vec<Locator>,
    multicast_reply_locator_list: Vec<Locator>,
    have_timestamp: bool,
    timestamp: RtpsTime,
    rtps_message: Option<Arc<RtpsMessage<'static>>>,
    sender_addr: SocketAddr, // for extended discovery message
}

#[allow(dead_code)]
impl MessageReceiver {
    // 8.3.4 Initial state of Receiver
    pub(crate) fn new(participant_guid_prefix: GuidPrefix, from_addr: &SocketAddr) -> Self {
        Self {
            source_version: ProtocolVersion::PROTOCOLVERSION,
            source_vendor_id: VENDORID_UNKNOWN,
            source_guid_prefix: GUIDPREFIX_UNKNOWN,
            dest_guid_prefix: participant_guid_prefix,
            participant_guid_prefix,
            unicast_reply_locator_list: vec![Locator::from_ip(
                from_addr.ip(),
                LOCATOR_PORT_INVALID,
            )],
            multicast_reply_locator_list: vec![Locator::new(
                LOCATOR_KIND_UDP_V4,
                LOCATOR_PORT_INVALID,
                LOCATOR_ADDRESS_INVALID,
            )],
            have_timestamp: false,
            timestamp: RtpsTime::INVALID,
            rtps_message: None,
            sender_addr: *from_addr,
        }
    }

    pub(crate) fn get_source_timestamp(&self) -> Option<RtpsTime> {
        if self.have_timestamp {
            Some(self.timestamp)
        } else {
            None
        }
    }

    pub(crate) fn get_source_guid_prefix(&self) -> GuidPrefix {
        self.source_guid_prefix
    }

    // 8.3.6.4 Change in state of Receiver
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_header(mut self, header: Header) {
        self.source_guid_prefix = header.guid_prefix();
        self.source_version = header.version();
        self.source_vendor_id = header.vendor_id();
        self.have_timestamp = false;
    }

    // 8.3.8.8.4 Change in state of Receiver
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_destination(&mut self, info_destination: &InfoDestination) {
        // If there is a specified destination other than the current participant's GUID prefix, set the destination to that GUID prefix
        if info_destination.guid_prefix() != GUIDPREFIX_UNKNOWN {
            self.dest_guid_prefix = info_destination.guid_prefix()
        }
        // Restore the destination back to the current participant's GUID, which may have been changed by a previous InfoDestination
        else {
            self.dest_guid_prefix = self.participant_guid_prefix;
        }
    }

    // 8.3.8.9.4 Change in state of Receiver
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_reply(
        &mut self,
        info_reply_header: &SubmessageHeader,
        info_reply: &InfoReply,
    ) {
        self.unicast_reply_locator_list = info_reply.unicast_locator_list().to_vec();
        if let Some(true) = info_reply_header.multicast_flag() {
            self.multicast_reply_locator_list = info_reply.multicast_locator_list().to_vec();
        } else {
            self.multicast_reply_locator_list.clear();
        }
    }

    // 9.4.5.14
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_reply_ip4(
        &mut self,
        info_reply_ip4_header: &SubmessageHeader,
        info_reply_ip4: &InfoReplyIp4,
    ) {
        if let Some(endianness) = info_reply_ip4_header.endianness_flag() {
            self.unicast_reply_locator_list.clear();
            self.unicast_reply_locator_list
                .push(info_reply_ip4.unicast_locator().to_locator(endianness));
            self.multicast_reply_locator_list.clear();
            if let Some(locator) = info_reply_ip4.multicast_locator() {
                self.multicast_reply_locator_list.push(locator.to_locator(endianness));
            }
        }
    }

    // 8.3.8.10.4 Change in state of Receiver
    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_source(&mut self, info_source: &InfoSource) {
        self.source_guid_prefix = info_source.guid_prefix();
        self.source_version = info_source.protocol_version();
        self.source_vendor_id = info_source.vendor_id();
        self.unicast_reply_locator_list.clear();
        self.unicast_reply_locator_list.push(LOCATOR_INVALID);
        self.multicast_reply_locator_list.clear();
        self.multicast_reply_locator_list.push(LOCATOR_INVALID);
        self.have_timestamp = false
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_timestamp(
        &mut self,
        info_timestamp_header: &SubmessageHeader,
        info_timestamp: &InfoTimestamp,
    ) {
        match info_timestamp_header.invalidate_flag() {
            Some(false) => {
                self.have_timestamp = true;

                let seconds = info_timestamp.seconds();
                self.timestamp = if seconds >= 0 {
                    RtpsTime::new(seconds as u32, info_timestamp.fraction())
                } else {
                    RtpsTime::INVALID
                };
            }
            _ => {
                self.have_timestamp = false;
            }
        }
    }

    pub(crate) fn init(&mut self, buffer: &Bytes) -> RtpsResult<Arc<RtpsMessage<'static>>> {
        if buffer.len() < RTPS_HEADER_LENGTH as usize {
            return Err(RtpsError::new(
                RtpsErrorCode::BufferTooShortForRtpsHeader,
                format!("Buffer too short to contain RTPS header: {}", buffer.len()),
            ));
        }
        if &buffer[0..4] != b"RTPS" {
            return Err(RtpsError::new(RtpsErrorCode::InvalidRtpsMagic, None));
        }

        let header: Header = Header::read_from_buffer(buffer)
            .map_err(|e| RtpsError::new(RtpsErrorCode::InvalidRtpsHeader, e.to_string()))?;
        if !header.is_valid() {
            return Err(RtpsError::new(RtpsErrorCode::InvalidRtpsHeader, None));
        }

        let mut message = RtpsMessage::new(header);
        let mut submessages_buffer: Bytes = buffer.slice(RTPS_HEADER_LENGTH as usize..);

        // info!("#### header {:?}", message);

        // submessage loop
        while !submessages_buffer.is_empty() {
            if let Ok(Some(submessage)) =
                Submessage::read_from_buffer(self, &mut submessages_buffer)
            {
                // info!("#### submessage {:?}", submessage);
                message.submessages.push(submessage);
            }
        }

        self.rtps_message = Some(Arc::new(message));
        match &self.rtps_message {
            Some(rtps_message) => RtpsResult::Ok(Arc::clone(rtps_message)),
            None => RtpsResult::Err(RtpsError::new(RtpsErrorCode::InvalidRtpsHeader, None)),
        }
    }

    pub(crate) fn extract_participant_proxy_data(
        &self,
        domain_id: DomainId,
    ) -> Option<(SPDPDiscoveredParticipantData, Option<ParameterList>)> {
        let log_on = false;
        if log_on {
            debug!("Starting to extract participant proxy data for domain_id: {}", domain_id);
        }

        match &self.rtps_message {
            Some(rtps_message) => {
                let header = rtps_message.header;
                if log_on {
                    debug!("RTPS message header: {:?}", header);
                }

                let mut spdp_discovered_participant_data = SPDPDiscoveredParticipantData::new(
                    domain_id,
                    header.guid_prefix(),
                    BuiltinEndpointSet::default(),
                );

                let submessages = &rtps_message.submessages;
                if log_on {
                    debug!("Processing {} submessages", submessages.len());
                }

                let mut inline_qos_params: Option<ParameterList> = None;

                for (index, submessage) in submessages.iter().enumerate() {
                    if log_on {
                        debug!("Processing submessage {}: {:?}", index, submessage.header);
                    }

                    match &submessage.body {
                        SubmessageBody::Data(data) => {
                            inline_qos_params = data.inline_qos();

                            if log_on {
                                debug!(
                                    "Found Data submessage, serialized data length: {} bytes",
                                    data.serialized_data().len()
                                );
                            }

                            match parse_discovery_data(&data.serialized_data()[..]) {
                                Ok(parameters) => {
                                    if log_on {
                                        debug!(
                                            "Successfully parsed {} parameters using PL-CDR",
                                            parameters.len()
                                        );
                                    }

                                    // Log parsed parameters
                                    for (param_index, parameter) in parameters.iter().enumerate() {
                                        if log_on {
                                            debug!(
                                                "Parameter {}: ID=0x{:04X}, Value={:?}",
                                                param_index, parameter.id as u16, parameter.value
                                            );
                                        }
                                    }

                                    self.process_discovery_parameters(
                                        &parameters,
                                        &mut spdp_discovered_participant_data,
                                    );
                                }
                                Err(e) => {
                                    error!("Failed to parse discovery data with pl_cdr: {}", e);
                                    error!("Falling back to legacy parser");

                                    self.fallback_parse_data(
                                        data,
                                        &mut spdp_discovered_participant_data,
                                    );
                                }
                            }
                        }
                        SubmessageBody::InfoTimestamp(info_timestamp) => {
                            if log_on {
                                debug!("Found InfoTimestamp submessage: {:?}", info_timestamp);
                            }
                        }
                        _ => {
                            if log_on {
                                debug!("Found other submessage type: {:?}", submessage.body);
                            }
                        }
                    }
                }

                if log_on {
                    debug!("Successfully extracted participant proxy data");
                    debug!(
                        "Final SPDP data: domain_id={}, guid_prefix={:?}",
                        domain_id,
                        header.guid_prefix()
                    );
                }

                Some((spdp_discovered_participant_data, inline_qos_params))
            }
            None => {
                if log_on {
                    error!("No RTPS message available for participant proxy data extraction");
                }
                None
            }
        }
    }

    pub(crate) fn timestamp(&self) -> Option<RtpsTime> {
        if self.have_timestamp {
            Some(self.timestamp)
        } else {
            None
        }
    }

    pub(crate) fn source_guid_prefix(&self) -> GuidPrefix {
        self.source_guid_prefix
    }

    pub(crate) fn rtps_message_header(&self) -> Option<&Header> {
        self.rtps_message.as_ref().map(|msg| &msg.header)
    }

    pub(crate) fn is_dst_me(&self, guid_prefix: GuidPrefix) -> bool {
        // self.dest_guid_prefix is set from InfoDestination submessage,
        // check from_destination()
        self.dest_guid_prefix == guid_prefix
    }

    fn process_discovery_parameters(
        &self,
        parameters: &[Parameter],
        spdp_data: &mut SPDPDiscoveredParticipantData,
    ) {
        let log_on = false;
        if log_on {
            debug!("Processing {} discovery parameters", parameters.len());
        }

        for (index, parameter) in parameters.iter().enumerate() {
            match &parameter.value {
                ParameterValue::Locator(locator) => {
                    let rtps_locator = self.convert_locator(locator);
                    match parameter.id {
                        CommonParameterId::PidMetatrafficUnicastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding metatraffic unicast locator: {:?}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_metatraffic_unicast_locator(rtps_locator);
                        }
                        CommonParameterId::PidMetatrafficMulticastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding metatraffic multicast locator: {:?}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_metatraffic_multicast_locator(rtps_locator);
                        }
                        CommonParameterId::PidDefaultUnicastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding default unicast locator: {:?}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_default_unicast_locator(rtps_locator);
                        }
                        CommonParameterId::PidDefaultMulticastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding default multicast locator: {:?}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_default_multicast_locator(rtps_locator);
                        }
                        _ => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Unhandled locator parameter ID: 0x{:04X}",
                                    index, parameter.id as u16
                                );
                            }
                        }
                    }
                }
                ParameterValue::VendorId(vendor_id) => {
                    if log_on {
                        debug!(
                            "Parameter {}: Setting vendor ID: {:02X}{:02X}",
                            index, vendor_id[0], vendor_id[1]
                        );
                    }
                    spdp_data.set_vendor_id(*vendor_id);
                }
                ParameterValue::ProtocolVersion(protocol_version) => {
                    if log_on {
                        debug!(
                            "Parameter {}: Setting protocol version: {}.{}",
                            index, protocol_version.major, protocol_version.minor
                        );
                    }
                    spdp_data.set_protocol_version(*protocol_version);
                }
                ParameterValue::DomainId(domain_id) => {
                    if log_on {
                        debug!("Parameter {}: Setting domain ID: {}", index, domain_id);
                    }
                    spdp_data.set_domain_id(*domain_id);
                }
                ParameterValue::ParticipantGuid(guid) => {
                    // let guid = self.convert_guid_prefix_to_guid(guid_prefix);
                    if log_on {
                        debug!("Parameter {}: Setting participant GUID: {:?}", index, guid);
                    }
                    spdp_data.set_participant_guid(*guid);
                }
                ParameterValue::BuiltinEndpointSet(endpoint_set) => {
                    if log_on {
                        debug!(
                            "Parameter {}: Setting builtin endpoint set: 0x{:08X}",
                            index, endpoint_set
                        );
                    }
                    spdp_data.set_available_builtin_endpoints(BuiltinEndpointSet::from_bits(
                        *endpoint_set,
                    ));
                }
                ParameterValue::ParticipantLeaseDuration(duration) => {
                    if log_on {
                        debug!(
                            "Parameter {}: Setting lease duration: {}s + {} fraction",
                            index,
                            duration.seconds(),
                            duration.fraction()
                        );
                    }
                    spdp_data.set_lease_duration(self.convert_duration(duration));
                }
                ParameterValue::EntityName(name) => {
                    if log_on {
                        debug!("Parameter {}: Entity name: '{}'", index, name);
                    }
                    spdp_data.set_entity_name(name.clone());
                }
                ParameterValue::PropertyList(properties) => {
                    if log_on {
                        debug!(
                            "Parameter {}: Property list with {} properties:",
                            index,
                            properties.len()
                        );
                    }
                    for (prop_index, property) in properties.iter().enumerate() {
                        if log_on {
                            debug!(
                                "  Property {}: {} = {}",
                                prop_index, property.name, property.value
                            );
                        }
                    }
                }
                ParameterValue::UserData(data) => {
                    if log_on {
                        debug!("Parameter {}: User data ({} bytes)", index, data.len());
                    }
                }
                ParameterValue::TopicData(data) => {
                    if log_on {
                        debug!("Parameter {}: Topic data ({} bytes)", index, data.len());
                    }
                }
                ParameterValue::GroupData(data) => {
                    if log_on {
                        debug!("Parameter {}: Group data ({} bytes)", index, data.len());
                    }
                }
                ParameterValue::Unknown(data) => {
                    if log_on {
                        debug!(
                            "Parameter {}: Unknown parameter ID 0x{:04X} ({} bytes)",
                            index,
                            parameter.id as u16,
                            data.len()
                        );
                    }
                }
                _ => {
                    if log_on {
                        debug!(
                            "Parameter {}: Unprocessed parameter type: {:?}",
                            index, parameter.value
                        );
                    }
                }
            }
        }

        if log_on {
            debug!("Finished processing discovery parameters");
        }
    }

    fn fallback_parse_data(&self, data: &Data, spdp_data: &mut SPDPDiscoveredParticipantData) {
        // Use existing deserialize_submessage_participant_data logic
        let deserialized_data = deserialize_submessage_participant_data(&data.serialized_data());
        match deserialized_data {
            Ok((parameter_list, _)) => {
                for parameter in parameter_list.parameters() {
                    if parameter.parameter_id() == ParameterId::PidMetatrafficUnicastLocator {
                        match Locator::read_from_buffer(parameter.value()) {
                            Ok(locator) => {
                                spdp_data.add_metatraffic_unicast_locator(locator);
                            }
                            Err(e) => {
                                error!(
                                    "Failed to deserialize metatraffic unicast locator: {:?}",
                                    e
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                error!("Failed to deserialize submessage participant data: {:?}", e);
            }
        }
    }

    fn convert_locator(&self, common_locator: &Locator) -> Locator {
        Locator::new(common_locator.kind(), common_locator.port(), common_locator.address)
    }

    fn convert_duration(&self, common_duration: &RtpsDuration) -> RtpsDuration {
        RtpsDuration::new(common_duration.seconds(), common_duration.fraction())
    }
    pub(crate) fn has_dst_submessage(&self) -> bool {
        self.has_submessage_type(|body| matches!(body, SubmessageBody::InfoDestination(_)))
    }

    fn has_submessage_type<F>(&self, predicate: F) -> bool
    where
        F: Fn(&SubmessageBody) -> bool,
    {
        if let Some(rtps_message) = &self.rtps_message {
            for submessage in &rtps_message.submessages {
                if predicate(&submessage.body) {
                    return true;
                }
            }
        }
        false
    }

    pub(crate) fn payload_from_data(
        &self,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
    ) -> Option<Arc<[u8]>> {
        if let Some(rtps_message) = &self.rtps_message {
            let mut visited: bool = false;
            let mut payload: Option<Arc<[u8]>> = None;
            for submessage in &rtps_message.submessages {
                if let SubmessageBody::Data(data) = &submessage.body {
                    if data.reader_id == reader_entity_id && data.writer_id == writer_entity_id {
                        if visited {
                            warn!("payload_from_data: More than 2 Submessage Data");
                        }
                        visited = true;
                        payload = Some(data.serialized_data());
                    }
                }
            }
            return payload;
        }
        None
    }

    pub(crate) fn parse_submessages(&self) -> Vec<TypedSubmessage<'_>> {
        let mut submessages: Vec<TypedSubmessage> = if let Some(rtps_message) = &self.rtps_message {
            Vec::with_capacity(rtps_message.submessages.len())
        } else {
            Vec::new()
        };
        if let Some(rtps_message) = &self.rtps_message {
            for submessage in &rtps_message.submessages {
                match &submessage.body {
                    SubmessageBody::Heartbeat(hb) => {
                        submessages.push(TypedSubmessage::Heartbeat(&submessage.header, hb));
                    }
                    SubmessageBody::AckNack(an) => {
                        submessages.push(TypedSubmessage::AckNack(&submessage.header, an));
                    }
                    SubmessageBody::Data(data) => {
                        submessages.push(TypedSubmessage::Data(&submessage.header, data));
                    }
                    SubmessageBody::DataFrag(data_frag) => {
                        submessages.push(TypedSubmessage::DataFrag(&submessage.header, data_frag));
                    }
                    SubmessageBody::NackFrag(nack_frag) => {
                        submessages.push(TypedSubmessage::NackFrag(&submessage.header, nack_frag));
                    }
                    SubmessageBody::Gap(gap) => {
                        submessages.push(TypedSubmessage::Gap(&submessage.header, gap));
                    }
                    _ => continue, // Ignore other submessage types
                }
            }
        }
        submessages
    }
}

#[cfg(test)]
mod tests {}
