//! PL-CDR (Parameter List CDR) serialization for discovery and QoS.
//!
//! This module provides PL-CDR serialization for encoding QoS policies and discovery data
//! as parameter lists. PL-CDR is extensively used in DDS builtin topic data exchange.

use log::{error, warn};
use speedy::Endianness;

use super::{PARAMETER_ALIGNMENT, PL_CDR_BE, PL_CDR_LE};

use crate::{
    core::time::Duration as DcpsDuration,
    infrastructure::qos_policy::{
        DestinationOrderQosPolicyKind, DurabilityQosPolicy, DurabilityQosPolicyKind,
        HistoryQosPolicyKind, LivelinessQosPolicyKind, OwnershipQosPolicyKind,
        PresentationQosAccessScopeKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
    },
    rtps::{
        builtin::data::builtin_endpoint_set::{BuiltinEndpointFlag, BuiltinEndpointSet},
        common::{
            guid::Guid,
            locator::Locator,
            parameters::{ParameterId, ParameterValue, PlCdrParameter},
            time::RtpsDuration,
            types::{Count, ProtocolVersion, VendorId},
        },
    },
};

pub struct PlCdrSerializer {
    endianness: Endianness,
}

impl PlCdrSerializer {
    pub fn new(little_endian: bool) -> Self {
        Self {
            endianness: if little_endian {
                Endianness::LittleEndian
            } else {
                Endianness::BigEndian
            },
        }
    }

    pub fn serialize_parameters(&self, parameters: &[PlCdrParameter]) -> Result<Vec<u8>, String> {
        // Input validation
        if parameters.len() > 10000 {
            return Err("Too many parameters: maximum 10000 allowed".to_string());
        }

        // Pre-allocate buffer with estimated size
        let estimated_size = std::cmp::max(256, parameters.len() * 32);
        let mut buffer = Vec::with_capacity(estimated_size);
        let max_buffer_size = 1024 * 1024 * 10; // 10MB limit

        // Encapsulation header
        self.write_encapsulation_header(&mut buffer)?;

        // Serialize each parameter
        for (index, parameter) in parameters.iter().enumerate() {
            if buffer.len() > max_buffer_size {
                return Err(format!("Buffer size exceeded limit at parameter {}", index));
            }

            if let Err(e) = self.serialize_parameter(&mut buffer, parameter) {
                warn!(
                    "Failed to serialize parameter {} (ID: 0x{:04X}): {}",
                    index, parameter.id as u16, e
                );
                continue;
            }
        }

        // Sentinel
        self.write_sentinel(&mut buffer)?;

        // Final validation
        if buffer.len() < 4 {
            return Err("Serialized buffer too small".to_string());
        }

        // Pad entire serialization to 4-byte boundary (XCDR standard)
        // and store padding length in encapsulation header byte[3]
        let padding_len = match buffer.len() % 4 {
            1 => 3,
            2 => 2,
            3 => 1,
            _ => 0,
        };

        if padding_len > 0 {
            buffer.extend(std::iter::repeat_n(0u8, padding_len));
        }

        buffer[3] = padding_len as u8;

        Ok(buffer)
    }

    fn write_encapsulation_header(&self, buffer: &mut Vec<u8>) -> Result<(), String> {
        let encapsulation_kind = match self.endianness {
            Endianness::LittleEndian => PL_CDR_LE,
            Endianness::BigEndian => PL_CDR_BE,
        };

        // Encapsulation identifier is always written in big-endian (RTPS standard)
        buffer.extend_from_slice(&encapsulation_kind.to_be_bytes());

        // Options (2 bytes) - always 0x0000, also in big-endian
        buffer.extend_from_slice(&0x0000u16.to_be_bytes());

        Ok(())
    }

    fn serialize_parameter(
        &self,
        buffer: &mut Vec<u8>,
        parameter: &PlCdrParameter,
    ) -> Result<(), String> {
        // Parameter ID encoding
        self.write_u16(buffer, parameter.id as u16);

        // Serialize parameter value payload
        let param_data = self.serialize_parameter_value(&parameter.value)?;

        if param_data.len() > u16::MAX as usize {
            return Err(format!(
                "Parameter 0x{:04X} exceeds maximum allowed length: {} bytes",
                parameter.id as u16,
                param_data.len()
            ));
        }

        // Parameter length field
        self.write_u16(buffer, param_data.len() as u16);

        // Parameter data bytes
        buffer.extend_from_slice(&param_data);

        // Align to 4-byte boundary between parameters (RTPS 2.5, Section 9.6.2)
        let padding =
            (PARAMETER_ALIGNMENT - (param_data.len() % PARAMETER_ALIGNMENT)) % PARAMETER_ALIGNMENT;
        if padding > 0 {
            buffer.extend(std::iter::repeat_n(0u8, padding));
        }

        Ok(())
    }

