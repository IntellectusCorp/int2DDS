//! TypeLookup builtin service: wire-level contract plus live two-participant
//! discovery, for both flat and nested types.
//!
//! - `get_types_closure_reply_resolves_nested_on_requester` pins the data flow
//!   `type_lookup_logic` depends on: a replier serves the complete closure of a
//!   nested type via `getTypes`, the bytes survive a `TypeLookup_Reply`
//!   round-trip, and the requester's registry ends up able to resolve the
//!   nested type.
//! - `consumer_fetches_type_object_via_type_lookup` exercises the live wiring
//!   added in stage 2 (endpoint matching, request/reply dispatch, registry
//!   population) end-to-end over real RTPS with the simplest possible payload:
//!   participant A advertises only a `TypeIdentifier` (inline `TypeObject`
//!   disabled) so B must pull the `TypeObject` through the service.
//! - `consumer_resolves_nested_member_via_type_lookup` /
//!   `consumer_resolves_sequence_member_via_type_lookup` extend the live path to
//!   nested members. Building the dynamic type alone does not prove the nested
//!   member was fetched (an unresolved member stays an `ExternalType` and still
//!   "builds"), so these assert the consumer-side `DynamicType` resolves the
//!   nested member to a `TypeRef`, which only happens when the child
//!   `TypeObject` crossed the wire and landed in the registry.

mod common;

use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration as StdDuration;

use common::next_domain_id;
use int2dds::rtps::common::{guid::Guid, sequence::SequenceNumber};
use int2dds::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::qos::{DataReaderQos, SubscriberQos},
    topic::{qos::TopicQos, type_support::DdsType},
    xtypes::{
        CompleteStructMember, CompleteStructType, CompleteTypeObject, DynamicType, DynamicTypeKind,
        EquivalenceHash, ExtensibilityKind, GetTypesOut, MemberFlag, ReplyHeader, SampleIdentity,
        TypeFlag, TypeIdentifier, TypeLookupReply, TypeLookupReturn, TypeObject, TypeRegistry,
    },
};

// ---------------------------------------------------------------------------
// Wire-level contract: getTypes closure survives a TypeLookup_Reply round-trip.
// ---------------------------------------------------------------------------

fn nested_pair() -> (CompleteTypeObject, EquivalenceHash, CompleteTypeObject, EquivalenceHash) {
    let inner = CompleteTypeObject::Struct(CompleteStructType::new(
        TypeFlag::new(ExtensibilityKind::Final, false, false),
        "Inner".to_string(),
        None,
    ));
    let inner_hash = TypeObject::Complete(inner.clone()).compute_hash();

    let mut outer_struct = CompleteStructType::new(
        TypeFlag::new(ExtensibilityKind::Final, false, false),
        "Outer".to_string(),
        None,
    );
    outer_struct.add_member(CompleteStructMember::new(
        0,
        MemberFlag::default(),
        TypeIdentifier::Int32,
        "id".to_string(),
    ));
    outer_struct.add_member(CompleteStructMember::new(
        1,
        MemberFlag::default(),
        TypeIdentifier::CompleteTypeId(inner_hash),
        "child".to_string(),
    ));
    let outer = CompleteTypeObject::Struct(outer_struct);
    let outer_hash = TypeObject::Complete(outer.clone()).compute_hash();
    (inner, inner_hash, outer, outer_hash)
}

