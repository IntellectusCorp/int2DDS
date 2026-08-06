//! Subscription builtin topic data for discovery.
//!
//! This module defines the `SubscriptionBuiltinTopicData` structure that represents
//! discovered DataReader information in the DDS discovery protocol.

use super::builtin_topic_key::BuiltinTopicKey;
use crate::infrastructure::qos_policy::ReaderMulticastExtensionQosPolicy;
use crate::serialize::DeserializerReader;
use crate::{
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, GroupDataQosPolicy, LatencyBudgetQosPolicy, LivelinessQosPolicy,
        OwnershipQosPolicy, PartitionQosPolicy, PresentationQosPolicy,
        ReaderReliabilityExtensionQosPolicy, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        TimeBasedFilterQosPolicy, TopicDataQosPolicy, TypeConsistencyEnforcementQosPolicy,
        UserDataQosPolicy,
    },
    rtps::common::{guid::Guid, locator::Locator, types::SerializedData},
    subscription::qos::{DataReaderQos, SubscriberQos},
    topic::{qos::TopicQos, DdsType},
    xtypes::{TypeIdentifier, TypeInformation, TypeObject},
};

#[derive(DdsType, Eq)]
#[dds_type(crate_path = "crate", no_default, extensibility = "Mutable")]
pub struct SubscriptionBuiltinTopicData {
    #[dds(key, id = 0x005a)] // PidEndpointGuid
    endpoint_guid: Guid,
    #[dds(non_serialized)] // derived from endpoint_guid
    key: BuiltinTopicKey,
    #[dds(non_serialized)] // derived from endpoint_guid prefix
    participant_key: BuiltinTopicKey,
    #[dds(id = 0x0005)] // PidTopicName
    topic_name: String,
    #[dds(id = 0x0007)] // PidTypeName
    type_name: String,
    #[dds(id = 0x001d)] // PidDurability
    durability: DurabilityQosPolicy,
    #[dds(id = 0x0023)] // PidDeadline
    deadline: DeadlineQosPolicy,
    #[dds(id = 0x0027)] // PidLatencyBudget
    latency_budget: LatencyBudgetQosPolicy,
    #[dds(id = 0x001b)] // PidLiveliness
    liveliness: LivelinessQosPolicy,
    #[dds(id = 0x001a)] // PidReliability
    reliability: ReliabilityQosPolicy,
    #[dds(id = 0x001f)] // PidOwnership
    ownership: OwnershipQosPolicy,
    #[dds(id = 0x0025)] // PidDestinationOrder
    destination_order: DestinationOrderQosPolicy,
    #[dds(id = 0x002c)] // PidUserData
    user_data: UserDataQosPolicy,
    #[dds(id = 0x0004)] // PidTimeBasedFilter
    time_based_filter: TimeBasedFilterQosPolicy,
    #[dds(id = 0x0021)] // PidPresentation
    presentation: PresentationQosPolicy,
    #[dds(id = 0x0029)] // PidPartition
    partition: PartitionQosPolicy,
    #[dds(id = 0x002e)] // PidTopicData
    topic_data: TopicDataQosPolicy,
    #[dds(id = 0x002d)] // PidGroupData
    group_data: GroupDataQosPolicy,
    #[dds(non_serialized)]
    unicast_locator_list: Vec<Locator>,
    #[dds(non_serialized)]
    multicast_locator_list: Vec<Locator>,
    #[dds(id = 0x0073)] // PidDataRepresentation
    data_representation: DataRepresentationQosPolicy,
    #[dds(optional, id = 0x0069)]
    type_identifier: Option<TypeIdentifier>,
    #[dds(optional, id = 0x0072)] // PID_TYPE_OBJECTV1
    type_object: Option<TypeObject>,
    #[dds(id = 0x0074)] // PidTypeConsistencyEnforcement
    type_consistency_enforcement: TypeConsistencyEnforcementQosPolicy,
    #[dds(non_serialized)] // enriched 0x0075 payload; emitted by the manual serializer
    type_information: Option<TypeInformation>,
    #[dds(non_serialized)]
    reader_reliability_extension: ReaderReliabilityExtensionQosPolicy,
    #[dds(non_serialized)]
    reader_multicast_extension: ReaderMulticastExtensionQosPolicy,
}

