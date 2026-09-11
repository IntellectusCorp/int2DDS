//! PL-CDR (Parameter List CDR) deserialization for discovery and QoS.
//!
//! This module provides PL-CDR deserialization for encoding QoS policies and discovery data
//! as parameter lists. PL-CDR is extensively used in DDS builtin topic data exchange.

use log::{debug, warn};
use speedy::Endianness;

use crate::{
    infrastructure::qos_policy::{
        DataRepresentationId, DataRepresentationQosPolicy, DestinationOrderQosPolicy,
        DestinationOrderQosPolicyKind, DurabilityQosPolicy, DurabilityQosPolicyKind,
        DurabilityServiceQosPolicy, HistoryQosPolicy, HistoryQosPolicyKind, LivelinessQosPolicy,
        LivelinessQosPolicyKind, OwnershipQosPolicy, OwnershipQosPolicyKind,
        PresentationQosAccessScopeKind, PresentationQosPolicy, ReliabilityQosPolicy,
        ReliabilityQosPolicyKind, ResourceLimitsQosPolicy, TypeConsistencyEnforcementQosPolicy,
        TypeConsistencyKind,
    },
    rtps::{
        builtin::data::content_filtered_topic::ContentFilterProperty,
        common::{
            guid::Guid,
            locator::Locator,
            parameters::{
                u16_to_parameter_id, ParameterId, ParameterValue, PlCdrParameter, Property,
            },
            time::RtpsDuration,
            types::{GroupInfo, ProtocolVersion},
        },
    },
    xtypes::{TypeIdentifier, TypeObject},
};

use super::{reader::PlCdrReader, MAX_PARAMETER_ITERATIONS};

/// Returns true if `data` begins with a recognized CDR/XCDR encapsulation header
/// (the 2-byte big-endian encoding identifier). Used to distinguish standard
/// XTypes PID payloads from legacy headerless int2DDS bodies.
fn has_encapsulation_header(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    matches!(
        u16::from_be_bytes([data[0], data[1]]),
        0x0000 | 0x0001 | 0x0002 | 0x0003 | 0x0006 | 0x0007 | 0x0008 | 0x0009 | 0x000A | 0x000B
    )
}

pub struct PlCdrParser {
    endianness: Endianness,
}

impl PlCdrParser {
    pub fn new(big_endian: bool) -> Self {
        Self {
            endianness: if big_endian { Endianness::BigEndian } else { Endianness::LittleEndian },
        }
    }

