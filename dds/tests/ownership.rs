mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                DeadlineQosPolicy, LivelinessQosPolicy, LivelinessQosPolicyKind,
                OwnershipQosPolicy, OwnershipQosPolicyKind, OwnershipStrengthQosPolicy,
            },
            status::{RequestedDeadlineMissedStatus, StatusMask},
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            data_reader::DataReader,
            data_reader_listener::DataReaderListener,
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::qos::TopicQos,
    },
};
use log::debug;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

/**
 * Tests exceptional cases with QoS interactions
 * Since happy cases are covered in interoperability tests.
 */

struct ReaderDeadlineListener {
    miss_count: Arc<AtomicUsize>,
}

impl DataReaderListener for ReaderDeadlineListener {
    type Foo = KeyedDataType;

    fn on_requested_deadline_missed(
        &self,
        _reader: &DataReader<Self::Foo>,
        _status: &RequestedDeadlineMissedStatus,
    ) {
        self.miss_count.fetch_add(1, Ordering::SeqCst);
        // println!("Reader deadline missed detected!");
    }
}

#[test]
fn test_ownership_revoked_when_deadline_missed() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let weaker_writer_qos = DataWriterQos {
        ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
        ownership_strength: OwnershipStrengthQosPolicy { value: 10 },
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        ..Default::default()
    };

    let stronger_writer_qos = DataWriterQos {
        ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
        ownership_strength: OwnershipStrengthQosPolicy { value: 20 },
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        ..Default::default()
    };

    let weaker_data_writer =
        create_datawriter(&participant, PublisherQos::default(), weaker_writer_qos);
    let stronger_data_writer =
        create_datawriter(&participant, PublisherQos::default(), stronger_writer_qos);

    let reader_qos = DataReaderQos {
        ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        ..Default::default()
    };

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let topic = participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let deadline_miss_count = Arc::new(AtomicUsize::new(0));
    let listener =
        Arc::new(ReaderDeadlineListener { miss_count: Arc::clone(&deadline_miss_count) });

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reader_qos,
            Some(listener),
            StatusMask::REQUESTED_DEADLINE_MISSED,
        )
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &weaker_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &stronger_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    stronger_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(5))
        .unwrap();

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    // Stronger writer's data should be received
    assert!(!samples.is_empty());

    weaker_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    let res = wait_for_reader_status(
        &data_reader,
        StatusMask::DATA_AVAILABLE,
        Duration::from_millis(1500),
    );

    // Weaker writer's data should not be received
    assert!(res.is_err());

    // After deadline missed, stronger writer should lose ownership
    let miss_count = deadline_miss_count.load(Ordering::SeqCst);
    assert_eq!(miss_count, 1);

    weaker_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(5))
        .unwrap();

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    // Now weaker writer's data should be received because owner changed
    assert!(!samples.is_empty());
}

#[test]
fn test_ownership_revoked_when_liveliness_lost() {
    // set_log_type(LogType::File);
    // set_file_log_level(LogLevel::Debug);

    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let weaker_writer_qos = DataWriterQos {
        ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
        ownership_strength: OwnershipStrengthQosPolicy { value: 10 },
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_millis(1000),
        },
        ..Default::default()
    };

    let stronger_writer_qos = DataWriterQos {
        ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
        ownership_strength: OwnershipStrengthQosPolicy { value: 20 },
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_millis(1000),
        },
        ..Default::default()
    };

    let weaker_data_writer =
        create_datawriter(&participant, PublisherQos::default(), weaker_writer_qos);
    let stronger_data_writer =
        create_datawriter(&participant, PublisherQos::default(), stronger_writer_qos);

    let reader_qos = DataReaderQos {
        ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_millis(1000),
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
        &weaker_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &stronger_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    // Stronger writer's data should be received
    stronger_data_writer.write(&KeyedDataType::new(0, 1), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_millis(1000))
        .unwrap();

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    assert!(!samples.is_empty());
    assert_eq!(samples[0].data().unwrap().value, 1);

    weaker_data_writer.write(&KeyedDataType::new(0, 2), InstanceHandle::NIL).unwrap();

    let res = wait_for_reader_status(
        &data_reader,
        StatusMask::DATA_AVAILABLE,
        Duration::from_millis(300),
    );

    // Weaker writer's data should not be received
    assert!(res.is_err());

    let res = wait_for_reader_status(
        &data_reader,
        StatusMask::LIVELINESS_CHANGED,
        Duration::from_millis(5000),
    );

    // Wait for stronger writer's liveliness to be lost
    assert!(res.is_ok(), "Liveliness was not lost in time: {:?}", res);
    debug!("Stronger writer lost liveliness.");

    std::thread::sleep(std::time::Duration::from_millis(300));
    debug!("Stronger writer may have lost ownership by now.");

    // Now weaker writer should become owner
    debug!("Weaker writer trying to write data...");
    weaker_data_writer.write(&KeyedDataType::new(0, 3), InstanceHandle::NIL).unwrap();
    debug!("Weaker writer wrote data after stronger writer lost liveliness.");

    debug!("Waiting for data to be available on reader...");
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(1))
        .unwrap();

    let samples = data_reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();

    // Weaker writer's data should be received because stronger writer lost liveliness
    assert!(!samples.is_empty());
    assert_eq!(samples[0].data().unwrap().value, 3);
}
