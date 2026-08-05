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
            guid::{Guid, GuidPrefix, GUIDPREFIX_UNKNOWN},
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
                heartbeat_frag::HeartbeatFrag, info::InfoReplyIp4, nack_frag::NackFrag,
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
    HeartbeatFrag(&'a SubmessageHeader, &'a HeartbeatFrag),
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

    /// Drops the timestamp the receiver is holding, for an INFO_TS that sets the
    /// InvalidateFlag and therefore carries no timestamp field of its own.
    pub(crate) fn invalidate_timestamp(&mut self) {
        self.have_timestamp = false;
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

        // Submessage loop. Submessages are located only by walking the submessageLength
        // chain, so a submessage that cannot be framed also hides where the next one starts.
        // Such a message is invalid from that point on, but the submessages already parsed
        // keep their effect.
        while !submessages_buffer.is_empty() {
            let remaining_before = submessages_buffer.len();

            if let Ok(Some(submessage)) =
                Submessage::read_from_buffer(self, &mut submessages_buffer)
            {
                // info!("#### submessage {:?}", submessage);
                message.submessages.push(submessage);
            }

            // `read_from_buffer` advances the buffer only once it has framed a submessage.
            // Both framing failures - a header shorter than four bytes, and a declared
            // submessageLength larger than the bytes remaining - return before that point
            // and consume nothing, which would leave this loop condition unchanged forever.
            if submessages_buffer.len() >= remaining_before {
                debug!(
                    "Malformed submessage framing from {}: discarding the trailing {} byte(s)",
                    self.sender_addr, remaining_before
                );
                break;
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
                        debug!("Processing submessage {}: {}", index, submessage.header);
                    }

                    match &submessage.body {
                        SubmessageBody::Data(data) => {
                            if data.writer_id != EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER {
                                continue;
                            }

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
                        "Final SPDP data: domain_id={}, guid_prefix={}",
                        domain_id,
                        Guid::guid_prefix_to_string(&header.guid_prefix())
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
                                    "Parameter {}: Adding metatraffic unicast locator: {}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_metatraffic_unicast_locator(rtps_locator);
                        }
                        CommonParameterId::PidMetatrafficMulticastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding metatraffic multicast locator: {}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_metatraffic_multicast_locator(rtps_locator);
                        }
                        CommonParameterId::PidDefaultUnicastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding default unicast locator: {}",
                                    index, rtps_locator
                                );
                            }
                            spdp_data.add_default_unicast_locator(rtps_locator);
                        }
                        CommonParameterId::PidDefaultMulticastLocator => {
                            if log_on {
                                debug!(
                                    "Parameter {}: Adding default multicast locator: {}",
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
                        debug!("Parameter {}: Setting participant GUID: {}", index, guid);
                    }
                    spdp_data.set_participant_guid(*guid);
                }
                ParameterValue::EndpointGuid(guid) => {
                    warn!(
                        "Parameter {}: PID_ENDPOINT_GUID encountered on SPDP path, ignoring: {}",
                        index, guid
                    );
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
    ) -> Option<bytes::Bytes> {
        if let Some(rtps_message) = &self.rtps_message {
            let mut visited: bool = false;
            let mut payload: Option<bytes::Bytes> = None;
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
                    SubmessageBody::HeartbeatFrag(heartbeat_frag) => {
                        submessages.push(TypedSubmessage::HeartbeatFrag(
                            &submessage.header,
                            heartbeat_frag,
                        ));
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
mod tests {
    use super::*;
    use std::sync::mpsc::{sync_channel, RecvTimeoutError};
    use std::time::Duration;

    /// A valid 20-byte RTPS header. `Header::is_valid` requires the "RTPS" magic and
    /// a major protocol version of at most 2.
    const HDR: [u8; 20] = [
        0x52, 0x54, 0x50, 0x53, // protocol  = "RTPS"
        0x02, 0x03, // version   = 2.3
        0x01, 0x03, // vendorId
        0x01, 0x02, 0x03, 0x04, // guidPrefix[0..4]
        0x05, 0x06, 0x07, 0x08, // guidPrefix[4..8]
        0x09, 0x0A, 0x0B, 0x0C, // guidPrefix[8..12]
    ];

    /// A well-formed little-endian INFO_DST. Its body is exactly a 12-byte GuidPrefix,
    /// so it needs no other submessage deserializer to be correct.
    const INFO_DST: [u8; 16] = [
        0x0E, // submessageId       = INFO_DST
        0x01, // flags              = E (little-endian)
        0x0C, 0x00, // octetsToNextHeader = 12, little-endian
        0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, // guidPrefix[0..6]
        0x11, 0x22, 0x33, 0x44, 0x55, 0x66, // guidPrefix[6..12]
    ];

    const TIMEOUT: Duration = Duration::from_secs(2);

    fn datagram(parts: &[&[u8]]) -> Vec<u8> {
        let mut v = HDR.to_vec();
        for p in parts {
            v.extend_from_slice(p);
        }
        v
    }

    /// Runs `MessageReceiver::init` on a worker thread and fails if it does not return
    /// within `TIMEOUT`. Returns the submessage ids that were parsed, or `None` if `init`
    /// rejected the datagram outright.
    ///
    /// A runaway thread cannot be killed in Rust: if `init` spins, the worker keeps burning
    /// one core until the test binary exits. That is the accepted cost of being able to
    /// report a single failure instead of wedging the whole `cargo test` run.
    fn init_ids_within(datagram: Vec<u8>) -> Option<Vec<u8>> {
        let (tx, rx) = sync_channel::<Option<Vec<u8>>>(1);
        std::thread::Builder::new()
            .name("rtps-init-under-test".into())
            .spawn(move || {
                let addr: SocketAddr = "127.0.0.1:7400".parse().unwrap();
                let mut receiver = MessageReceiver::new(GUIDPREFIX_UNKNOWN, &addr);
                let ids = receiver.init(&Bytes::from(datagram)).ok().map(|message| {
                    message.submessages.iter().map(|s| s.header.submessage_id().as_u8()).collect()
                });
                let _ = tx.send(ids);
            })
            .expect("failed to spawn worker thread");

        match rx.recv_timeout(TIMEOUT) {
            Ok(ids) => ids,
            Err(RecvTimeoutError::Timeout) => panic!(
                "MessageReceiver::init did not return within {TIMEOUT:?}: \
                 the submessage parsing loop never terminated"
            ),
            Err(RecvTimeoutError::Disconnected) => panic!("MessageReceiver::init panicked"),
        }
    }

    /// The parsing loop can only terminate if every `read_from_buffer` call either consumes
    /// bytes or makes the caller stop. This pins the half of that contract that lives in
    /// `read_from_buffer`: on a framing error it consumes nothing, which is exactly why the
    /// caller has to detect the lack of progress. A single call, so it cannot hang.
    #[test]
    fn framing_error_consumes_nothing_so_caller_must_stop() {
        let cases: [(&str, Vec<u8>); 4] = [
            ("1 trailing byte", vec![0x0E]),
            ("2 trailing bytes", vec![0x0E, 0x01]),
            ("3 trailing bytes", vec![0x0E, 0x01, 0x0C]),
            // HEARTBEAT declaring 65535 body bytes with none following.
            ("declared length overruns", vec![0x07, 0x01, 0xFF, 0xFF]),
        ];

        for (name, bytes) in cases {
            let addr: SocketAddr = "127.0.0.1:7400".parse().unwrap();
            let mut receiver = MessageReceiver::new(GUIDPREFIX_UNKNOWN, &addr);
            let mut buffer = Bytes::from(bytes);
            let before = buffer.len();

            let result = Submessage::read_from_buffer(&mut receiver, &mut buffer);

            assert!(result.is_err(), "{name}: expected a framing error");
            assert_eq!(
                buffer.len(),
                before,
                "{name}: read_from_buffer consumed bytes on a framing error. The progress \
                 check in MessageReceiver::init relies on it not doing so; if this changed \
                 deliberately, revisit that check."
            );
        }
    }

    /// One to three bytes left over is too few for a 4-byte submessage header. The valid
    /// submessages that preceded them must still be reported.
    #[test]
    fn trailing_bytes_do_not_hang() {
        for n in 1..=3usize {
            let ids = init_ids_within(datagram(&[&INFO_DST, &vec![0xAB; n]]));
            assert_eq!(
                ids,
                Some(vec![0x0E]),
                "{n} trailing byte(s): the preceding INFO_DST must survive"
            );
        }
    }

    /// A submessage whose declared `octetsToNextHeader` exceeds the bytes actually present
    /// cannot be framed, and neither can anything after it.
    #[test]
    fn declared_length_overrunning_buffer_does_not_hang() {
        // HEARTBEAT, E=1, octetsToNextHeader = 0xFFFF, no body bytes follow.
        let ids = init_ids_within(datagram(&[&[0x07, 0x01, 0xFF, 0xFF]]));
        assert_eq!(ids, Some(vec![]), "no submessage in this datagram is parseable");

        // Same, but preceded by a valid submessage: the prefix must be kept.
        let ids = init_ids_within(datagram(&[&INFO_DST, &[0x15, 0x01, 0x00, 0x10]]));
        assert_eq!(
            ids,
            Some(vec![0x0E]),
            "a DATA claiming 4096 body bytes with none present must not discard the INFO_DST"
        );
    }

    #[test]
    fn garbage_tail_after_valid_submessages_does_not_hang() {
        let ids = init_ids_within(datagram(&[&INFO_DST, &INFO_DST, &[0x00, 0x00]]));
        assert_eq!(ids, Some(vec![0x0E, 0x0E]));
    }

    #[test]
    fn wellformed_multi_submessage_datagram_parses_fully() {
        let ids = init_ids_within(datagram(&[&INFO_DST, &INFO_DST, &INFO_DST]));
        assert_eq!(ids, Some(vec![0x0E, 0x0E, 0x0E]));
    }

    /// `octetsToNextHeader == 0` on the last submessage means it extends to the end of the
    /// message. This is how submessages larger than 64 KiB are sent, and the parser handles
    /// it deliberately for OpenDDS interoperability. Pins the case that a naive "reject a
    /// declared length of zero" fix would destroy.
    #[test]
    fn zero_length_last_submessage_extends_to_end_of_message() {
        let mut tail = vec![0x0E, 0x01, 0x00, 0x00]; // INFO_DST, E=1, octetsToNextHeader = 0
        tail.extend_from_slice(&INFO_DST[4..]); // followed by its 12-byte GuidPrefix
        let ids = init_ids_within(datagram(&[&tail]));
        assert_eq!(ids, Some(vec![0x0E]));
    }

    /// A submessage id this implementation does not know must be skipped using its declared
    /// length, and parsing must continue with the next one. Vendor-specific ids in the
    /// 0x80..=0xFF range rely on this.
    #[test]
    fn vendor_specific_submessage_is_skipped_and_parsing_continues() {
        let vendor = [0x80, 0x01, 0x04, 0x00, 0xDE, 0xAD, 0xBE, 0xEF];
        let ids = init_ids_within(datagram(&[&vendor, &INFO_DST]));
        assert_eq!(ids, Some(vec![0x0E]), "the INFO_DST after the vendor submessage is lost");
    }

    /// `octetsToNextHeader` is encoded in the endianness announced by the EndiannessFlag of
    /// the very same header, so a peer that clears that flag sends it big-endian. Reading it
    /// little-endian regardless turns a length of 12 into 3072 and loses the submessage.
    #[test]
    fn big_endian_submessage_length_is_honoured() {
        let mut big_endian = vec![0x0E, 0x00, 0x00, 0x0C]; // INFO_DST, E=0, length 12 big-endian
        big_endian.extend_from_slice(&INFO_DST[4..]); // followed by its 12-byte GuidPrefix
        let ids = init_ids_within(datagram(&[&big_endian]));
        assert_eq!(ids, Some(vec![0x0E]));
    }

    /// A big-endian submessage must not swallow whatever follows it either.
    #[test]
    fn big_endian_submessage_does_not_hide_the_next_one() {
        let mut big_endian = vec![0x0E, 0x00, 0x00, 0x0C];
        big_endian.extend_from_slice(&INFO_DST[4..]);
        let ids = init_ids_within(datagram(&[&big_endian, &INFO_DST]));
        assert_eq!(ids, Some(vec![0x0E, 0x0E]));
    }

    /// PAD is one of the two submessages for which `octetsToNextHeader == 0` means an empty
    /// body rather than "extends to the end of the message", so the next submessage header
    /// follows it immediately.
    #[test]
    fn zero_length_pad_does_not_swallow_the_rest() {
        // PAD, E=1, octetsToNextHeader = 0
        let ids = init_ids_within(datagram(&[&[0x01, 0x01, 0x00, 0x00], &INFO_DST]));
        assert_eq!(ids, Some(vec![0x01, 0x0E]));
    }

    /// INFO_TS is the other one: with the InvalidateFlag set it carries no timestamp field,
    /// so a declared length of zero is legal and it is not the last submessage. Reading eight
    /// bytes of whatever follows as a timestamp both invents a timestamp and loses a
    /// submessage.
    #[test]
    fn info_ts_with_invalidate_flag_does_not_swallow_the_rest() {
        // INFO_TS, E=1 with the InvalidateFlag set, octetsToNextHeader = 0
        let ids = init_ids_within(datagram(&[&[0x09, 0x03, 0x00, 0x00], &INFO_DST]));
        assert_eq!(ids, Some(vec![0x0E]), "an invalidated INFO_TS carries no body of its own");
    }

    /// An invalidated INFO_TS must also clear any timestamp a previous one established,
    /// rather than leaving the receiver holding a stale one.
    #[test]
    fn info_ts_with_invalidate_flag_clears_the_source_timestamp() {
        let addr: SocketAddr = "127.0.0.1:7400".parse().unwrap();
        let mut receiver = MessageReceiver::new(GUIDPREFIX_UNKNOWN, &addr);

        // INFO_TS carrying a timestamp, then INFO_TS with the InvalidateFlag set.
        let timestamped = [0x09, 0x01, 0x08, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let invalidated = [0x09, 0x03, 0x00, 0x00];
        let bytes = Bytes::from(datagram(&[&timestamped, &invalidated]));

        receiver.init(&bytes).expect("the datagram is well formed");

        assert_eq!(receiver.get_source_timestamp(), None);
    }
}