    /// Write sentinel parameter
    fn write_sentinel(&self, buffer: &mut Vec<u8>) -> Result<(), String> {
        self.write_u16(buffer, ParameterId::PidSentinel as u16);
        self.write_u16(buffer, 0); // Length = 0
        Ok(())
    }

    /// Serialize parameter value
    fn serialize_parameter_value(&self, value: &ParameterValue) -> Result<Vec<u8>, String> {
        let mut buffer: Vec<u8> = Vec::with_capacity(256);

        match value {
            ParameterValue::ProtocolVersion(v) => {
                buffer.push(v.major);
                buffer.push(v.minor);
                // CDR alignment - Protocol version must be 4 bytes in PL-CDR
                buffer.push(0); // padding byte 1
                buffer.push(0); // padding byte 2
            }
            ParameterValue::VendorId(v) => {
                buffer.extend_from_slice(v);
                // CDR alignment - Vendor ID must be 4 bytes
                buffer.push(0); // padding byte 1
                buffer.push(0); // padding byte 2
            }
            ParameterValue::DomainId(d) => {
                self.write_u32(&mut buffer, *d);
            }
            ParameterValue::Locator(l) => {
                self.write_i32(&mut buffer, l.kind());
                self.write_u32(&mut buffer, l.port());
                buffer.extend_from_slice(&l.address);
            }
            ParameterValue::ParticipantLeaseDuration(d) => {
                self.write_duration(&mut buffer, d);
            }
            ParameterValue::ParticipantGuid(guid) => {
                buffer.extend_from_slice(&guid.to_bytes());
            }
            ParameterValue::EndpointGuid(guid) => {
                buffer.extend_from_slice(&guid.to_bytes());
            }
            ParameterValue::BuiltinEndpointSet(bes) => {
                self.write_u32(&mut buffer, *bes);
            }
            ParameterValue::EntityName(name)
            | ParameterValue::TopicName(name)
            | ParameterValue::TypeName(name) => {
                self.write_string(&mut buffer, name);
            }
            ParameterValue::Reliability(rel) => {
                let kind_u32 = match rel.kind {
                    ReliabilityQosPolicyKind::BestEffort => 1,
                    ReliabilityQosPolicyKind::Reliable => 2,
                };
                self.write_u32(&mut buffer, kind_u32);
                self.write_duration(&mut buffer, &rel.max_blocking_time.into());
            }
            ParameterValue::Durability(dur) => {
                let kind_u32 = match dur.kind {
                    DurabilityQosPolicyKind::Volatile => 0,
                    DurabilityQosPolicyKind::TransientLocal => 1,
                    DurabilityQosPolicyKind::Transient => 2,
                    DurabilityQosPolicyKind::Persistent => 3,
                };
                self.write_u32(&mut buffer, kind_u32);
            }
            ParameterValue::Ownership(own) => {
                let kind_u32 = match own.kind {
                    OwnershipQosPolicyKind::Shared => 0,
                    OwnershipQosPolicyKind::Exclusive => 1,
                };
                self.write_u32(&mut buffer, kind_u32);
            }
            ParameterValue::OwnershipStrength(strength) => {
                self.write_u32(&mut buffer, *strength);
            }
            ParameterValue::Liveliness(live) => {
                let kind_u32 = match live.kind {
                    LivelinessQosPolicyKind::Automatic => 0,
                    LivelinessQosPolicyKind::ManualByParticipant => 1,
                    LivelinessQosPolicyKind::ManualByTopic => 2,
                };
                self.write_u32(&mut buffer, kind_u32);
                self.write_duration(&mut buffer, &live.lease_duration.into());
            }
            ParameterValue::Presentation(pres) => {
                let access_scope_u32 = match pres.access_scope {
                    PresentationQosAccessScopeKind::Instance => 0,
                    PresentationQosAccessScopeKind::Topic => 1,
                    PresentationQosAccessScopeKind::Group => 2,
                };
                self.write_u32(&mut buffer, access_scope_u32);
                buffer.push(if pres.coherent_access { 1 } else { 0 });
                buffer.push(if pres.ordered_access { 1 } else { 0 });
                // CDR alignment - efficient padding
                let padding = (4 - (buffer.len() % 4)) % 4;
                if padding > 0 {
                    buffer.extend(std::iter::repeat_n(0u8, padding));
                }
            }
            ParameterValue::DestinationOrder(dest) => {
                let kind_u32 = match dest.kind {
                    DestinationOrderQosPolicyKind::ByReceptionTimestamp => 0,
                    DestinationOrderQosPolicyKind::BySourceTimestamp => 1,
                };
                self.write_u32(&mut buffer, kind_u32);
            }
            ParameterValue::HistoryQosPolicy(hist) => {
                let (kind_u32, depth) = match hist.kind {
                    HistoryQosPolicyKind::KeepLast(depth) => (0, depth),
                    HistoryQosPolicyKind::KeepAll => (1, -1),
                };
                self.write_u32(&mut buffer, kind_u32);
                self.write_i32(&mut buffer, depth);
            }
            ParameterValue::ResourceLimits(res) => {
                self.write_i32(&mut buffer, res.max_samples);
                self.write_i32(&mut buffer, res.max_instances);
                self.write_i32(&mut buffer, res.max_samples_per_instance);
            }
            ParameterValue::TransportPriority(prio) => {
                self.write_u32(&mut buffer, *prio);
            }
            ParameterValue::Lifespan(lifespan) => {
                self.write_duration(&mut buffer, lifespan);
            }
            ParameterValue::Deadline(deadline) => {
                self.write_duration(&mut buffer, deadline);
            }
            ParameterValue::LatencyBudget(latency_budget) => {
                self.write_duration(&mut buffer, latency_budget);
            }
            ParameterValue::TimeBasedFilter(time_based_filter) => {
                self.write_duration(&mut buffer, time_based_filter);
            }
            ParameterValue::DurabilityService(dur_svc) => {
                self.write_duration(&mut buffer, &dur_svc.service_cleanup_delay.into());

                // Write history_kind as u32 and history_depth
                let (history_kind_value, history_depth) = match dur_svc.history_kind {
                    HistoryQosPolicyKind::KeepLast(depth) => (0u32, depth),
                    HistoryQosPolicyKind::KeepAll => (1u32, -1i32),
                };

                self.write_u32(&mut buffer, history_kind_value);
                self.write_i32(&mut buffer, history_depth);
                self.write_i32(&mut buffer, dur_svc.max_samples);
                self.write_i32(&mut buffer, dur_svc.max_instances);
                self.write_i32(&mut buffer, dur_svc.max_samples_per_instance);
            }
            ParameterValue::UserData(data)
            | ParameterValue::GroupData(data)
            | ParameterValue::TopicData(data) => {
                self.write_u32(&mut buffer, data.len() as u32);
                buffer.extend_from_slice(data);
            }
            ParameterValue::UserDataOwned(data) => {
                self.write_u32(&mut buffer, data.len() as u32);
                buffer.extend_from_slice(data);
            }
            ParameterValue::Partition(partitions) => {
                self.write_u32(&mut buffer, partitions.len() as u32);
                for partition in partitions {
                    self.write_string(&mut buffer, partition);
                }
            }
            ParameterValue::PropertyList(properties) => {
                self.write_u32(&mut buffer, properties.len() as u32);
                for property in properties {
                    self.write_string(&mut buffer, &property.name);
                    self.write_string(&mut buffer, &property.value);
                }
            }
            ParameterValue::ExpectsInlineQos(expects) => {
                buffer.push(if *expects { 1 } else { 0 });
                // CDR alignment - pad to 4-byte boundary
                let padding = (4 - (buffer.len() % 4)) % 4;
                if padding > 0 {
                    buffer.extend(std::iter::repeat_n(0u8, padding));
                }
            }
            ParameterValue::KeyHash(key_hash) => {
                buffer.extend_from_slice(key_hash);
            }
            ParameterValue::StatusInfo(status_info) => {
                self.write_u32(&mut buffer, status_info.flags());
            }
            ParameterValue::MaxSerializedSize(max_size) => {
                self.write_u32(&mut buffer, *max_size);
            }
            ParameterValue::DataRepresentation(data_rep) => {
                // CDR sequence format: length + data
                self.write_u32(&mut buffer, data_rep.value.len() as u32);
                for rep_id in &data_rep.value {
                    let id_value = *rep_id as u16;
                    self.write_u16(&mut buffer, id_value);
                }
                // CDR alignment - pad to 4-byte boundary if needed
                let padding = (4 - (buffer.len() % 4)) % 4;
                if padding > 0 {
                    buffer.extend(std::iter::repeat_n(0u8, padding));
                }
            }
            ParameterValue::TypeInformation(type_info) => {
                buffer.extend_from_slice(&type_info.serialize());
                let padding = (4 - (buffer.len() % 4)) % 4;
                if padding > 0 {
                    buffer.extend(std::iter::repeat_n(0u8, padding));
                }
            }
            ParameterValue::TypeConsistencyEnforcement(tce) => {
                // TypeConsistencyEnforcementQosPolicy: kind(2) + 5 bools(5) + padding(1)
                self.write_u16(&mut buffer, tce.kind as u16);
                buffer.push(if tce.ignore_sequence_bounds { 1 } else { 0 });
                buffer.push(if tce.ignore_string_bounds { 1 } else { 0 });
                buffer.push(if tce.ignore_member_names { 1 } else { 0 });
                buffer.push(if tce.prevent_type_widening { 1 } else { 0 });
                buffer.push(if tce.force_type_validation { 1 } else { 0 });
                buffer.push(0); // padding to 8 bytes total
            }
            ParameterValue::TypeObject(type_obj) => {
                // TypeObject: uses XCDR2 serialization (EK_MINIMAL/EK_COMPLETE marker included)
                buffer.extend_from_slice(&type_obj.serialize());
                // CDR alignment - pad to 4-byte boundary if needed
                let padding = (4 - (buffer.len() % 4)) % 4;
                if padding > 0 {
                    buffer.extend(std::iter::repeat_n(0u8, padding));
                }
            }
            ParameterValue::ContentFilterProperty(cfp) => {
                self.write_string(&mut buffer, &cfp.content_filtered_topic_name);
                self.write_string(&mut buffer, &cfp.related_topic_name);
                self.write_string(&mut buffer, &cfp.filter_class_name);
                self.write_string(&mut buffer, &cfp.filter_expression);
                self.write_u32(&mut buffer, cfp.expression_parameters.len() as u32);

                for param in &cfp.expression_parameters {
                    // Do not quote expression parameters for interoperability with OpenDDS.
                    // Ref: https://github.com/omg-dds/dds-rtps/issues/37
                    // // let quoted_param = format!("'{}'", param);
                    // self.write_string(&mut buffer, &quoted_param);
                    self.write_string(&mut buffer, param);
                }
            }
            ParameterValue::Sentinel => {
                // PID_SENTINEL has no data, just return empty buffer
            }
            ParameterValue::Count(count) => {
                self.write_u32(&mut buffer, *count);
            }
            ParameterValue::Unknown(data) => {
                buffer.extend_from_slice(data);
            }
            _ => {
                return Err(format!("Unsupported parameter value type: {:?}", value));
            }
        }

        Ok(buffer)
    }