#[test]
fn get_types_closure_reply_resolves_nested_on_requester() {
    let (inner, inner_hash, outer, outer_hash) = nested_pair();

    // Replier registry holds the full local type closure.
    let mut replier = TypeRegistry::new();
    replier.register_type_object(TypeObject::Complete(inner));
    replier.register_type_object(TypeObject::Complete(outer.clone()));

    // Replier answers getTypes(outer) with the transitive closure.
    let types = replier
        .complete_closure(&outer_hash)
        .into_iter()
        .map(|(h, obj)| (TypeIdentifier::CompleteTypeId(h), TypeObject::Complete(obj)))
        .collect::<Vec<_>>();
    assert_eq!(types.len(), 2, "closure must carry both outer and inner");

    let reply = TypeLookupReply {
        header: ReplyHeader {
            related_request_id: SampleIdentity::new(
                Guid::new(
                    [7u8; 12],
                    int2dds::rtps::common::entity_id::EntityId::TYPE_LOOKUP_REQUEST_WRITER,
                ),
                SequenceNumber::new(0, 1),
            ),
            remote_exception_code: 0,
        },
        data: TypeLookupReturn::GetTypes(GetTypesOut { types, complete_to_minimal: Vec::new() }),
    };

    // Wire round-trip (what the requester actually receives).
    let wire = reply.serialize();
    let received = TypeLookupReply::deserialize(&wire).expect("reply must deserialize");

    // Requester registers everything the reply carried.
    let mut requester = TypeRegistry::new();
    let TypeLookupReturn::GetTypes(out) = received.data else {
        panic!("expected getTypes return");
    };
    for (_, type_object) in out.types {
        requester.register_type_object(type_object);
    }

    // The requester can now resolve the nested type with no missing deps.
    assert!(requester.contains(&outer_hash));
    assert!(requester.contains(&inner_hash));
    assert!(requester.missing_dependencies(&outer_hash).is_empty());

    // And building a DynamicType against that registry succeeds (nested child
    // resolves through the registry rather than staying an unresolved ref).
    let dynamic = DynamicType::from_type_object_with_registry(
        std::sync::Arc::new(outer),
        TypeIdentifier::CompleteTypeId(outer_hash),
        &requester,
    );
    assert!(dynamic.is_ok(), "nested DynamicType must build from fetched closure");
}

// ---------------------------------------------------------------------------
// Live two-participant discovery (flat type).
// ---------------------------------------------------------------------------

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct FlatProbe {
    id: i32,
    value: i64,
}

const FLAT_TOPIC: &str = "type_lookup_flat_probe";

