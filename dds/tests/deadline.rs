mod common;

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::{RequestedDeadlineMissedStatus, StatusMask},
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            data_reader::DataReader,
            data_reader_listener::DataReaderListener,
            qos::{DataReaderQos, SubscriberQos},
        },
        topic::qos::TopicQos,
    },
};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    thread,
};
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
        println!("Reader deadline missed detected!");
    }
}

#[test]
fn test_reader_deadline_qos_basic() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut writer_qos = DataWriterQos::default();
    writer_qos.deadline.period = Duration::from_millis(200);
    let mut reader_qos = DataReaderQos::default();
    reader_qos.deadline.period = Duration::from_millis(200);

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let topic = participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Subscriber & Reader
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
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

    wait_for_reader_status(&data_reader, StatusMask::SUBSCRIPTION_MATCHED);
    wait_for_writer_status(&data_writer, StatusMask::PUBLICATION_MATCHED);

    // Send first data
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    thread::sleep(std::time::Duration::from_millis(100));

    // Send again before deadline
    data_writer.write(&KeyedDataType::default(), InstanceHandle::NIL).unwrap();
    thread::sleep(std::time::Duration::from_millis(100));

    // Should not have deadline miss yet
    assert_eq!(
        deadline_miss_count.load(Ordering::SeqCst),
        0,
        "No deadline miss should occur when receiving data within deadline"
    );

    // Wait to exceed deadline
    thread::sleep(std::time::Duration::from_millis(300));

    // Deadline miss occurs
    let miss_count = deadline_miss_count.load(Ordering::SeqCst);
    assert!(miss_count >= 1, "At least one deadline miss should be detected, got {}", miss_count);
}

#[test]
fn test_reader_deadline_qos_on_dispose() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut writer_qos = DataWriterQos::default();
    writer_qos.deadline.period = Duration::from_millis(150);
    let mut reader_qos = DataReaderQos::default();
    reader_qos.deadline.period = Duration::from_millis(150);

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let topic = participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Subscriber & Reader
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
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

    wait_for_reader_status(&data_reader, StatusMask::SUBSCRIPTION_MATCHED);
    wait_for_writer_status(&data_writer, StatusMask::PUBLICATION_MATCHED);

    // Send data (register instance)
    let handle = data_writer.register_instance(&KeyedDataType::default()).unwrap();
    data_writer.write(&KeyedDataType::default(), handle).unwrap();

    thread::sleep(std::time::Duration::from_millis(100));

    // Wait for deadline miss to occur
    thread::sleep(std::time::Duration::from_millis(200));
    assert!(deadline_miss_count.load(Ordering::SeqCst) >= 1, "Deadline miss should occur");

    // Send dispose
    data_writer.dispose(&KeyedDataType::default(), handle).unwrap();
    println!("Instance disposed");

    thread::sleep(std::time::Duration::from_millis(100));

    let count_before = deadline_miss_count.load(Ordering::SeqCst);

    // Wait to exceed deadline after dispose
    thread::sleep(std::time::Duration::from_millis(200));

    let count_after = deadline_miss_count.load(Ordering::SeqCst);

    println!("Count before dispose processing: {}, after: {}", count_before, count_after);

    // After dispose, deadline miss should not occur anymore (or very rarely)
    assert!(count_after - count_before <= 1, "Deadline miss should stop after dispose");
}

#[test]
fn test_reader_deadline_qos_multiple_instances() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut writer_qos = DataWriterQos::default();
    writer_qos.deadline.period = Duration::from_millis(150);
    let mut reader_qos = DataReaderQos::default();
    reader_qos.deadline.period = Duration::from_millis(150);

    let data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let topic = participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    // Subscriber & Reader
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
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

    wait_for_reader_status(&data_reader, StatusMask::SUBSCRIPTION_MATCHED);
    wait_for_writer_status(&data_writer, StatusMask::PUBLICATION_MATCHED);

    // Send multiple instances
    let data1 = KeyedDataType { key: 1, value: 0 };
    let data2 = KeyedDataType { key: 2, value: 0 };
    let data3 = KeyedDataType { key: 3, value: 0 };

    data_writer.write(&data1, InstanceHandle::NIL).unwrap();
    data_writer.write(&data2, InstanceHandle::NIL).unwrap();
    data_writer.write(&data3, InstanceHandle::NIL).unwrap();

    println!("Three instances sent");

    thread::sleep(std::time::Duration::from_millis(100));

    // Wait for all instances' deadlines to expire
    thread::sleep(std::time::Duration::from_millis(200));

    // Deadline miss should occur in all 3 instances
    let miss_count = deadline_miss_count.load(Ordering::SeqCst);
    println!("Reader deadline miss count for 3 instances: {}", miss_count);
    assert!(
        miss_count >= 3,
        "At least 3 deadline misses expected (one per instance), got {}",
        miss_count
    );
}