    /// Serialize duration
    fn write_duration(&self, buffer: &mut Vec<u8>, duration: &RtpsDuration) {
        self.write_i32(buffer, duration.seconds());
        self.write_u32(buffer, duration.fraction());
    }

    /// Serialize string (length + data + null terminator + padding)
    fn write_string(&self, buffer: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        let length_with_null = bytes.len() + 1; // Include null terminator in length

        // Calculate total size with 4-byte alignment
        let padded_length = (length_with_null + 3) & !3; // Round up to 4-byte boundary
        let padding_count = padded_length - length_with_null;

        // Reserve space efficiently
        buffer.reserve(4 + padded_length); // 4 bytes for length + padded string

        // Write length (including null terminator)
        self.write_u32(buffer, length_with_null as u32);

        // Write string bytes
        buffer.extend_from_slice(bytes);

        // Write null terminator
        buffer.push(0);

        // Write padding bytes (0 to 3 bytes)
        buffer.resize(buffer.len() + padding_count, 0);
    }

    /// Write u16 value
    #[inline(always)]
    fn write_u16(&self, buffer: &mut Vec<u8>, value: u16) {
        let bytes = match self.endianness {
            Endianness::LittleEndian => value.to_le_bytes(),
            Endianness::BigEndian => value.to_be_bytes(),
        };
        buffer.extend_from_slice(&bytes);
    }

