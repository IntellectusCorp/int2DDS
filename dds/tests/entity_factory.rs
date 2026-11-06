mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::error::DdsError,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{qos_policy::EntityFactoryQosPolicy, status::StatusMask},
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::qos::TopicQos,
    },
};

#[test]
fn autoenable_created_entities_true() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    // Publisher with autoenable_created_entities = true (default)
    let data_writer =
        create_datawriter(&participant, PublisherQos::default(), DataWriterQos::default());

    // DataWriter should be automatically enabled
    let res = data_writer.write(&KeyedDataType::new(1, 100), InstanceHandle::NIL);
    assert!(res.is_ok());

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn autoenable_created_entities_false_direct_parent() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            "KeyedData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Publisher with autoenable_created_entities = false
    let publisher_qos = PublisherQos {
        entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        ..Default::default()
    };

    let publisher =
        participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();

    let writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // write() should fail when not enabled
    let res = writer.write(&KeyedDataType::new(1, 100), InstanceHandle::NIL);
    assert!(res.is_err());

    // Manually enable the writer
    writer.enable().unwrap();

    // Now write() should succeed
    let res_2 = writer.write(&KeyedDataType::new(1, 100), InstanceHandle::NIL);
    assert!(res_2.is_ok());

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn autoenable_created_entities_false_dp() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let domain_participant_qos = DomainParticipantQos {
        entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        ..Default::default()
    };

    let participant = factory
        .create_participant(domain_id, domain_participant_qos, None, StatusMask::default())
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

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // write() should fail when not enabled
    let res = writer.write(&KeyedDataType::new(1, 100), InstanceHandle::NIL);
    assert!(res.is_err());

    participant.enable().unwrap();
    publisher.enable().unwrap();
    writer.enable().unwrap();

    // Now write() should succeed
    let res_2 = writer.write(&KeyedDataType::new(1, 100), InstanceHandle::NIL);
    assert!(res_2.is_ok());

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn reader_created_under_disabled_subscriber_is_enabled_with_it() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let domain_participant_qos = DomainParticipantQos {
        entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        ..Default::default()
    };

    let participant = factory
        .create_participant(domain_id, domain_participant_qos, None, StatusMask::default())
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

    let reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let take_reader = || {
        reader.take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
    };

    assert!(matches!(take_reader(), Err(DdsError::NotEnabled)));

    // The subscriber enables the reader through its own list, not through the user handle.
    participant.enable().unwrap();
    subscriber.enable().unwrap();

    assert!(!matches!(take_reader(), Err(DdsError::NotEnabled)));

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}
