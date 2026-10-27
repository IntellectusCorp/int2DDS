use crate::{
    common::*, create_disabled_participant, create_participant, delete_participant, wait_for,
};
use int2dds::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    dcps::{
        core::error::DdsError,
        infrastructure::qos_policy::UserDataQosPolicy,
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
    },
};

#[test]
fn ignore_participant_requires_enabled_participant() {
    let participant = create_disabled_participant(next_domain_id());
    let handle = participant.get_instance_handle().unwrap();

    assert_eq!(participant.ignore_participant(handle), Err(DdsError::NotEnabled));

    delete_participant(participant);
}

#[test]
fn ignore_participant_rejects_own_participant() {
    let participant = create_participant(next_domain_id());
    let handle = participant.get_instance_handle().unwrap();

    assert_eq!(participant.ignore_participant(handle), Err(DdsError::BadParameter));

    delete_participant(participant);
}

#[test]
fn ignore_participant_before_discovery_prevents_discovery_and_matching() {
    let domain_id = next_domain_id();
    let local_participant = create_participant(domain_id);
    let data_reader =
        create_datareader(&local_participant, SubscriberQos::default(), DataReaderQos::default());

    // The remote participants stay disabled, so they are not announced until enable().
    let ignored_participant = create_disabled_participant(domain_id);
    let control_participant = create_disabled_participant(domain_id);
    let ignored_participant_handle = ignored_participant.get_instance_handle().unwrap();
    let control_participant_handle = control_participant.get_instance_handle().unwrap();

    local_participant.ignore_participant(ignored_participant_handle).unwrap();

    ignored_participant.enable().unwrap();
    control_participant.enable().unwrap();
    let ignored_writer =
        create_datawriter(&ignored_participant, PublisherQos::default(), DataWriterQos::default());
    let control_writer =
        create_datawriter(&control_participant, PublisherQos::default(), DataWriterQos::default());
    let ignored_writer_handle = ignored_writer.get_instance_handle().unwrap();
    let control_writer_handle = control_writer.get_instance_handle().unwrap();

    // The two participants are not announced in order, so this only detects an ignored
    // participant that is discovered before the control writer matches.
    let matched_publications = wait_for(|| {
        let handles = data_reader.get_matched_publications().ok()?;
        handles.contains(&control_writer_handle).then_some(handles)
    })
    .expect("control writer was not matched");
    assert!(!matched_publications.contains(&ignored_writer_handle));

    let discovered_participants = local_participant.get_discovered_participants().unwrap();
    assert!(discovered_participants.contains(&control_participant_handle));
    assert!(!discovered_participants.contains(&ignored_participant_handle));
    assert_eq!(
        local_participant.get_discovered_participant_data(ignored_participant_handle).err(),
        Some(DdsError::PreconditionNotMet)
    );

    let discovered_publications = local_participant.get_discovered_publications().unwrap();
    assert!(!discovered_publications
        .iter()
        .any(|data| &data.endpoint_guid().to_bytes() == ignored_writer_handle.value()));

    delete_participant(control_participant);
    delete_participant(ignored_participant);
    delete_participant(local_participant);
}

#[test]
fn ignore_participant_after_matching_unmatches_its_endpoints() {
    let domain_id = next_domain_id();
    let local_participant = create_participant(domain_id);
    let ignored_participant = create_participant(domain_id);
    let control_participant = create_participant(domain_id);
    let data_reader =
        create_datareader(&local_participant, SubscriberQos::default(), DataReaderQos::default());
    let ignored_writer =
        create_datawriter(&ignored_participant, PublisherQos::default(), DataWriterQos::default());
    let control_writer =
        create_datawriter(&control_participant, PublisherQos::default(), DataWriterQos::default());
    let ignored_participant_handle = ignored_participant.get_instance_handle().unwrap();
    let ignored_writer_handle = ignored_writer.get_instance_handle().unwrap();
    let control_writer_handle = control_writer.get_instance_handle().unwrap();

    let is_both_matched = wait_for(|| {
        let handles = data_reader.get_matched_publications().ok()?;
        (handles.contains(&ignored_writer_handle) && handles.contains(&control_writer_handle))
            .then_some(())
    });
    assert!(is_both_matched.is_some(), "writers were not matched");

    let matched_status = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(matched_status.current_count(), 2);

    local_participant.ignore_participant(ignored_participant_handle).unwrap();

    let matched_status = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(matched_status.current_count(), 1);
    assert_eq!(data_reader.get_matched_publications().unwrap(), vec![control_writer_handle]);

    let discovered_participants = local_participant.get_discovered_participants().unwrap();
    assert!(!discovered_participants.contains(&ignored_participant_handle));
    assert_eq!(
        local_participant.get_discovered_participant_data(ignored_participant_handle).err(),
        Some(DdsError::PreconditionNotMet)
    );

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
        received_publication_handles.contains(&control_writer_handle).then_some(())
    });
    assert!(is_control_received.is_some(), "control sample was not received");
    assert!(!received_publication_handles.contains(&ignored_writer_handle));

    delete_participant(control_participant);
    delete_participant(ignored_participant);
    delete_participant(local_participant);
}