    /// Write u32 value
    #[inline(always)]
    fn write_u32(&self, buffer: &mut Vec<u8>, value: u32) {
        let bytes = match self.endianness {
            Endianness::LittleEndian => value.to_le_bytes(),
            Endianness::BigEndian => value.to_be_bytes(),
        };
        buffer.extend_from_slice(&bytes);
    }

    /// Write i32 value
    #[inline(always)]
    fn write_i32(&self, buffer: &mut Vec<u8>, value: i32) {
        let bytes = match self.endianness {
            Endianness::LittleEndian => value.to_le_bytes(),
            Endianness::BigEndian => value.to_be_bytes(),
        };
        buffer.extend_from_slice(&bytes);
    }
}

/// Helper functions for RTPS message creation
pub struct RtpsMessageBuilder<'a> {
    parameters: Vec<PlCdrParameter<'a>>,
    little_endian: bool,
}

impl<'a> RtpsMessageBuilder<'a> {
    pub fn new(little_endian: bool) -> Self {
        Self {
            parameters: Vec::with_capacity(16), // Pre-allocate reasonable capacity
            little_endian,
        }
    }

    pub fn with_capacity(little_endian: bool, capacity: usize) -> Self {
        Self { parameters: Vec::with_capacity(capacity), little_endian }
    }

