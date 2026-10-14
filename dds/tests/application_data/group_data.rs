// GROUP_DATA set on a Publisher or Subscriber must be readable by the matched remote endpoint.

use crate::{common::*, create_participant, delete_participant, wait_for};
use int2dds::{
    common::instance_handle::InstanceHandle,
    domain::qos::DomainParticipantQos,
    infrastructure::{qos_policy::GroupDataQosPolicy, status::StatusMask},
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
    },
};

// Publisher GROUP_DATA of the matched writer as the reader sees it.
fn matched_publication_group_data(
    data_reader: &DataReader<KeyedDataType>,
    writer_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let publication_data = data_reader.get_matched_publication_data(writer_handle).ok()?;
    Some(publication_data.group_data().value.clone())
}

// Subscriber GROUP_DATA of the matched reader as the writer sees it.
fn matched_subscription_group_data(
    data_writer: &DataWriter<KeyedDataType>,
    reader_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let subscription_data = data_writer.get_matched_subscription_data(reader_handle).ok()?;
    Some(subscription_data.group_data().value.clone())
}

#[test]
fn group_data_is_visible_to_remote_endpoints() {
    let domain_id = next_domain_id();
    let publisher_group_data = b"publisher-group-data".to_vec();
    let subscriber_group_data = b"subscriber-group-data".to_vec();

    let writer_participant = create_participant(domain_id, DomainParticipantQos::default());
    let reader_participant = create_participant(domain_id, DomainParticipantQos::default());

    let data_writer = create_datawriter(
        &writer_participant,
        PublisherQos {
            group_data: GroupDataQosPolicy { value: publisher_group_data.clone() },
            ..Default::default()
        },
        DataWriterQos::default(),
    );
    let data_reader = create_datareader(
        &reader_participant,
        SubscriberQos {
            group_data: GroupDataQosPolicy { value: subscriber_group_data.clone() },
            ..Default::default()
        },
        DataReaderQos::default(),
    );

    let writer_handle = data_writer.get_instance_handle().unwrap();
    let reader_handle = data_reader.get_instance_handle().unwrap();

    let is_publisher_group_data_received = wait_for(|| {
        let group_data = matched_publication_group_data(&data_reader, writer_handle)?;
        (group_data == publisher_group_data).then_some(())
    });
    assert!(is_publisher_group_data_received.is_some(), "publisher group data was not received");

    let is_subscriber_group_data_received = wait_for(|| {
        let group_data = matched_subscription_group_data(&data_writer, reader_handle)?;
        (group_data == subscriber_group_data).then_some(())
    });
    assert!(is_subscriber_group_data_received.is_some(), "subscriber group data was not received");

    drop(data_writer);
    drop(data_reader);
    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn group_data_change_is_visible_to_remote_endpoints() {
    let domain_id = next_domain_id();
    let initial_publisher_group_data = b"initial-publisher-group-data".to_vec();
    let initial_subscriber_group_data = b"initial-subscriber-group-data".to_vec();
    let changed_publisher_group_data = b"changed-publisher-group-data".to_vec();
    let changed_subscriber_group_data = b"changed-subscriber-group-data".to_vec();

    let writer_participant = create_participant(domain_id, DomainParticipantQos::default());
    let reader_participant = create_participant(domain_id, DomainParticipantQos::default());

    let data_writer = create_datawriter(
        &writer_participant,
        PublisherQos {
            group_data: GroupDataQosPolicy { value: initial_publisher_group_data.clone() },
            ..Default::default()
        },
        DataWriterQos::default(),
    );
    let data_reader = create_datareader(
        &reader_participant,
        SubscriberQos {
            group_data: GroupDataQosPolicy { value: initial_subscriber_group_data.clone() },
            ..Default::default()
        },
        DataReaderQos::default(),
    );

    let writer_handle = data_writer.get_instance_handle().unwrap();
    let reader_handle = data_reader.get_instance_handle().unwrap();

    let is_initial_received = wait_for(|| {
        let publisher_group_data = matched_publication_group_data(&data_reader, writer_handle)?;
        let subscriber_group_data = matched_subscription_group_data(&data_writer, reader_handle)?;
        (publisher_group_data == initial_publisher_group_data
            && subscriber_group_data == initial_subscriber_group_data)
            .then_some(())
    });
    assert!(is_initial_received.is_some(), "initial group data was not received");

    let publisher = data_writer.get_publisher().unwrap();
    let mut changed_publisher_qos = publisher.get_qos().unwrap();
    changed_publisher_qos.group_data =
        GroupDataQosPolicy { value: changed_publisher_group_data.clone() };
    publisher.set_qos(changed_publisher_qos).unwrap();

    let subscriber = data_reader.get_subscriber().unwrap();
    let mut changed_subscriber_qos = subscriber.get_qos().unwrap();
    changed_subscriber_qos.group_data =
        GroupDataQosPolicy { value: changed_subscriber_group_data.clone() };
    subscriber.set_qos(changed_subscriber_qos).unwrap();

    let is_publisher_change_received = wait_for(|| {
        let group_data = matched_publication_group_data(&data_reader, writer_handle)?;
        (group_data == changed_publisher_group_data).then_some(())
    });
    assert!(
        is_publisher_change_received.is_some(),
        "changed publisher group data was not received"
    );

    let is_subscriber_change_received = wait_for(|| {
        let group_data = matched_subscription_group_data(&data_writer, reader_handle)?;
        (group_data == changed_subscriber_group_data).then_some(())
    });
    assert!(
        is_subscriber_change_received.is_some(),
        "changed subscriber group data was not received"
    );

    drop(publisher);
    drop(subscriber);
    drop(data_writer);
    drop(data_reader);
    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn group_data_change_is_visible_to_endpoints_created_later_on_same_participant() {
    let domain_id = next_domain_id();
    let initial_publisher_group_data = b"initial-publisher-group-data".to_vec();
    let initial_subscriber_group_data = b"initial-subscriber-group-data".to_vec();
    let changed_publisher_group_data = b"changed-publisher-group-data".to_vec();
    let changed_subscriber_group_data = b"changed-subscriber-group-data".to_vec();

    let participant = create_participant(domain_id, DomainParticipantQos::default());

    let data_writer = create_datawriter(
        &participant,
        PublisherQos {
            group_data: GroupDataQosPolicy { value: initial_publisher_group_data.clone() },
            ..Default::default()
        },
        DataWriterQos::default(),
    );
    let topic = data_writer.get_topic().unwrap();
    let subscriber = participant
        .create_subscriber(
            SubscriberQos {
                group_data: GroupDataQosPolicy { value: initial_subscriber_group_data.clone() },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher = data_writer.get_publisher().unwrap();
    let mut changed_publisher_qos = publisher.get_qos().unwrap();
    changed_publisher_qos.group_data =
        GroupDataQosPolicy { value: changed_publisher_group_data.clone() };
    publisher.set_qos(changed_publisher_qos).unwrap();

    let mut changed_subscriber_qos = subscriber.get_qos().unwrap();
    changed_subscriber_qos.group_data =
        GroupDataQosPolicy { value: changed_subscriber_group_data.clone() };
    subscriber.set_qos(changed_subscriber_qos).unwrap();

    // Endpoints created after the change must match with the changed values.
    let later_data_reader = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let later_data_writer = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let writer_handle = data_writer.get_instance_handle().unwrap();
    let reader_handle = data_reader.get_instance_handle().unwrap();

    let received_publisher_group_data =
        wait_for(|| matched_publication_group_data(&later_data_reader, writer_handle));
    assert_eq!(received_publisher_group_data, Some(changed_publisher_group_data));

    let received_subscriber_group_data =
        wait_for(|| matched_subscription_group_data(&later_data_writer, reader_handle));
    assert_eq!(received_subscriber_group_data, Some(changed_subscriber_group_data));

    drop(later_data_writer);
    drop(later_data_reader);
    drop(publisher);
    drop(subscriber);
    drop(data_writer);
    drop(data_reader);
    drop(topic);
    delete_participant(participant);
}
