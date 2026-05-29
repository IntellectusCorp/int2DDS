//! Publication builtin topic data for discovery.
//!
//! This module defines the `PublicationBuiltinTopicData` structure that represents
//! discovered DataWriter information in the DDS discovery protocol.

use crate::{
    dcps::infrastructure::qos_policy::{
        DataRepresentationQosPolicy, DeadlineQosPolicy, DestinationOrderQosPolicy,
        DurabilityQosPolicy, DurabilityServiceQosPolicy, GroupDataQosPolicy,
        LatencyBudgetQosPolicy, LifespanQosPolicy, LivelinessQosPolicy, OwnershipQosPolicy,
        OwnershipStrengthQosPolicy, PartitionQosPolicy, PresentationQosPolicy,
        ReliabilityQosPolicy, ReliabilityQosPolicyKind, TopicDataQosPolicy, UserDataQosPolicy,
        WriterReliabilityExtensionQosPolicy,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    rtps::common::{guid::Guid, locator::Locator, types::SerializedData},
    topic::{qos::TopicQos, type_support::DdsType},
    xtypes::{TypeIdentifier, TypeObject},
};

use super::builtin_topic_key::BuiltinTopicKey;
use crate::serialize::DeserializerReader;

#[derive(DdsType, Eq)]
#[dds_type(crate_path = "crate", no_default, extensibility = "Mutable")]
pub struct PublicationBuiltinTopicData {
    #[dds(key, id = 0x005a)] // PidEndpointGuid
    endpoint_guid: Guid,
    #[dds(non_serialized)] // derived from endpoint_guid; never on the wire
    key: BuiltinTopicKey,
    #[dds(non_serialized)] // derived from endpoint_guid prefix
    participant_key: BuiltinTopicKey,
    #[dds(id = 0x0005)] // PidTopicName
    topic_name: String,
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
    #[dds(id = 0x002b)] // PidLifespan
    lifespan: LifespanQosPolicy,
    #[dds(id = 0x002c)] // PidUserData
    user_data: UserDataQosPolicy,
    #[dds(id = 0x001f)] // PidOwnership
    ownership: OwnershipQosPolicy,
    #[dds(id = 0x0006)] // PidOwnershipStrength
    ownership_strength: OwnershipStrengthQosPolicy,
    #[dds(id = 0x0025)] // PidDestinationOrder
    destination_order: DestinationOrderQosPolicy,
    #[dds(id = 0x0021)] // PidPresentation
    presentation: PresentationQosPolicy,
    #[dds(id = 0x0029)] // PidPartition
    partition: PartitionQosPolicy,
    #[dds(id = 0x002e)] // PidTopicData
    topic_data: TopicDataQosPolicy,
    #[dds(id = 0x002d)] // PidGroupData
    group_data: GroupDataQosPolicy,
    #[dds(optional, id = 0x0070)] // PidKeyHash
    key_hash: Option<[u8; 16]>,
    #[dds(optional, id = 0x0060)] // PidTypeMaxSizeSerialized
    type_max_size_serialized: Option<u32>,
    #[dds(non_serialized)]
    unicast_locator_list: Vec<Locator>,
    #[dds(non_serialized)]
    multicast_locator_list: Vec<Locator>,
    #[dds(id = 0x0073)] // PidDataRepresentation
    data_representation: DataRepresentationQosPolicy,
    #[dds(optional, id = 0x0069)] // PidTypeIdentifier (DDS-XTypes)
    type_identifier: Option<TypeIdentifier>,
    #[dds(optional, id = 0x0072)] // PidTypeObject (DDS-XTypes)
    type_object: Option<TypeObject>,
    #[dds(non_serialized)]
    writer_reliability_extension: WriterReliabilityExtensionQosPolicy,
}

impl PublicationBuiltinTopicData {
    pub fn new(
        datawriter_qos: &DataWriterQos,
        publisher_qos: &PublisherQos,
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
            durability: datawriter_qos.durability,
            durability_service: datawriter_qos.durability_service,
            deadline: datawriter_qos.deadline,
            latency_budget: datawriter_qos.latency_budget,
            liveliness: datawriter_qos.liveliness,
            reliability: datawriter_qos.reliability,
            lifespan: datawriter_qos.lifespan,
            user_data: datawriter_qos.user_data.clone(),
            ownership: datawriter_qos.ownership,
            ownership_strength: datawriter_qos.ownership_strength,
            destination_order: datawriter_qos.destination_order,
            presentation: publisher_qos.presentation,
            partition: publisher_qos.partition.clone(),
            topic_data: topic_qos.topic_data.clone(),
            group_data: publisher_qos.group_data.clone(),
            key_hash: None,
            type_max_size_serialized: None,
            unicast_locator_list: Vec::new(),
            multicast_locator_list: Vec::new(),
            data_representation: datawriter_qos.data_representation.clone(),
            type_identifier: None,
            type_object: None,
            writer_reliability_extension: datawriter_qos.writer_reliability_extension,
        }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn default() -> Self {
        Self::new(&DataWriterQos::default(), &PublisherQos::default(), &TopicQos::default())
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

    pub fn key(&self) -> &BuiltinTopicKey {
        &self.key
    }

    pub fn participant_key(&self) -> &BuiltinTopicKey {
        &self.participant_key
    }

    pub fn topic_name(&self) -> String {
        self.topic_name.clone()
    }

    pub fn type_name(&self) -> String {
        self.type_name.clone()
    }

    pub fn set_topic_name(&mut self, topic_name: String) {
        self.topic_name = topic_name;
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

    pub fn durability_service(&self) -> &DurabilityServiceQosPolicy {
        &self.durability_service
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

    pub fn lifespan(&self) -> &LifespanQosPolicy {
        &self.lifespan
    }

    pub fn user_data(&self) -> &UserDataQosPolicy {
        &self.user_data
    }

    pub fn ownership(&self) -> &OwnershipQosPolicy {
        &self.ownership
    }

    pub fn ownership_strength(&self) -> &OwnershipStrengthQosPolicy {
        &self.ownership_strength
    }

    pub fn destination_order(&self) -> &DestinationOrderQosPolicy {
        &self.destination_order
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

    pub fn writer_reliability_extension(&self) -> &WriterReliabilityExtensionQosPolicy {
        &self.writer_reliability_extension
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
        let mut publication_data =
            Self::new(&DataWriterQos::default(), &PublisherQos::default(), &TopicQos::default());

        assign_optional!(
            publication_data,
            parsed,
            endpoint_guid,
            key,
            participant_key,
            topic_name,
            type_name,
            durability,
            durability_service,
            deadline,
            latency_budget,
            liveliness,
            reliability,
            lifespan,
            user_data,
            ownership,
            ownership_strength,
            destination_order,
            presentation,
            partition,
            topic_data,
            group_data,
            data_representation
        );

        // Handle Option<T> fields that are already Option in both source and target
        publication_data.key_hash = parsed.key_hash;
        publication_data.type_max_size_serialized = parsed.type_max_size_serialized;
        publication_data.unicast_locator_list = parsed.unicast_locator_list;
        publication_data.multicast_locator_list = parsed.multicast_locator_list;

        // DDS-XTypes fields
        publication_data.type_identifier = parsed.type_identifier;
        publication_data.type_object = parsed.type_object;

        Ok(publication_data)
    }

    pub fn to_serialized_data(&self) -> SerializedData {
        use crate::serialize::pl_cdr::ParsedBuiltinTopicData;

        let parsed = ParsedBuiltinTopicData::from_publication_data(self);
        parsed.to_serialized_data()
    }

    /// Compares the changeable QoS policies of two `PublicationBuiltinTopicData` instances.
    pub(crate) fn changeable_qos_equals(&self, other: &Self) -> bool {
        self.deadline == other.deadline
            && self.ownership_strength == other.ownership_strength
            && self.latency_budget == other.latency_budget
            && self.lifespan == other.lifespan
            && self.partition == other.partition
            && self.user_data == other.user_data
            && self.topic_data == other.topic_data
            && self.group_data == other.group_data
    }
}

// Required by the DdsType derive's `deserialize_key` impl, which constructs an
// empty holder via `Default::default()` and then overwrites the key field.
// The non-key field values are never observed.
impl Default for PublicationBuiltinTopicData {
    fn default() -> Self {
        Self::new(&DataWriterQos::default(), &PublisherQos::default(), &TopicQos::default())
    }
}
