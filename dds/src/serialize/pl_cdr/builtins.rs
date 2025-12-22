use crate::{
    common::builtin::topic::builtin_topic_key::BuiltinTopicKey,
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, DurabilityServiceQosPolicy, GroupDataQosPolicy,
        LatencyBudgetQosPolicy, LifespanQosPolicy, LivelinessQosPolicy, OwnershipQosPolicy,
        OwnershipStrengthQosPolicy, PartitionQosPolicy, PresentationQosPolicy,
        ReliabilityQosPolicy, TimeBasedFilterQosPolicy, TopicDataQosPolicy, UserDataQosPolicy,
    },
    rtps::common::{
        guid::Guid,
        locator::Locator,
        parameters::{ParameterId, ParameterValue, PlCdrParameter},
        types::SerializedData,
    },
};

use super::pl_cdr_deserialize::PlCdrParser;

/// Parsed builtin topic data containing all possible fields from discovery parameters.
/// This structure serves as an intermediate representation between serialized PL-CDR data
/// and specific builtin topic data types (Publication, Subscription, etc.).
#[derive(Debug, Default)]
pub struct ParsedBuiltinTopicData {
    // GUID-related fields
    pub endpoint_guid: Option<Guid>,
    pub participant_guid: Option<Guid>,
    pub key: Option<BuiltinTopicKey>,
    pub participant_key: Option<BuiltinTopicKey>,

    // Topic information
    pub topic_name: Option<String>,
    pub type_name: Option<String>,

    // QoS Policies (common)
    pub durability: Option<DurabilityQosPolicy>,
    pub deadline: Option<DeadlineQosPolicy>,
    pub latency_budget: Option<LatencyBudgetQosPolicy>,
    pub liveliness: Option<LivelinessQosPolicy>,
    pub reliability: Option<ReliabilityQosPolicy>,
    pub ownership: Option<OwnershipQosPolicy>,
    pub destination_order: Option<DestinationOrderQosPolicy>,
    pub user_data: Option<UserDataQosPolicy>,
    pub presentation: Option<PresentationQosPolicy>,
    pub partition: Option<PartitionQosPolicy>,
    pub topic_data: Option<TopicDataQosPolicy>,
    pub group_data: Option<GroupDataQosPolicy>,
    pub data_representation: Option<DataRepresentationQosPolicy>,

    // QoS Policies (publication-specific)
    pub durability_service: Option<DurabilityServiceQosPolicy>,
    pub lifespan: Option<LifespanQosPolicy>,
    pub ownership_strength: Option<OwnershipStrengthQosPolicy>,

    // QoS Policies (subscription-specific)
    pub time_based_filter: Option<TimeBasedFilterQosPolicy>,

    // Locators
    pub unicast_locator_list: Vec<Locator>,
    pub multicast_locator_list: Vec<Locator>,

    // Optional fields
    pub key_hash: Option<[u8; 16]>,
    pub type_max_size_serialized: Option<u32>,
}

impl ParsedBuiltinTopicData {
    pub fn from_serialized_data(data: SerializedData) -> Result<Self, String> {
        let parser = PlCdrParser::new(false); // default little endian
        let parameters = parser.parse(&data)?;

        let mut parsed = Self::default();

        for parameter in parameters {
            parsed.apply_parameter(parameter)?;
        }

        Ok(parsed)
    }

    pub fn from_publication_data(
        data: &crate::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
    ) -> Self {
        Self {
            endpoint_guid: Some(data.endpoint_guid()),
            participant_guid: None,
            key: Some(data.key().clone()),
            participant_key: Some(data.participant_key().clone()),
            topic_name: Some(data.topic_name().to_string()),
            type_name: Some(data.type_name().to_string()),
            durability: Some(*data.durability()),
            durability_service: Some(*data.durability_service()),
            deadline: Some(*data.deadline()),
            latency_budget: Some(*data.latency_budget()),
            liveliness: Some(*data.liveliness()),
            reliability: Some(*data.reliability()),
            lifespan: Some(*data.lifespan()),
            user_data: Some(data.user_data().clone()),
            ownership: Some(*data.ownership()),
            ownership_strength: Some(*data.ownership_strength()),
            destination_order: Some(*data.destination_order()),
            presentation: Some(*data.presentation()),
            partition: Some(data.partition().clone()),
            topic_data: Some(data.topic_data().clone()),
            group_data: Some(data.group_data().clone()),
            data_representation: Some(data.data_representation().clone()),
            time_based_filter: None,
            unicast_locator_list: data.unicast_locator_list(),
            multicast_locator_list: data.multicast_locator_list(),
            key_hash: None,
            type_max_size_serialized: None,
        }
    }

