mod common;

use std::io::Write;
use tempfile::NamedTempFile;

use common::*;
use int2dds::dcps::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{HistoryQosPolicyKind, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::PublisherQos,
    subscription::qos::SubscriberQos,
    topic::qos::TopicQos,
};

fn create_test_profile_file() -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    // max_blocking_time is optional - defaults to 100ms if not specified
    let json = r#"{
        "name": "TestLibrary",
        "qos_profiles": [{
            "name": "ReliableProfile",
            "publisher_qos": {
                "partition": {
                    "name": { "element": ["partition1"] }
                }
            },
            "subscriber_qos": {
                "partition": {
                    "name": { "element": ["partition1"] }
                }
            },
            "topic_qos": {
                "reliability": {
                    "kind": "RELIABLE_RELIABILITY_QOS"
                }
            },
            "datawriter_qos": {
                "reliability": {
                    "kind": "RELIABLE_RELIABILITY_QOS"
                },
                "history": {
                    "kind": "KEEP_LAST_HISTORY_QOS",
                    "depth": 10
                }
            },
            "datareader_qos": {
                "reliability": {
                    "kind": "RELIABLE_RELIABILITY_QOS"
                },
                "history": {
                    "kind": "KEEP_LAST_HISTORY_QOS",
                    "depth": 10
                }
            }
        }]
    }"#;
    file.write_all(json.as_bytes()).unwrap();
    file
}

#[test]
fn test_create_publisher_with_profile() {
    let file = create_test_profile_file();
    let domain_id = next_domain_id();

    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let publisher = participant.create_publisher_with_profile(
        "TestLibrary::ReliableProfile",
        None,
        StatusMask::default(),
    );

    assert!(publisher.is_ok());

    let publisher = publisher.unwrap();
    let qos = publisher.get_qos().unwrap();
    assert_eq!(qos.partition.name, vec!["partition1".to_string()]);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_create_subscriber_with_profile() {
    let file = create_test_profile_file();
    let domain_id = next_domain_id();

    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let subscriber = participant.create_subscriber_with_profile(
        "TestLibrary::ReliableProfile",
        None,
        StatusMask::default(),
    );

    assert!(subscriber.is_ok());

    let subscriber = subscriber.unwrap();
    let qos = subscriber.get_qos().unwrap();
    assert_eq!(qos.partition.name, vec!["partition1".to_string()]);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_create_topic_with_profile() {
    let file = create_test_profile_file();
    let domain_id = next_domain_id();

    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant.create_topic_with_profile::<KeyedDataType>(
        "TestTopic",
        "KeyedDataType",
        "TestLibrary::ReliableProfile",
        None,
        StatusMask::default(),
    );

    assert!(topic.is_ok());

    let topic = topic.unwrap();
    let qos = topic.get_qos().unwrap();
    assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_create_datawriter_with_profile() {
    let file = create_test_profile_file();
    let domain_id = next_domain_id();

    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "TestTopic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let writer = publisher.create_datawriter_with_profile::<KeyedDataType>(
        &topic,
        "TestLibrary::ReliableProfile",
        None,
        StatusMask::default(),
    );

    assert!(writer.is_ok());

    let writer = writer.unwrap();
    let qos = writer.get_qos().unwrap();
    assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(10)));

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_create_datareader_with_profile() {
    let file = create_test_profile_file();
    let domain_id = next_domain_id();

    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "TestTopic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let reader = subscriber.create_datareader_with_profile::<KeyedDataType>(
        &topic,
        "TestLibrary::ReliableProfile",
        None,
        StatusMask::default(),
    );

    assert!(reader.is_ok());

    let reader = reader.unwrap();
    let qos = reader.get_qos().unwrap();
    assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(10)));

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
