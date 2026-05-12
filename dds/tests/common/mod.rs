#![allow(dead_code)]

use std::sync::atomic::{AtomicI32, Ordering};

use int2dds::dcps::{
    core::time::Duration,
    domain::domain_participant::DomainParticipant,
    infrastructure::{condition::Condition, status::StatusMask, wait_set::WaitSet},
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
use int2dds::serialize::cdr::{CdrSerialize, CdrSerializer};
use int2dds::serialize::BufferManager;

/// Serialize a value with the default CDR encoding and return the wire bytes including header.
pub fn encode_cdr<T: CdrSerialize>(value: &T) -> Vec<u8> {
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();
    serializer.into_bytes()
}

/// Compare `actual` bytes against a space/whitespace-separated hex string.
pub fn parse_hex(spec: &str) -> Vec<u8> {
    spec.split_ascii_whitespace()
        .map(|tok| u8::from_str_radix(tok.trim_start_matches("0x"), 16).expect("invalid hex"))
        .collect()
}

/// Assert that a `CdrSerialize` value produces the given hex wire layout (header + payload).
#[macro_export]
macro_rules! assert_wire_bytes {
    ($value:expr, $expected_hex:expr $(,)?) => {{
        let actual = $crate::common::encode_cdr(&$value);
        let expected = $crate::common::parse_hex($expected_hex);
        assert_eq!(actual, expected, "wire bytes mismatch");
    }};
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct NoKeyDataType {
    pub value: i16,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
pub struct KeyedDataType {
    #[dds(key)]
    pub key: i16,
    pub value: i16,
}

impl NoKeyDataType {
    pub fn new(value: i16) -> Self {
        Self { value }
    }

    pub fn default() -> Self {
        Self { value: 0 }
    }

    pub fn get_topic_name() -> &'static str {
        "test_topic_no_key"
    }

    pub fn get_type_name() -> &'static str {
        "NoKeyDataType"
    }
}

impl KeyedDataType {
    pub fn new(key: i16, value: i16) -> Self {
        Self { key, value }
    }

    pub fn default() -> Self {
        Self { key: 0, value: 0 }
    }

    pub fn get_topic_name() -> &'static str {
        "test_topic"
    }

    pub fn get_type_name() -> &'static str {
        "KeyedDataType"
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
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
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
            KeyedDataType::get_topic_name(),
            KeyedDataType::get_type_name(),
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

pub fn create_nokey_datareader(
    domain_participant: &DomainParticipant,
    subscriber_qos: SubscriberQos,
    data_reader_qos: DataReaderQos,
) -> DataReader<NoKeyDataType> {
    let topic = domain_participant
        .create_topic::<NoKeyDataType>(
            NoKeyDataType::get_topic_name(),
            NoKeyDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let subscriber =
        domain_participant.create_subscriber(subscriber_qos, None, StatusMask::default()).unwrap();

    let reader = subscriber
        .create_datareader::<NoKeyDataType>(&topic, data_reader_qos, None, StatusMask::default())
        .unwrap();

    reader
}

pub fn create_nokey_datawriter(
    domain_participant: &DomainParticipant,
    publisher_qos: PublisherQos,
    datawriter_qos: DataWriterQos,
) -> DataWriter<NoKeyDataType> {
    let topic = domain_participant
        .create_topic::<NoKeyDataType>(
            NoKeyDataType::get_topic_name(),
            NoKeyDataType::get_type_name(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let publisher =
        domain_participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();

    let writer = publisher
        .create_datawriter::<NoKeyDataType>(&topic, datawriter_qos, None, StatusMask::default())
        .unwrap();

    writer
}

pub fn wait_for_writer_status<Foo: DdsType>(
    data_writer: &DataWriter<Foo>,
    status_mask: StatusMask,
    duration: Duration,
) -> Result<Vec<std::sync::Arc<dyn Condition + Send + Sync>>, int2dds::dcps::core::error::DdsError>
{
    let mut condition = data_writer.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(status_mask).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(duration)
}

pub fn wait_for_reader_status<Foo: DdsType>(
    data_reader: &DataReader<Foo>,
    status_mask: StatusMask,
    duration: Duration,
) -> Result<Vec<std::sync::Arc<dyn Condition + Send + Sync>>, int2dds::dcps::core::error::DdsError>
{
    let mut condition = data_reader.get_statuscondition().unwrap().clone();
    condition.set_enabled_statuses(status_mask).unwrap();
    let wait_set = WaitSet::new();
    wait_set.attach_condition(condition).unwrap();
    wait_set.wait(duration)
}
