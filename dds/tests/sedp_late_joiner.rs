//! A participant that joins after its peer already announced its endpoints must still discover
//! every one of them.
//!
//! The peer's SEDP writer holds those announcements in its history and nothing pumps a builtin
//! writer's unsent changes, so discovery has to push them. Heartbeat plus repair is the backstop,
//! not the mechanism.

mod common;

use std::{
    thread::sleep,
    time::{Duration as StdDuration, Instant},
};

use common::*;
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

/// Endpoints the established participant owns before the late joiner appears.
const ENDPOINTS: usize = 12;

/// Time for the established participant's announcements to land in its own SEDP history.
const SETTLE: StdDuration = StdDuration::from_millis(1500);

const DISCOVERY_TIMEOUT: StdDuration = StdDuration::from_secs(20);

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
        sleep(StdDuration::from_millis(20));
    }
}

fn matched_readers(readers: &[DataReader<KeyedDataType>]) -> usize {
    readers
        .iter()
        .filter(|reader| {
            reader
                .get_subscription_matched_status()
                .map(|s| s.current_count() >= 1)
                .unwrap_or(false)
        })
        .count()
}

fn matched_writers(writers: &[DataWriter<KeyedDataType>]) -> usize {
    writers
        .iter()
        .filter(|writer| {
            writer.get_publication_matched_status().map(|s| s.current_count() >= 1).unwrap_or(false)
        })
        .count()
}

#[test]
fn late_joiner_recovers_endpoints_announced_before_it_started() {
    let domain = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let established = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topics: Vec<String> =
        (0..ENDPOINTS).map(|i| format!("late_joiner_topic_{}_{}", domain, i)).collect();

    let writers: Vec<_> = topics.iter().map(|t| make_writer(&established, t)).collect::<Vec<_>>();

    // Every announcement is in the established participant's SEDP history before the late joiner
    // exists, so none of them can be pushed at creation time.
    sleep(SETTLE);

    let late_joiner = factory
        .create_participant(domain, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let readers: Vec<_> = topics.iter().map(|t| make_reader(&late_joiner, t)).collect::<Vec<_>>();

    let all_matched = wait_until(|| matched_readers(&readers) == ENDPOINTS, DISCOVERY_TIMEOUT);

    let recovered = matched_readers(&readers);
    let peer_side = matched_writers(&writers);

    late_joiner.delete_contained_entities().unwrap();
    factory.delete_participant(late_joiner).unwrap();
    established.delete_contained_entities().unwrap();
    factory.delete_participant(established).unwrap();

    assert!(
        all_matched,
        "late joiner matched {recovered}/{ENDPOINTS} of the peer's writers (the peer matched \
         {peer_side}/{ENDPOINTS} of the late joiner's readers)"
    );
}
