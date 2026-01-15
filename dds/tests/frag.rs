mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
                ReliabilityQosPolicyKind,
            },
            status::StatusMask,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::{qos::TopicQos, type_support::DdsType},
    },
};

/**
 * Tests fragmentation with large data samples
 * Need to configure the underlying transport to support fragmentation
 */

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct LargeData {
    index: i32,
    data: Vec<u8>,
}

#[test]
fn test_large_data_best_effort() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<LargeData>(
            "test_topic",
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_writer = publisher
        .create_datawriter::<LargeData>(&topic, writer_qos, None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<LargeData>(&topic, reader_qos, None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    // Write 1MB data
    for i in 0..10 {
        data_writer
            .write(&LargeData { index: i, data: vec![0; 1024 * 1024] }, InstanceHandle::NIL)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples.len(), 10);
    for (i, sample) in samples.iter().enumerate() {
        assert_eq!(sample.data().unwrap().index, i as i32);
        assert_eq!(sample.data().unwrap().data.len(), 1024 * 1024);
    }
}

#[test]
fn test_large_data_reliable() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<LargeData>(
            "test_topic",
            "LargeData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        ..Default::default()
    };

    let data_writer = publisher
        .create_datawriter::<LargeData>(&topic, writer_qos, None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<LargeData>(&topic, reader_qos, None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    // Write 1MB data
    for i in 0..10 {
        data_writer
            .write(&LargeData { index: i, data: vec![0; 1024 * 1024] }, InstanceHandle::NIL)
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples.len(), 10);
    for (i, sample) in samples.iter().enumerate() {
        assert_eq!(sample.data().unwrap().index, i as i32);
        assert_eq!(sample.data().unwrap().data.len(), 1024 * 1024);
    }
}