    pub fn from_subscription_data(
        data: &crate::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    ) -> Self {
        Self {
            endpoint_guid: Some(data.endpoint_guid()),
            participant_guid: None,
            key: Some(data.key().clone()),
            participant_key: Some(data.participant_key().clone()),
            topic_name: Some(data.topic_name()),
            type_name: Some(data.type_name()),
            durability: Some(*data.durability()),
            durability_service: None,
            deadline: Some(*data.deadline()),
            latency_budget: Some(*data.latency_budget()),
            liveliness: Some(*data.liveliness()),
            reliability: Some(*data.reliability()),
            lifespan: None,
            user_data: Some(data.user_data().clone()),
            ownership: Some(*data.ownership()),
            ownership_strength: None,
            destination_order: Some(*data.destination_order()),
            presentation: Some(*data.presentation()),
            partition: Some(data.partition().clone()),
            topic_data: Some(data.topic_data().clone()),
            group_data: Some(data.group_data().clone()),
            data_representation: Some(data.data_representation().clone()),
            time_based_filter: Some(*data.time_based_filter()),
            unicast_locator_list: data.unicast_locator_list(),
            multicast_locator_list: data.multicast_locator_list(),
            key_hash: None,
            type_max_size_serialized: None,
        }
    }

    pub fn apply_parameter(&mut self, parameter: PlCdrParameter) -> Result<(), String> {
        match parameter.id {
            ParameterId::PidEndpointGuid => {
                if let ParameterValue::ParticipantGuid(guid) = parameter.value {
                    self.key = Some(BuiltinTopicKey {
                        value: Self::convert_u8_to_i32_array(guid.prefix().to_owned()),
                    });
                    self.endpoint_guid = Some(guid);
                }
            }
            ParameterId::PidParticipantGuid => {
                if let ParameterValue::ParticipantGuid(guid) = parameter.value {
                    self.participant_key = Some(BuiltinTopicKey {
                        value: Self::convert_u8_to_i32_array(guid.prefix().to_owned()),
                    });
                    self.participant_guid = Some(guid);
                }
            }
            ParameterId::PidTopicName => {
                if let ParameterValue::TopicName(name) = parameter.value {
                    self.topic_name = Some(name);
                }
            }
            ParameterId::PidTypeName => {
                if let ParameterValue::TypeName(name) = parameter.value {
                    self.type_name = Some(name);
                }
            }
            ParameterId::PidDurability => {
                if let ParameterValue::Durability(durability) = parameter.value {
                    self.durability = Some(DurabilityQosPolicy { kind: durability.kind });
                }
            }
            ParameterId::PidDurabilityService => {
                if let ParameterValue::DurabilityService(durability_service) = parameter.value {
                    self.durability_service = Some(DurabilityServiceQosPolicy {
                        service_cleanup_delay: durability_service.service_cleanup_delay,
                        history_kind: durability_service.history_kind,
                        max_samples: durability_service.max_samples,
                        max_instances: durability_service.max_instances,
                        max_samples_per_instance: durability_service.max_samples_per_instance,
                    });
                }
            }
            ParameterId::PidDeadline => {
                if let ParameterValue::Deadline(deadline) = parameter.value {
                    self.deadline = Some(DeadlineQosPolicy { period: deadline.into() });
                }
            }
            ParameterId::PidLatencyBudget => {
                if let ParameterValue::LatencyBudget(latency_budget) = parameter.value {
                    self.latency_budget =
                        Some(LatencyBudgetQosPolicy { duration: latency_budget.into() });
                }
            }
            ParameterId::PidLiveliness => {
                if let ParameterValue::Liveliness(liveliness) = parameter.value {
                    self.liveliness = Some(LivelinessQosPolicy {
                        kind: liveliness.kind,
                        lease_duration: liveliness.lease_duration,
                    });
                }
            }
            ParameterId::PidReliability => {
                if let ParameterValue::Reliability(reliability) = parameter.value {
                    self.reliability = Some(ReliabilityQosPolicy {
                        kind: reliability.kind,
                        max_blocking_time: reliability.max_blocking_time,
                    });
                }
            }
            ParameterId::PidLifespan => {
                if let ParameterValue::Lifespan(lifespan) = parameter.value {
                    self.lifespan = Some(LifespanQosPolicy { duration: lifespan.into() });
                }
            }
            ParameterId::PidUserData => {
                if let ParameterValue::UserData(user_data) = parameter.value {
                    self.user_data = Some(UserDataQosPolicy { value: user_data.to_vec() });
                }
            }
            ParameterId::PidOwnership => {
                if let ParameterValue::Ownership(ownership) = parameter.value {
                    self.ownership = Some(OwnershipQosPolicy { kind: ownership.kind });
                }
            }
            ParameterId::PidOwnershipStrength => {
                if let ParameterValue::OwnershipStrength(strength) = parameter.value {
                    self.ownership_strength =
                        Some(OwnershipStrengthQosPolicy { value: strength as i32 });
                }
            }
            ParameterId::PidDestinationOrder => {
                if let ParameterValue::DestinationOrder(dest_order) = parameter.value {
                    self.destination_order =
                        Some(DestinationOrderQosPolicy { kind: dest_order.kind });
                }
            }
            ParameterId::PidPresentation => {
                if let ParameterValue::Presentation(presentation) = parameter.value {
                    self.presentation = Some(PresentationQosPolicy {
                        access_scope: presentation.access_scope,
                        coherent_access: presentation.coherent_access,
                        ordered_access: presentation.ordered_access,
                    });
                }
            }
            ParameterId::PidPartition => {
                if let ParameterValue::Partition(partition) = parameter.value {
                    self.partition = Some(PartitionQosPolicy { name: partition });
                }
            }
            ParameterId::PidTopicData => {
                if let ParameterValue::TopicData(topic_data) = parameter.value {
                    self.topic_data = Some(TopicDataQosPolicy { value: topic_data.to_vec() });
                }
            }
            ParameterId::PidGroupData => {
                if let ParameterValue::GroupData(group_data) = parameter.value {
                    self.group_data = Some(GroupDataQosPolicy { value: group_data.to_vec() });
                }
            }
            ParameterId::PidTimeBasedFilter => {
                if let ParameterValue::TimeBasedFilter(time_based_filter) = parameter.value {
                    self.time_based_filter = Some(TimeBasedFilterQosPolicy {
                        minimum_separation: time_based_filter.into(),
                    });
                }
            }
            ParameterId::PidKeyHash => {
                if let ParameterValue::KeyHash(key_hash) = parameter.value {
                    self.key_hash = Some(key_hash);
                }
            }
            ParameterId::PidTypeMaxSizeSerialized => {
                if let ParameterValue::MaxSerializedSize(max_size) = parameter.value {
                    self.type_max_size_serialized = Some(max_size);
                }
            }
            ParameterId::PidUnicastLocator => {
                if let ParameterValue::Locator(locator) = parameter.value {
                    self.unicast_locator_list.push(locator);
                }
            }
            ParameterId::PidMulticastLocator => {
                if let ParameterValue::Locator(locator) = parameter.value {
                    self.multicast_locator_list.push(locator);
                }
            }
            ParameterId::PidDataRepresentation => {
                if let ParameterValue::DataRepresentation(data_rep) = parameter.value {
                    self.data_representation = Some(data_rep);
                }
            }
            _ => {
                // Unsupported parameter for builtin topic data.
            }
        }

        Ok(())
    }

    /// Convert u8 array to i32 array for BuiltinTopicKey
    fn convert_u8_to_i32_array(data: [u8; 12]) -> [i32; 3] {
        [
            i32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            i32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            i32::from_be_bytes([data[8], data[9], data[10], data[11]]),
        ]
    }
}