#[test]
fn ignore_participant_marks_builtin_endpoints_not_alive_no_writers() {
    let domain_id = next_domain_id();
    let local_participant = create_participant(domain_id);
    let ignored_participant = create_participant(domain_id);
    let control_participant = create_participant(domain_id);
    let ignored_writer =
        create_datawriter(&ignored_participant, PublisherQos::default(), DataWriterQos::default());
    let ignored_reader =
        create_datareader(&ignored_participant, SubscriberQos::default(), DataReaderQos::default());
    let control_writer =
        create_datawriter(&control_participant, PublisherQos::default(), DataWriterQos::default());
    let ignored_participant_handle = ignored_participant.get_instance_handle().unwrap();
    let ignored_writer_handle = ignored_writer.get_instance_handle().unwrap();
    let ignored_reader_handle = ignored_reader.get_instance_handle().unwrap();
    let control_writer_handle = control_writer.get_instance_handle().unwrap();
    let builtin_subscriber = local_participant.get_builtin_subscriber().unwrap();
    let publication_reader = builtin_subscriber
        .lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication")
        .unwrap();
    let subscription_reader = builtin_subscriber
        .lookup_datareader::<SubscriptionBuiltinTopicData>("DCPSSubscription")
        .unwrap();

    let is_all_discovered = wait_for(|| {
        let publication_samples = publication_reader
            .read(
                i32::MAX,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            )
            .ok()?;
        let subscription_samples = subscription_reader
            .read(
                i32::MAX,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
            )
            .ok()?;
        let publication_handles: Vec<InstanceHandle> =
            publication_samples.iter().map(|sample| sample.sample_info().instance_handle).collect();
        let subscription_handles: Vec<InstanceHandle> = subscription_samples
            .iter()
            .map(|sample| sample.sample_info().instance_handle)
            .collect();
        (publication_handles.contains(&ignored_writer_handle)
            && publication_handles.contains(&control_writer_handle)
            && subscription_handles.contains(&ignored_reader_handle))
        .then_some(())
    });
    assert!(is_all_discovered.is_some(), "endpoints were not discovered");

    local_participant.ignore_participant(ignored_participant_handle).unwrap();

    let ignored_writer_samples = publication_reader
        .read_instance(
            i32::MAX,
            ignored_writer_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert!(!ignored_writer_samples.is_empty());
    assert!(ignored_writer_samples.iter().all(|sample| sample.sample_info().instance_state
        == InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE));

    let ignored_reader_samples = subscription_reader
        .read_instance(
            i32::MAX,
            ignored_reader_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert!(!ignored_reader_samples.is_empty());
    assert!(ignored_reader_samples.iter().all(|sample| sample.sample_info().instance_state
        == InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE));

    let control_writer_samples = publication_reader
        .read_instance(
            i32::MAX,
            control_writer_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert!(!control_writer_samples.is_empty());
    assert!(control_writer_samples.iter().all(
        |sample| sample.sample_info().instance_state == InstanceStateKind::ALIVE_INSTANCE_STATE
    ));

    delete_participant(control_participant);
    delete_participant(ignored_participant);
    delete_participant(local_participant);
}

#[test]
fn ignore_participant_survives_reannouncement() {
    let domain_id = next_domain_id();
    let local_participant = create_participant(domain_id);
    let ignored_participant = create_participant(domain_id);
    let control_participant = create_participant(domain_id);
    let ignored_participant_handle = ignored_participant.get_instance_handle().unwrap();
    let control_participant_handle = control_participant.get_instance_handle().unwrap();

    let is_both_discovered = wait_for(|| {
        let handles = local_participant.get_discovered_participants().ok()?;
        (handles.contains(&ignored_participant_handle)
            && handles.contains(&control_participant_handle))
        .then_some(())
    });
    assert!(is_both_discovered.is_some(), "participants were not discovered");

    local_participant.ignore_participant(ignored_participant_handle).unwrap();

    // USER_DATA changes make both participants send SPDP again. The two announcements are not
    // ordered, so this only detects a rediscovery that happens before the control update is seen.
    let changed_user_data = b"changed-user-data".to_vec();
    for participant in [&ignored_participant, &control_participant] {
        let mut changed_qos = participant.get_qos().unwrap();
        changed_qos.user_data = UserDataQosPolicy { value: changed_user_data.clone() };
        participant.set_qos(changed_qos).unwrap();
    }

    let is_control_updated = wait_for(|| {
        let participant_data =
            local_participant.get_discovered_participant_data(control_participant_handle).ok()?;
        (participant_data.user_data().value == changed_user_data).then_some(())
    });
    assert!(is_control_updated.is_some(), "control participant update was not received");

    let discovered_participants = local_participant.get_discovered_participants().unwrap();
    assert!(!discovered_participants.contains(&ignored_participant_handle));

    delete_participant(control_participant);
    delete_participant(ignored_participant);
    delete_participant(local_participant);
}
