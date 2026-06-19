//! Topic builtin topic data for discovery.
//!
//! This module defines the `TopicBuiltinTopicData` structure that represents
//! discovered Topic information in the DDS discovery protocol.

use crate::{
    infrastructure::qos_policy::{
        DeadlineQosPolicy, DestinationOrderQosPolicy, DurabilityQosPolicy,
        DurabilityServiceQosPolicy, HistoryQosPolicy, LatencyBudgetQosPolicy, LifespanQosPolicy,
        LivelinessQosPolicy, OwnershipQosPolicy, ReliabilityQosPolicy, ResourceLimitsQosPolicy,
        TopicDataQosPolicy, TransportPriorityQosPolicy,
    },
    rtps::common::{guid::Guid, types::SerializedData},
    topic::{qos::TopicQos, DdsType},
};

use super::builtin_topic_key::BuiltinTopicKey;
use crate::serialize::DeserializerReader;

#[derive(DdsType)]
#[dds_type(crate_path = "crate", no_default, extensibility = "Mutable")]
pub struct TopicBuiltinTopicData {
    #[dds(non_serialized)]
    key: BuiltinTopicKey,
    #[dds(id = 0x0005)] // PidTopicName
    name: String,
    #[dds(id = 0x0007)] // PidTypeName
    type_name: String,
    #[dds(id = 0x001d)] // PidDurability
    durability: DurabilityQosPolicy,
    #[dds(id = 0x001e)] // PidDurabilityService
    durability_service: DurabilityServiceQosPolicy,
    #[dds(id = 0x0023)] // PidDeadline
    deadline: DeadlineQosPolicy,
    #[dds(id = 0x0027)] // PidLatencyBudget
    latency_budget: LatencyBudgetQosPolicy,
    #[dds(id = 0x001b)] // PidLiveliness
    liveliness: LivelinessQosPolicy,
    #[dds(id = 0x001a)] // PidReliability
    reliability: ReliabilityQosPolicy,
    #[dds(id = 0x0049)] // PidTransportPriority
    transport_priority: TransportPriorityQosPolicy,
    #[dds(id = 0x002b)] // PidLifespan
    lifespan: LifespanQosPolicy,
    #[dds(id = 0x0025)] // PidDestinationOrder
    destination_order: DestinationOrderQosPolicy,
    #[dds(id = 0x0040)] // PidHistory
    history: HistoryQosPolicy,
    #[dds(id = 0x0041)] // PidResourceLimits
    resource_limits: ResourceLimitsQosPolicy,
    #[dds(id = 0x001f)] // PidOwnership
    ownership: OwnershipQosPolicy,
    #[dds(id = 0x002e)] // PidTopicData
    topic_data: TopicDataQosPolicy,
}

