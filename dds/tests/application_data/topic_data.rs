// TOPIC_DATA set on a Topic must be readable by the matched remote endpoint.

use crate::{common::*, create_participant, delete_participant, wait_for};
use int2dds::{
    common::instance_handle::InstanceHandle,
    domain::{domain_participant::DomainParticipant, qos::DomainParticipantQos},
    infrastructure::{qos_policy::TopicDataQosPolicy, status::StatusMask},
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
    },
    topic::{qos::TopicQos, topic::Topic},
};

fn create_topic(participant: &DomainParticipant, topic_data: &[u8]) -> Topic {
    participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos {
                topic_data: TopicDataQosPolicy { value: topic_data.to_vec() },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn create_datawriter_on_topic(
    participant: &DomainParticipant,
    topic: &Topic,
) -> DataWriter<KeyedDataType> {
    participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datawriter::<KeyedDataType>(
            topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn create_datareader_on_topic(
    participant: &DomainParticipant,
    topic: &Topic,
) -> DataReader<KeyedDataType> {
    participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader::<KeyedDataType>(
            topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

// Topic TOPIC_DATA of the matched writer as the reader sees it.
fn matched_publication_topic_data(
    data_reader: &DataReader<KeyedDataType>,
    writer_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let publication_data = data_reader.get_matched_publication_data(writer_handle).ok()?;
    Some(publication_data.topic_data().value.clone())
}

// Topic TOPIC_DATA of the matched reader as the writer sees it.
fn matched_subscription_topic_data(
    data_writer: &DataWriter<KeyedDataType>,
    reader_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let subscription_data = data_writer.get_matched_subscription_data(reader_handle).ok()?;
    Some(subscription_data.topic_data().value.clone())
}

#[test]
fn topic_data_is_visible_to_remote_endpoints() {
    let domain_id = next_domain_id();
    let writer_topic_data = b"writer-topic-data".to_vec();
    let reader_topic_data = b"reader-topic-data".to_vec();

    let writer_participant = create_participant(domain_id, DomainParticipantQos::default());
    let reader_participant = create_participant(domain_id, DomainParticipantQos::default());

    let writer_topic = create_topic(&writer_participant, &writer_topic_data);
    let data_writer = create_datawriter_on_topic(&writer_participant, &writer_topic);

    let reader_topic = create_topic(&reader_participant, &reader_topic_data);
    let data_reader = create_datareader_on_topic(&reader_participant, &reader_topic);

    let writer_handle = data_writer.get_instance_handle().unwrap();
    let reader_handle = data_reader.get_instance_handle().unwrap();

    let is_writer_topic_data_received = wait_for(|| {
        let topic_data = matched_publication_topic_data(&data_reader, writer_handle)?;
        (topic_data == writer_topic_data).then_some(())
    });
    assert!(is_writer_topic_data_received.is_some(), "writer topic data was not received");

    let is_reader_topic_data_received = wait_for(|| {
        let topic_data = matched_subscription_topic_data(&data_writer, reader_handle)?;
        (topic_data == reader_topic_data).then_some(())
    });
    assert!(is_reader_topic_data_received.is_some(), "reader topic data was not received");

    drop(data_writer);
    drop(data_reader);
    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn topic_data_change_is_visible_to_remote_endpoints() {
    let domain_id = next_domain_id();
    let initial_writer_topic_data = b"initial-writer-topic-data".to_vec();
    let initial_reader_topic_data = b"initial-reader-topic-data".to_vec();
    let changed_writer_topic_data = b"changed-writer-topic-data".to_vec();
    let changed_reader_topic_data = b"changed-reader-topic-data".to_vec();

    let writer_participant = create_participant(domain_id, DomainParticipantQos::default());
    let reader_participant = create_participant(domain_id, DomainParticipantQos::default());

    let writer_topic = create_topic(&writer_participant, &initial_writer_topic_data);
    let data_writer = create_datawriter_on_topic(&writer_participant, &writer_topic);

    let reader_topic = create_topic(&reader_participant, &initial_reader_topic_data);
    let data_reader = create_datareader_on_topic(&reader_participant, &reader_topic);

    let writer_handle = data_writer.get_instance_handle().unwrap();
    let reader_handle = data_reader.get_instance_handle().unwrap();

    let is_initial_received = wait_for(|| {
        let writer_topic_data = matched_publication_topic_data(&data_reader, writer_handle)?;
        let reader_topic_data = matched_subscription_topic_data(&data_writer, reader_handle)?;
        (writer_topic_data == initial_writer_topic_data
            && reader_topic_data == initial_reader_topic_data)
            .then_some(())
    });
    assert!(is_initial_received.is_some(), "initial topic data was not received");

    let mut changed_writer_topic_qos = writer_topic.get_qos().unwrap();
    changed_writer_topic_qos.topic_data =
        TopicDataQosPolicy { value: changed_writer_topic_data.clone() };
    writer_topic.set_qos(changed_writer_topic_qos).unwrap();

    let mut changed_reader_topic_qos = reader_topic.get_qos().unwrap();
    changed_reader_topic_qos.topic_data =
        TopicDataQosPolicy { value: changed_reader_topic_data.clone() };
    reader_topic.set_qos(changed_reader_topic_qos).unwrap();

    let is_writer_change_received = wait_for(|| {
        let topic_data = matched_publication_topic_data(&data_reader, writer_handle)?;
        (topic_data == changed_writer_topic_data).then_some(())
    });
    assert!(is_writer_change_received.is_some(), "changed writer topic data was not received");

    let is_reader_change_received = wait_for(|| {
        let topic_data = matched_subscription_topic_data(&data_writer, reader_handle)?;
        (topic_data == changed_reader_topic_data).then_some(())
    });
    assert!(is_reader_change_received.is_some(), "changed reader topic data was not received");

    drop(data_writer);
    drop(data_reader);
    delete_participant(writer_participant);
    delete_participant(reader_participant);
}
