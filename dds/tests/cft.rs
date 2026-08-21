mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{HistoryQosPolicy, HistoryQosPolicyKind},
            status::StatusMask,
        },
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

// A filter expression naming a field the topic type does not have can never
// match, so CFT and QueryCondition creation reject it (RTI parity). A `%n`
// operand may resolve to a field name at read time, so those are accepted.
#[test]
fn test_filter_unknown_field_rejected_at_creation() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "CFT_Validation_Test",
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let unknown = participant.create_contentfilteredtopic::<KeyedDataType>(
        "CFT_Validation_Bad",
        &topic,
        "nonexistent > 5",
        vec![],
    );
    assert!(unknown.is_err());

    let partly_unknown = participant.create_contentfilteredtopic::<KeyedDataType>(
        "CFT_Validation_Partly",
        &topic,
        "key > 0 AND bogus = 3",
        vec![],
    );
    assert!(partly_unknown.is_err());

    let cft = participant
        .create_contentfilteredtopic::<KeyedDataType>(
            "CFT_Validation_Good",
            &topic,
            "key > %0",
            vec!["0".to_string()],
        )
        .unwrap();
    assert!(cft.set_filter_expression("nonexistent BETWEEN 1 AND 2", vec![]).is_err());
    assert_eq!(cft.get_filter_expression().unwrap(), "key > %0");

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let qc_unknown = data_reader.create_querycondition(
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
        "nonexistent = 1",
        vec![],
    );
    assert!(qc_unknown.is_err());

    let qc_ok = data_reader.create_querycondition(
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
        "key >= 0",
        vec![],
    );
    assert!(qc_ok.is_ok());

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[derive(int2dds::DdsType)]
struct OptionalScore {
    #[dds(key)]
    id: i32,
    score: Option<i32>,
    grade: i32,
}

// An unset optional makes the comparison false at the predicate level, not a
// sample-level error: the other branch of an OR still decides the match.
#[test]
fn test_unset_optional_comparison_is_predicate_false() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<OptionalScore>(
            "QC_Unset_Optional_Test",
            "OptionalScore",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let data_writer = publisher
        .create_datawriter::<OptionalScore>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let data_reader = subscriber
        .create_datareader::<OptionalScore>(
            &topic,
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

    data_writer
        .write(&OptionalScore { id: 1, score: None, grade: 1 }, InstanceHandle::NIL)
        .unwrap();
    data_writer
        .write(&OptionalScore { id: 2, score: Some(10), grade: 0 }, InstanceHandle::NIL)
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));

    // id 1: score unset -> false, but grade = 1 -> the OR matches.
    // id 2: score 10 > 5 -> matches.
    let qc_or = data_reader
        .create_querycondition(
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            "score > 5 OR grade = 1",
            vec![],
        )
        .unwrap();
    let samples = data_reader.read_w_condition(10, qc_or).unwrap();
    let mut ids: Vec<i32> = samples.iter().map(|s| s.data().unwrap().id).collect();
    ids.sort();
    assert_eq!(ids, vec![1, 2]);

    // Under AND the unset comparison is false for id 1, and id 2 fails on grade.
    let qc_and = data_reader
        .create_querycondition(
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            "score > 5 AND grade = 1",
            vec![],
        )
        .unwrap();
    let result = data_reader.read_w_condition(10, qc_and);
    assert_eq!(result.err().unwrap(), DdsError::NoData);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

// A sample the QueryCondition cannot evaluate must not fail or consume the
// whole read/take batch, and key-only dispose notifications always pass
// filters (RTI/Fast-DDS convention).
#[test]
fn test_query_condition_error_and_dispose_isolation() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "QC_Error_Dispose_Test",
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

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    // Depth > 1 keeps the data sample of the disposed instance alongside the
    // dispose notification, so both flow through one read/take batch.
    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(&topic, reader_qos, None, StatusMask::default())
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
    data_writer.write(&KeyedDataType { key: 2, value: 2 }, InstanceHandle::NIL).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Unknown field: every sample fails to evaluate. The call must report
    // NoData (no matches), not an evaluator error, and must consume nothing.
    let qc_bad = data_reader
        .create_querycondition(
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            "nonexistent > %0",
            vec!["0".to_string()],
        )
        .unwrap();
    let result = data_reader.take_w_condition(10, qc_bad);
    assert_eq!(result.err().unwrap(), DdsError::NoData);

    let samples = data_reader
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(samples.len(), 2);

    data_writer.dispose(&KeyedDataType { key: 1, value: 1 }, InstanceHandle::NIL).unwrap();
    std::thread::sleep(std::time::Duration::from_secs(1));

    // The key-only dispose notification cannot be evaluated against the
    // expression; it must pass the filter instead of failing the take after
    // the data samples were already consumed.
    let qc_good = data_reader
        .create_querycondition(
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            "key >= %0",
            vec!["0".to_string()],
        )
        .unwrap();
    let samples = data_reader.take_w_condition(10, qc_good).unwrap();
    assert_eq!(samples.len(), 3);
    let valid: Vec<_> = samples.iter().filter(|s| s.sample_info().valid_data).collect();
    let mut keys: Vec<i16> = valid.iter().map(|s| s.data().unwrap().key).collect();
    keys.sort();
    assert_eq!(keys, vec![1, 2]);
    let dispose: Vec<_> = samples.iter().filter(|s| !s.sample_info().valid_data).collect();
    assert_eq!(dispose.len(), 1);
    assert_eq!(
        dispose[0].sample_info().instance_state,
        InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE
    );

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
