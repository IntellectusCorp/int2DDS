#![allow(dead_code)]

use std::sync::atomic::{AtomicI32, Ordering};

use int2dds::dcps::{
    core::time::Duration,
    domain::domain_participant::DomainParticipant,
    infrastructure::{status::StatusMask, wait_set::WaitSet},
    publication::{
        data_writer::DataWriter,
        qos::{DataWriterQos, PublisherQos},
    },
    subscription::{
        data_reader::DataReader,
        qos::{DataReaderQos, SubscriberQos},
    },
    topic::{qos::TopicQos, type_support::DdsType},
};
use speedy::{Readable, Writable};

#[derive(DdsType, Readable, Writable)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct KeyedDataType {
    #[dds(key)]
    pub key: i16,
    pub value: i16,
}

impl KeyedDataType {
    pub fn new(key: i16, value: i16) -> Self {
        Self { key, value }
    }

    pub fn default() -> Self {
        Self { key: 0, value: 0 }
    }
}

static DOMAIN_ID: AtomicI32 = AtomicI32::new(0);

pub fn next_domain_id() -> i32 {
    DOMAIN_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn create_datareader(
    domain_participant: &DomainParticipant,
    subscriber_qos: SubscriberQos,
    data_reader_qos: DataReaderQos,
) -> DataReader<KeyedDataType> {
    let topic = domain_participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber =
        domain_participant.create_subscriber(subscriber_qos, None, StatusMask::default()).unwrap();

    let reader = subscriber
        .create_datareader::<KeyedDataType>(&topic, data_reader_qos, None, StatusMask::default())
        .unwrap();

    reader
}

pub fn create_datawriter(
    domain_participant: &DomainParticipant,
    publisher_qos: PublisherQos,
    datawriter_qos: DataWriterQos,
) -> DataWriter<KeyedDataType> {
    let topic = domain_participant
        .create_topic::<KeyedDataType>(
            "test_topic",
            "KeyedDataType",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        domain_participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();

    let writer = publisher
        .create_datawriter::<KeyedDataType>(&topic, datawriter_qos, None, StatusMask::default())
        .unwrap();

    writer
}

pub fn wait_for_writer_status(data_writer: &DataWriter<KeyedDataType>, status_mask: StatusMask) {
    let mut condition = data_writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(status_mask).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::from_seconds(1)).unwrap();
}

pub fn wait_for_reader_status(data_reader: &DataReader<KeyedDataType>, status_mask: StatusMask) {
    let mut condition = data_reader.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(status_mask).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(Duration::from_seconds(1)).unwrap();
}
