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
    },
};
use std::thread;

// Reliable + KEEP_ALL so every sample that passes the filter is retained for take().
fn reliable_keep_all_writer_qos() -> DataWriterQos {
    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, ..Default::default() },
        ..Default::default()
    }
}

fn reliable_keep_all_reader_qos(min_separation: Duration) -> DataReaderQos {
    let mut qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, ..Default::default() },
        ..Default::default()
    };
    qos.time_based_filter.minimum_separation = min_separation;
    qos
}

fn collect_values(
    data_reader: &int2dds::dcps::subscription::data_reader::DataReader<KeyedDataType>,
) -> Vec<i16> {
    let samples = data_reader
        .take(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap_or_default();
    samples.iter().filter_map(|s| s.data().ok().map(|d| d.value)).collect()
}

// Rapid writes to one instance within a single separation window must be reduced to the first
// sample plus the most recent one, which the one-shot timer delivers after the writer stops.
// This is the steady-state last-sample guarantee: a drop-on-arrival filter would lose value 40.
#[test]
fn test_time_based_filter_delivers_first_and_last() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let min_separation = Duration::from_millis(400);
    let data_writer =
        create_datawriter(&participant, PublisherQos::default(), reliable_keep_all_writer_qos());

    let topic = participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            int2dds::dcps::topic::qos::TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reliable_keep_all_reader_qos(min_separation),
            None,
            StatusMask::default(),
        )
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

    // Four writes to the same instance inside one 400ms window.
    for value in [10_i16, 20, 30, 40] {
        data_writer.write(&KeyedDataType { key: 1, value }, InstanceHandle::NIL).unwrap();
        thread::sleep(std::time::Duration::from_millis(80));
    }

    // Wait past the window so the held last sample is delivered by the timer.
    thread::sleep(std::time::Duration::from_millis(600));

    let values = collect_values(&data_reader);
    println!("delivered values: {:?}", values);

    assert!(values.contains(&10), "first sample must pass immediately, got {:?}", values);
    assert!(
        values.contains(&40),
        "last sample must be delivered after the window (steady-state guarantee), got {:?}",
        values
    );
    assert!(values.len() < 4, "filter must drop intermediate samples, got {:?}", values);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

// After the separation window elapses, the next write passes immediately.
#[test]
fn test_time_based_filter_passes_after_separation() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let min_separation = Duration::from_millis(200);
    let data_writer =
        create_datawriter(&participant, PublisherQos::default(), reliable_keep_all_writer_qos());

    let topic = participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            int2dds::dcps::topic::qos::TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reliable_keep_all_reader_qos(min_separation),
            None,
            StatusMask::default(),
        )
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

    data_writer.write(&KeyedDataType { key: 1, value: 1 }, InstanceHandle::NIL).unwrap();
    thread::sleep(std::time::Duration::from_millis(350)); // exceed the window
    data_writer.write(&KeyedDataType { key: 1, value: 2 }, InstanceHandle::NIL).unwrap();
    thread::sleep(std::time::Duration::from_millis(150));

    let values = collect_values(&data_reader);
    println!("delivered values: {:?}", values);

    assert!(
        values.contains(&1) && values.contains(&2),
        "both spaced writes must pass, got {:?}",
        values
    );

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

// The filter is per instance: the first sample of each distinct key passes without affecting
// the other instance's window.
#[test]
fn test_time_based_filter_per_instance() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let min_separation = Duration::from_millis(400);
    let data_writer =
        create_datawriter(&participant, PublisherQos::default(), reliable_keep_all_writer_qos());

    let topic = participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            int2dds::dcps::topic::qos::TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reliable_keep_all_reader_qos(min_separation),
            None,
            StatusMask::default(),
        )
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

    // First sample of each instance, written back to back, must both pass immediately.
    data_writer.write(&KeyedDataType { key: 1, value: 100 }, InstanceHandle::NIL).unwrap();
    data_writer.write(&KeyedDataType { key: 2, value: 200 }, InstanceHandle::NIL).unwrap();
    thread::sleep(std::time::Duration::from_millis(150));

    let values = collect_values(&data_reader);
    println!("delivered values: {:?}", values);

    assert!(
        values.contains(&100) && values.contains(&200),
        "each instance's first sample must pass independently, got {:?}",
        values
    );

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