    /// Add protocol version parameter
    pub fn add_protocol_version(mut self, major: u8, minor: u8) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidProtocolVersion,
            value: ParameterValue::ProtocolVersion(ProtocolVersion { major, minor }),
        });
        self
    }

    /// Add vendor ID parameter
    pub fn add_vendor_id(mut self, vendor_id: VendorId) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidVendorId,
            value: ParameterValue::VendorId(vendor_id),
        });
        self
    }

    /// Add domain ID parameter
    pub fn add_domain_id(mut self, domain_id: u32) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidDomainId,
            value: ParameterValue::DomainId(domain_id),
        });
        self
    }

    /// Add participant GUID parameter
    pub fn add_participant_guid(mut self, guid: [u8; 16]) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidParticipantGuid,
            value: ParameterValue::ParticipantGuid(Guid::from_bytes(guid)),
        });
        self
    }

    /// Add lease duration parameter
    pub fn add_lease_duration(mut self, seconds: i16, fraction: u32) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidParticipantLeaseDuration,
            value: ParameterValue::ParticipantLeaseDuration(RtpsDuration::new(
                seconds as i32,
                fraction,
            )),
        });
        self
    }

    pub fn add_manual_liveliness_count(mut self, count: Count) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidParticipantManualLivelinessCount,
            value: ParameterValue::Count(count),
        });
        self
    }

    /// Add builtin endpoint set parameter
    pub fn add_builtin_endpoint_set(mut self, endpoint_set: u32) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidBuiltinEndpointSet,
            value: ParameterValue::BuiltinEndpointSet(endpoint_set),
        });
        self
    }

    pub fn add_default_builtin_endpoint_set(mut self) -> Self {
        let mut endpoint_set = BuiltinEndpointSet::new();

        // Add Participant Message endpoints
        endpoint_set.add(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_WRITER);
        endpoint_set.add(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_READER);

        // Add basic discovery endpoints
        endpoint_set.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PARTICIPANT_ANNOUNCER);
        endpoint_set.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PARTICIPANT_DETECTOR);
        endpoint_set.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_ANNOUNCER);
        endpoint_set.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_DETECTOR);
        endpoint_set.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_ANNOUNCER);
        endpoint_set.add(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_DETECTOR);

        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidBuiltinEndpointSet,
            value: ParameterValue::BuiltinEndpointSet(endpoint_set.bits()),
        });
        self
    }

    /// Add locator parameter
    pub fn add_locator(
        mut self,
        param_id: ParameterId,
        kind: i32,
        port: u32,
        address: [u8; 16],
    ) -> Self {
        self.parameters.push(PlCdrParameter {
            id: param_id,
            value: ParameterValue::Locator(Locator::new(kind, port, address)),
        });
        self
    }

    /// Add entity name parameter
    pub fn add_entity_name(mut self, name: String) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidEntityName,
            value: ParameterValue::EntityName(name),
        });
        self
    }

    /// Add user data parameter
    pub fn add_user_data(mut self, data: Vec<u8>) -> Self {
        self.parameters.push(PlCdrParameter {
            id: ParameterId::PidUserData,
            value: ParameterValue::UserDataOwned(data),
        });
        self
    }

    /// Add custom parameter
    pub fn add_parameter(mut self, parameter: PlCdrParameter<'a>) -> Self {
        self.parameters.push(parameter);
        self
    }

    /// Create PL-CDR serialized data
    pub fn build(self) -> Result<Vec<u8>, String> {
        let serializer = PlCdrSerializer::new(self.little_endian);
        serializer.serialize_parameters(&self.parameters)
    }

    /// Create default SPDP participant data
    pub fn build_spdp_participant_data(
        domain_id: u32,
        participant_guid: Guid,
        vendor_id: VendorId,
        little_endian: bool,
    ) -> Result<Vec<u8>, String> {
        RtpsMessageBuilder::new(little_endian)
            .add_protocol_version(
                ProtocolVersion::PROTOCOLVERSION.major,
                ProtocolVersion::PROTOCOLVERSION.minor,
            )
            .add_vendor_id(vendor_id)
            .add_domain_id(domain_id)
            .add_participant_guid(participant_guid.to_bytes())
            .add_lease_duration(100, 0) // Default 100 seconds
            .add_default_builtin_endpoint_set() // Reserved + Participant Message + Discovery endpoints
            .build()
    }
}

