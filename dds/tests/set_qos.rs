mod common;

use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc,
};

use common::*;
use int2dds::dcps::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{qos_policy::DeadlineQosPolicy, status::StatusMask},
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        data_reader_listener::DataReaderListener,
        qos::{DataReaderQos, SubscriberQos},
    },
    topic::qos::TopicQos,
};

struct SubListener {
    sender: SyncSender<bool>,
}

impl DataReaderListener for SubListener {
    type Foo = KeyedDataType;

    fn on_subscription_matched(
        &self,
        _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
    ) {
        if status.current_count() > 0 {
            // println!("Matched");
            let _ = self.sender.send(true);
        } else {
            // println!("Unmatched");
            let _ = self.sender.send(false);
        }
    }
}

#[test]
fn test_unmatch_after_set_qos() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut writer_qos = DataWriterQos::default();
    writer_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(1000) };

    let _data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

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

    let (sender, receiver) = sync_channel(10);
    let read_listener = SubListener { sender };

    let mut reader_qos = DataReaderQos::default();
    reader_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(1000) };

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reader_qos.clone(),
            Some(Arc::new(read_listener)),
            StatusMask::default(),
        )
        .unwrap();

    let res = receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert!(res);

    reader_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(500) };
    data_reader.set_qos(reader_qos).unwrap();

    let res = receiver.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
    assert!(!res);
}

#[test]
fn test_match_after_set_qos() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut writer_qos = DataWriterQos::default();
    writer_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(1000) };

    let _data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

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

    let (sender, receiver) = sync_channel(10);
    let read_listener = SubListener { sender };

    let mut reader_qos = DataReaderQos::default();
    reader_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(500) };

    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(
            &topic,
            reader_qos.clone(),
            Some(Arc::new(read_listener)),
            StatusMask::default(),
        )
        .unwrap();

    let res = receiver.recv_timeout(std::time::Duration::from_millis(500));
    assert!(res.is_err());

    reader_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(1000) };
    data_reader.set_qos(reader_qos).unwrap();

    let res = receiver.recv_timeout(std::time::Duration::from_millis(500)).unwrap();
    assert!(res);
}
