mod common;

use std::thread;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, LivelinessQosPolicy,
                LivelinessQosPolicyKind, ReaderDataLifecycleQosPolicy,
                WriterDataLifecycleQosPolicy,
            },
            status::StatusMask,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
    },
};

#[test]
fn test_autodispose_unregistered_instances_true() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default() // autodispose_unregistered_instances is true by default
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

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

    // Register first
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer.unregister_instance(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE],
        )
        .unwrap();

    assert!(samples.len() == 2);
}

#[test]
fn test_autodispose_unregistered_instances_false() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        writer_data_lifecycle: WriterDataLifecycleQosPolicy {
            autodispose_unregistered_instances: false,
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);
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

    // Register first
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer.unregister_instance(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE],
        )
        .unwrap();

    assert!(samples.len() == 2);
}

#[test]
fn test_autopurge_dispose_samples() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let data_writer_1 =
        create_datawriter(&participant, PublisherQos::default(), DataWriterQos::default());
    let data_writer_2 =
        create_datawriter(&participant, PublisherQos::default(), DataWriterQos::default());

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reader_data_lifecycle: ReaderDataLifecycleQosPolicy {
            autopurge_disposed_samples_delay: Duration::from_millis(100),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer_1,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer_2,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    data_writer_1.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer_2.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let samples = data_reader
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert!(samples.len() == 2);

    // Instance state is DISPOSED now
    data_writer_1.dispose(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    thread::sleep(std::time::Duration::from_millis(500));

    let samples = data_reader.read(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );

    assert!(samples.is_err());
    assert_eq!(samples.err().unwrap(), DdsError::NoData);
}

#[test]
fn test_autopurge_nowriter_samples() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    // This has to be set to false because if true, the instance will be disposed automatically upon unregistering.
    let writer_qos = DataWriterQos {
        writer_data_lifecycle: WriterDataLifecycleQosPolicy {
            autodispose_unregistered_instances: false,
        },
        ..Default::default()
    };

    let data_writer_1 =
        create_datawriter(&participant, PublisherQos::default(), writer_qos.clone());
    let data_writer_2 =
        create_datawriter(&participant, PublisherQos::default(), writer_qos.clone());

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        reader_data_lifecycle: ReaderDataLifecycleQosPolicy {
            autopurge_nowriter_samples_delay: Duration::from_millis(100),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer_1,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer_2,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    data_writer_1.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer_2.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let samples = data_reader
        .read(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert!(samples.len() == 2);

    data_writer_1.unregister_instance(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    data_writer_2.unregister_instance(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    // Instance state is NOT_ALIVE_NO_WRITERS now
    thread::sleep(std::time::Duration::from_millis(500));

    let samples = data_reader.read(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );

    assert!(samples.is_err());
    assert_eq!(samples.err().unwrap(), DdsError::NoData);
}

#[test]
fn test_synthetic_invalid_data_on_no_writers() {
    // After the reader has taken every real sample (cache empty), losing the
    // last writer via liveliness must surface exactly one synthetic
    // invalid-data sample carrying the NOT_ALIVE_NO_WRITERS transition; a
    // subsequent take must report NoData (the synthetic is one-shot).
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        writer_data_lifecycle: WriterDataLifecycleQosPolicy {
            // Force the NOT_ALIVE_NO_WRITERS path instead of NOT_ALIVE_DISPOSED.
            autodispose_unregistered_instances: false,
        },
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_millis(300),
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let reader_qos = DataReaderQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_millis(300),
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

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

    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    // Drain the real sample so the reader cache is empty.
    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(samples.len(), 1);
    assert!(samples[0].sample_info().valid_data);

    // Let the lease expire (we never call assert_liveliness) so the reader
    // sees LIVELINESS_CHANGED, which is what marks the pending synthetic.
    wait_for_reader_status(&data_reader, StatusMask::LIVELINESS_CHANGED, Duration::from_seconds(2))
        .unwrap();

    // Cache is empty, but the synthetic invalid-data sample must surface once.
    let synthetic = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(synthetic.len(), 1);
    let info = synthetic[0].sample_info();
    assert!(!info.valid_data);
    assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);

    // The synthetic notification is one-shot — a follow-up take is NoData again.
    let again = data_reader.take(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );
    assert!(again.is_err());
    assert_eq!(again.err().unwrap(), DdsError::NoData);
}

// #[test]
// fn test_synthetic_invalid_data_on_writer_unmatch_same_participant() {
//     // Same end-state as the liveliness variant, but reached via an explicit
//     // publisher.delete_datawriter() that unmatches the writer. After the
//     // reader cache has been drained, losing the last writer through delete
//     // must still surface a synthetic invalid-data sample carrying the
//     // NOT_ALIVE_NO_WRITERS transition exactly once.
//     let domain_id = next_domain_id();
//     let factory = DomainParticipantFactory::get_instance();
//     let participant = factory
//         .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
//         .unwrap();

//     let writer_qos = DataWriterQos {
//         writer_data_lifecycle: WriterDataLifecycleQosPolicy {
//             // Suppress the auto-dispose on unregister so the unmatch path
//             // settles into NOT_ALIVE_NO_WRITERS rather than NOT_ALIVE_DISPOSED.
//             autodispose_unregistered_instances: false,
//         },
//         history: HistoryQosPolicy {
//             kind: HistoryQosPolicyKind::KeepLast(10),
//             ..Default::default()
//         },
//         ..Default::default()
//     };

//     let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

//     let reader_qos = DataReaderQos {
//         history: HistoryQosPolicy {
//             kind: HistoryQosPolicyKind::KeepLast(10),
//             ..Default::default()
//         },
//         ..Default::default()
//     };

//     let data_reader = create_datareader(&participant, SubscriberQos::default(), reader_qos);

//     wait_for_reader_status(
//         &data_reader,
//         StatusMask::SUBSCRIPTION_MATCHED,
//         Duration::from_seconds(1),
//     )
//     .unwrap();
//     wait_for_writer_status(
//         &data_writer,
//         StatusMask::PUBLICATION_MATCHED,
//         Duration::from_seconds(1),
//     )
//     .unwrap();

//     data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
//     wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
//         .unwrap();

//     // Drain the real sample so the reader cache is empty.
//     let samples = data_reader
//         .take(
//             10,
//             &[SampleStateKind::ANY_SAMPLE_STATE],
//             &[ViewStateKind::ANY_VIEW_STATE],
//             &[InstanceStateKind::ANY_INSTANCE_STATE],
//         )
//         .unwrap();
//     assert_eq!(samples.len(), 1);
//     assert!(samples[0].sample_info().valid_data);

//     // Tear the writer down via the publisher; this is what triggers unmatch.
//     let publisher = data_writer.get_publisher().unwrap();
//     publisher.delete_datawriter(data_writer.clone()).unwrap();

//     std::thread::sleep(std::time::Duration::from_millis(10000000));

//     // Wait for the reader to observe the unmatch.
//     wait_for_reader_status(
//         &data_reader,
//         StatusMask::LIVELINESS_CHANGED,
//         Duration::from_seconds(2),
//     )
//     .unwrap();

//     // Cache is empty, but the synthetic invalid-data sample must surface once.
//     let synthetic = data_reader
//         .take(
//             10,
//             &[SampleStateKind::ANY_SAMPLE_STATE],
//             &[ViewStateKind::ANY_VIEW_STATE],
//             &[InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE],
//         )
//         .unwrap();
//     assert_eq!(synthetic.len(), 1);
//     let info = synthetic[0].sample_info();
//     assert!(!info.valid_data);
//     assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);

//     // The synthetic notification is one-shot — a follow-up take is NoData again.
//     let again = data_reader.take(
//         10,
//         &[SampleStateKind::ANY_SAMPLE_STATE],
//         &[ViewStateKind::ANY_VIEW_STATE],
//         &[InstanceStateKind::ANY_INSTANCE_STATE],
//     );
//     assert!(again.is_err());
//     assert_eq!(again.err().unwrap(), DdsError::NoData);
// }

#[test]
fn test_synthetic_invalid_data_on_writer_unmatch_two_participant() {
    // Same end-state as the liveliness variant, but reached via an explicit
    // publisher.delete_datawriter() that unmatches the writer. After the
    // reader cache has been drained, losing the last writer through delete
    // must still surface a synthetic invalid-data sample carrying the
    // NOT_ALIVE_NO_WRITERS transition exactly once.
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant_1 = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let participant_2 = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        writer_data_lifecycle: WriterDataLifecycleQosPolicy {
            // Suppress the auto-dispose on unregister so the unmatch path
            // settles into NOT_ALIVE_NO_WRITERS rather than NOT_ALIVE_DISPOSED.
            autodispose_unregistered_instances: false,
        },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_writer = create_datawriter(&participant_1, PublisherQos::default(), writer_qos);

    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };

    let data_reader = create_datareader(&participant_2, SubscriberQos::default(), reader_qos);

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

    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    // Drain the real sample so the reader cache is empty.
    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(samples.len(), 1);
    assert!(samples[0].sample_info().valid_data);

    // Tear the writer down via the publisher; this is what triggers unmatch.
    let publisher = data_writer.get_publisher().unwrap();
    publisher.delete_datawriter(data_writer.clone()).unwrap();

    // Wait for the reader to observe the unmatch.
    wait_for_reader_status(&data_reader, StatusMask::LIVELINESS_CHANGED, Duration::from_seconds(2))
        .unwrap();

    std::thread::sleep(std::time::Duration::from_millis(100));

    // Cache is empty, but the synthetic invalid-data sample must surface once.
    let synthetic = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(synthetic.len(), 1);
    let info = synthetic[0].sample_info();
    assert!(!info.valid_data);
    assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);

    // The synthetic notification is one-shot — a follow-up take is NoData again.
    let again = data_reader.take(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );
    assert!(again.is_err());
    assert_eq!(again.err().unwrap(), DdsError::NoData);
}