pub fn remove_pl_cdr_sentinel(data: &mut Vec<u8>) {
    if data.len() >= 4 {
        let len = data.len();

        // Check little endian sentinel: 0x01, 0x00, 0x00, 0x00
        let is_le_sentinel = data[len - 4] == 0x01
            && data[len - 3] == 0x00
            && data[len - 2] == 0x00
            && data[len - 1] == 0x00;

        // Check big endian sentinel: 0x00, 0x01, 0x00, 0x00
        let is_be_sentinel = data[len - 4] == 0x00
            && data[len - 3] == 0x01
            && data[len - 2] == 0x00
            && data[len - 1] == 0x00;

        if is_le_sentinel || is_be_sentinel {
            data.truncate(len - 4);
        }
    }
}

/// Remove encapsulation header (first 4 bytes)
pub fn remove_encapsulation_header(data: &mut Vec<u8>) {
    if data.len() >= 4 {
        // Remove first 4 bytes (encapsulation identifier 2bytes + options 2bytes)
        data.drain(0..4);
    }
}

pub fn combine_pl_cdr_parameters(lists: Vec<Vec<u8>>) -> Vec<u8> {
    // Pre-calculate total capacity to avoid reallocations
    // First list: -4 (sentinel), Others: -4 (sentinel) -4 (encapsulation header)
    let total_capacity: usize = lists
        .iter()
        .enumerate()
        .map(|(index, list)| {
            if index == 0 {
                list.len().saturating_sub(4) // Remove sentinel only
            } else {
                list.len().saturating_sub(8) // Remove sentinel + encapsulation header
            }
        })
        .sum();

    let mut result = Vec::with_capacity(total_capacity);

    for (index, mut list) in lists.into_iter().enumerate() {
        if index == 0 {
            // First list: remove only sentinel (keep encapsulation header)
            remove_pl_cdr_sentinel(&mut list);
        } else {
            // Subsequent lists: remove both encapsulation header and sentinel
            remove_pl_cdr_sentinel(&mut list);
            remove_encapsulation_header(&mut list);
        }

        // Merge
        result.extend_from_slice(&list);
    }

    result
}

pub fn append_pl_cdr_sentinel(mut data: Vec<u8>, little_endian: bool) -> Vec<u8> {
    if little_endian {
        data.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
    } else {
        data.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    }
    data
}

pub fn merge_serialized_data(
    base_data: Vec<u8>,
    additional_data: Option<Vec<u8>>,
    little_endian: bool,
) -> Vec<u8> {
    // Return base_data as-is if no additional_data
    let Some(additional) = additional_data else {
        return base_data;
    };

    // Merge two parameter lists
    let combined = combine_pl_cdr_parameters(vec![base_data, additional]);

    // Add final sentinel
    append_pl_cdr_sentinel(combined, little_endian)
}

/// Convenience functions for creating RTPS discovery messages
pub mod discovery_helpers {
    use super::*;

    /// Create SPDP participant discovery message
    pub fn create_spdp_participant_message(
        domain_id: u32,
        participant_guid: Guid,
        vendor_id: VendorId,
        entity_name: Option<String>,
        locators: Vec<(ParameterId, i32, u32, [u8; 16])>, // (param_id, kind, port, address)
    ) -> Result<Vec<u8>, String> {
        let mut builder = RtpsMessageBuilder::new(true) // Little endian
            .add_protocol_version(
                ProtocolVersion::PROTOCOLVERSION.major,
                ProtocolVersion::PROTOCOLVERSION.minor,
            )
            .add_vendor_id(vendor_id)
            .add_domain_id(domain_id)
            .add_participant_guid(participant_guid.to_bytes())
            .add_lease_duration(100, 0)
            .add_default_builtin_endpoint_set();

        // Add entity name (optional)
        if let Some(name) = entity_name {
            builder = builder.add_entity_name(name);
        }

        // Add locators
        for (param_id, kind, port, address) in locators {
            builder = builder.add_locator(param_id, kind, port, address);
        }

        builder.build()
    }

