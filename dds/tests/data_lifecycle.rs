mod common;

use common::*;
use int2dds::{
    common::{
        env::{set_file_log_level, set_log_type},
        instance_handle::InstanceHandle,
        log::{LogLevel, LogType},
    },
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{HistoryQosPolicy, HistoryQosPolicyKind, WriterDataLifecycleQosPolicy},
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
fn test_autodispose_unregistered_instances_true() {
    // set_log_type(LogType::File);
    // set_file_log_level(LogLevel::Debug);

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
        ..Default::default() // autodispose_unregistered_instances is true by default
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let reader_qos = DataReaderQos {
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

    // Register first
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer.unregister_instance(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE],
        )
        .unwrap();

    assert!(samples.len() == 2);
}

#[test]
fn test_autodispose_unregistered_instances_false() {
    set_log_type(LogType::File);
    set_file_log_level(LogLevel::Debug);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        writer_data_lifecycle: WriterDataLifecycleQosPolicy {
            autodispose_unregistered_instances: false,
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let reader_qos = DataReaderQos {
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

    // Register first
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer.unregister_instance(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE],
        )
        .unwrap();

    for sample in samples.clone() {
        println!("Sample Info: {:?}", sample.sample_info());
    }

    assert!(samples.len() == 2);
}
