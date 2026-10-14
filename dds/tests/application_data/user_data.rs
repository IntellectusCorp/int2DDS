// USER_DATA set on one participant's entities must be readable by a remote participant.

use crate::{common::*, create_participant, delete_participant, wait_for};
use int2dds::{
    common::instance_handle::InstanceHandle,
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory,
        qos::{DomainParticipantFactoryQos, DomainParticipantQos},
    },
    infrastructure::{
        qos_policy::{EntityFactoryQosPolicy, UserDataQosPolicy},
        status::StatusMask,
    },
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

// Local participant with a DataWriter and a DataReader on the same topic.
struct UserDataEntities {
    participant: DomainParticipant,
    data_writer: DataWriter<KeyedDataType>,
    data_reader: DataReader<KeyedDataType>,
}

fn create_entities(
    domain_id: i32,
    participant_user_data: &[u8],
    writer_user_data: &[u8],
    reader_user_data: &[u8],
) -> UserDataEntities {
    let participant = create_participant(
        domain_id,
        DomainParticipantQos {
            user_data: UserDataQosPolicy { value: participant_user_data.to_vec() },
            ..Default::default()
        },
    );

    create_endpoints(participant, writer_user_data, reader_user_data)
}

// Endpoints created on a disabled participant stay disabled until the participant is enabled.
fn create_endpoints(
    participant: DomainParticipant,
    writer_user_data: &[u8],
    reader_user_data: &[u8],
) -> UserDataEntities {
    let topic = create_topic(&participant);

    let data_writer = participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos {
                user_data: UserDataQosPolicy { value: writer_user_data.to_vec() },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let data_reader = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos {
                user_data: UserDataQosPolicy { value: reader_user_data.to_vec() },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    UserDataEntities { participant, data_writer, data_reader }
}

fn create_topic(participant: &DomainParticipant) -> Topic {
    participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap()
}

fn delete_entities(entities: UserDataEntities) {
    drop(entities.data_writer);
    drop(entities.data_reader);
    delete_participant(entities.participant);
}

// USER_DATA of each sending entity as the receiving side sees it.
fn discovered_participant_user_data(
    receiving: &UserDataEntities,
    participant_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let participant_data =
        receiving.participant.get_discovered_participant_data(participant_handle).ok()?;
    Some(participant_data.user_data().value.clone())
}

fn matched_publication_user_data(
    receiving: &UserDataEntities,
    writer_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let publication_data =
        receiving.data_reader.get_matched_publication_data(writer_handle).ok()?;
    Some(publication_data.user_data().value.clone())
}

fn matched_subscription_user_data(
    receiving: &UserDataEntities,
    reader_handle: InstanceHandle,
) -> Option<Vec<u8>> {
    let subscription_data =
        receiving.data_writer.get_matched_subscription_data(reader_handle).ok()?;
    Some(subscription_data.user_data().value.clone())
}

#[test]
fn user_data_is_visible_to_remote_entities() {
    let domain_id = next_domain_id();
    let participant_user_data = b"participant-user-data".to_vec();
    let writer_user_data = b"datawriter-user-data".to_vec();
    let reader_user_data = b"datareader-user-data".to_vec();

    let sending =
        create_entities(domain_id, &participant_user_data, &writer_user_data, &reader_user_data);
    let receiving = create_entities(domain_id, &[], &[], &[]);

    let participant_handle = sending.participant.get_instance_handle().unwrap();
    let writer_handle = sending.data_writer.get_instance_handle().unwrap();
    let reader_handle = sending.data_reader.get_instance_handle().unwrap();

    let is_participant_received = wait_for(|| {
        let user_data = discovered_participant_user_data(&receiving, participant_handle)?;
        (user_data == participant_user_data).then_some(())
    });
    assert!(is_participant_received.is_some(), "participant user data was not received");

    let is_writer_received = wait_for(|| {
        let user_data = matched_publication_user_data(&receiving, writer_handle)?;
        (user_data == writer_user_data).then_some(())
    });
    assert!(is_writer_received.is_some(), "datawriter user data was not received");

    let is_reader_received = wait_for(|| {
        let user_data = matched_subscription_user_data(&receiving, reader_handle)?;
        (user_data == reader_user_data).then_some(())
    });
    assert!(is_reader_received.is_some(), "datareader user data was not received");

    delete_entities(sending);
    delete_entities(receiving);
}

#[test]
fn user_data_change_is_visible_to_remote_entities() {
    let domain_id = next_domain_id();
    let initial_participant_user_data = b"initial-participant-user-data".to_vec();
    let initial_writer_user_data = b"initial-datawriter-user-data".to_vec();
    let initial_reader_user_data = b"initial-datareader-user-data".to_vec();
    let changed_participant_user_data = b"changed-participant-user-data".to_vec();
    let changed_writer_user_data = b"changed-datawriter-user-data".to_vec();
    let changed_reader_user_data = b"changed-datareader-user-data".to_vec();

    let sending = create_entities(
        domain_id,
        &initial_participant_user_data,
        &initial_writer_user_data,
        &initial_reader_user_data,
    );
    let receiving = create_entities(domain_id, &[], &[], &[]);

    let participant_handle = sending.participant.get_instance_handle().unwrap();
    let writer_handle = sending.data_writer.get_instance_handle().unwrap();
    let reader_handle = sending.data_reader.get_instance_handle().unwrap();

    let is_initial_received = wait_for(|| {
        let participant_user_data =
            discovered_participant_user_data(&receiving, participant_handle)?;
        let writer_user_data = matched_publication_user_data(&receiving, writer_handle)?;
        let reader_user_data = matched_subscription_user_data(&receiving, reader_handle)?;
        (participant_user_data == initial_participant_user_data
            && writer_user_data == initial_writer_user_data
            && reader_user_data == initial_reader_user_data)
            .then_some(())
    });
    assert!(is_initial_received.is_some(), "initial user data was not received");

    let mut changed_participant_qos = sending.participant.get_qos().unwrap();
    changed_participant_qos.user_data =
        UserDataQosPolicy { value: changed_participant_user_data.clone() };
    sending.participant.set_qos(changed_participant_qos).unwrap();

    let mut changed_writer_qos = sending.data_writer.get_qos().unwrap();
    changed_writer_qos.user_data = UserDataQosPolicy { value: changed_writer_user_data.clone() };
    sending.data_writer.set_qos(changed_writer_qos).unwrap();

    let mut changed_reader_qos = sending.data_reader.get_qos().unwrap();
    changed_reader_qos.user_data = UserDataQosPolicy { value: changed_reader_user_data.clone() };
    sending.data_reader.set_qos(changed_reader_qos).unwrap();

    let is_participant_change_received = wait_for(|| {
        let user_data = discovered_participant_user_data(&receiving, participant_handle)?;
        (user_data == changed_participant_user_data).then_some(())
    });
    assert!(
        is_participant_change_received.is_some(),
        "changed participant user data was not received"
    );

    let is_writer_change_received = wait_for(|| {
        let user_data = matched_publication_user_data(&receiving, writer_handle)?;
        (user_data == changed_writer_user_data).then_some(())
    });
    assert!(is_writer_change_received.is_some(), "changed datawriter user data was not received");

    let is_reader_change_received = wait_for(|| {
        let user_data = matched_subscription_user_data(&receiving, reader_handle)?;
        (user_data == changed_reader_user_data).then_some(())
    });
    assert!(is_reader_change_received.is_some(), "changed datareader user data was not received");

    delete_entities(sending);
    delete_entities(receiving);
}

#[test]
fn user_data_set_while_disabled_is_visible_after_enable() {
    let domain_id = next_domain_id();
    let initial_participant_user_data = b"initial-participant-user-data".to_vec();
    let initial_writer_user_data = b"initial-datawriter-user-data".to_vec();
    let initial_reader_user_data = b"initial-datareader-user-data".to_vec();
    let changed_participant_user_data = b"changed-participant-user-data".to_vec();
    let changed_writer_user_data = b"changed-datawriter-user-data".to_vec();
    let changed_reader_user_data = b"changed-datareader-user-data".to_vec();
    let factory = DomainParticipantFactory::get_instance();

    let original_factory_qos = factory.get_qos().unwrap();
    factory
        .set_qos(DomainParticipantFactoryQos {
            entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        })
        .unwrap();
    let sending_participant = factory
        .create_participant(
            domain_id,
            DomainParticipantQos {
                user_data: UserDataQosPolicy { value: initial_participant_user_data.clone() },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();
    factory.set_qos(original_factory_qos).unwrap();

    let sending =
        create_endpoints(sending_participant, &initial_writer_user_data, &initial_reader_user_data);
    let receiving = create_entities(domain_id, &[], &[], &[]);

    let mut changed_participant_qos = sending.participant.get_qos().unwrap();
    changed_participant_qos.user_data =
        UserDataQosPolicy { value: changed_participant_user_data.clone() };
    sending.participant.set_qos(changed_participant_qos).unwrap();

    let mut changed_writer_qos = sending.data_writer.get_qos().unwrap();
    changed_writer_qos.user_data = UserDataQosPolicy { value: changed_writer_user_data.clone() };
    sending.data_writer.set_qos(changed_writer_qos).unwrap();

    let mut changed_reader_qos = sending.data_reader.get_qos().unwrap();
    changed_reader_qos.user_data = UserDataQosPolicy { value: changed_reader_user_data.clone() };
    sending.data_reader.set_qos(changed_reader_qos).unwrap();

    sending.participant.enable().unwrap();

    let participant_handle = sending.participant.get_instance_handle().unwrap();
    let writer_handle = sending.data_writer.get_instance_handle().unwrap();
    let reader_handle = sending.data_reader.get_instance_handle().unwrap();

    // The first value received must already be the changed one.
    let received_participant_user_data =
        wait_for(|| discovered_participant_user_data(&receiving, participant_handle));
    assert_eq!(received_participant_user_data, Some(changed_participant_user_data));

    let received_writer_user_data =
        wait_for(|| matched_publication_user_data(&receiving, writer_handle));
    assert_eq!(received_writer_user_data, Some(changed_writer_user_data));

    let received_reader_user_data =
        wait_for(|| matched_subscription_user_data(&receiving, reader_handle));
    assert_eq!(received_reader_user_data, Some(changed_reader_user_data));

    delete_entities(sending);
    delete_entities(receiving);
}
