//! Regression test for SEDP built-in topic instance keying.
//!
//! The DCPSPublication built-in reader uses KeepLast(1). When every
//! CacheChange is stored under InstanceHandle::NIL, only the most recent
//! publication survives in the cache — earlier ones are evicted on
//! add_change. A polling consumer like AutoRelay that calls read() after
//! all announcements have arrived will therefore see only the last one.
//!
//! This test deliberately lets all SEDP announcements settle into the
//! built-in cache *before* the first discover_once() call, so the outcome
//! is deterministic: with NIL instances only 1 relay is created (the last
//! writer's topic), with per-endpoint instances all N relays are created.

mod common;

use std::{sync::Arc, thread::sleep};

use common::*;
use int2dds::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
        },
        status::StatusMask,
    },
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    route_gateway::{AutoRelay, TopicFilter},
    topic::qos::TopicQos,
};

const N_TOPICS: usize = 8;

fn reliable_writer_qos() -> DataWriterQos {
    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
        ..Default::default()
    }
}

fn make_writer(
    participant: &int2dds::domain::domain_participant::DomainParticipant,
    topic_name: &str,
) -> DataWriter<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            reliable_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

/// Create the AutoRelay local_node first so it receives SEDP from the
/// publisher participant.  Then burst-create N writers and wait long enough
/// for all SEDP announcements to be delivered and stored in the built-in
/// cache.  Only then call discover_once() — by this point the cache either
/// holds all N samples (instance-per-endpoint) or just the last one (NIL).
#[test]
fn auto_relay_discovers_all_topics_after_sedp_settles() {
    let domain_local = next_domain_id();
    let domain_remote = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    // Step 1: Create AutoRelay's local_node FIRST so it is already
    // listening when the publisher participant joins.
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
    let auto = AutoRelay::new(
        Arc::clone(&local_node),
        Arc::clone(&remote_node),
        TopicFilter::default(),
    )
    .expect("Failed to create AutoRelay");

    // Step 2: Create publisher participant + N writers in a burst.
    let publisher_participant = factory
        .create_participant(
            domain_local,
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let topic_names: Vec<String> =
        (0..N_TOPICS).map(|i| format!("burst_topic_{i}")).collect();
    let _writers: Vec<DataWriter<KeyedDataType>> = topic_names
        .iter()
        .map(|name| make_writer(&publisher_participant, name))
        .collect();

    // Step 3: Wait for SEDP to deliver all announcements to local_node's
    // built-in cache. With KeepLast(1) + NIL, only the last writer's
    // sample survives; with per-endpoint keying, all N survive.
    sleep(std::time::Duration::from_secs(3));

    // Step 4: Now poll discovery. If instances were correctly separated,
    // all N topics produce relays. If NIL, only 1 (or very few).
    for _ in 0..20 {
        let _ = auto.discover_once();
        sleep(std::time::Duration::from_millis(50));
    }

    let active = auto.active_topics();
    let relay_count = auto.relay_count();

    if relay_count < N_TOPICS {
        let missing: Vec<&String> =
            topic_names.iter().filter(|n| !active.contains(n)).collect();
        panic!(
            "AutoRelay missed topics: relay_count={}, expected={}, missing={:?}",
            relay_count, N_TOPICS, missing
        );
    }

    for name in &topic_names {
        assert!(
            active.contains(name),
            "expected topic '{name}' to be relayed, got {active:?}"
        );
    }
}
