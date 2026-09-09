mod common;

use std::sync::{
    mpsc::{sync_channel, SyncSender},
    Arc,
};

use common::*;
use int2dds::{
    common::instance_handle::InstanceHandle,
    dcps::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{DeadlineQosPolicy, HistoryQosPolicy, HistoryQosPolicyKind},
            status::StatusMask,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            data_reader_listener::DataReaderListener,
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::qos::TopicQos,
    },
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

    let writer_qos = DataWriterQos {
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        ..Default::default()
    };

    let _data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
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

    let (sender, receiver) = sync_channel(10);
    let read_listener = SubListener { sender };

    let mut reader_qos = DataReaderQos {
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        ..Default::default()
    };

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

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

#[test]
fn test_match_after_set_qos() {
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let writer_qos = DataWriterQos {
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        ..Default::default()
    };

    let _data_writer = create_datawriter(&participant, PublisherQos::default(), writer_qos);

    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
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

    let (sender, receiver) = sync_channel(10);
    let read_listener = SubListener { sender };

    let mut reader_qos = DataReaderQos {
        deadline: DeadlineQosPolicy { period: Duration::from_millis(500) },
        ..Default::default()
    };

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

    participant.delete_contained_entities().unwrap();
    factory.delete_participant(participant).unwrap();
}

// Reader's set_qos to an incompatible deadline must remove the writer match
// and surface liveliness transition on the reader side.
#[test]
fn test_liveliness_on_qos_change_incompatible() {
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
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
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
    let mut reader_qos = DataReaderQos {
        deadline: DeadlineQosPolicy { period: Duration::from_millis(1000) },
        history: HistoryQosPolicy {
            kind: HistoryQosPolicyKind::KeepLast(10),
            ..Default::default()
        },
        ..Default::default()
    };
    let data_reader = subscriber
        .create_datareader::<KeyedDataType>(&topic, reader_qos.clone(), None, StatusMask::default())
        .unwrap();

    wait_for_reader_status(
        &data_reader,
        StatusMask::SUBSCRIPTION_MATCHED,
        Duration::from_seconds(3),
    )
    .unwrap();
    assert_eq!(data_reader.get_subscription_matched_status().unwrap().current_count(), 1);
    assert!(data_reader.get_liveliness_changed_status().unwrap().alive_count() >= 1);

    // One real sample flows; drain it so the cache is empty before unmatch.
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

    // Make reader.deadline < writer.deadline -> incompatible (RxO violated).
    reader_qos.deadline = DeadlineQosPolicy { period: Duration::from_millis(500) };
    data_reader.set_qos(reader_qos).unwrap();

    wait_for_reader_status(&data_reader, StatusMask::LIVELINESS_CHANGED, Duration::from_seconds(3))
        .unwrap();
    assert_eq!(data_reader.get_subscription_matched_status().unwrap().current_count(), 0);
    let liveliness = data_reader.get_liveliness_changed_status().unwrap();
    assert_eq!(liveliness.alive_count(), 0);
    // QoS-incompatible unmatch from ALIVE: must not bump not_alive_count.
    assert_eq!(liveliness.not_alive_count(), 0);

    // Synthetic NOT_ALIVE_NO_WRITERS sample must surface once even though the
    // cache is empty.
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
