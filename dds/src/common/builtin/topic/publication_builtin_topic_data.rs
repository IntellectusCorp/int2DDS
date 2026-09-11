//! Publication builtin topic data for discovery.
//!
//! This module defines the `PublicationBuiltinTopicData` structure that represents
//! discovered DataWriter information in the DDS discovery protocol.

use crate::{
    dcps::infrastructure::qos_policy::{
        DataFragQosPolicy, DataRepresentationQosPolicy, DeadlineQosPolicy,
        DestinationOrderQosPolicy, DurabilityQosPolicy, DurabilityServiceQosPolicy,
        GroupDataQosPolicy, LatencyBudgetQosPolicy, LifespanQosPolicy, LivelinessQosPolicy,
        OwnershipQosPolicy, OwnershipStrengthQosPolicy, PartitionQosPolicy, PresentationQosPolicy,
        ReliabilityQosPolicy, ReliabilityQosPolicyKind, TopicDataQosPolicy, UserDataQosPolicy,
        WriterReliabilityExtensionQosPolicy,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    rtps::common::{guid::Guid, locator::Locator, types::SerializedData},
    topic::{qos::TopicQos, type_support::DdsType},
    xtypes::{TypeIdentifier, TypeInformation, TypeObject},
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
    #[dds(optional, id = 0x0052)] // PidGroupGuid: the owning Publisher
    group_guid: Option<Guid>,
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
    #[dds(optional, id = 0x0069)]
    type_identifier: Option<TypeIdentifier>,
    #[dds(optional, id = 0x0072)] // PID_TYPE_OBJECTV1
    type_object: Option<TypeObject>,
    #[dds(non_serialized)] // enriched 0x0075 payload; emitted by the manual serializer
    type_information: Option<TypeInformation>,
    #[dds(non_serialized)]
    writer_reliability_extension: WriterReliabilityExtensionQosPolicy,
    #[dds(non_serialized)]
    data_frag: DataFragQosPolicy,
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
            group_guid: None,
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
            type_information: None,
            writer_reliability_extension: datawriter_qos.writer_reliability_extension,
            data_frag: datawriter_qos.data_frag,
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

    // GUID of the Publisher the writer belongs to; None when the peer did not announce one.
    pub fn group_guid(&self) -> Option<Guid> {
        self.group_guid
    }

    pub fn set_group_guid(&mut self, group_guid: Guid) {
        self.group_guid = Some(group_guid);
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

    pub fn type_information(&self) -> Option<&TypeInformation> {
        self.type_information.as_ref()
    }

    pub fn set_type_information(&mut self, type_info: Option<TypeInformation>) {
        self.type_information = type_info;
    }

    pub fn writer_reliability_extension(&self) -> &WriterReliabilityExtensionQosPolicy {
        &self.writer_reliability_extension
    }

    pub fn data_frag(&self) -> &DataFragQosPolicy {
        &self.data_frag
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
        publication_data.group_guid = parsed.group_guid;
        publication_data.key_hash = parsed.key_hash;
        publication_data.type_max_size_serialized = parsed.type_max_size_serialized;
        publication_data.unicast_locator_list = parsed.unicast_locator_list;
        publication_data.multicast_locator_list = parsed.multicast_locator_list;

        // DDS-XTypes fields
        publication_data.type_identifier = parsed.type_identifier;
        publication_data.type_object = parsed.type_object;
        publication_data.type_information = parsed.type_information;

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
    fn sample_publication() -> PublicationBuiltinTopicData {
        let mut p = PublicationBuiltinTopicData::new(
            &DataWriterQos::default(),
            &PublisherQos::default(),
            &TopicQos::default(),
        );
        p.set_endpoint_guid(make_guid(0x11));
        p.set_topic_name("test/pub_topic".into());
        p.set_type_name("PubType".into());
        p.add_unicast_locator(locator(7400, 0x20));
        p.add_unicast_locator(locator(7401, 0x21));
        p
    }
    #[test]
    fn publication_pl_cdr_roundtrip() {
        let original = sample_publication();
        let bytes = original.to_serialized_data().to_vec();
        assert_starts_with_pl_cdr_le(&bytes);

        let parsed = PublicationBuiltinTopicData::from_serialized_data(&bytes)
            .expect("PL_CDR parse should succeed");
        assert_eq!(parsed.endpoint_guid(), original.endpoint_guid());
        assert_eq!(parsed.topic_name(), original.topic_name());
        assert_eq!(parsed.type_name(), original.type_name());
        assert_eq!(parsed.unicast_locator_list(), original.unicast_locator_list());
    }

    #[test]
    fn publication_group_guid_roundtrips_as_pid_group_guid() {
        let mut original = sample_publication();
        original.set_group_guid(make_guid(0x33));

        let bytes = original.to_serialized_data().to_vec();
        assert_eq!(count_pid_occurrences(&bytes, 0x0052), 1, "PID_GROUP_GUID emitted once");
        let parsed = PublicationBuiltinTopicData::from_serialized_data(&bytes)
            .expect("PL_CDR parse should succeed");
        assert_eq!(parsed.group_guid(), Some(make_guid(0x33)));

        // DdsType::serialize must place the parameter where the PL_CDR helper does.
        let dds_bytes = DdsType::serialize(&original).expect("serialize").to_vec();
        assert_eq!(dds_bytes, bytes);
    }

    #[test]
    fn publication_without_group_guid_parses_as_none() {
        let bytes = sample_publication().to_serialized_data().to_vec();
        assert_eq!(count_pid_occurrences(&bytes, 0x0052), 0);

        let parsed = PublicationBuiltinTopicData::from_serialized_data(&bytes)
            .expect("PL_CDR parse should succeed");
        assert_eq!(parsed.group_guid(), None);
    }

    #[test]
    fn publication_dds_serialize_emits_pl_cdr() {
        let original = sample_publication();
        let dds_bytes = DdsType::serialize(&original).expect("serialize").to_vec();
        assert_starts_with_pl_cdr_le(&dds_bytes);

        // Bytes from DdsType::serialize must round-trip via the PL_CDR helper.
        let parsed = PublicationBuiltinTopicData::from_serialized_data(&dds_bytes)
            .expect("DdsType bytes are valid PL_CDR");
        assert_eq!(parsed.topic_name(), original.topic_name());
        assert_eq!(parsed.type_name(), original.type_name());

        // And they must be byte-identical to the PL_CDR helper output.
        let helper_bytes = original.to_serialized_data().to_vec();
        assert_eq!(
            dds_bytes, helper_bytes,
            "DdsType::serialize must produce the same bytes as to_serialized_data"
        );
    }

    #[test]
    fn publication_pid_topic_name_present() {
        let p = sample_publication();
        let bytes = p.to_serialized_data().to_vec();
        // PidTopicName = 0x0005 must appear exactly once.
        assert_eq!(count_pid_occurrences(&bytes, 0x0005), 1);
        // PidEndpointGuid = 0x005a must appear exactly once.
        assert_eq!(count_pid_occurrences(&bytes, 0x005a), 1);
    }

    #[test]
    fn publication_with_locators_emits_one_pid_per_locator() {
        let p = sample_publication();
        let bytes = p.to_serialized_data().to_vec();
        // PidUnicastLocator = 0x002f, one per locator added (2).
        assert_eq!(count_pid_occurrences(&bytes, 0x002f), 2);
    }
    const PID_TYPE_IDV1: u16 = 0x0069;
    const PID_TYPE_INFORMATION: u16 = 0x0075;
    const PID_TYPE_OBJECT: u16 = 0x0072;

    #[test]
    fn publication_never_emits_inline_type_object() {
        let mut p = publication_with_type_info();
        p.set_type_object(Some(TypeObject::Minimal(MinimalTypeObject::default())));
        let bytes = p.to_serialized_data().to_vec();

        // XTypes 1.2/1.3: SEDP carries TypeInformation only; TypeObject is served via TypeLookup.
        assert_eq!(count_pid_occurrences(&bytes, PID_TYPE_OBJECT), 0);
        assert_eq!(count_pid_occurrences(&bytes, PID_TYPE_INFORMATION), 1);
    }

    #[test]
    fn publication_emits_enriched_type_information_deps() {
        let mut p = publication_with_type_info();
        // Distinct COMPLETE and MINIMAL slots: each slot carries its own EK ids/sizes.
        let complete_root = TypeIdentifier::CompleteTypeId(EquivalenceHash::new([7; 14]));
        let complete_dep = TypeIdentifier::CompleteTypeId(EquivalenceHash::new([8; 14]));
        let minimal_root = TypeIdentifier::MinimalTypeId(EquivalenceHash::new([70; 14]));
        let minimal_dep = TypeIdentifier::MinimalTypeId(EquivalenceHash::new([80; 14]));
        let complete = TypeIdentifierWithDependencies {
            typeid_with_size: TypeIdentifierWithSize::new(complete_root, 40),
            dependent_typeids: vec![TypeIdentifierWithSize::new(complete_dep.clone(), 16)],
        };
        let minimal = TypeIdentifierWithDependencies {
            typeid_with_size: TypeIdentifierWithSize::new(minimal_root.clone(), 32),
            dependent_typeids: vec![TypeIdentifierWithSize::new(minimal_dep.clone(), 12)],
        };
        p.set_type_information(Some(TypeInformation::new(minimal, complete)));

        let bytes = p.to_serialized_data().to_vec();
        let v75 = find_pid_value(&bytes, PID_TYPE_INFORMATION).expect("0x0075 present");
        let ti = TypeInformation::deserialize_for_parameter(v75).expect("parse 0x0075");
        assert_eq!(ti.complete.dependent_typeids.len(), 1);
        assert_eq!(ti.complete.dependent_typeids[0].type_id, complete_dep);
        assert_eq!(ti.complete.dependent_typeids[0].typeobject_serialized_size, 16);
        // The minimal slot holds different (minimal) ids/sizes than the complete slot.
        assert_eq!(ti.minimal.typeid_with_size.type_id, minimal_root);
        assert_eq!(ti.minimal.dependent_typeids[0].type_id, minimal_dep);
        assert_eq!(ti.minimal.dependent_typeids[0].typeobject_serialized_size, 12);
    }

    const PL_CDR2_LE_HEADER: [u8; 4] = [0x00, 0x0b, 0x00, 0x00];
    /// CDR_LE (XCDRv1) encapsulation header expected at the start of 0x0072 / standard 0x0069.
    const CDR_LE_HEADER: [u8; 4] = [0x00, 0x01, 0x00, 0x00];
    fn sample_type_information() -> TypeInformation {
        let main_hash = EquivalenceHash::new([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
        let dep_hash = EquivalenceHash::new([21; 14]);

        let mut minimal = TypeIdentifierWithDependencies::new(TypeIdentifierWithSize::new(
            TypeIdentifier::MinimalTypeId(main_hash),
            42,
        ));
        // Exercise the non-primitive sequence (DHEADER + count + element) framing.
        minimal
            .dependent_typeids
            .push(TypeIdentifierWithSize::new(TypeIdentifier::MinimalTypeId(dep_hash), 7));

        let complete = TypeIdentifierWithDependencies::new(TypeIdentifierWithSize::new(
            TypeIdentifier::CompleteTypeId(main_hash),
            42,
        ));
        TypeInformation::new(minimal, complete)
    }

    fn find_pid_value<'a>(payload: &'a [u8], pid_le: u16) -> Option<&'a [u8]> {
        let mut pos = 4usize;
        while pos + 4 <= payload.len() {
            let pid = u16::from_le_bytes([payload[pos], payload[pos + 1]]);
            let len = u16::from_le_bytes([payload[pos + 2], payload[pos + 3]]) as usize;
            if pid == 0x0001 {
                break;
            }
            if pid == pid_le {
                return payload.get(pos + 4..pos + 4 + len);
            }
            pos += 4 + len;
            pos = pos.div_ceil(4) * 4;
        }
        None
    }
    #[test]
    fn type_information_parameter_roundtrip() {
        let ti = sample_type_information();
        let bytes = ti.serialize_for_parameter();

        // Headerless PL_CDR2: must NOT begin with an encapsulation header.
        assert_ne!(&bytes[..4], &PL_CDR2_LE_HEADER, "0x0075 must be headerless (no encap header)");

        let emh = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(emh & 0x0FFF_FFFF, 0x1001, "first member id must be 0x1001 (minimal)");
        assert_eq!(emh >> 28, 5, "TypeInformation members must use LC=5");

        let parsed = TypeInformation::deserialize_for_parameter(&bytes)
            .expect("0x0075 payload must round-trip");
        assert_eq!(parsed, ti, "TypeInformation must survive serialize/deserialize");
    }

    #[test]
    fn type_object_parameter_has_cdr_header() {
        let obj = TypeObject::Minimal(MinimalTypeObject::default());
        let bytes = obj.serialize_for_parameter();
        assert_eq!(&bytes[..4], &CDR_LE_HEADER, "0x0072 must start with CDR_LE header");
        // Helper must be purely additive over the legacy body (guards hash entanglement).
        assert_eq!(&bytes[4..], obj.serialize().as_slice());
    }

    #[test]
    fn type_identifier_v1_parameter_has_cdr_header() {
        let hash = EquivalenceHash::new([9; 14]);
        let tid = TypeIdentifier::MinimalTypeId(hash);
        let bytes = tid.serialize_for_parameter_v1();
        assert_eq!(&bytes[..4], &CDR_LE_HEADER, "standard 0x0069 must start with CDR_LE header");
        assert_eq!(&bytes[4..], tid.serialize().as_slice());
    }
    fn publication_with_type_info() -> PublicationBuiltinTopicData {
        let mut p = sample_publication();
        p.set_type_identifier(Some(TypeIdentifier::MinimalTypeId(EquivalenceHash::new([7; 14]))));
        p
    }
    #[test]
    fn publication_emits_both_0x0075_and_legacy_0x0069() {
        let p = publication_with_type_info();
        let bytes = p.to_serialized_data().to_vec();

        // Standard 0x0075 exactly once, legacy 0x0069 exactly once (backward compat).
        assert_eq!(count_pid_occurrences(&bytes, PID_TYPE_INFORMATION), 1);
        assert_eq!(count_pid_occurrences(&bytes, PID_TYPE_IDV1), 1);

        // The 0x0075 value is headerless PL_CDR2: top DHEADER first, then EMHEADER 0x1001.
        let v75 = find_pid_value(&bytes, PID_TYPE_INFORMATION).expect("0x0075 present");
        assert_ne!(&v75[..4], &PL_CDR2_LE_HEADER, "0x0075 must be headerless");
        let emh = u32::from_le_bytes([v75[4], v75[5], v75[6], v75[7]]);
        assert_eq!(emh & 0x0FFF_FFFF, 0x1001, "0x0075 first member id must be 0x1001");

        // The legacy 0x0069 value is a single CDR_LE-encapsulated TypeIdentifier.
        let v69 = find_pid_value(&bytes, PID_TYPE_IDV1).expect("0x0069 present");
        assert_eq!(&v69[..4], &CDR_LE_HEADER, "legacy 0x0069 must carry a CDR_LE TypeIdentifier");
        let expected_tid = TypeIdentifier::MinimalTypeId(EquivalenceHash::new([7; 14]));
        let tid_bytes = expected_tid.serialize();
        assert_eq!(&v69[4..4 + tid_bytes.len()], tid_bytes.as_slice());
    }

    #[test]
    fn publication_type_identifier_roundtrips_via_0x0075() {
        let original = publication_with_type_info();
        let bytes = original.to_serialized_data().to_vec();
        let parsed = PublicationBuiltinTopicData::from_serialized_data(&bytes)
            .expect("PL_CDR parse should succeed");
        assert_eq!(parsed.type_identifier(), original.type_identifier());
    }
}
