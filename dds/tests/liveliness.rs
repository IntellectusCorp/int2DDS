mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, LivelinessQosPolicy,
                LivelinessQosPolicyKind, WriterDataLifecycleQosPolicy,
            },
            status::StatusMask,
        },
        publication::{
            data_writer_listener::DataWriterListener,
            qos::{DataWriterQos, PublisherQos},
        },
        subscription::{
            data_reader_listener::DataReaderListener,
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
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

    // ManualByParticipant: write() on any writer asserts liveliness for the
    // whole participant, so both writers must show as alive.
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert_eq!(liveliness.alive_count(), 2);
    assert_eq!(liveliness.not_alive_count(), 0);
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

    // ManualByTopic: write() asserts liveliness only for that writer, so the
    // busy writer stays alive while the lazy writer transitions to not_alive.
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert_eq!(liveliness.alive_count(), 1);
    assert_eq!(liveliness.not_alive_count(), 1);
}

/// Tracks alive_count and not_alive_count from on_liveliness_changed.
struct LivelinessStateListener {
    alive: Arc<AtomicI32>,
    not_alive: Arc<AtomicI32>,
}

impl DataReaderListener for LivelinessStateListener {
    type Foo = KeyedDataType;

    fn on_liveliness_changed(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::LivelinessChangedStatus,
    ) {
        self.alive.store(status.alive_count(), Ordering::SeqCst);
        self.not_alive.store(status.not_alive_count(), Ordering::SeqCst);
    }
}

use std::sync::atomic::AtomicI32;

/// ManualByTopic: assert_liveliness() alone (no write) must keep writer alive,
/// and stopping assert must trigger LOST, then resuming must recover.
#[test]
fn test_manual_by_topic_assert_liveliness_lifecycle() {
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

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    let writer_qos = DataWriterQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_seconds(2),
        },
        ..Default::default()
    };

    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(&topic, writer_qos, None, StatusMask::default())
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let reader_qos = DataReaderQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByTopic,
            lease_duration: Duration::from_seconds(2),
        },
        ..Default::default()
    };

    let alive = Arc::new(AtomicI32::new(0));
    let not_alive = Arc::new(AtomicI32::new(0));
    let listener =
        LivelinessStateListener { alive: Arc::clone(&alive), not_alive: Arc::clone(&not_alive) };

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reader_qos,
            Some(Arc::new(listener)),
            StatusMask::default(),
        )
        .unwrap();

    // Wait for matching
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();

    // Phase 1: assert_liveliness() every 500ms for 3s — no LOST must occur
    for _ in 0..6 {
        data_writer.assert_liveliness().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    assert_eq!(
        not_alive.load(Ordering::SeqCst),
        0,
        "Phase 1 failed: writer should not be not_alive while asserting"
    );

    // Phase 2: stop asserting — lease (2s) must expire
    std::thread::sleep(std::time::Duration::from_millis(3000));
    assert!(
        not_alive.load(Ordering::SeqCst) >= 1,
        "Phase 2 failed: writer should be not_alive after lease expired (not_alive={})",
        not_alive.load(Ordering::SeqCst)
    );

    // Phase 3: resume assert_liveliness() — writer must recover to alive
    for _ in 0..4 {
        data_writer.assert_liveliness().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    assert!(
        alive.load(Ordering::SeqCst) >= 1,
        "Phase 3 failed: writer should have recovered to alive (alive={})",
        alive.load(Ordering::SeqCst)
    );
    assert_eq!(
        not_alive.load(Ordering::SeqCst),
        0,
        "Phase 3 failed: not_alive should be 0 after recovery"
    );
}

// After delete_datawriter, the reader must observe
// (a) subscription_matched -> 0, (b) liveliness alive -> not_alive, and
// (c) a one-shot synthetic NOT_ALIVE_NO_WRITERS sample.
#[test]
fn test_liveliness_on_local_writer_delete() {
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

    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();

    // autodispose_unregistered_instances=false so that the unmatch path settles
    // into NOT_ALIVE_NO_WRITERS (not NOT_ALIVE_DISPOSED).
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
    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(&topic, writer_qos, None, StatusMask::default())
        .unwrap();

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

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

    // (1) Match
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    let sub_matched = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(sub_matched.current_count(), 1, "Reader must see writer matched");
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert!(
        liveliness.alive_count() >= 1,
        "Reader must see alive_count >= 1 after match (alive={})",
        liveliness.alive_count()
    );
    assert_eq!(liveliness.not_alive_count(), 0, "not_alive must be 0 while writer is alive");

    // (2) One real sample flows; drain it so the cache is empty.
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(3))
        .unwrap();
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

    // (3) Delete the local writer.
    publisher.delete_datawriter(data_writer).unwrap();

    // (4) Reader observes unmatch + liveliness transition.
    wait_for_reader_status(&data_reader, StatusMask::LIVELINESS_CHANGED, Duration::from_seconds(3))
        .unwrap();
    let sub_matched = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(
        sub_matched.current_count(),
        0,
        "Reader must see 0 matched writers after local delete_datawriter"
    );
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert_eq!(
        liveliness.alive_count(),
        0,
        "alive_count must drop to 0 after local writer deletion (alive={})",
        liveliness.alive_count()
    );
    assert!(
        liveliness.not_alive_count() >= 1,
        "not_alive_count must be >= 1 after local writer deletion (not_alive={})",
        liveliness.not_alive_count()
    );

    // (5) Synthetic NOT_ALIVE_NO_WRITERS sample surfaces once even though cache is empty.
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

    // (6) The synthetic notification is one-shot.
    let again = data_reader.take(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );
    assert!(again.is_err());
    assert_eq!(again.err().unwrap(), DdsError::NoData);

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

// Cross-participant pair. After the writer participant deletes its
// writer, the SEDP dispose travels over the wire and the remote reader must
// observe the same end-state as the local case.
#[test]
fn test_liveliness_on_remote_writer_dispose() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant_writer = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let participant_reader = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic_writer = participant_writer
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let topic_reader = participant_reader
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher = participant_writer
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
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
    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(&topic_writer, writer_qos, None, StatusMask::default())
        .unwrap();

    let subscriber = participant_reader
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_qos = DataReaderQos {
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(&topic_reader, reader_qos, None, StatusMask::default())
        .unwrap();

    // (1) Match across participants
    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    let sub_matched = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(sub_matched.current_count(), 1);
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert!(liveliness.alive_count() >= 1);
    assert_eq!(liveliness.not_alive_count(), 0);

    // (2) One real sample flows; drain it.
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    wait_for_reader_status(&data_reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(3))
        .unwrap();
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

    // (3) Delete the remote writer; SEDP dispose travels to the reader side.
    publisher.delete_datawriter(data_writer).unwrap();

    // (4) Reader observes unmatch + liveliness transition over the wire.
    wait_for_reader_status(&data_reader, StatusMask::LIVELINESS_CHANGED, Duration::from_seconds(3))
        .unwrap();
    let sub_matched = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(sub_matched.current_count(), 0);
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert_eq!(liveliness.alive_count(), 0);
    assert!(liveliness.not_alive_count() >= 1);

    // (5) Synthetic NOT_ALIVE_NO_WRITERS sample surfaces once.
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

    // (6) The synthetic notification is one-shot.
    let again = data_reader.take(
        10,
        &[SampleStateKind::ANY_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    );
    assert!(again.is_err());
    assert_eq!(again.err().unwrap(), DdsError::NoData);

    participant_writer.delete_contained_entities().unwrap();
    factory.delete_participant(participant_writer).unwrap();
    participant_reader.delete_contained_entities().unwrap();
    factory.delete_participant(participant_reader).unwrap();
}

// Cross-participant pair. Deleting the writer participant entirely (cascade
// via factory.delete_participant) must surface unmatch + liveliness transition
// on the reader side.
#[test]
fn test_liveliness_on_remote_participant_deleted() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant_writer = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let participant_reader = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic_writer = participant_writer
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let topic_reader = participant_reader
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher = participant_writer
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let writer_qos = DataWriterQos { ..Default::default() };
    let _data_writer = publisher
        .create_datawriter::<KeyedDataType>(&topic_writer, writer_qos, None, StatusMask::default())
        .unwrap();

    let subscriber = participant_reader
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_qos = DataReaderQos { ..Default::default() };
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(&topic_reader, reader_qos, None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(2),
    )
    .unwrap();
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert!(liveliness.alive_count() >= 1);

    // Tear down the writer participant entirely.
    participant_writer.delete_contained_entities().unwrap();
    factory.delete_participant(participant_writer).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::LIVELINESS_CHANGED, Duration::from_seconds(2))
        .unwrap();
    let sub_matched = data_reader.get_subscription_matched_status().unwrap();
    assert_eq!(sub_matched.current_count(), 0);
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert_eq!(liveliness.alive_count(), 0);
    assert!(liveliness.not_alive_count() >= 1);

    participant_reader.delete_contained_entities().unwrap();
    factory.delete_participant(participant_reader).unwrap();
}

