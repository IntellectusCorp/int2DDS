//! Regression tests for the SEDP store/lookup ordering race.
//!
//! Two independent code paths must meet for a local endpoint and a remote
//! endpoint to match:
//!
//! * the **local creation** path (`DcpsBridge::create_rtps_reader/writer`)
//!   registers the endpoint in the participant store, then scans the
//!   already-known remote endpoints for the topic;
//! * the **SEDP receive** path (`SedpLogic::handle_*_builtin_topic_data`)
//!   looks up local endpoints for the topic, then stores the announcement.
//!
//! Each path publishes one fact and reads the other path's fact. As long as
//! both publish *before* they read, at least one of the two is guaranteed to
//! observe the other, so the pair always matches. If one path reads before it
//! publishes, an interleaving exists where both read too early and *neither*
//! matches — SEDP endpoint announcements are sent once, so nothing repairs it
//! and the endpoints stay unmatched forever.
//!
//! These tests create the two sides of a pair concurrently, so the local
//! creation of one endpoint overlaps the arrival of the other's announcement.

use std::{
    thread::sleep,
    time::{Duration as StdDuration, Instant},
};

use crate::common::*;
use int2dds::{
    core::time::Duration,
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
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
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
    },
    topic::qos::TopicQos,
};

/// Number of independent endpoint pairs created per stress test. Each pair is
/// one trial of the race, so a low per-trial probability still shows up.
const PAIRS: usize = 24;

/// How long the announcement of the first-created side is given to arrive
/// while the second side is being created.
const OVERLAP: StdDuration = StdDuration::from_millis(60);

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

fn make_writer(participant: &DomainParticipant, topic_name: &str) -> DataWriter<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            reliable_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn make_reader(participant: &DomainParticipant, topic_name: &str) -> DataReader<KeyedDataType> {
    let topic = participant
        .create_topic::<KeyedDataType>(
            topic_name,
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reliable_reader_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn wait_until<F: FnMut() -> bool>(mut check: F, timeout: StdDuration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if check() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        sleep(StdDuration::from_millis(10));
    }
}

fn reader_matched(reader: &DataReader<KeyedDataType>) -> bool {
    reader.get_subscription_matched_status().map(|s| s.current_count() >= 1).unwrap_or(false)
}

fn writer_matched(writer: &DataWriter<KeyedDataType>) -> bool {
    writer.get_publication_matched_status().map(|s| s.current_count() >= 1).unwrap_or(false)
}

struct Peers {
    factory: &'static DomainParticipantFactory,
    left: DomainParticipant,
    right: DomainParticipant,
}

impl Peers {
    fn new() -> Self {
        let domain = next_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let left = factory
            .create_participant(
                domain,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let right = factory
            .create_participant(
                domain,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        // Let SPDP settle so endpoint announcements are delivered promptly
        // rather than being queued behind participant discovery.
        sleep(StdDuration::from_millis(1500));
        Self { factory, left, right }
    }

    fn shutdown(self) {
        self.left.delete_contained_entities().unwrap();
        self.factory.delete_participant(self.left).unwrap();
        self.right.delete_contained_entities().unwrap();
        self.factory.delete_participant(self.right).unwrap();
    }
}

/// The remote **writer** is announced while the local reader is being created.
///
/// Exercises `handle_publication_builtin_topic_data` (lookup of local readers)
/// against `create_rtps_reader` (registration of the local reader).
#[test]
fn reader_matches_writer_announced_during_reader_creation() {
    let peers = Peers::new();

    let mut writers = Vec::new();
    let mut readers = Vec::new();

    for i in 0..PAIRS {
        let topic = format!("sedp_race_pub_first_{i}");
        // The writer's SEDP announcement is in flight toward `right` ...
        writers.push(make_writer(&peers.left, &topic));
        sleep(OVERLAP);
        // ... and lands while the reader is registering itself.
        readers.push(make_reader(&peers.right, &topic));
    }

    let unmatched: Vec<usize> = (0..PAIRS)
        .filter(|&i| !wait_until(|| reader_matched(&readers[i]), StdDuration::from_secs(3)))
        .collect();

    assert!(
        unmatched.is_empty(),
        "{}/{} readers never matched the remote writer announced during their creation: {:?}",
        unmatched.len(),
        PAIRS,
        unmatched
    );

    drop(readers);
    drop(writers);
    peers.shutdown();
}

/// The remote **reader** is announced while the local writer is being created.
///
/// Exercises `handle_subscription_builtin_topic_data` (lookup of local writers)
/// against `create_rtps_writer` (registration of the local writer).
#[test]
fn writer_matches_reader_announced_during_writer_creation() {
    let peers = Peers::new();

    let mut readers = Vec::new();
    let mut writers = Vec::new();

    for i in 0..PAIRS {
        let topic = format!("sedp_race_sub_first_{i}");
        // The reader's SEDP announcement is in flight toward `left` ...
        readers.push(make_reader(&peers.right, &topic));
        sleep(OVERLAP);
        // ... and lands while the writer is registering itself.
        writers.push(make_writer(&peers.left, &topic));
    }

    let unmatched: Vec<usize> = (0..PAIRS)
        .filter(|&i| !wait_until(|| writer_matched(&writers[i]), StdDuration::from_secs(3)))
        .collect();

    assert!(
        unmatched.is_empty(),
        "{}/{} writers never matched the remote reader announced during their creation: {:?}",
        unmatched.len(),
        PAIRS,
        unmatched
    );

    drop(writers);
    drop(readers);
    peers.shutdown();
}

/// A matched pair must report exactly one counterpart, not two.
///
/// Both the local-creation path and the SEDP-receive path may legitimately
/// reach the same (local endpoint, remote endpoint) pair concurrently. The
/// match must be applied once: a second application would push a duplicate
/// proxy and double-count `current_count`.
#[test]
fn concurrent_match_paths_do_not_double_count() {
    let peers = Peers::new();

    let mut writers = Vec::new();
    let mut readers = Vec::new();

    for i in 0..PAIRS {
        let topic = format!("sedp_race_dup_{i}");
        // Let the writer's announcement reach `right` and be recorded there,
        // then create the reader: the reader's own scan of the recorded
        // announcements and the announcement handler's scan of the freshly
        // registered reader can now both reach this pair.
        writers.push(make_writer(&peers.left, &topic));
        sleep(OVERLAP);
        readers.push(make_reader(&peers.right, &topic));
    }

    for i in 0..PAIRS {
        assert!(
            wait_until(|| reader_matched(&readers[i]), StdDuration::from_secs(3)),
            "reader {i} never matched"
        );
        assert!(
            wait_until(|| writer_matched(&writers[i]), StdDuration::from_secs(3)),
            "writer {i} never matched"
        );
    }

    // Give any duplicate application time to land before sampling the counts.
    sleep(StdDuration::from_millis(500));

    let bad_readers: Vec<(usize, i32)> = (0..PAIRS)
        .filter_map(|i| {
            let c = readers[i].get_subscription_matched_status().unwrap().current_count();
            (c != 1).then_some((i, c))
        })
        .collect();
    let bad_writers: Vec<(usize, i32)> = (0..PAIRS)
        .filter_map(|i| {
            let c = writers[i].get_publication_matched_status().unwrap().current_count();
            (c != 1).then_some((i, c))
        })
        .collect();

    assert!(
        bad_readers.is_empty() && bad_writers.is_empty(),
        "matched counts must be exactly 1 per pair; readers {:?}, writers {:?}",
        bad_readers,
        bad_writers
    );

    drop(readers);
    drop(writers);
    peers.shutdown();
}