impl SubscriptionBuiltinTopicData {
    pub fn new(
        datareader_qos: &DataReaderQos,
        subscriber_qos: &SubscriberQos,
        topic_qos: &TopicQos,
    ) -> Self {
        let empty_guid = Guid::from_bytes([0u8; 16]);
        let prefix = empty_guid.prefix();
        Self {
            endpoint_guid: empty_guid,
            key: BuiltinTopicKey { value: Self::convert_u8_to_i32_array(prefix) },
            participant_key: BuiltinTopicKey { value: Self::convert_u8_to_i32_array(prefix) },
            topic_name: String::new(),
            type_name: String::new(),
            durability: datareader_qos.durability,
            deadline: datareader_qos.deadline,
            latency_budget: datareader_qos.latency_budget,
            liveliness: datareader_qos.liveliness,
            reliability: datareader_qos.reliability,
            ownership: datareader_qos.ownership,
            destination_order: datareader_qos.destination_order,
            user_data: datareader_qos.user_data.clone(),
            time_based_filter: datareader_qos.time_based_filter,
            presentation: subscriber_qos.presentation,
            partition: subscriber_qos.partition.clone(),
            topic_data: topic_qos.topic_data.clone(),
            group_data: subscriber_qos.group_data.clone(),
            unicast_locator_list: Vec::new(),
            multicast_locator_list: Vec::new(),
            data_representation: datareader_qos.data_representation.clone(),
            type_identifier: None,
            type_object: None,
            type_consistency_enforcement: datareader_qos.type_consistency_enforcement,
            type_information: None,
            reader_reliability_extension: datareader_qos.reader_reliability_extension,
            reader_multicast_extension: datareader_qos.reader_multicast_extension.clone(),
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(&DataReaderQos::default(), &SubscriberQos::default(), &TopicQos::default())
    }

    pub fn endpoint_guid(&self) -> Guid {
        self.endpoint_guid
    }

    pub fn set_endpoint_guid(&mut self, endpoint_guid: Guid) {
        self.endpoint_guid = endpoint_guid;
        self.key = BuiltinTopicKey {
            value: Self::convert_u8_to_i32_array(endpoint_guid.prefix().to_owned()),
        };
        self.participant_key = BuiltinTopicKey {
            value: Self::convert_u8_to_i32_array(endpoint_guid.prefix().to_owned()),
        };
    }

    pub fn unicast_locator_list(&self) -> Vec<Locator> {
        self.unicast_locator_list.clone()
    }

    pub fn add_unicast_locator(&mut self, locator: Locator) {
        self.unicast_locator_list.push(locator);
    }

    pub fn multicast_locator_list(&self) -> Vec<Locator> {
        self.multicast_locator_list.clone()
    }

    pub fn add_multicast_locator(&mut self, locator: Locator) {
        self.multicast_locator_list.push(locator);
    }

    pub fn key(&self) -> &BuiltinTopicKey {
        &self.key
    }

    pub fn participant_key(&self) -> &BuiltinTopicKey {
        &self.participant_key
    }

    pub fn topic_name(&self) -> String {
        self.topic_name.clone()
    }

    pub fn set_topic_name(&mut self, topic_name: String) {
        self.topic_name = topic_name;
    }

    pub fn type_name(&self) -> String {
        self.type_name.clone()
    }

    pub fn set_type_name(&mut self, type_name: String) {
        self.type_name = type_name;
    }

    pub fn is_reliable(&self) -> bool {
        self.reliability().kind == ReliabilityQosPolicyKind::Reliable
    }

    pub fn is_best_effort(&self) -> bool {
        self.reliability().kind == ReliabilityQosPolicyKind::BestEffort
    }

    pub fn is_stateless(&self) -> bool {
        self.is_best_effort()
    }

    pub fn is_stateful(&self) -> bool {
        !self.is_stateless()
    }

    pub fn is_with_key(&self) -> bool {
        self.key().value[0] != 0
    }

    pub fn is_no_key(&self) -> bool {
        self.key().value[0] == 0
    }

    pub fn is_with_participant_key(&self) -> bool {
        self.participant_key().value[0] != 0
    }

    pub fn is_no_participant_key(&self) -> bool {
        self.participant_key().value[0] == 0
    }

    pub fn is_with_topic_name(&self) -> bool {
        !self.topic_name.is_empty()
    }

    pub fn is_no_topic_name(&self) -> bool {
        self.topic_name.is_empty()
    }

    pub fn is_with_type_name(&self) -> bool {
        !self.type_name.is_empty()
    }

    pub fn is_no_type_name(&self) -> bool {
        self.type_name.is_empty()
    }

    pub fn durability(&self) -> &DurabilityQosPolicy {
        &self.durability
    }

    pub fn deadline(&self) -> &DeadlineQosPolicy {
        &self.deadline
    }

    pub fn latency_budget(&self) -> &LatencyBudgetQosPolicy {
        &self.latency_budget
    }

    pub fn liveliness(&self) -> &LivelinessQosPolicy {
        &self.liveliness
    }

    pub fn reliability(&self) -> &ReliabilityQosPolicy {
        &self.reliability
    }

    pub fn ownership(&self) -> &OwnershipQosPolicy {
        &self.ownership
    }

    pub fn destination_order(&self) -> &DestinationOrderQosPolicy {
        &self.destination_order
    }

    pub fn user_data(&self) -> &UserDataQosPolicy {
        &self.user_data
    }

    pub fn time_based_filter(&self) -> &TimeBasedFilterQosPolicy {
        &self.time_based_filter
    }

    pub fn presentation(&self) -> &PresentationQosPolicy {
        &self.presentation
    }

    pub fn partition(&self) -> &PartitionQosPolicy {
        &self.partition
    }

    pub fn topic_data(&self) -> &TopicDataQosPolicy {
        &self.topic_data
    }

    pub fn group_data(&self) -> &GroupDataQosPolicy {
        &self.group_data
    }

    pub fn data_representation(&self) -> &DataRepresentationQosPolicy {
        &self.data_representation
    }

    pub fn type_identifier(&self) -> Option<&TypeIdentifier> {
        self.type_identifier.as_ref()
    }

    pub fn set_type_identifier(&mut self, type_id: Option<TypeIdentifier>) {
        self.type_identifier = type_id;
    }

    pub fn type_object(&self) -> Option<&TypeObject> {
        self.type_object.as_ref()
    }

    pub fn set_type_object(&mut self, type_obj: Option<TypeObject>) {
        self.type_object = type_obj;
    }

    pub fn type_information(&self) -> Option<&TypeInformation> {
        self.type_information.as_ref()
    }

    pub fn set_type_information(&mut self, type_info: Option<TypeInformation>) {
        self.type_information = type_info;
    }

    pub fn type_consistency_enforcement(&self) -> &TypeConsistencyEnforcementQosPolicy {
        &self.type_consistency_enforcement
    }

    pub fn reader_reliability_extension(&self) -> &ReaderReliabilityExtensionQosPolicy {
        &self.reader_reliability_extension
    }

    pub fn reader_multicast_extension(&self) -> &ReaderMulticastExtensionQosPolicy {
        &self.reader_multicast_extension
    }

    pub fn convert_u8_to_i32_array(data: [u8; 12]) -> [i32; 3] {
        [
            i32::from_be_bytes([data[0], data[1], data[2], data[3]]),
            i32::from_be_bytes([data[4], data[5], data[6], data[7]]),
            i32::from_be_bytes([data[8], data[9], data[10], data[11]]),
        ]
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
        let mut subscription_data =
            Self::new(&DataReaderQos::default(), &SubscriberQos::default(), &TopicQos::default());

        assign_optional!(
            subscription_data,
            parsed,
            endpoint_guid,
            key,
            participant_key,
            topic_name,
            type_name,
            durability,
            deadline,
            latency_budget,
            liveliness,
            reliability,
            user_data,
            ownership,
            destination_order,
            time_based_filter,
            presentation,
            partition,
            topic_data,
            group_data,
            data_representation
        );

        subscription_data.unicast_locator_list = parsed.unicast_locator_list;
        subscription_data.multicast_locator_list = parsed.multicast_locator_list;

        // DDS-XTypes fields
        subscription_data.type_identifier = parsed.type_identifier;
        subscription_data.type_object = parsed.type_object;
        subscription_data.type_information = parsed.type_information;
        if let Some(tce) = parsed.type_consistency_enforcement {
            subscription_data.type_consistency_enforcement = tce;
        }

        Ok(subscription_data)
    }

    pub fn to_serialized_data(&self) -> SerializedData {
        use crate::serialize::pl_cdr::ParsedBuiltinTopicData;

        let parsed = ParsedBuiltinTopicData::from_subscription_data(self);
        parsed.to_serialized_data()
    }

    /// Compares the changeable QoS policies of two `SubscriptionBuiltinTopicData` instances.
    pub(crate) fn changeable_qos_equals(&self, other: &Self) -> bool {
        self.deadline == other.deadline
            && self.latency_budget == other.latency_budget
            && self.partition == other.partition
            && self.user_data == other.user_data
            && self.topic_data == other.topic_data
            && self.group_data == other.group_data
            && self.time_based_filter == other.time_based_filter
    }
}

// Required by the DdsType derive's `deserialize_key` impl, which constructs an
// empty holder via `Default::default()` and then overwrites the key field.
// The non-key field values are never observed.
impl Default for SubscriptionBuiltinTopicData {
    fn default() -> Self {
        Self::new(&DataReaderQos::default(), &SubscriberQos::default(), &TopicQos::default())
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
    fn locator(port: u32, addr_seed: u8) -> Locator {
        let mut addr = [0u8; 16];
        for (i, b) in addr.iter_mut().enumerate() {
            *b = addr_seed.wrapping_add(i as u8);
        }
        Locator::new(1, port, addr)
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
    fn sample_subscription() -> SubscriptionBuiltinTopicData {
        let qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 0 },
            },
            ..Default::default()
        };
        let mut s = SubscriptionBuiltinTopicData::new(
            &qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        s.set_endpoint_guid(make_guid(0x33));
        s.set_topic_name("test/sub_topic".into());
        s.set_type_name("SubType".into());
        s.add_multicast_locator(locator(7402, 0x30));
        s
    }
    #[test]
    fn subscription_pl_cdr_roundtrip() {
        let original = sample_subscription();
        let bytes = original.to_serialized_data().to_vec();
        assert_starts_with_pl_cdr_le(&bytes);

        let parsed = SubscriptionBuiltinTopicData::from_serialized_data(&bytes)
            .expect("PL_CDR parse should succeed");
        assert_eq!(parsed.endpoint_guid(), original.endpoint_guid());
        assert_eq!(parsed.topic_name(), original.topic_name());
        assert_eq!(parsed.type_name(), original.type_name());
        assert_eq!(parsed.reliability().kind, original.reliability().kind);
        assert_eq!(parsed.multicast_locator_list(), original.multicast_locator_list());
    }

    #[test]
    fn subscription_dds_serialize_emits_pl_cdr() {
        let original = sample_subscription();
        let dds_bytes = DdsType::serialize(&original).expect("serialize").to_vec();
        assert_starts_with_pl_cdr_le(&dds_bytes);

        let helper_bytes = original.to_serialized_data().to_vec();
        assert_eq!(
            dds_bytes, helper_bytes,
            "DdsType::serialize must produce the same bytes as to_serialized_data"
        );
    }

    #[test]
    fn subscription_pid_topic_name_present() {
        let s = sample_subscription();
        let bytes = s.to_serialized_data().to_vec();
        assert_eq!(count_pid_occurrences(&bytes, 0x0005), 1);
        assert_eq!(count_pid_occurrences(&bytes, 0x005a), 1);
        // PidMulticastLocator = 0x0030
        assert_eq!(count_pid_occurrences(&bytes, 0x0030), 1);
    }
}
