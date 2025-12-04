mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{
            domain_participant::DomainParticipant,
            domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
        },
        infrastructure::{status::StatusMask, wait_set::WaitSet},
        publication::{
            data_writer::DataWriter,
            qos::{DataWriterQos, PublisherQos},
        },
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
    },
};

fn create_abc_datawriter(domain_participant: &DomainParticipant) -> DataWriter<KeyedDataType> {
    let mut publisher_qos = PublisherQos::default();
    publisher_qos.partition.name.push("partition_A".to_string());
    publisher_qos.partition.name.push("partition_B".to_string());
    publisher_qos.partition.name.push("partition_C".to_string());

    create_datawriter(domain_participant, publisher_qos, DataWriterQos::default())
}

#[test]
fn test_partition_a_subscriber_success() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer = create_abc_datawriter(&participant);

    let mut subscriber_qos = SubscriberQos::default();
    subscriber_qos.partition.name.push("partition_A".to_string());

    let data_reader = create_datareader(&participant, subscriber_qos, DataReaderQos::default());

    wait_for_reader_status(&data_reader, StatusMask::SUBSCRIPTION_MATCHED);
    wait_for_writer_status(&data_writer, StatusMask::PUBLICATION_MATCHED);

    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE);

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert!(!samples.is_empty());
}

#[test]
fn test_partition_abc_subscriber_success() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer = create_abc_datawriter(&participant);

    let mut subscriber_qos = SubscriberQos::default();
    subscriber_qos.partition.name.push("partition_A".to_string());
    subscriber_qos.partition.name.push("partition_B".to_string());
    subscriber_qos.partition.name.push("partition_C".to_string());

    let data_reader = create_datareader(&participant, subscriber_qos, DataReaderQos::default());

    wait_for_reader_status(&data_reader, StatusMask::SUBSCRIPTION_MATCHED);
    wait_for_writer_status(&data_writer, StatusMask::PUBLICATION_MATCHED);

    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE);

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert!(!samples.is_empty());
}

#[test]
fn test_partition_asterisk_subscriber_success() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer = create_abc_datawriter(&participant);

    let mut subscriber_qos = SubscriberQos::default();
    subscriber_qos.partition.name.push("*".to_string());

    let data_reader = create_datareader(&participant, subscriber_qos, DataReaderQos::default());

    wait_for_reader_status(&data_reader, StatusMask::SUBSCRIPTION_MATCHED);
    wait_for_writer_status(&data_writer, StatusMask::PUBLICATION_MATCHED);

    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE);

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert!(!samples.is_empty());
}

#[test]
fn test_partition_d_subscriber_fail() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer = create_abc_datawriter(&participant);

    let mut subscriber_qos = SubscriberQos::default();
    subscriber_qos.partition.name.push("partition_D".to_string());

    let data_reader = create_datareader(&participant, subscriber_qos, DataReaderQos::default());

    let mut condition = data_reader.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
    let reader_waitset = WaitSet::new();
    reader_waitset.attach_condition(condition).unwrap();

    let mut condition = data_writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
    let writer_waitset = WaitSet::new();
    writer_waitset.attach_condition(condition).unwrap();

    // They should not match
    assert!(reader_waitset.wait(Duration::from_seconds(1)).is_err());
    assert!(writer_waitset.wait(Duration::from_seconds(1)).is_err());
}
