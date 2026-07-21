mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::StatusMask,
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::qos::TopicQos,
    },
};

#[test]
fn test_datareader_with_topic() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    // Create DataReader with Topic
    let reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let cft = participant
        .create_contentfilteredtopic::<KeyedDataType>(
            "filtered_topic",
            &topic,
            "key > %0",
            vec!["10".to_string()],
        )
        .unwrap();

    let cft_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &cft,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Verify get_topicdescription returns the Topic
    let topic_desc = reader.get_topicdescription().unwrap();
    assert_eq!(topic_desc.get_name(), KeyedDataType::get_topic_name());
    assert_eq!(topic_desc.get_type_name(), KeyedDataType::get_type_name());

    let topic_desc = cft_reader.get_topicdescription().unwrap();
    assert_eq!(topic_desc.get_name(), "filtered_topic");
    assert_eq!(topic_desc.get_type_name(), KeyedDataType::get_type_name());

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_content_filtered_topic_read() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "CFT_Read_Test",
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Create ContentFilteredTopic that filters key > 1
    let cft = participant
        .create_contentfilteredtopic::<KeyedDataType>(
            "CFT_Read_Test",
            &topic,
            "key > %0",
            vec!["1".to_string()],
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &cft,
            DataReaderQos::default(),
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

    // Write test data
    let data1 = KeyedDataType { key: 0, value: 1 };
    let data2 = KeyedDataType { key: 1, value: 1 };
    let data3 = KeyedDataType { key: 2, value: 0 };
    let data4 = KeyedDataType { key: 3, value: 0 };

    data_writer.write(&data1, InstanceHandle::NIL).unwrap();
    data_writer.write(&data2, InstanceHandle::NIL).unwrap();
    data_writer.write(&data3, InstanceHandle::NIL).unwrap();
    data_writer.write(&data4, InstanceHandle::NIL).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    // Read with ContentFilteredTopic - should only get data3 and data4
    let result = data_reader.read(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );

    assert!(result.is_ok());
    let samples = result.unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].data().unwrap().key, 2);
    assert_eq!(samples[1].data().unwrap().key, 3);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_content_filtered_topic_take_serialized() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "CFT_Serialized_Test",
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let cft = participant
        .create_contentfilteredtopic::<KeyedDataType>(
            "CFT_Serialized_Test",
            &topic,
            "key > %0",
            vec!["1".to_string()],
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &cft,
            DataReaderQos::default(),
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

    for (key, value) in [(0, 1), (1, 1), (2, 0), (3, 0)] {
        data_writer.write(&KeyedDataType { key, value }, InstanceHandle::NIL).unwrap();
    }

    std::thread::sleep(std::time::Duration::from_secs(1));

    // Serialized take goes through read_or_take_serialized_bytes. CFT is applied on the
    // receive path, so only key > 1 (keys 2 and 3) should remain in the cache.
    let result = data_reader.take_serialized(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );

    assert!(result.is_ok());
    let samples = result.unwrap();
    assert_eq!(samples.len(), 2);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_content_filtered_topic_with_read_condition() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "CFT_ReadCondition_Test",
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Create ContentFilteredTopic that filters key > 0
    let cft = participant
        .create_contentfilteredtopic::<KeyedDataType>(
            "CFT_ReadCondition_Test",
            &topic,
            "key > %0",
            vec!["0".to_string()],
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &cft,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Create ReadCondition for NOT_READ samples
    let read_condition = data_reader
        .create_readcondition(
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
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

    // Write test data
    let data1 = KeyedDataType { key: 0, value: 0 };
    let data2 = KeyedDataType { key: 1, value: 1 };
    let data3 = KeyedDataType { key: 2, value: 1 };

    data_writer.write(&data1, InstanceHandle::NIL).unwrap();
    data_writer.write(&data2, InstanceHandle::NIL).unwrap();
    data_writer.write(&data3, InstanceHandle::NIL).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    // Read with both ContentFilteredTopic and ReadCondition
    let result = data_reader.read_w_condition(10, read_condition.clone());

    assert!(result.is_ok());
    let samples = result.unwrap();
    assert_eq!(samples.len(), 2); // Only data2 and data3 pass CFT filter
    assert_eq!(samples[0].data().unwrap().key, 1);
    assert_eq!(samples[1].data().unwrap().key, 2);

    // Read again - should get no data since samples are now READ
    let result = data_reader.read_w_condition(10, read_condition.clone());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), DdsError::NoData);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_content_filtered_topic_with_query_condition() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "CFT_QueryCondition_Test",
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Create ContentFilteredTopic that filters key >= 1
    let cft = participant
        .create_contentfilteredtopic::<KeyedDataType>(
            "CFT_QueryCondition_Test",
            &topic,
            "key >= %0",
            vec!["1".to_string()],
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &cft,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Create QueryCondition for key < 3
    let query_condition = data_reader
        .create_querycondition(
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            "key < %0",
            vec!["3".to_string()],
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

    // Write test data
    let data1 = KeyedDataType { key: 0, value: 0 };
    let data2 = KeyedDataType { key: 1, value: 1 };
    let data3 = KeyedDataType { key: 2, value: 1 };
    let data4 = KeyedDataType { key: 3, value: 0 };
    let data5 = KeyedDataType { key: 4, value: 0 };

    data_writer.write(&data1, InstanceHandle::NIL).unwrap();
    data_writer.write(&data2, InstanceHandle::NIL).unwrap();
    data_writer.write(&data3, InstanceHandle::NIL).unwrap();
    data_writer.write(&data4, InstanceHandle::NIL).unwrap();
    data_writer.write(&data5, InstanceHandle::NIL).unwrap();

    std::thread::sleep(std::time::Duration::from_secs(1));

    // Read with both ContentFilteredTopic (key >= 1) and QueryCondition (key < 3)
    // Should only get data2 and data3 (key = 1, 2)
    let result = data_reader.read_w_condition(10, query_condition.clone());

    assert!(result.is_ok());
    let samples = result.unwrap();
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].data().unwrap().key, 1);
    assert_eq!(samples[1].data().unwrap().key, 2);

    // Test with ORDER BY in QueryCondition (currently only ascending order is supported)
    let query_condition_ordered = data_reader
        .create_querycondition(
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            "key < %0 ORDER BY value",
            vec!["3".to_string()],
        )
        .unwrap();

    let result = data_reader.read_w_condition(10, query_condition_ordered.clone());
    assert!(result.is_ok());
    let samples = result.unwrap();
    assert_eq!(samples.len(), 2);
    // Sorted by value field in ascending order
    assert_eq!(samples[0].data().unwrap().value, 1);
    assert_eq!(samples[1].data().unwrap().value, 1);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
