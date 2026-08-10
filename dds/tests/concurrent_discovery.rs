//! Regression test for SEDP built-in topic instance keying.
//!
//! The DCPSPublication built-in reader uses KeepLast(1). When every
//! CacheChange is stored under InstanceHandle::NIL, only the most recent
//! publication survives in the cache — earlier ones are evicted on
//! add_change. A consumer that reads the built-in reader after all
//! announcements have arrived will therefore see only the last one.
//!
//! This test deliberately lets all SEDP announcements settle into the
//! built-in cache *before* reading it, so the outcome is deterministic: with
//! NIL instances only the last writer's topic survives, with per-endpoint
//! instances all N do.

mod common;

use std::{collections::HashSet, thread::sleep};

use common::*;
use int2dds::{
    common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
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
    subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    topic::qos::TopicQos,
};

const N_TOPICS: usize = 8;

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

/// Every topic name the observer's built-in publication reader can still see.
fn discovered_topics(observer: &DomainParticipant) -> HashSet<String> {
    let builtin = observer.get_builtin_subscriber().expect("no builtin subscriber");
    let reader = builtin
        .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")
        .expect("no DCPSPublication reader");

    let mut topics = HashSet::new();
    // Read rather than take, so repeated polling keeps seeing what the cache
    // holds: whether earlier announcements survived at all is the question.
    if let Ok(samples) = reader.read(
        1000,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ALIVE_INSTANCE_STATE],
    ) {
        for sample in samples.iter() {
            if let Ok(data) = sample.data() {
                topics.insert(data.topic_name().to_string());
            }
        }
    }
    topics
}

/// Create the observer first so it receives SEDP from the publisher
/// participant. Then burst-create N writers and wait long enough for all
/// announcements to be delivered and stored in the built-in cache. Only then
/// read it — by that point the cache either holds all N samples
/// (instance-per-endpoint) or just the last one (NIL).
#[test]
fn every_burst_created_writer_survives_in_the_builtin_cache() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let observer = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let publisher_participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic_names: Vec<String> = (0..N_TOPICS).map(|i| format!("burst_topic_{i}")).collect();
    let _writers: Vec<DataWriter<KeyedDataType>> =
        topic_names.iter().map(|name| make_writer(&publisher_participant, name)).collect();

    sleep(std::time::Duration::from_secs(3));

    let mut discovered = discovered_topics(&observer);
    for _ in 0..20 {
        if topic_names.iter().all(|name| discovered.contains(name)) {
            break;
        }
        sleep(std::time::Duration::from_millis(50));
        discovered = discovered_topics(&observer);
    }

    let missing: Vec<&String> = topic_names.iter().filter(|n| !discovered.contains(*n)).collect();
    assert!(
        missing.is_empty(),
        "built-in cache lost {} of {} publications, missing={:?}",
        missing.len(),
        N_TOPICS,
        missing
    );

    observer.delete_contained_entities().unwrap();
    factory.delete_participant(observer).unwrap();
    publisher_participant.delete_contained_entities().unwrap();
    factory.delete_participant(publisher_participant).unwrap();
}
