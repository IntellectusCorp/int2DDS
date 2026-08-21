//! Integration tests for `route_gateway::TopicRelay`.
//!
//! Scenario:
//! ```text
//!   [Publisher (domA)] ──→ [LocalNode (domA)]
//!                                │ relay
//!                                ▼
//!                          [RemoteNode (domB)] ──→ [Subscriber (domB)]
//! ```
//! TopicRelay forwards data between two participants on different domains
//! using DynamicData (no compile-time type knowledge in the gateway).

use std::thread::sleep;

use crate::common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    route_gateway::TopicRelay,
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
    xtypes::{HasTypeObject, TypeObject},
};

fn reliable_writer_qos() -> DataWriterQos {
    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

fn reliable_reader_qos() -> DataReaderQos {
    DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    }
}

/// Spin until at least `expected` matched publications/subscriptions or timeout.
fn wait_matched<F: Fn() -> bool>(check: F, timeout_ms: u64) -> bool {
    let step = std::time::Duration::from_millis(50);
    let mut waited = 0u64;
    while waited < timeout_ms {
        if check() {
            return true;
        }
        sleep(step);
        waited += step.as_millis() as u64;
    }
    check()
}

#[test]
fn topic_relay_forwards_local_to_remote() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Source publisher on the LAN side (domain A).
    let publisher_participant = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let pub_writer =
        create_datawriter(&publisher_participant, PublisherQos::default(), reliable_writer_qos());

    // Sink subscriber on the WAN side (domain B).
    let subscriber_participant = factory
        .create_participant(
            domain_remote,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let sub_reader =
        create_datareader(&subscriber_participant, SubscriberQos::default(), reliable_reader_qos());

    // Route Gateway: LocalNode on domain A, RemoteNode on domain B.
    let local_node = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let remote_node = factory
        .create_participant(
            domain_remote,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Build TopicRelay using KeyedDataType's TypeObject (no SEDP wait needed in test).
    let type_object = TypeObject::Complete(KeyedDataType::complete_type_object());
    let relay =
        TopicRelay::new(&local_node, &remote_node, KeyedDataType::get_topic_name(), type_object)
            .expect("Failed to create TopicRelay");

    // Wait until the publisher matches the LocalNode reader,
    // and the RemoteNode writer matches the subscriber.
    let matched = wait_matched(
        || {
            pub_writer
                .get_publication_matched_status()
                .map(|s| s.current_count() >= 1)
                .unwrap_or(false)
                && sub_reader
                    .get_subscription_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
        },
        5000,
    );
    assert!(matched, "publisher/subscriber failed to match through the relay");

    // Publish a few samples on the LAN side.
    for v in 1..=5i16 {
        pub_writer.write(&KeyedDataType::new(1, v * 10), InstanceHandle::NIL).unwrap();
    }

    // Drive the relay until samples appear on the WAN side or timeout.
    let received = wait_matched(
        || {
            // Pump the relay each iteration.
            let _ = relay.forward_local_to_remote();

            // Try to read all samples currently buffered.
            sub_reader
                .read(
                    100,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                )
                .map(|samples| samples.len() >= 5)
                .unwrap_or(false)
        },
        5000,
    );
    assert!(received, "subscriber did not receive 5 forwarded samples");

    // Consume and verify values are intact.
    let samples = sub_reader
        .take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();

    let values: Vec<i16> = samples.iter().filter_map(|s| s.data().ok().map(|d| d.value)).collect();
    assert_eq!(values, vec![10, 20, 30, 40, 50]);

    drop(relay);
    publisher_participant.delete_contained_entities().unwrap();
    factory.delete_participant(publisher_participant).unwrap();
    subscriber_participant.delete_contained_entities().unwrap();
    factory.delete_participant(subscriber_participant).unwrap();
    local_node.delete_contained_entities().unwrap();
    factory.delete_participant(local_node).unwrap();
    remote_node.delete_contained_entities().unwrap();
    factory.delete_participant(remote_node).unwrap();
}

#[test]
fn topic_relay_forwards_remote_to_local() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Source publisher on the WAN side (domain B).
    let publisher_participant = factory
        .create_participant(
            domain_remote,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let pub_writer =
        create_datawriter(&publisher_participant, PublisherQos::default(), reliable_writer_qos());

    // Sink subscriber on the LAN side (domain A).
    let subscriber_participant = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let sub_reader =
        create_datareader(&subscriber_participant, SubscriberQos::default(), reliable_reader_qos());

    // Route Gateway.
    let local_node = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let remote_node = factory
        .create_participant(
            domain_remote,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let type_object = TypeObject::Complete(KeyedDataType::complete_type_object());
    let relay =
        TopicRelay::new(&local_node, &remote_node, KeyedDataType::get_topic_name(), type_object)
            .unwrap();

    let matched = wait_matched(
        || {
            pub_writer
                .get_publication_matched_status()
                .map(|s| s.current_count() >= 1)
                .unwrap_or(false)
                && sub_reader
                    .get_subscription_matched_status()
                    .map(|s| s.current_count() >= 1)
                    .unwrap_or(false)
        },
        5000,
    );
    assert!(matched, "publisher/subscriber failed to match through the relay");

    for v in 1..=3i16 {
        pub_writer.write(&KeyedDataType::new(2, v * 100), InstanceHandle::NIL).unwrap();
    }

    let received = wait_matched(
        || {
            let _ = relay.forward_remote_to_local();
            sub_reader
                .read(
                    100,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                )
                .map(|samples| samples.len() >= 3)
                .unwrap_or(false)
        },
        5000,
    );
    assert!(received, "subscriber did not receive 3 forwarded samples");

    let samples = sub_reader
        .take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();
    let values: Vec<i16> = samples.iter().filter_map(|s| s.data().ok().map(|d| d.value)).collect();
    assert_eq!(values, vec![100, 200, 300]);

    drop(relay);
    publisher_participant.delete_contained_entities().unwrap();
    factory.delete_participant(publisher_participant).unwrap();
    subscriber_participant.delete_contained_entities().unwrap();
    factory.delete_participant(subscriber_participant).unwrap();
    local_node.delete_contained_entities().unwrap();
    factory.delete_participant(local_node).unwrap();
    remote_node.delete_contained_entities().unwrap();
    factory.delete_participant(remote_node).unwrap();
}

#[test]
fn topic_relay_no_data_when_no_publisher() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let local_node = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let remote_node = factory
        .create_participant(
            domain_remote,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let type_object = TypeObject::Complete(KeyedDataType::complete_type_object());
    let relay =
        TopicRelay::new(&local_node, &remote_node, KeyedDataType::get_topic_name(), type_object)
            .unwrap();

    // No publishers anywhere; forward should yield zero samples cleanly.
    let (l2r, r2l) = relay.forward_once().unwrap();
    assert_eq!(l2r, 0);
    assert_eq!(r2l, 0);
    assert_eq!(relay.topic_name(), KeyedDataType::get_topic_name());

    drop(relay);
    local_node.delete_contained_entities().unwrap();
    factory.delete_participant(local_node).unwrap();
    remote_node.delete_contained_entities().unwrap();
    factory.delete_participant(remote_node).unwrap();
}
