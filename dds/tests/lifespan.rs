mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy,
                HistoryQosPolicyKind, LifespanQosPolicy,
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
use std::time::Duration as StdDuration;

#[test]
fn test_lifespan() {
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
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        lifespan: LifespanQosPolicy { duration: Duration::from_millis(1000) },
        ..Default::default()
    };

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);
    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos.clone());

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

    for _ in 0..5 {
        data_writer.write(&KeyedDataType::new(1, 0), InstanceHandle::NIL).unwrap();
    }

    std::thread::sleep(StdDuration::from_millis(100));

    // Read, not take
    let samples = data_reader
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert_eq!(samples.len(), 5, "All written samples should be received");

    // Wait for expiration
    std::thread::sleep(StdDuration::from_millis(900));

    let read_res = data_reader.read(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );

    assert_eq!(
        read_res,
        Err(DdsError::NoData),
        "No data should be available after lifespan expiration"
    );

    let late_joiner = create_datareader(&participant, SubscriberQos::default(), reader_qos);

    wait_for_reader_status(
        &late_joiner,
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

    let res = wait_for_reader_status(
        &late_joiner,
        StatusMask::DATA_AVAILABLE,
        Duration::from_millis(100),
    );

    assert!(res.is_err(), "No data should be received by late joiner because data writer cache is empty due to lifespan expiration");
}