impl TopicBuiltinTopicData {
    pub fn new(topic_guid: Guid, name: String, type_name: String, qos: TopicQos) -> Self {
        let prefix = topic_guid.prefix();
        Self {
            key: BuiltinTopicKey {
                value: [
                    i32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]),
                    i32::from_be_bytes([prefix[4], prefix[5], prefix[6], prefix[7]]),
                    i32::from_be_bytes([prefix[8], prefix[9], prefix[10], prefix[11]]),
                ],
            },
            name,
            type_name,
            durability: qos.durability,
            durability_service: qos.durability_service,
            deadline: qos.deadline,
            latency_budget: qos.latency_budget,
            liveliness: qos.liveliness,
            reliability: qos.reliability,
            transport_priority: qos.transport_priority,
            lifespan: qos.lifespan,
            destination_order: qos.destination_order,
            history: qos.history,
            resource_limits: qos.resource_limits,
            ownership: qos.ownership,
            topic_data: qos.topic_data,
        }
    }
    pub fn key(&self) -> BuiltinTopicKey {
        self.key.clone()
    }
    pub fn name(&self) -> String {
        self.name.clone()
    }
    pub fn type_name(&self) -> String {
        self.type_name.clone()
    }
    pub fn durability(&self) -> DurabilityQosPolicy {
        self.durability
    }
    pub fn durability_service(&self) -> DurabilityServiceQosPolicy {
        self.durability_service
    }
    pub fn deadline(&self) -> DeadlineQosPolicy {
        self.deadline
    }
    pub fn latency_budget(&self) -> LatencyBudgetQosPolicy {
        self.latency_budget
    }
    pub fn liveliness(&self) -> LivelinessQosPolicy {
        self.liveliness
    }
    pub fn reliability(&self) -> ReliabilityQosPolicy {
        self.reliability
    }
    pub fn transport_priority(&self) -> TransportPriorityQosPolicy {
        self.transport_priority
    }
    pub fn lifespan(&self) -> LifespanQosPolicy {
        self.lifespan
    }
    pub fn destination_order(&self) -> DestinationOrderQosPolicy {
        self.destination_order
    }
    pub fn history(&self) -> HistoryQosPolicy {
        self.history
    }
    pub fn resource_limits(&self) -> ResourceLimitsQosPolicy {
        self.resource_limits
    }
    pub fn ownership(&self) -> OwnershipQosPolicy {
        self.ownership
    }
    pub fn topic_data(&self) -> TopicDataQosPolicy {
        self.topic_data.clone()
    }

    pub fn from_serialized_data(data: &[u8]) -> Result<Self, String> {
        use crate::serialize::pl_cdr::ParsedBuiltinTopicData;

        macro_rules! assign_optional {
            ($target:expr, $source:expr, $($field:ident),+ $(,)?) => {
                $(
                    if let Some(value) = $source.$field {
                        $target.$field = value;
                    }
                )+
            };
        }

        let parsed = ParsedBuiltinTopicData::from_serialized_data(data)?;
        let mut topic_data = Self::new(
            Guid::from_bytes([0u8; 16]),
            String::new(),
            String::new(),
            TopicQos::default(),
        );

        if let Some(key) = parsed.key {
            topic_data.key = key;
        }
        if let Some(name) = parsed.topic_name {
            topic_data.name = name;
        }
        if let Some(type_name) = parsed.type_name {
            topic_data.type_name = type_name;
        }

        assign_optional!(
            topic_data,
            parsed,
            durability,
            durability_service,
            deadline,
            latency_budget,
            liveliness,
            reliability,
            transport_priority,
            lifespan,
            destination_order,
            history,
            resource_limits,
            ownership,
            topic_data,
        );

        Ok(topic_data)
    }

    pub fn to_serialized_data(&self) -> SerializedData {
        use crate::serialize::pl_cdr::ParsedBuiltinTopicData;

        let parsed = ParsedBuiltinTopicData::from_topic_topic_data(self);
        parsed.to_serialized_data()
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use crate::{
        common::builtin::topic::{
            participant_builtin_topic_data::ParticipantBuiltinTopicData,
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
            topic_builtin_topic_data::TopicBuiltinTopicData,
        },
        core::time::Duration,
        infrastructure::qos_policy::{
            ReliabilityQosPolicy, ReliabilityQosPolicyKind, UserDataQosPolicy,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        rtps::common::{guid::Guid, locator::Locator},
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::{qos::TopicQos, type_support::DdsType},
        xtypes::{
            EquivalenceHash, MinimalTypeObject, TypeIdentifier, TypeIdentifierWithDependencies,
            TypeIdentifierWithSize, TypeInformation, TypeObject,
        },
    };
    const PL_CDR_LE_HEADER: [u8; 4] = [0x00, 0x03, 0x00, 0x00];
    fn make_guid(seed: u8) -> Guid {
        let mut bytes = [0u8; 16];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        Guid::from_bytes(bytes)
    }
    fn count_pid_occurrences(payload: &[u8], pid_le: u16) -> usize {
        assert!(payload.len() >= 4, "payload too short for encap header");
        let mut pos = 4usize;
        let mut count = 0usize;
        while pos + 4 <= payload.len() {
            let pid = u16::from_le_bytes([payload[pos], payload[pos + 1]]);
            let len = u16::from_le_bytes([payload[pos + 2], payload[pos + 3]]) as usize;
            // PID_SENTINEL = 0x0001 ends the list.
            if pid == 0x0001 {
                break;
            }
            if pid == pid_le {
                count += 1;
            }
            pos += 4 + len;
            // 4-byte alignment for next parameter (already aligned because len is u16
            // and PlCdrSerializer pads each parameter to 4-byte boundary, but be safe).
            pos = pos.div_ceil(4) * 4;
        }
        count
    }
    fn assert_starts_with_pl_cdr_le(payload: &[u8]) {
        assert!(payload.len() >= 4, "payload too short");
        assert_eq!(&payload[..4], &PL_CDR_LE_HEADER, "expected PL_CDR_LE encapsulation header");
    }
    fn sample_topic() -> TopicBuiltinTopicData {
        TopicBuiltinTopicData::new(
            make_guid(0x77),
            "test/topic".into(),
            "TopicType".into(),
            TopicQos::default(),
        )
    }
    #[test]
    fn topic_pl_cdr_roundtrip() {
        let original = sample_topic();
        let bytes = original.to_serialized_data().to_vec();
        assert_starts_with_pl_cdr_le(&bytes);

        let parsed = TopicBuiltinTopicData::from_serialized_data(&bytes)
            .expect("PL_CDR parse should succeed");
        assert_eq!(parsed.name(), original.name());
        assert_eq!(parsed.type_name(), original.type_name());
        assert_eq!(parsed.reliability().kind, original.reliability().kind);
        assert_eq!(parsed.durability().kind, original.durability().kind);
    }

    #[test]
    fn topic_pid_name_present() {
        let t = sample_topic();
        let bytes = t.to_serialized_data().to_vec();
        // PidTopicName = 0x0005, PidTypeName = 0x0007
        assert_eq!(count_pid_occurrences(&bytes, 0x0005), 1);
        assert_eq!(count_pid_occurrences(&bytes, 0x0007), 1);
        // PidHistory = 0x0040, PidResourceLimits = 0x0041 — Topic-specific.
        assert_eq!(count_pid_occurrences(&bytes, 0x0040), 1);
        assert_eq!(count_pid_occurrences(&bytes, 0x0041), 1);
    }
}
