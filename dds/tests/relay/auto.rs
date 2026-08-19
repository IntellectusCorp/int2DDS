//! Integration tests for `route_gateway::AutoRelay`.
//!
//! AutoRelay polls SEDP discovery on both LocalNode and RemoteNode and
//! creates TopicRelays on the fly when a non-builtin publication appears.

use std::{sync::Arc, thread::sleep};

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
    route_gateway::{AutoRelay, TopicFilter},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
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

fn wait_until<F: Fn() -> bool>(check: F, timeout_ms: u64) -> bool {
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
fn auto_relay_discovers_and_forwards_topic() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Real publisher on the LAN side.
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

    // Real subscriber on the WAN side.
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

    // Route Gateway.
    let local_node = Arc::new(
        factory
            .create_participant(
                domain_local,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );
    let remote_node = Arc::new(
        factory
            .create_participant(
                domain_remote,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );

    let auto = AutoRelay::new(local_node.clone(), remote_node.clone(), TopicFilter::default())
        .expect("Failed to create AutoRelay");

    // Wait for SEDP to deliver the publisher's announcement to LocalNode.
    let discovered = wait_until(
        || {
            let _ = auto.discover_once();
            auto.relay_count() >= 1
        },
        5000,
    );
    assert!(discovered, "AutoRelay did not discover the publisher's topic");
    assert!(
        auto.active_topics().contains(&KeyedDataType::get_topic_name().to_string()),
        "Expected topic was not registered"
    );

    // Wait for the relay's own writer/reader pair to match the real endpoints.
    let matched = wait_until(
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

    // Publish on LAN and pump the auto relay.
    for v in 1..=4i16 {
        pub_writer.write(&KeyedDataType::new(1, v * 11), InstanceHandle::NIL).unwrap();
    }

    let received = wait_until(
        || {
            let _ = auto.forward_once();
            sub_reader
                .read(
                    100,
                    &[SampleStateKind::ANY_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                )
                .map(|s| s.len() >= 4)
                .unwrap_or(false)
        },
        5000,
    );
    assert!(received, "subscriber did not receive 4 forwarded samples");

    let samples = sub_reader
        .take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ALIVE_INSTANCE_STATE],
        )
        .unwrap();
    let values: Vec<i16> = samples.iter().filter_map(|s| s.data().ok().map(|d| d.value)).collect();
    assert_eq!(values, vec![11, 22, 33, 44]);

    drop(auto);
    publisher_participant.delete_contained_entities().unwrap();
    factory.delete_participant(publisher_participant).unwrap();
    subscriber_participant.delete_contained_entities().unwrap();
    factory.delete_participant(subscriber_participant).unwrap();
    local_node.delete_contained_entities().unwrap();
    factory.delete_participant((*local_node).clone()).unwrap();
    remote_node.delete_contained_entities().unwrap();
    factory.delete_participant((*remote_node).clone()).unwrap();
}

#[test]
fn auto_relay_filter_excludes_nonmatching_topics() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Publisher on a topic that should NOT match the filter.
    let publisher_participant = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    // KeyedDataType publishes on "test_topic"; we filter for "sensor/*".
    let _pub_writer =
        create_datawriter(&publisher_participant, PublisherQos::default(), reliable_writer_qos());

    let local_node = Arc::new(
        factory
            .create_participant(
                domain_local,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );
    let remote_node = Arc::new(
        factory
            .create_participant(
                domain_remote,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );

    let auto =
        AutoRelay::new(local_node.clone(), remote_node.clone(), TopicFilter::new("sensor/*"))
            .unwrap();

    // Run discovery several times: even though the publication is discoverable,
    // it must not produce a relay because the topic name does not match.
    for _ in 0..20 {
        let _ = auto.discover_once();
        sleep(std::time::Duration::from_millis(50));
    }

    assert_eq!(
        auto.relay_count(),
        0,
        "AutoRelay created relays for topics that do not match the filter"
    );

    drop(auto);
    publisher_participant.delete_contained_entities().unwrap();
    factory.delete_participant(publisher_participant).unwrap();
    local_node.delete_contained_entities().unwrap();
    factory.delete_participant((*local_node).clone()).unwrap();
    remote_node.delete_contained_entities().unwrap();
    factory.delete_participant((*remote_node).clone()).unwrap();
}

#[test]
fn auto_relay_skips_builtin_topics() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // No real publishers; only builtin topics will exist on each participant.
    let local_node = Arc::new(
        factory
            .create_participant(
                domain_local,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );
    let remote_node = Arc::new(
        factory
            .create_participant(
                domain_remote,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(),
    );

    let auto =
        AutoRelay::new(local_node.clone(), remote_node.clone(), TopicFilter::default()).unwrap();

    for _ in 0..10 {
        let _ = auto.discover_once();
        sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(auto.relay_count(), 0);

    drop(auto);
    local_node.delete_contained_entities().unwrap();
    factory.delete_participant((*local_node).clone()).unwrap();
    remote_node.delete_contained_entities().unwrap();
    factory.delete_participant((*remote_node).clone()).unwrap();
}

#[test]
fn topic_filter_matches() {
    assert!(TopicFilter::new("*").matches("anything"));
    assert!(TopicFilter::new("sensor/*").matches("sensor/temp"));
    assert!(TopicFilter::new("sensor/*").matches("sensor/"));
    assert!(!TopicFilter::new("sensor/*").matches("camera/feed"));
    assert!(TopicFilter::new("exact").matches("exact"));
    assert!(!TopicFilter::new("exact").matches("exactly"));
}
