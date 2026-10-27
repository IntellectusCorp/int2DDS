use crate::{
    common::*, create_disabled_participant, create_participant, delete_participant, wait_for,
};
use int2dds::{
    common::{
        builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
        instance_handle::InstanceHandle,
    },
    dcps::{
        core::error::DdsError,
        infrastructure::{
            qos_policy::{
                EntityFactoryQosPolicy, GroupDataQosPolicy, HistoryQosPolicy, HistoryQosPolicyKind,
            },
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
fn ignore_publication_requires_enabled_participant() {
    let participant = create_disabled_participant(next_domain_id());
    let handle = participant.get_instance_handle().unwrap();

    assert_eq!(participant.ignore_publication(handle), Err(DdsError::NotEnabled));

    delete_participant(participant);
}

#[test]
fn ignore_publication_rejects_local_data_writer() {
    let participant = create_participant(next_domain_id());
    let data_writer =
        create_datawriter(&participant, PublisherQos::default(), DataWriterQos::default());
    let writer_handle = data_writer.get_instance_handle().unwrap();

    assert_eq!(participant.ignore_publication(writer_handle), Err(DdsError::BadParameter));

    delete_participant(participant);
}

#[test]
fn ignore_publication_before_discovery_prevents_matching() {
    let domain_id = next_domain_id();
    let reader_participant = create_participant(domain_id);
    let writer_participant = create_participant(domain_id);
    let data_reader =
        create_datareader(&reader_participant, SubscriberQos::default(), DataReaderQos::default());

    // The writers stay disabled, so they are not announced until enable().
    let topic = writer_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher_qos = PublisherQos {
        entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        ..Default::default()
    };
    let publisher =
        writer_participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();
    let ignored_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_writer.get_instance_handle().unwrap();
    let control_handle = control_writer.get_instance_handle().unwrap();

    reader_participant.ignore_publication(ignored_handle).unwrap();

    // The ignored writer is announced first, so the control writer matching means the ignored
    // writer's announcement has already been received.
    ignored_writer.enable().unwrap();
    control_writer.enable().unwrap();

    let matched_publications = wait_for(|| {
        let handles = data_reader.get_matched_publications().ok()?;
        handles.contains(&control_handle).then_some(handles)
    })
    .expect("control writer was not matched");
    assert!(!matched_publications.contains(&ignored_handle));

    let discovered_publications = reader_participant.get_discovered_publications().unwrap();
    assert!(!discovered_publications
        .iter()
        .any(|data| &data.endpoint_guid().to_bytes() == ignored_handle.value()));

    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn ignore_publication_after_matching_unmatches_writer() {
    let domain_id = next_domain_id();
    let reader_participant = create_participant(domain_id);
    let writer_participant = create_participant(domain_id);
    let data_reader =
        create_datareader(&reader_participant, SubscriberQos::default(), DataReaderQos::default());

    let topic = writer_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = writer_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_writer.get_instance_handle().unwrap();
    let control_handle = control_writer.get_instance_handle().unwrap();

    let is_both_matched = wait_for(|| {
        let handles = data_reader.get_matched_publications().ok()?;
        (handles.contains(&ignored_handle) && handles.contains(&control_handle)).then_some(())
    });
    assert!(is_both_matched.is_some(), "writers were not matched");

    let matched_status = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(matched_status.current_count(), 2);

    reader_participant.ignore_publication(ignored_handle).unwrap();

    let matched_status = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(matched_status.current_count(), 1);
    assert_eq!(data_reader.get_matched_publications().unwrap(), vec![control_handle]);

    // Samples of different writers are not ordered, so this only detects an ignored sample that
    // arrives before the control sample.
    ignored_writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    control_writer.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();

    let mut received_publication_handles = Vec::new();
    let is_control_received = wait_for(|| {
        let samples = data_reader
            .take(
                10,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap_or_default();
        received_publication_handles
            .extend(samples.iter().map(|sample| sample.sample_info().publication_handle));
        received_publication_handles.contains(&control_handle).then_some(())
    });
    assert!(is_control_received.is_some(), "control sample was not received");
    assert!(!received_publication_handles.contains(&ignored_handle));

    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn ignore_publication_marks_instances_without_other_writers_not_alive_no_writers() {
    let domain_id = next_domain_id();
    let reader_participant = create_participant(domain_id);
    let writer_participant = create_participant(domain_id);
    let data_reader_qos = DataReaderQos {
        history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
        ..Default::default()
    };
    let data_reader =
        create_datareader(&reader_participant, SubscriberQos::default(), data_reader_qos);

    let topic = writer_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = writer_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_writer.get_instance_handle().unwrap();
    let control_handle = control_writer.get_instance_handle().unwrap();

    let is_both_matched = wait_for(|| {
        let handles = data_reader.get_matched_publications().ok()?;
        (handles.contains(&ignored_handle) && handles.contains(&control_handle)).then_some(())
    });
    assert!(is_both_matched.is_some(), "writers were not matched");

    // Key 1 is written only by the ignored writer; key 2 by both writers.
    ignored_writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();
    ignored_writer.write(&KeyedDataType::new(2, 1), InstanceHandle::NIL).unwrap();
    control_writer.write(&KeyedDataType::new(2, 2), InstanceHandle::NIL).unwrap();

    // Read, not take, so the instances stay in the reader to change state.
    let is_all_received = wait_for(|| {
        let samples = data_reader
            .read(
                10,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap_or_default();
        (samples.len() == 3).then_some(())
    });
    assert!(is_all_received.is_some(), "samples were not received");

    reader_participant.ignore_publication(ignored_handle).unwrap();

    let ignored_only_instance = data_reader.lookup_instance(&KeyedDataType::new(1, 0)).unwrap();
    let is_ignored_only_instance_no_writers = wait_for(|| {
        let samples = data_reader
            .read_instance(
                10,
                ignored_only_instance,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .ok()?;
        let instance_state = samples.first()?.sample_info().instance_state;
        (instance_state == InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE).then_some(())
    });
    assert!(is_ignored_only_instance_no_writers.is_some(), "key 1 did not become NO_WRITERS");

    let shared_instance = data_reader.lookup_instance(&KeyedDataType::new(2, 0)).unwrap();
    let shared_samples = data_reader
        .read_instance(
            10,
            shared_instance,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert!(!shared_samples.is_empty());
    assert!(shared_samples.iter().all(
        |sample| sample.sample_info().instance_state == InstanceStateKind::ALIVE_INSTANCE_STATE
    ));

    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn ignore_publication_survives_reannouncement() {
    let domain_id = next_domain_id();
    let reader_participant = create_participant(domain_id);
    let writer_participant = create_participant(domain_id);
    let data_reader =
        create_datareader(&reader_participant, SubscriberQos::default(), DataReaderQos::default());

    let topic = writer_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = writer_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_writer.get_instance_handle().unwrap();
    let control_handle = control_writer.get_instance_handle().unwrap();

    let is_both_matched = wait_for(|| {
        let handles = data_reader.get_matched_publications().ok()?;
        (handles.contains(&ignored_handle) && handles.contains(&control_handle)).then_some(())
    });
    assert!(is_both_matched.is_some(), "writers were not matched");

    reader_participant.ignore_publication(ignored_handle).unwrap();

    // Publisher set_qos re-announces both writers. The two announcements are not ordered, so
    // this only detects a re-match that happens before the control writer's update is seen.
    let changed_group_data = b"changed-group-data".to_vec();
    publisher
        .set_qos(PublisherQos {
            group_data: GroupDataQosPolicy { value: changed_group_data.clone() },
            ..Default::default()
        })
        .unwrap();

    let is_control_updated = wait_for(|| {
        let publication_data = data_reader.get_matched_publication_data(control_handle).ok()?;
        (publication_data.group_data().value == changed_group_data).then_some(())
    });
    assert!(is_control_updated.is_some(), "control writer update was not received");
    assert_eq!(data_reader.get_matched_publications().unwrap(), vec![control_handle]);

    let discovered_publications = reader_participant.get_discovered_publications().unwrap();
    assert!(!discovered_publications
        .iter()
        .any(|data| &data.endpoint_guid().to_bytes() == ignored_handle.value()));

    delete_participant(writer_participant);
    delete_participant(reader_participant);
}

#[test]
fn ignore_publication_marks_builtin_publication_not_alive_no_writers() {
    let domain_id = next_domain_id();
    let reader_participant = create_participant(domain_id);
    let writer_participant = create_participant(domain_id);

    let topic = writer_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher = writer_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_writer.get_instance_handle().unwrap();
    let control_handle = control_writer.get_instance_handle().unwrap();
    let publication_reader = reader_participant
        .get_builtin_subscriber()
        .unwrap()
        .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")
        .unwrap();

    let is_both_discovered = wait_for(|| {
        let samples = publication_reader
            .read(
                i32::MAX,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            )
            .ok()?;
        let handles: Vec<InstanceHandle> =
            samples.iter().map(|sample| sample.sample_info().instance_handle).collect();
        (handles.contains(&ignored_handle) && handles.contains(&control_handle)).then_some(())
    });
    assert!(is_both_discovered.is_some(), "writers were not discovered");

    reader_participant.ignore_publication(ignored_handle).unwrap();

    let ignored_samples = publication_reader
        .read_instance(
            i32::MAX,
            ignored_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert!(!ignored_samples.is_empty());
    assert!(ignored_samples.iter().all(|sample| sample.sample_info().instance_state
        == InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE));

    let control_samples = publication_reader
        .read_instance(
            i32::MAX,
            control_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert!(!control_samples.is_empty());
    assert!(control_samples.iter().all(
        |sample| sample.sample_info().instance_state == InstanceStateKind::ALIVE_INSTANCE_STATE
    ));

    delete_participant(writer_participant);
    delete_participant(reader_participant);
}