    // Create SEDP subscription discovery message
    pub fn create_sedp_subscription_message(
        topic_name: String,
        type_name: String,
        reliability_kind: u32,
        durability_kind: u32,
    ) -> Result<Vec<u8>, String> {
        RtpsMessageBuilder::new(true)
            .add_parameter(PlCdrParameter {
                id: ParameterId::PidTopicName,
                value: ParameterValue::TopicName(topic_name),
            })
            .add_parameter(PlCdrParameter {
                id: ParameterId::PidTypeName,
                value: ParameterValue::TypeName(type_name),
            })
            .add_parameter(PlCdrParameter {
                id: ParameterId::PidReliability,
                value: ParameterValue::Reliability(ReliabilityQosPolicy {
                    kind: match reliability_kind {
                        1 => ReliabilityQosPolicyKind::BestEffort,
                        2 => ReliabilityQosPolicyKind::Reliable,
                        _ => ReliabilityQosPolicyKind::BestEffort,
                    },
                    max_blocking_time: DcpsDuration::new(0, 100000000), // 0.1 seconds
                }),
            })
            .add_parameter(PlCdrParameter {
                id: ParameterId::PidDurability,
                value: ParameterValue::Durability(DurabilityQosPolicy {
                    kind: match durability_kind {
                        0 => DurabilityQosPolicyKind::Volatile,
                        1 => DurabilityQosPolicyKind::TransientLocal,
                        2 => DurabilityQosPolicyKind::Transient,
                        3 => DurabilityQosPolicyKind::Persistent,
                        _ => DurabilityQosPolicyKind::Volatile,
                    },
                }),
            })
            .build()
    }
}