#[test]
fn consumer_fetches_type_object_via_type_lookup() {
    // SEDP advertises TypeIdentifier only; the consumer fetches the TypeObject via TypeLookup.
    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Participant A: publishes FlatProbe (no inline TypeObject on the wire).
    let producer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = producer
        .create_topic::<FlatProbe>(
            FLAT_TOPIC,
            "FlatProbe",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        producer.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let _writer = publisher
        .create_datawriter::<FlatProbe>(
            &topic,
            DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration { sec: 1, nanosec: 0 },
                },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Participant B: no compile-time knowledge of FlatProbe.
    let consumer = Arc::new(
        factory
            .create_participant(
                domain,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );

    // Poll: each attempt that finds only a TypeIdentifier triggers a getTypes
    // request; once the reply has populated the registry the topic builds.
    let mut built = false;
    for _ in 0..100 {
        if consumer.create_topic_from_discovered_type(FLAT_TOPIC).is_ok() {
            built = true;
            break;
        }
        sleep(StdDuration::from_millis(100));
    }

    assert!(
        built,
        "consumer must fetch the FlatProbe TypeObject via TypeLookup and build its topic"
    );

    producer.delete_contained_entities().unwrap();
    factory.delete_participant(producer).unwrap();
    consumer.delete_contained_entities().unwrap();
    factory.delete_participant((*consumer).clone()).unwrap();
}

// ---------------------------------------------------------------------------
// Live two-participant discovery (nested + sequence members).
// ---------------------------------------------------------------------------

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct NestedLeaf {
    a: i32,
    b: i64,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct NestedRoot {
    id: i32,
    leaf: NestedLeaf,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct SeqRoot {
    id: i32,
    leaves: Vec<NestedLeaf>,
}

const NESTED_TOPIC: &str = "type_lookup_nested_probe";
const SEQ_TOPIC: &str = "type_lookup_sequence_probe";

#[test]
fn consumer_resolves_nested_member_via_type_lookup() {
    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let producer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = producer
        .create_topic::<NestedRoot>(
            NESTED_TOPIC,
            "NestedRoot",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        producer.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let _writer = publisher
        .create_datawriter::<NestedRoot>(
            &topic,
            DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration { sec: 1, nanosec: 0 },
                },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let consumer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut support = None;
    for _ in 0..100 {
        if let Ok(s) = consumer.create_dynamic_type_support_from_discovered_type(NESTED_TOPIC) {
            support = Some(s);
            break;
        }
        sleep(StdDuration::from_millis(100));
    }

    let support = support.expect(
        "consumer must fetch the NestedRoot closure via TypeLookup and build its type support",
    );

    let dynamic_type = support.dynamic_type();
    let struct_desc = dynamic_type.as_struct().expect("NestedRoot must be a struct");
    let leaf =
        struct_desc.members().iter().find(|m| &*m.name == "leaf").expect("'leaf' member present");

    assert!(
        matches!(leaf.member_type, DynamicTypeKind::TypeRef(_)),
        "nested 'leaf' member must resolve to a TypeRef over the wire, got {:?}",
        leaf.member_type
    );

    producer.delete_contained_entities().unwrap();
    factory.delete_participant(producer).unwrap();
    consumer.delete_contained_entities().unwrap();
    factory.delete_participant(consumer).unwrap();
}

#[test]
fn consumer_resolves_sequence_member_via_type_lookup() {
    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let producer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = producer
        .create_topic::<SeqRoot>(
            SEQ_TOPIC,
            "SeqRoot",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        producer.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let _writer = publisher
        .create_datawriter::<SeqRoot>(
            &topic,
            DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration { sec: 1, nanosec: 0 },
                },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let consumer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut support = None;
    for _ in 0..100 {
        if let Ok(s) = consumer.create_dynamic_type_support_from_discovered_type(SEQ_TOPIC) {
            support = Some(s);
            break;
        }
        sleep(StdDuration::from_millis(100));
    }

    let support = support.expect(
        "consumer must fetch the SeqRoot closure via TypeLookup and build its type support",
    );

    let dynamic_type = support.dynamic_type();
    let struct_desc = dynamic_type.as_struct().expect("SeqRoot must be a struct");
    let leaves = struct_desc
        .members()
        .iter()
        .find(|m| &*m.name == "leaves")
        .expect("'leaves' member present");

    match &leaves.member_type {
        DynamicTypeKind::Sequence { element_type, .. } => assert!(
            matches!(element_type.as_ref(), DynamicTypeKind::TypeRef(_)),
            "sequence element must resolve to a TypeRef over the wire, got {:?}",
            element_type
        ),
        other => panic!("'leaves' member must be a Sequence, got {:?}", other),
    }

    producer.delete_contained_entities().unwrap();
    factory.delete_participant(producer).unwrap();
    consumer.delete_contained_entities().unwrap();
    factory.delete_participant(consumer).unwrap();
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct CoercionWriter {
    id: i32,
    value: i64,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct CoercionReader {
    id: i32,
    value: i64,
}

const COERCION_TOPIC: &str = "type_lookup_deferred_match_probe";
const COERCION_TYPE: &str = "CoercionProbe";

#[test]
fn deferred_match_resolves_via_type_lookup() {
    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let producer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let w_topic = producer
        .create_topic::<CoercionWriter>(
            COERCION_TOPIC,
            COERCION_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        producer.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let writer = publisher
        .create_datawriter::<CoercionWriter>(
            &w_topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let consumer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let r_topic = consumer
        .create_topic::<CoercionReader>(
            COERCION_TOPIC,
            COERCION_TYPE,
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber =
        consumer.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
    let reader = subscriber
        .create_datareader::<CoercionReader>(
            &r_topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let mut matched = false;
    for _ in 0..150 {
        let r = reader
            .get_subscription_matched_status()
            .map(|s| s.current_count() >= 1)
            .unwrap_or(false);
        let w = writer
            .get_publication_matched_status()
            .map(|s| s.current_count() >= 1)
            .unwrap_or(false);
        if r && w {
            matched = true;
            break;
        }
        sleep(StdDuration::from_millis(100));
    }

    assert!(
        matched,
        "deferred match must complete via TypeLookup: writer and reader advertise \
         structurally-identical types under different hashes with no inline TypeObject"
    );
}
