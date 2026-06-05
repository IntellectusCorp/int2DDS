//! Live two-participant TypeLookup integration test (nested type).
//!
//! Participant A publishes a type whose member is itself a struct, advertising
//! only the top-level `TypeIdentifier` (inline `TypeObject` disabled). Participant
//! B has no compile-time knowledge of either type, so it must pull the whole
//! nested closure from A through the TypeLookup builtin service before it can
//! resolve the nested member.
//!
//! Unlike the flat variant, building the dynamic type alone does not prove the
//! nested member was fetched: an unresolved member stays an `ExternalType` and
//! still "builds". So this test asserts the consumer-side `DynamicType` resolves
//! the nested member to a `TypeRef`, which only happens when the child
//! `TypeObject` crossed the wire and landed in the registry.

mod common;

use std::thread::sleep;
use std::time::Duration as StdDuration;

use common::next_domain_id;
use int2dds::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    topic::{qos::TopicQos, type_support::DdsType},
    xtypes::DynamicTypeKind,
};

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

const TOPIC: &str = "type_lookup_nested_probe";

#[test]
fn consumer_resolves_nested_member_via_type_lookup() {
    unsafe { std::env::set_var("INT2DDS_DISABLE_INLINE_TYPE_OBJECT", "1") };

    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let producer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = producer
        .create_topic::<NestedRoot>(
            TOPIC,
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
        if let Ok(s) = consumer.create_dynamic_type_support_from_discovered_type(TOPIC) {
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
}
