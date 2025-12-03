use std::sync::atomic::{AtomicI32, Ordering};

use int2dds::dcps::{
    domain::{
        domain_participant::DomainParticipant,
        domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
    },
    infrastructure::status::StatusMask,
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
use socket2::Domain;
use speedy::{Readable, Writable};

#[derive(DdsType, Readable, Writable)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct KeyedDataType {
    #[dds(key)]
    pub key: i16,
    pub value: i16,
}

static DOMAIN_ID: AtomicI32 = AtomicI32::new(0);

pub fn next_domain_id() -> i32 {
    DOMAIN_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn create_datareader(
    domain_participant: &DomainParticipant,
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

    let subscriber = domain_participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();

    let reader = subscriber
        .create_datareader::<KeyedDataType>(&topic, data_reader_qos, None, StatusMask::default())
        .unwrap();

    reader
}

fn create_datawriter(
    domain_participant: &DomainParticipant,
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

    let publisher = domain_participant
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap();

    let writer = publisher
        .create_datawriter::<KeyedDataType>(&topic, datawriter_qos, None, StatusMask::default())
        .unwrap();

    writer
}
