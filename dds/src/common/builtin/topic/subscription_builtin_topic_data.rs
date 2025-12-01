//! Subscription builtin topic data for discovery.
//!
//! This module defines the `SubscriptionBuiltinTopicData` structure that represents
//! discovered DataReader information in the DDS discovery protocol.

use super::builtin_topic_key::BuiltinTopicKey;
use crate::{
    infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, GroupDataQosPolicy, LatencyBudgetQosPolicy, LivelinessQosPolicy,
        OwnershipQosPolicy, PartitionQosPolicy, PresentationQosPolicy, ReliabilityQosPolicy,
        ReliabilityQosPolicyKind, TimeBasedFilterQosPolicy, TopicDataQosPolicy, UserDataQosPolicy,
    },
    rtps::common::{guid::Guid, locator::Locator, types::SerializedData},
    subscription::qos::{DataReaderQos, SubscriberQos},
    topic::qos::TopicQos,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionBuiltinTopicData {
    endpoint_guid: Guid,
    key: BuiltinTopicKey,
    participant_key: BuiltinTopicKey,
    topic_name: String,
    type_name: String,
    durability: DurabilityQosPolicy,
    deadline: DeadlineQosPolicy,
    latency_budget: LatencyBudgetQosPolicy,
    liveliness: LivelinessQosPolicy,
    reliability: ReliabilityQosPolicy,
    ownership: OwnershipQosPolicy,
    destination_order: DestinationOrderQosPolicy,
    user_data: UserDataQosPolicy,
    time_based_filter: TimeBasedFilterQosPolicy,
    presentation: PresentationQosPolicy,
    partition: PartitionQosPolicy,
    topic_data: TopicDataQosPolicy,
    group_data: GroupDataQosPolicy,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    data_representation: DataRepresentationQosPolicy,
}

impl SubscriptionBuiltinTopicData {
    pub fn new(
        datareader_qos: &DataReaderQos,
        subscriber_qos: &SubscriberQos,
        topic_qos: &TopicQos,
    ) -> Self {
        let empty_guid = Guid::from_bytes([0u8; 16]);
        Self {
            endpoint_guid: empty_guid,
            key: BuiltinTopicKey {
                value: [
                    empty_guid.prefix()[0] as i32,
                    empty_guid.prefix()[1] as i32,
                    empty_guid.prefix()[2] as i32,
                ],
            },
            participant_key: BuiltinTopicKey {
                value: [
                    empty_guid.prefix()[0] as i32,
                    empty_guid.prefix()[1] as i32,
                    empty_guid.prefix()[2] as i32,
                ],
            },
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
        }
    }

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

    pub fn convert_u8_to_i32_array(data: [u8; 12]) -> [i32; 3] {
        [
            i32::from_ne_bytes([data[0], data[1], data[2], data[3]]),
            i32::from_ne_bytes([data[4], data[5], data[6], data[7]]),
            i32::from_ne_bytes([data[8], data[9], data[10], data[11]]),
        ]
    }

    pub fn from_serialized_data(data: SerializedData) -> Result<Self, String> {
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
