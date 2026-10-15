use crate::{
    common::*, create_disabled_participant, create_participant, delete_participant, wait_for,
};
use int2dds::{
    common::{
        builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        instance_handle::InstanceHandle,
    },
    dcps::{
        core::error::DdsError,
        infrastructure::{
            qos_policy::{EntityFactoryQosPolicy, GroupDataQosPolicy},
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
fn ignore_subscription_requires_enabled_participant() {
    let participant = create_disabled_participant(next_domain_id());
    let handle = participant.get_instance_handle().unwrap();

    assert_eq!(participant.ignore_subscription(handle), Err(DdsError::NotEnabled));

    delete_participant(participant);
}

#[test]
fn ignore_subscription_rejects_local_data_reader() {
    let participant = create_participant(next_domain_id());
    let data_reader =
        create_datareader(&participant, SubscriberQos::default(), DataReaderQos::default());
    let reader_handle = data_reader.get_instance_handle().unwrap();

    assert_eq!(participant.ignore_subscription(reader_handle), Err(DdsError::BadParameter));

    delete_participant(participant);
}

#[test]
fn ignore_subscription_before_discovery_prevents_matching() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);
    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), DataWriterQos::default());

    // The readers stay disabled, so they are not announced until enable().
    let topic = reader_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber_qos = SubscriberQos {
        entity_factory: EntityFactoryQosPolicy { autoenable_created_entities: false },
        ..Default::default()
    };
    let subscriber =
        reader_participant.create_subscriber(subscriber_qos, None, StatusMask::default()).unwrap();
    let ignored_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_reader.get_instance_handle().unwrap();
    let control_handle = control_reader.get_instance_handle().unwrap();

    writer_participant.ignore_subscription(ignored_handle).unwrap();

    // The ignored reader is announced first, so the control reader matching means the ignored
    // reader's announcement has already been received.
    ignored_reader.enable().unwrap();
    control_reader.enable().unwrap();

    let matched_subscriptions = wait_for(|| {
        let handles = data_writer.get_matched_subscriptions().ok()?;
        handles.contains(&control_handle).then_some(handles)
    })
    .expect("control reader was not matched");
    assert!(!matched_subscriptions.contains(&ignored_handle));

    let discovered_subscriptions = writer_participant.get_discovered_subscriptions().unwrap();
    assert!(!discovered_subscriptions
        .iter()
        .any(|data| &data.endpoint_guid().to_bytes() == ignored_handle.value()));

    delete_participant(reader_participant);
    delete_participant(writer_participant);
}

#[test]
fn ignore_subscription_after_matching_unmatches_reader() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);
    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), DataWriterQos::default());

    let topic = reader_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = reader_participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_reader.get_instance_handle().unwrap();
    let control_handle = control_reader.get_instance_handle().unwrap();

    let is_both_matched = wait_for(|| {
        let handles = data_writer.get_matched_subscriptions().ok()?;
        (handles.contains(&ignored_handle) && handles.contains(&control_handle)).then_some(())
    });
    assert!(is_both_matched.is_some(), "readers were not matched");

    let matched_status = data_writer.get_publication_matched_status().unwrap();
    assert_eq!(matched_status.current_count(), 2);

    writer_participant.ignore_subscription(ignored_handle).unwrap();

    let matched_status = data_writer.get_publication_matched_status().unwrap();
    assert_eq!(matched_status.current_count(), 1);
    assert_eq!(data_writer.get_matched_subscriptions().unwrap(), vec![control_handle]);

    // The writer sends one sample to both readers, so the control reader receiving it bounds the
    // time the ignored reader had to receive it too.
    data_writer.write(&KeyedDataType::new(1, 1), InstanceHandle::NIL).unwrap();

    let is_control_received = wait_for(|| {
        let samples = control_reader
            .take(
                10,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .ok()?;
        (!samples.is_empty()).then_some(())
    });
    assert!(is_control_received.is_some(), "control sample was not received");

    let ignored_reader_samples = ignored_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap_or_default();
    assert!(ignored_reader_samples.is_empty());

    delete_participant(reader_participant);
    delete_participant(writer_participant);
}

#[test]
fn ignore_subscription_survives_reannouncement() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);
    let data_writer =
        create_datawriter(&writer_participant, PublisherQos::default(), DataWriterQos::default());

    let topic = reader_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = reader_participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_reader.get_instance_handle().unwrap();
    let control_handle = control_reader.get_instance_handle().unwrap();

    let is_both_matched = wait_for(|| {
        let handles = data_writer.get_matched_subscriptions().ok()?;
        (handles.contains(&ignored_handle) && handles.contains(&control_handle)).then_some(())
    });
    assert!(is_both_matched.is_some(), "readers were not matched");

    writer_participant.ignore_subscription(ignored_handle).unwrap();

    // Subscriber set_qos re-announces both readers. The two announcements are not ordered, so
    // this only detects a re-match that happens before the control reader's update is seen.
    let changed_group_data = b"changed-group-data".to_vec();
    subscriber
        .set_qos(SubscriberQos {
            group_data: GroupDataQosPolicy { value: changed_group_data.clone() },
            ..Default::default()
        })
        .unwrap();

    let is_control_updated = wait_for(|| {
        let subscription_data = data_writer.get_matched_subscription_data(control_handle).ok()?;
        (subscription_data.group_data().value == changed_group_data).then_some(())
    });
    assert!(is_control_updated.is_some(), "control reader update was not received");
    assert_eq!(data_writer.get_matched_subscriptions().unwrap(), vec![control_handle]);

    let discovered_subscriptions = writer_participant.get_discovered_subscriptions().unwrap();
    assert!(!discovered_subscriptions
        .iter()
        .any(|data| &data.endpoint_guid().to_bytes() == ignored_handle.value()));

    delete_participant(reader_participant);
    delete_participant(writer_participant);
}

#[test]
fn ignore_subscription_marks_builtin_subscription_not_alive_no_writers() {
    let domain_id = next_domain_id();
    let writer_participant = create_participant(domain_id);
    let reader_participant = create_participant(domain_id);

    let topic = reader_participant
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = reader_participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let ignored_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let control_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let ignored_handle = ignored_reader.get_instance_handle().unwrap();
    let control_handle = control_reader.get_instance_handle().unwrap();
    let subscription_reader = writer_participant
        .get_builtin_subscriber()
        .unwrap()
        .lookup_datareader::<SubscriptionBuiltinTopicData>("DCPSSubscription")
        .unwrap();

    let is_both_discovered = wait_for(|| {
        let samples = subscription_reader
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
    assert!(is_both_discovered.is_some(), "readers were not discovered");

    writer_participant.ignore_subscription(ignored_handle).unwrap();

    let ignored_samples = subscription_reader
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

    let control_samples = subscription_reader
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

    delete_participant(reader_participant);
    delete_participant(writer_participant);
}
