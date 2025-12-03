mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{qos_policy::EntityFactoryQosPolicy, status::StatusMask},
        publication::qos::{DataWriterQos, PublisherQos},
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

    let topic = participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Publisher with autoenable_created_entities = true (default)
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

    // DataWriter should be automatically enabled
    let res = writer.write(&KeyedDataType::new(1, 100), InstanceHandle::NIL);
    assert!(res.is_ok());
}

#[test]
fn autoenable_created_entities_false() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedData",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Publisher with autoenable_created_entities = false
    let mut publisher_qos = PublisherQos::default();
    publisher_qos.entity_factory = EntityFactoryQosPolicy { autoenable_created_entities: false };

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
}