// Serialization implementation for ParsedBuiltinTopicData
impl super::ParsedBuiltinTopicData {
    /// Serialize this parsed data into SerializedData (PL-CDR format)
    pub fn to_serialized_data(&self) -> crate::rtps::common::types::SerializedData {
        use crate::rtps::common::types::SerializedData;

        let mut parameters = Vec::new();

        // GUID-related fields
        if let Some(guid) = &self.endpoint_guid {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidEndpointGuid,
                value: ParameterValue::EndpointGuid(*guid),
            });
        }
        if let Some(guid) = &self.participant_guid {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidParticipantGuid,
                value: ParameterValue::ParticipantGuid(*guid),
            });
        }

        // Topic information
        if let Some(name) = &self.topic_name {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTopicName,
                value: ParameterValue::TopicName(name.clone()),
            });
        }
        if let Some(name) = &self.type_name {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTypeName,
                value: ParameterValue::TypeName(name.clone()),
            });
        }

        // QoS Policies (common)
        if let Some(durability) = &self.durability {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidDurability,
                value: ParameterValue::Durability(DurabilityQosPolicy { kind: durability.kind }),
            });
        }
        if let Some(durability_service) = &self.durability_service {
            use crate::infrastructure::qos_policy::DurabilityServiceQosPolicy;
            parameters.push(PlCdrParameter {
                id: ParameterId::PidDurabilityService,
                value: ParameterValue::DurabilityService(DurabilityServiceQosPolicy {
                    service_cleanup_delay: durability_service.service_cleanup_delay,
                    history_kind: durability_service.history_kind,
                    max_samples: durability_service.max_samples,
                    max_instances: durability_service.max_instances,
                    max_samples_per_instance: durability_service.max_samples_per_instance,
                }),
            });
        }
        if let Some(deadline) = &self.deadline {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidDeadline,
                value: ParameterValue::Deadline(deadline.period.into()),
            });
        }
        if let Some(latency_budget) = &self.latency_budget {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidLatencyBudget,
                value: ParameterValue::LatencyBudget(latency_budget.duration.into()),
            });
        }
        if let Some(liveliness) = &self.liveliness {
            use crate::infrastructure::qos_policy::LivelinessQosPolicy;
            parameters.push(PlCdrParameter {
                id: ParameterId::PidLiveliness,
                value: ParameterValue::Liveliness(LivelinessQosPolicy {
                    kind: liveliness.kind,
                    lease_duration: liveliness.lease_duration,
                }),
            });
        }
        if let Some(reliability) = &self.reliability {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidReliability,
                value: ParameterValue::Reliability(ReliabilityQosPolicy {
                    kind: reliability.kind,
                    max_blocking_time: reliability.max_blocking_time,
                }),
            });
        }
        if let Some(lifespan) = &self.lifespan {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidLifespan,
                value: ParameterValue::Lifespan(lifespan.duration.into()),
            });
        }
        if let Some(user_data) = &self.user_data {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidUserData,
                value: ParameterValue::UserData(&user_data.value),
            });
        }
        if let Some(ownership) = &self.ownership {
            use crate::infrastructure::qos_policy::OwnershipQosPolicy;
            parameters.push(PlCdrParameter {
                id: ParameterId::PidOwnership,
                value: ParameterValue::Ownership(OwnershipQosPolicy { kind: ownership.kind }),
            });
        }
        if let Some(ownership_strength) = &self.ownership_strength {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidOwnershipStrength,
                value: ParameterValue::OwnershipStrength(ownership_strength.value as u32),
            });
        }
        if let Some(destination_order) = &self.destination_order {
            use crate::infrastructure::qos_policy::DestinationOrderQosPolicy;
            parameters.push(PlCdrParameter {
                id: ParameterId::PidDestinationOrder,
                value: ParameterValue::DestinationOrder(DestinationOrderQosPolicy {
                    kind: destination_order.kind,
                }),
            });
        }
        if let Some(presentation) = &self.presentation {
            use crate::infrastructure::qos_policy::PresentationQosPolicy;
            parameters.push(PlCdrParameter {
                id: ParameterId::PidPresentation,
                value: ParameterValue::Presentation(PresentationQosPolicy {
                    access_scope: presentation.access_scope,
                    coherent_access: presentation.coherent_access,
                    ordered_access: presentation.ordered_access,
                }),
            });
        }
        if let Some(partition) = &self.partition {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidPartition,
                value: ParameterValue::Partition(partition.name.clone()),
            });
        }
        if let Some(topic_data) = &self.topic_data {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTopicData,
                value: ParameterValue::TopicData(&topic_data.value),
            });
        }
        if let Some(group_data) = &self.group_data {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidGroupData,
                value: ParameterValue::GroupData(&group_data.value),
            });
        }
        if let Some(time_based_filter) = &self.time_based_filter {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTimeBasedFilter,
                value: ParameterValue::TimeBasedFilter(time_based_filter.minimum_separation.into()),
            });
        }

        // Optional fields
        if let Some(key_hash) = self.key_hash {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidKeyHash,
                value: ParameterValue::KeyHash(key_hash),
            });
        }
        if let Some(type_max_size) = self.type_max_size_serialized {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTypeMaxSizeSerialized,
                value: ParameterValue::MaxSerializedSize(type_max_size),
            });
        }

        // Locators (multiple values possible)
        for locator in &self.unicast_locator_list {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidUnicastLocator,
                value: ParameterValue::Locator(locator.clone()),
            });
        }
        for locator in &self.multicast_locator_list {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidMulticastLocator,
                value: ParameterValue::Locator(locator.clone()),
            });
        }

        // DataRepresentation (only if explicitly set)
        if let Some(data_representation) = &self.data_representation {
            if !data_representation.value.is_empty() {
                parameters.push(PlCdrParameter {
                    id: ParameterId::PidDataRepresentation,
                    value: ParameterValue::DataRepresentation(data_representation.clone()),
                });
            }
        }

        // TypeInformation (DDS-XTypes)
        if let Some(type_id) = &self.type_identifier {
            let type_info = crate::xtypes::TypeInformation::from_type_identifier(type_id.clone());
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTypeInformation,
                value: ParameterValue::TypeInformation(type_info),
            });
        }

        // TypeConsistencyEnforcement (DDS-XTypes, subscription only)
        if let Some(tce) = &self.type_consistency_enforcement {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTypeConsistencyEnforcement,
                value: ParameterValue::TypeConsistencyEnforcement(*tce),
            });
        }

        // TypeObject (DDS-XTypes)
        if let Some(type_obj) = &self.type_object {
            parameters.push(PlCdrParameter {
                id: ParameterId::PidTypeObject,
                value: ParameterValue::TypeObject(type_obj.clone()),
            });
        }

        // Serialize parameters to PL-CDR format
        let serializer = PlCdrSerializer::new(true); // little endian
        match serializer.serialize_parameters(&parameters) {
            Ok(serialized) => SerializedData::from(serialized),
            Err(e) => {
                error!("Serialization error: {}", e);
                SerializedData::default()
            }
        }
    }
}
