mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{LivelinessQosPolicy, LivelinessQosPolicyKind},
            status::StatusMask,
        },
        publication::{
            data_writer_listener::DataWriterListener,
            qos::{DataWriterQos, PublisherQos},
        },
        subscription::{
            data_reader_listener::DataReaderListener,
            qos::{DataReaderQos, SubscriberQos},
        },
        topic::qos::TopicQos,
    },
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct ReaderListener {
    change_count: Arc<AtomicUsize>,
}

impl DataReaderListener for ReaderListener {
    type Foo = KeyedDataType;

    fn on_liveliness_changed(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        _status: &int2dds::infrastructure::status::LivelinessChangedStatus,
    ) {
        self.change_count.fetch_add(1, Ordering::SeqCst);
    }
}

struct WriterListener {
    change_count: Arc<AtomicUsize>,
}

impl DataWriterListener for WriterListener {
    type Foo = KeyedDataType;

    fn on_liveliness_lost(
        &self,
        _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
        _status: &int2dds::infrastructure::status::LivelinessLostStatus,
    ) {
        self.change_count.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn test_manual_by_participant() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
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

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let writer_qos = DataWriterQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByParticipant,
            lease_duration: Duration::from_seconds(1),
        },
        ..Default::default()
    };

    let lazy_writer_count = Arc::new(AtomicUsize::new(0));
    let busy_writer_count = Arc::new(AtomicUsize::new(0));

    let lazy_writer_listener = WriterListener { change_count: Arc::clone(&lazy_writer_count) };
    let busy_writer_listener = WriterListener { change_count: Arc::clone(&busy_writer_count) };

    let lazy_data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            writer_qos.clone(),
            Some(Arc::new(lazy_writer_listener)),
            StatusMask::default(),
        )
        .unwrap();
    let busy_data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            writer_qos.clone(),
            Some(Arc::new(busy_writer_listener)),
            StatusMask::default(),
        )
        .unwrap();

    let reader_qos = DataReaderQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByParticipant,
            lease_duration: Duration::from_seconds(1),
        },
        ..Default::default()
    };

    let reader_count = Arc::new(AtomicUsize::new(0));
    let reader_listener = ReaderListener { change_count: Arc::clone(&reader_count) };

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reader_qos,
            Some(Arc::new(reader_listener)),
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
        &lazy_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &busy_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    // Busy writer keeps writing to maintain liveliness
    std::thread::sleep(std::time::Duration::from_millis(600));
    busy_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(600));
    busy_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    // Lazy writer never writes within 1 sec but still maintains liveliness
    assert_eq!(busy_writer_count.load(Ordering::SeqCst), 0);
    assert_eq!(lazy_writer_count.load(Ordering::SeqCst), 0);
    assert_eq!(reader_count.load(Ordering::SeqCst), 0);
}

#[test]
fn test_manual_by_topic() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
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

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let writer_qos = DataWriterQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_seconds(1),
        },
        ..Default::default()
    };

    let lazy_writer_count = Arc::new(AtomicUsize::new(0));
    let busy_writer_count = Arc::new(AtomicUsize::new(0));

    let lazy_writer_listener = WriterListener { change_count: Arc::clone(&lazy_writer_count) };
    let busy_writer_listener = WriterListener { change_count: Arc::clone(&busy_writer_count) };

    let lazy_data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            writer_qos.clone(),
            Some(Arc::new(lazy_writer_listener)),
            StatusMask::default(),
        )
        .unwrap();
    let busy_data_writer = publisher
        .create_datawriter::<KeyedDataType>(
            &topic,
            writer_qos.clone(),
            Some(Arc::new(busy_writer_listener)),
            StatusMask::default(),
        )
        .unwrap();

    let reader_qos = DataReaderQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_seconds(1),
        },
        ..Default::default()
    };

    let reader_count = Arc::new(AtomicUsize::new(0));
    let reader_listener = ReaderListener { change_count: Arc::clone(&reader_count) };

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reader_qos,
            Some(Arc::new(reader_listener)),
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
        &lazy_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();
    wait_for_writer_status(
        &busy_data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(1),
    )
    .unwrap();

    // Busy writer keeps writing to maintain liveliness
    std::thread::sleep(std::time::Duration::from_millis(600));
    busy_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(600));
    busy_data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();

    // Lazy writer lost liveliness
    assert_eq!(busy_writer_count.load(Ordering::SeqCst), 0);
    assert_eq!(lazy_writer_count.load(Ordering::SeqCst), 1);
    assert_eq!(reader_count.load(Ordering::SeqCst), 1);
}
