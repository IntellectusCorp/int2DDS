mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy,
                HistoryQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
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

#[test]
fn test_volatile_stateless_best_effort() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
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

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    // Write data from seq 0 to 4 before matching
    for i in 0..=4 {
        data_writer.write(&KeyedDataType::new(1, i), InstanceHandle::NIL).unwrap();
    }

    let data_reader =
        create_datareader(&participant, SubscriberQos::default(), DataReaderQos::default());

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

    data_writer.write(&KeyedDataType::new(1, 5), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();
    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples[0].data().unwrap().value, 5);
    assert_eq!(samples.len(), 1);
}

#[test]
fn test_volatile_stateful_best_effort() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    // Stateful writer to best effort reader proxy
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

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    // Write data from seq 0 to 4 before matching
    for i in 0..=4 {
        data_writer.write(&KeyedDataType::new(1, i), InstanceHandle::NIL).unwrap();
    }

    let data_reader =
        create_datareader(&participant, SubscriberQos::default(), DataReaderQos::default());

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

    data_writer.write(&KeyedDataType::new(1, 5), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();
    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples[0].data().unwrap().value, 5);
    assert_eq!(samples.len(), 1);
}

#[test]
fn test_volatile_reliable() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
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

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    // Write data from seq 0 to 4 before matching
    for i in 0..=4 {
        data_writer.write(&KeyedDataType::new(1, i), InstanceHandle::NIL).unwrap();
    }

    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

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

    data_writer.write(&KeyedDataType::new(1, 5), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();
    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples[0].data().unwrap().value, 5);
    assert_eq!(samples.len(), 1);
}

#[test]
fn test_transient_local_best_effort() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::BestEffort,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    // Write data from seq 0 to 4 before matching
    for i in 0..=4 {
        data_writer.write(&KeyedDataType::new(1, i), InstanceHandle::NIL).unwrap();
    }

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

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

    data_writer.write(&KeyedDataType::new(1, 5), InstanceHandle::NIL).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples[0].data().unwrap().value, 0);
    assert_eq!(samples.len(), 6);
}

#[test]
fn test_transient_local_reliable() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    // Write data from seq 0 to 4 before matching
    for i in 0..=4 {
        data_writer.write(&KeyedDataType::new(1, i), InstanceHandle::NIL).unwrap();
    }

    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

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

    data_writer.write(&KeyedDataType::new(1, 5), InstanceHandle::NIL).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples[0].data().unwrap().value, 0);
    assert_eq!(samples.len(), 6);
}