    pub fn parse<'a>(&self, data: &'a [u8]) -> Result<Vec<PlCdrParameter<'a>>, String> {
        let mut reader = PlCdrReader::new(data, self.endianness);
        let mut parameters = Vec::with_capacity(32);
        let max_iterations = MAX_PARAMETER_ITERATIONS;
        let mut iteration_count = 0;

        if data.is_empty() {
            return Ok(parameters);
        }

        while !reader.finished() && iteration_count < max_iterations {
            iteration_count += 1;

            match self.read_next_parameter(&mut reader) {
                Ok(Some(parameter)) => {
                    let is_sentinel = parameter.id == ParameterId::PidSentinel;
                    parameters.push(parameter);
                    if is_sentinel {
                        break;
                    }
                }
                Ok(None) => break, // End of data
                Err(e) => return Err(e),
            }
        }

        if iteration_count >= max_iterations {
            warn!("Maximum iteration limit reached during parsing");
        }

        Ok(parameters)
    }

    fn read_next_parameter<'a>(
        &self,
        reader: &mut PlCdrReader<'a>,
    ) -> Result<Option<PlCdrParameter<'a>>, String> {
        loop {
            // Read parameter ID
            let param_id = match reader.read_u16() {
                Ok(id) => id,
                Err(_) => return Ok(None), // No more data
            };

            // Check for sentinel - this marks the end of the parameter list
            if param_id == ParameterId::PidSentinel as u16 {
                let _ = reader.read_u16(); // Read and ignore param_length
                let parameter = self
                    .parse_parameter(ParameterId::PidSentinel, &[])
                    .map_err(|e| format!("Failed to parse PID_SENTINEL: {}", e))?;
                return Ok(Some(parameter));
            }

            // Read parameter length
            let param_length = match reader.read_u16() {
                Ok(len) => len,
                Err(_) => return Ok(None),
            };

            if param_length as usize > 32768 {
                warn!(
                    "Parameter length very large: {} bytes for ID 0x{:04X}, proceeding with caution",
                    param_length, param_id
                );
            }

            // Read parameter data
            let param_data = match reader.read_bytes(param_length as usize) {
                Ok(data) => data,
                Err(_) => return Ok(None),
            };

            // Skip PID_PAD and continue reading next parameter
            if param_id == ParameterId::PidPad as u16 {
                reader.align_parameters();
                continue;
            }

            // Parse parameter based on ID with error recovery
            let param_id_enum = u16_to_parameter_id(param_id);
            let parameter = match self.parse_parameter(param_id_enum, param_data) {
                Ok(parameter) => parameter,
                Err(e) => {
                    warn!("Failed to parse parameter 0x{:04X}: {}", param_id, e);
                    PlCdrParameter { id: param_id_enum, value: ParameterValue::Unknown(param_data) }
                }
            };

            // Align to the next 4-byte boundary
            reader.align_parameters();

            return Ok(Some(parameter));
        }
    }

    #[allow(dead_code)]
    #[inline(always)]
    fn read_u16(&self, data: &[u8]) -> u16 {
        if data.len() < 2 {
            debug!("Insufficient data for u16, got {} bytes", data.len());
            return 0;
        }

        let bytes = [data[0], data[1]];
        match self.endianness {
            Endianness::LittleEndian => u16::from_le_bytes(bytes),
            Endianness::BigEndian => u16::from_be_bytes(bytes),
        }
    }

    #[inline(always)]
    fn read_u32(&self, data: &[u8]) -> u32 {
        if data.len() < 4 {
            debug!("Insufficient data for u32, got {} bytes", data.len());
            return 0;
        }

        let bytes = [data[0], data[1], data[2], data[3]];
        match self.endianness {
            Endianness::LittleEndian => u32::from_le_bytes(bytes),
            Endianness::BigEndian => u32::from_be_bytes(bytes),
        }
    }

    #[inline(always)]
    fn read_i32(&self, data: &[u8]) -> i32 {
        if data.len() < 4 {
            debug!("Insufficient data for i32, got {} bytes", data.len());
            return 0;
        }

        let bytes = [data[0], data[1], data[2], data[3]];
        match self.endianness {
            Endianness::LittleEndian => i32::from_le_bytes(bytes),
            Endianness::BigEndian => i32::from_be_bytes(bytes),
        }
    }

    /// Parse an 8-byte duration (seconds + fraction) into RtpsDuration
    fn parse_duration(&self, data: &[u8]) -> Result<RtpsDuration, String> {
        if data.len() < 8 {
            return Err(format!(
                "Insufficient data for duration: need 8 bytes, got {}",
                data.len()
            ));
        }
        let seconds = self.read_i32(&data[0..4]);
        let fraction = self.read_u32(&data[4..8]);
        Ok(RtpsDuration::new(seconds, fraction))
    }

    /// Parse a 24-byte locator (kind + port + address) into Locator
    fn parse_locator(&self, data: &[u8]) -> Result<Locator, String> {
        if data.len() < 24 {
            return Err(format!(
                "Insufficient data for locator: need 24 bytes, got {}",
                data.len()
            ));
        }
        let kind = self.read_i32(&data[0..4]);
        let port = self.read_u32(&data[4..8]);
        let address = <[u8; 16]>::try_from(&data[8..24])
            .map_err(|_| "Invalid locator address data".to_string())?;
        Ok(Locator::new(kind, port, address))
    }

    /// Parse a 16-byte GUID into Guid
    fn parse_guid(&self, data: &[u8]) -> Result<Guid, String> {
        if data.len() < 16 {
            return Err(format!("Insufficient data for GUID: need 16 bytes, got {}", data.len()));
        }
        let guid_bytes =
            <[u8; 16]>::try_from(&data[0..16]).map_err(|_| "Invalid GUID data".to_string())?;
        Ok(Guid::from_bytes(guid_bytes))
    }

    fn parse_string(&self, data: &[u8]) -> Result<String, String> {
        if data.len() < 4 {
            debug!("String data too short, returning empty string");
            return Ok(String::new());
        }

        let mut reader = PlCdrReader::new(data, self.endianness);
        let length = reader.read_u32()? as usize;

        if length > 1024 * 1024 {
            warn!("String length too large: {}, truncating", length);
            return Ok("TRUNCATED_STRING".to_string());
        }

        let string_data = match reader.read_bytes(length) {
            Ok(bytes) => bytes,
            Err(_) => {
                let available_length = reader.remaining();
                debug!(
                    "String data incomplete, using {} bytes instead of {}",
                    available_length, length
                );

                if available_length == 0 {
                    return Ok(String::new());
                }

                reader.read_bytes(available_length)?
            }
        };

        self.bytes_to_string(string_data)
    }

    #[inline]
    fn bytes_to_string(&self, bytes: &[u8]) -> Result<String, String> {
        // Find null terminator for early truncation
        let end_pos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let trimmed_bytes = &bytes[..end_pos];

        match std::str::from_utf8(trimmed_bytes) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => {
                debug!("Invalid UTF-8 in string, using lossy conversion");
                Ok(String::from_utf8_lossy(trimmed_bytes).to_string())
            }
        }
    }

    fn parse_string_sequence(&self, data: &[u8]) -> Result<Vec<String>, String> {
        let mut reader = PlCdrReader::new(data, self.endianness);
        let count = reader.read_u32()? as usize;
        let mut strings = Vec::new();

        for _ in 0..count {
            let length = reader.read_u32()? as usize;
            let string_bytes = reader.read_bytes(length)?;
            let string = self.bytes_to_string(string_bytes)?;

            reader.align_parameters();
            strings.push(string);
        }

        Ok(strings)
    }

    fn parse_content_filter_property(&self, data: &[u8]) -> Result<ContentFilterProperty, String> {
        let mut reader = PlCdrReader::new(data, self.endianness);

        let mut read_string = |field: &str| -> Result<String, String> {
            let len = reader.read_u32().map_err(|_| {
                format!("ContentFilterProperty: insufficient data for {} length", field)
            })? as usize;
            let bytes = reader.read_bytes(len).map_err(|_| {
                format!(
                    "ContentFilterProperty: insufficient data for {} (expected {} bytes)",
                    field, len
                )
            })?;
            let value = self.bytes_to_string(bytes)?;
            reader.align_parameters();
            Ok(value)
        };

        let content_filtered_topic_name = read_string("content_filtered_topic_name")?;
        let related_topic_name = read_string("related_topic_name")?;
        let filter_class_name = read_string("filter_class_name")?;
        let filter_expression = read_string("filter_expression")?;

        let remaining = &data[reader.position()..];
        let mut expression_parameters =
            if remaining.is_empty() { Vec::new() } else { self.parse_string_sequence(remaining)? };

        // Remove single quotes from each parameter value
        for param in &mut expression_parameters {
            if param.starts_with('\'') && param.ends_with('\'') && param.len() >= 2 {
                *param = param[1..param.len() - 1].to_string();
            }
        }

        Ok(ContentFilterProperty {
            content_filtered_topic_name,
            related_topic_name,
            filter_class_name,
            filter_expression,
            expression_parameters,
        })
    }

    fn parse_group_info<'a>(&self, data: &'a [u8]) -> Result<GroupInfo<'a>, String> {
        let mut reader = PlCdrReader::new(data, self.endianness);
        let group_entity_id = reader.read_u32()?;
        let group_data = &data[reader.position()..];

        Ok(GroupInfo { group_entity_id, group_data })
    }

    fn parse_property_list(&self, data: &[u8]) -> Result<Vec<Property>, String> {
        let mut reader = PlCdrReader::new(data, self.endianness);
        let count = reader.read_u32()? as usize;
        let mut properties = Vec::new();

        for _ in 0..count {
            let name_len = reader.read_u32()? as usize;
            let name_bytes = reader.read_bytes(name_len)?;
            let name = String::from_utf8_lossy(name_bytes).trim_end_matches('\0').to_string();
            reader.align_parameters();

            let value_len = reader.read_u32()? as usize;
            let value_bytes = reader.read_bytes(value_len)?;
            let value = String::from_utf8_lossy(value_bytes).trim_end_matches('\0').to_string();
            reader.align_parameters();

            properties.push(Property { name, value });
        }

        Ok(properties)
    }

    fn parse_opaque_blob<'a>(&self, data: &'a [u8], field: &str) -> Result<&'a [u8], String> {
        let mut reader = PlCdrReader::new(data, self.endianness);
        let length = reader
            .read_u32()
            .map_err(|_| format!("{}: insufficient data for length field (need 4 bytes)", field))?
            as usize;
        let payload = reader.read_bytes(length).map_err(|_| {
            let available = data.len().saturating_sub(4);
            format!("{field}: declared length {length} exceeds available data ({available})")
        })?;
        Ok(payload)
    }

    fn parse_parameter<'a>(
        &self,
        id: ParameterId,
        data: &'a [u8],
    ) -> Result<PlCdrParameter<'a>, String> {
        let value = match id {
            ParameterId::PidSentinel => {
                // PID_SENTINEL should have zero length
                if !data.is_empty() {
                    return Err(format!(
                        "Invalid PID_SENTINEL: expected empty data, got {} bytes",
                        data.len()
                    ));
                }
                ParameterValue::Sentinel
            }
            ParameterId::PidParticipantManualLivelinessCount => {
                if data.len() >= 4 {
                    ParameterValue::Count(self.read_u32(data))
                } else {
                    return Err("Invalid Manual Liveliness Count data".to_string());
                }
            }
            ParameterId::PidProtocolVersion => {
                if data.len() >= 2 {
                    ParameterValue::ProtocolVersion(ProtocolVersion {
                        major: data[0],
                        minor: data[1],
                    })
                } else {
                    return Err("Invalid protocol version data".to_string());
                }
            }
            ParameterId::PidVendorId => {
                if data.len() >= 4 {
                    // Vendor ID is a 2-byte value but requires 4 bytes for CDR alignment
                    ParameterValue::VendorId([data[0], data[1]])
                } else {
                    return Err("Invalid vendor ID data: expected 4 bytes".to_string());
                }
            }
            ParameterId::PidDomainId => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let domain_id = reader.read_u32()?;
                ParameterValue::DomainId(domain_id)
            }
            ParameterId::PidUnicastLocator
            | ParameterId::PidMulticastLocator
            | ParameterId::PidDefaultUnicastLocator
            | ParameterId::PidDefaultMulticastLocator
            | ParameterId::PidMetatrafficUnicastLocator
            | ParameterId::PidMetatrafficMulticastLocator => {
                let locator = self.parse_locator(data)?;
                ParameterValue::Locator(locator)
            }
            ParameterId::PidParticipantGuid => {
                let guid = self.parse_guid(data)?;
                ParameterValue::ParticipantGuid(guid)
            }
            ParameterId::PidEndpointGuid => {
                let guid = self.parse_guid(data)?;
                ParameterValue::EndpointGuid(guid)
            }
            ParameterId::PidGroupGuid => {
                let guid = self.parse_guid(data)?;
                ParameterValue::GroupGuid(guid)
            }
            ParameterId::PidParticipantLeaseDuration
            | ParameterId::PidDeadline
            | ParameterId::PidLatencyBudget
            | ParameterId::PidTimeBasedFilter
            | ParameterId::PidLifespan => {
                let duration = self.parse_duration(data)?;
                match id {
                    ParameterId::PidParticipantLeaseDuration => {
                        ParameterValue::ParticipantLeaseDuration(duration)
                    }
                    ParameterId::PidDeadline => ParameterValue::Deadline(duration),
                    ParameterId::PidLatencyBudget => ParameterValue::LatencyBudget(duration),
                    ParameterId::PidTimeBasedFilter => ParameterValue::TimeBasedFilter(duration),
                    ParameterId::PidLifespan => ParameterValue::Lifespan(duration),
                    _ => unreachable!(),
                }
            }
            ParameterId::PidBuiltinEndpointSet => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let endpoint_set = reader.read_u32()?;
                ParameterValue::BuiltinEndpointSet(endpoint_set)
            }
            ParameterId::PidOwnershipStrength | ParameterId::PidTransportPriority => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let value = reader.read_u32()?;
                match id {
                    ParameterId::PidOwnershipStrength => ParameterValue::OwnershipStrength(value),
                    ParameterId::PidTransportPriority => ParameterValue::TransportPriority(value),
                    _ => unreachable!(),
                }
            }
            ParameterId::PidEntityName | ParameterId::PidTopicName | ParameterId::PidTypeName => {
                let string_value = self.parse_string(data)?;
                match id {
                    ParameterId::PidEntityName => ParameterValue::EntityName(string_value),
                    ParameterId::PidTopicName => ParameterValue::TopicName(string_value),
                    ParameterId::PidTypeName => ParameterValue::TypeName(string_value),
                    _ => unreachable!(),
                }
            }
            ParameterId::PidReliability => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let kind_u32 = reader.read_u32()?;
                let max_blocking_seconds = reader.read_i32()?;
                let max_blocking_fraction = reader.read_u32()?;
                let max_blocking_time =
                    RtpsDuration::new(max_blocking_seconds, max_blocking_fraction);
                let kind = ReliabilityQosPolicyKind::from_u32(kind_u32)
                    .unwrap_or(ReliabilityQosPolicyKind::BestEffort);
                ParameterValue::Reliability(ReliabilityQosPolicy {
                    kind,
                    max_blocking_time: max_blocking_time.into(),
                })
            }
            ParameterId::PidDurability => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let kind_u32 = reader.read_u32()?;
                let kind = DurabilityQosPolicyKind::from_u32(kind_u32)
                    .unwrap_or(DurabilityQosPolicyKind::Volatile);
                ParameterValue::Durability(DurabilityQosPolicy { kind })
            }
            ParameterId::PidOwnership => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let kind_u32 = reader.read_u32()?;
                let kind = OwnershipQosPolicyKind::from_u32(kind_u32)
                    .unwrap_or(OwnershipQosPolicyKind::Shared);
                ParameterValue::Ownership(OwnershipQosPolicy { kind })
            }
            ParameterId::PidLiveliness => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let kind_u32 = reader.read_u32()?;
                let lease_seconds = reader.read_i32()?;
                let lease_fraction = reader.read_u32()?;
                let lease_duration = RtpsDuration::new(lease_seconds, lease_fraction);
                let kind = LivelinessQosPolicyKind::from_u32(kind_u32)
                    .unwrap_or(LivelinessQosPolicyKind::Automatic);
                ParameterValue::Liveliness(LivelinessQosPolicy {
                    kind,
                    lease_duration: lease_duration.into(),
                })
            }
            ParameterId::PidPresentation => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let access_scope_u32 = reader.read_u32()?;
                let coherent_access_byte = reader.read_bytes(1)?[0];
                let ordered_access_byte = reader.read_bytes(1)?[0];
                let access_scope = PresentationQosAccessScopeKind::from_u32(access_scope_u32)
                    .unwrap_or(PresentationQosAccessScopeKind::Instance);
                ParameterValue::Presentation(PresentationQosPolicy {
                    access_scope,
                    coherent_access: coherent_access_byte != 0,
                    ordered_access: ordered_access_byte != 0,
                })
            }
            ParameterId::PidDestinationOrder => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let kind_u32 = reader.read_u32()?;
                let kind = DestinationOrderQosPolicyKind::from_u32(kind_u32)
                    .unwrap_or(DestinationOrderQosPolicyKind::ByReceptionTimestamp);
                ParameterValue::DestinationOrder(DestinationOrderQosPolicy { kind })
            }
            ParameterId::PidHistory => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let kind_u32 = reader.read_u32()?;
                let depth = reader.read_i32()?;
                let kind = match kind_u32 {
                    0 => HistoryQosPolicyKind::KeepLast(depth),
                    1 => HistoryQosPolicyKind::KeepAll,
                    _ => HistoryQosPolicyKind::KeepLast(depth),
                };
                ParameterValue::HistoryQosPolicy(HistoryQosPolicy { kind, strict: true })
            }
            ParameterId::PidResourceLimits => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let max_samples = reader.read_i32()?;
                let max_instances = reader.read_i32()?;
                let max_samples_per_instance = reader.read_i32()?;
                ParameterValue::ResourceLimits(ResourceLimitsQosPolicy {
                    max_samples,
                    max_instances,
                    max_samples_per_instance,
                })
            }
            ParameterId::PidDurabilityService => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let service_cleanup_seconds = reader.read_i32()?;
                let service_cleanup_fraction = reader.read_u32()?;
                let service_cleanup_delay =
                    RtpsDuration::new(service_cleanup_seconds, service_cleanup_fraction);
                let history_kind_value = reader.read_u32()?;
                let history_depth = reader.read_i32()?;
                let max_samples = reader.read_i32()?;
                let max_instances = reader.read_i32()?;
                let max_samples_per_instance = reader.read_i32()?;

                let history_kind = match history_kind_value {
                    0 => HistoryQosPolicyKind::KeepLast(history_depth),
                    1 => HistoryQosPolicyKind::KeepAll,
                    _ => HistoryQosPolicyKind::KeepLast(1),
                };

                ParameterValue::DurabilityService(DurabilityServiceQosPolicy {
                    service_cleanup_delay: service_cleanup_delay.into(),
                    history_kind,
                    max_samples,
                    max_instances,
                    max_samples_per_instance,
                })
            }
            ParameterId::PidPartition => {
                let partition = self.parse_string_sequence(data)?;
                ParameterValue::Partition(partition)
            }
            ParameterId::PidContentFilterProperty => {
                let property = self.parse_content_filter_property(data)?;
                ParameterValue::ContentFilterProperty(property)
            }
            ParameterId::PidWriterGroupInfo => {
                let group_info = self.parse_group_info(data)?;
                ParameterValue::WriterGroupInfo(group_info)
            }
            ParameterId::PidPropertyList => {
                let property_list = self.parse_property_list(data)?;
                ParameterValue::PropertyList(property_list)
            }
            ParameterId::PidUserData => {
                let payload = self.parse_opaque_blob(data, "user_data")?;
                ParameterValue::UserData(payload)
            }
            ParameterId::PidGroupData => {
                let payload = self.parse_opaque_blob(data, "group_data")?;
                ParameterValue::GroupData(payload)
            }
            ParameterId::PidTopicData => {
                let payload = self.parse_opaque_blob(data, "topic_data")?;
                ParameterValue::TopicData(payload)
            }
            ParameterId::PidExpectsInlineQos => {
                if !data.is_empty() {
                    ParameterValue::ExpectsInlineQos(data[0] != 0)
                } else {
                    return Err("Invalid expects inline QoS data".to_string());
                }
            }
            ParameterId::PidTypeMaxSizeSerialized => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let max_size = reader.read_u32()?;
                ParameterValue::MaxSerializedSize(max_size)
            }
            ParameterId::PidReceiveBufferSize => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let size = reader.read_u32()?;
                ParameterValue::ReceiveBufferSize(size)
            }
            ParameterId::PidDataRepresentation => {
                let mut reader = PlCdrReader::new(data, self.endianness);
                let sequence_length = reader.read_u32()? as usize;

                let mut representations = Vec::new();

                // Read each DataRepresentationId (2 bytes each)
                for _ in 0..sequence_length {
                    match reader.read_u16() {
                        Ok(id) => {
                            if let Some(rep_id) = DataRepresentationId::from_u16(id) {
                                representations.push(rep_id);
                            } else {
                                warn!("Unknown data representation ID: {}", id);
                                // Skip unknown representation IDs for forward compatibility
                            }
                        }
                        Err(_) => break,
                    }
                }

                if representations.is_empty() {
                    return Err("No valid data representation IDs found".to_string());
                }

                ParameterValue::DataRepresentation(DataRepresentationQosPolicy {
                    value: representations,
                })
            }
            ParameterId::PidTypeInformation => {
                match crate::xtypes::TypeInformation::deserialize_for_parameter(data) {
                    Ok(type_info) => ParameterValue::TypeInformation(type_info),
                    Err(_) => match crate::xtypes::TypeInformation::deserialize(data) {
                        Ok((type_info, _consumed)) => ParameterValue::TypeInformation(type_info),
                        Err(e) => {
                            warn!("Failed to parse TypeInformation (0x0075): {}", e);
                            ParameterValue::Unknown(data)
                        }
                    },
                }
            }
            ParameterId::PidTypeIdV1 => {
                let standard = if has_encapsulation_header(data) {
                    TypeIdentifier::deserialize(&data[4..]).ok().map(|(tid, _)| tid)
                } else {
                    None
                };
                match standard {
                    Some(type_id) => ParameterValue::TypeIdentifierV1(type_id),
                    None => match crate::xtypes::TypeInformation::deserialize(data) {
                        Ok((type_info, _)) => {
                            let mut tid = type_info.minimal.typeid_with_size.type_id;
                            if tid == TypeIdentifier::None {
                                tid = type_info.complete.typeid_with_size.type_id;
                            }
                            ParameterValue::TypeIdentifierV1(tid)
                        }
                        Err(e) => {
                            warn!("Failed to parse PID_TYPE_IDV1: {}", e);
                            ParameterValue::Unknown(data)
                        }
                    },
                }
            }
            ParameterId::PidTypeConsistencyEnforcement => {
                // TypeConsistencyEnforcementQosPolicy: kind(2) + 5 bools(5) + padding(1) = 8 bytes
                if data.len() < 7 {
                    warn!("Insufficient data for TypeConsistencyEnforcement");
                    ParameterValue::Unknown(data)
                } else {
                    let mut reader = PlCdrReader::new(data, self.endianness);
                    let kind_u16 = reader.read_u16().unwrap_or(0);
                    let kind = TypeConsistencyKind::from_u16(kind_u16)
                        .unwrap_or(TypeConsistencyKind::DisallowTypeCoercion);

                    // Read boolean flags (1 byte each)
                    let ignore_sequence_bounds =
                        reader.read_bytes(1).map(|b| b[0] != 0).unwrap_or(false);
                    let ignore_string_bounds =
                        reader.read_bytes(1).map(|b| b[0] != 0).unwrap_or(false);
                    let ignore_member_names =
                        reader.read_bytes(1).map(|b| b[0] != 0).unwrap_or(false);
                    let prevent_type_widening =
                        reader.read_bytes(1).map(|b| b[0] != 0).unwrap_or(false);
                    let force_type_validation =
                        reader.read_bytes(1).map(|b| b[0] != 0).unwrap_or(false);

                    ParameterValue::TypeConsistencyEnforcement(
                        TypeConsistencyEnforcementQosPolicy {
                            kind,
                            ignore_sequence_bounds,
                            ignore_string_bounds,
                            ignore_member_names,
                            prevent_type_widening,
                            force_type_validation,
                        },
                    )
                }
            }
            ParameterId::PidTypeObject => {
                let result = if has_encapsulation_header(data) {
                    TypeObject::deserialize(&data[4..]).or_else(|_| TypeObject::deserialize(data))
                } else {
                    TypeObject::deserialize(data)
                };
                match result {
                    Ok((type_obj, _consumed)) => ParameterValue::TypeObject(type_obj),
                    Err(e) => match crate::xtypes::TypeObjectV1::deserialize(data) {
                        Ok((type_obj_v1, _consumed)) => ParameterValue::TypeObjectV1(type_obj_v1),
                        Err(e_v1) => {
                            warn!("Failed to parse TypeObject (v2: {}; v1: {})", e, e_v1);
                            ParameterValue::Unknown(data)
                        }
                    },
                }
            }
            _ => ParameterValue::Unknown(data),
        };

        Ok(PlCdrParameter { id, value })
    }
}
