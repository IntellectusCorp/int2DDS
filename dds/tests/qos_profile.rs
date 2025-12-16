mod common;

use std::io::Write;
use tempfile::NamedTempFile;

use common::*;
use int2dds::dcps::{
    core::time::Duration,
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
        },
        {
            "name": "BestEffortProfile",
            "datawriter_qos": {
                "reliability": {
                    "kind": "BEST_EFFORT_RELIABILITY_QOS"
                }
            },
            "datareader_qos": {
                "reliability": {
                    "kind": "BEST_EFFORT_RELIABILITY_QOS"
                }
            }
        },
        {
            "name": "BlockingTimeOnly",
            "datawriter_qos": {
                "reliability": {
                    "max_blocking_time": { "sec": 1, "nanosec": 0 }
                }
            },
            "datareader_qos": {
                "reliability": {
                    "max_blocking_time": { "sec": 1, "nanosec": 0 }
                }
            }
        }]
    }"#;
    file.write_all(json.as_bytes()).unwrap();
    file
}

#[test]
fn test_factory_load_profiles() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();

    let result = factory.load_profiles(&[file.path()]);
    assert!(result.is_ok());
}

#[test]
fn test_get_datawriter_qos_from_profile() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory.get_datawriter_qos_from_profile("TestLibrary::ReliableProfile");
    assert!(qos.is_ok());

    let qos = qos.unwrap();
    assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(10)));
}

#[test]
fn test_get_datareader_qos_from_profile() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory.get_datareader_qos_from_profile("TestLibrary::ReliableProfile");
    assert!(qos.is_ok());

    let qos = qos.unwrap();
    assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    assert!(matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(10)));
}

#[test]
fn test_get_publisher_qos_from_profile() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory.get_publisher_qos_from_profile("TestLibrary::ReliableProfile");
    assert!(qos.is_ok());

    let qos = qos.unwrap();
    assert_eq!(qos.partition.name, vec!["partition1".to_string()]);
}

#[test]
fn test_get_subscriber_qos_from_profile() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory.get_subscriber_qos_from_profile("TestLibrary::ReliableProfile");
    assert!(qos.is_ok());

    let qos = qos.unwrap();
    assert_eq!(qos.partition.name, vec!["partition1".to_string()]);
}

#[test]
fn test_get_topic_qos_from_profile() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory.get_topic_qos_from_profile("TestLibrary::ReliableProfile");
    assert!(qos.is_ok());

    let qos = qos.unwrap();
    assert_eq!(qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
}

#[test]
fn test_profile_not_found() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let qos = factory.get_datawriter_qos_from_profile("NonExistent::Profile");
    assert!(qos.is_err());
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
}

#[test]
fn test_multiple_profiles() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let reliable_qos =
        factory.get_datawriter_qos_from_profile("TestLibrary::ReliableProfile").unwrap();
    let best_effort_qos =
        factory.get_datawriter_qos_from_profile("TestLibrary::BestEffortProfile").unwrap();

    assert_eq!(reliable_qos.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    assert_eq!(best_effort_qos.reliability.kind, ReliabilityQosPolicyKind::BestEffort);
}

#[test]
fn test_blocking_time_only() {
    let file = create_test_profile_file();
    let factory = DomainParticipantFactory::get_instance();
    factory.load_profiles(&[file.path()]).unwrap();

    let datawriter =
        factory.get_datawriter_qos_from_profile("TestLibrary::BlockingTimeOnly").unwrap();
    let datareader =
        factory.get_datareader_qos_from_profile("TestLibrary::BlockingTimeOnly").unwrap();

    assert_eq!(datawriter.reliability.kind, ReliabilityQosPolicyKind::Reliable);
    assert_eq!(datawriter.reliability.max_blocking_time, Duration::from_seconds(1));
    assert_eq!(datareader.reliability.kind, ReliabilityQosPolicyKind::BestEffort);
    assert_eq!(datareader.reliability.max_blocking_time, Duration::from_seconds(1));
}
