//! Live two-participant TypeLookup integration test (flat type).
//!
//! Participant A publishes a type while advertising only its TypeIdentifier
//! (inline TypeObject disabled). Participant B has no compile-time knowledge of
//! the type, so it must pull the TypeObject from A through the TypeLookup
//! builtin service before it can build a dynamic topic for it.
//!
//! This pins the live wiring added in stage 2 (endpoint matching, request/reply
//! dispatch, registry population) end-to-end over real RTPS, with the simplest
//! possible payload. The nested-type variant is covered separately once the
//! derive macro registers nested object closures.

mod common;

use std::sync::Arc;
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
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct FlatProbe {
    id: i32,
    value: i64,
}

const TOPIC: &str = "type_lookup_flat_probe";

#[test]
fn consumer_fetches_type_object_via_type_lookup() {
    // Advertise TypeIdentifier only, forcing the TypeLookup fetch path.
    unsafe { std::env::set_var("INT2DDS_DISABLE_INLINE_TYPE_OBJECT", "1") };

    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Participant A: publishes FlatProbe (no inline TypeObject on the wire).
    let producer = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = producer
        .create_topic::<FlatProbe>(
            TOPIC,
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
        if consumer.create_topic_from_discovered_type(TOPIC).is_ok() {
            built = true;
            break;
        }
        sleep(StdDuration::from_millis(100));
    }

    assert!(
        built,
        "consumer must fetch the FlatProbe TypeObject via TypeLookup and build its topic"
    );
}