// ManualByParticipant: when the writer participant stops asserting, the
// reader must observe alive -> not_alive after the lease expires, and recover
// when assertion resumes.
#[test]
fn test_liveliness_lost_manual_by_participant() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let participant_writer = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let participant_reader = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let topic_writer = participant_writer
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let topic_reader = participant_reader
        .create_topic::<KeyedDataType>(
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher = participant_writer
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();
    let writer_qos = DataWriterQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByParticipant,
            lease_duration: Duration::from_seconds(1),
        },
        ..Default::default()
    };
    let data_writer = publisher
        .create_datawriter::<KeyedDataType>(&topic_writer, writer_qos, None, StatusMask::default())
        .unwrap();

    let subscriber = participant_reader
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader_qos = DataReaderQos {
        liveliness: LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::ManualByParticipant,
            lease_duration: Duration::from_seconds(1),
        },
        ..Default::default()
    };
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(&topic_reader, reader_qos, None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    wait_for_writer_status(
        &data_writer,
        StatusMask::PUBLICATION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();

    // Phase 1: assert every 250ms for 1s -> writer must stay alive.
    for _ in 0..4 {
        participant_writer.assert_liveliness().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    assert_eq!(
        data_reader.get_liveliness_changed_status().unwrap().not_alive_count(),
        0,
        "writer must stay alive while assertion is active"
    );

    // Phase 2: stop asserting; lease (1s) must expire.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert!(
        liveliness.not_alive_count() >= 1,
        "writer must be not_alive after lease expired (not_alive={})",
        liveliness.not_alive_count()
    );

    // Phase 3: resume assertion -> writer must recover.
    for _ in 0..3 {
        participant_writer.assert_liveliness().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert!(
        liveliness.alive_count() >= 1,
        "writer must have recovered (alive={})",
        liveliness.alive_count()
    );

    participant_writer.delete_contained_entities().unwrap();
    factory.delete_participant(participant_writer).unwrap();
    participant_reader.delete_contained_entities().unwrap();
    factory.delete_participant(participant_reader).unwrap();
}
