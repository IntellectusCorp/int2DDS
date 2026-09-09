use crate::common::*;
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

    // Poll rather than sleep a fixed span: 10 MiB of RELIABLE fragments needs however long
    // repair takes, and a fixed wait turns a slow run into a failure instead of a wait.
    let mut collected: Vec<LargeData> = Vec::new();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    while collected.len() < 10 && std::time::Instant::now() < deadline {
        if let Ok(samples) = data_reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples {
                if let Ok(data) = sample.data() {
                    collected.push(data);
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    assert_eq!(collected.len(), 10, "not every RELIABLE sample was reassembled within 60 s");
    for (i, data) in collected.iter().enumerate() {
        assert_eq!(data.index, i as i32);
        assert_eq!(data.data.len(), 1024 * 1024);
    }

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
