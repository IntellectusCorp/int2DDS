//! `float128` end to end: a derived `Float128` struct and the equivalent XML DynamicType must match
//! and carry every bit of the binary128 value in both directions.

mod common;

use std::sync::Arc;

use common::*;
use int2dds::common::instance_handle::InstanceHandle;
use int2dds::config::xml::XmlTypeRegistry;
use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::{
    core::time::Duration,
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::{
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        status::StatusMask,
    },
    publication::qos::{DataWriterQos, PublisherQos},
    subscription::{
        qos::{DataReaderQos, SubscriberQos},
        sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    },
};
use int2dds::serialize::{DeserializerReader, Float128};
use int2dds::{
    topic::{qos::TopicQos, type_support::DdsType},
    xtypes::{DynamicData, DynamicTypeSupport, DynamicValue},
};

const TOPIC: &str = "Float128Topic";

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct F128Sample {
    #[dds(key)]
    id: Float128,
    value: Float128,
    values: Vec<Float128>,
    pair: [Float128; 2],
}

const F128_XML: &str = r#"<types>
      <struct name="F128Sample" extensibility="mutable">
        <member name="id" type="float128" key="true"/>
        <member name="value" type="float128"/>
        <member name="values" type="float128" sequenceMaxLength="-1"/>
        <member name="pair" type="float128" arrayDimensions="2"/>
      </struct>
    </types>"#;

fn f128(text: &str) -> Float128 {
    text.parse().unwrap()
}

// Values whose binary128 form needs more than f64's 53 significand bits.
fn sample() -> F128Sample {
    F128Sample {
        id: f128("7.1"),
        value: f128("12.300000"),
        values: vec![
            f128("0.1"),
            f128("-1e-4000"),
            f128("1.18973149535723176508575932662800702e4932"),
        ],
        pair: [f128("3.141592653589793238462643383279502884"), f128("-0")],
    }
}

fn dynamic_support() -> Arc<DynamicTypeSupport> {
    let mut registry = XmlTypeRegistry::new();
    registry.load_str(F128_XML).unwrap();
    Arc::new(registry.get("F128Sample").unwrap())
}

fn reliable_writer_qos() -> DataWriterQos {
    DataWriterQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration { sec: 1, nanosec: 0 },
        },
        ..Default::default()
    }
}

fn participant(domain_id: i32) -> DomainParticipant {
    DomainParticipantFactory::get_instance()
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap()
}

fn cleanup(participants: Vec<DomainParticipant>) {
    for p in participants {
        p.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(p).unwrap();
    }
}

fn take_all<T: DdsType>(
    reader: &int2dds::dcps::subscription::data_reader::DataReader<T>,
) -> Vec<T> {
    wait_for_reader_status(reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(5)).unwrap();
    reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap()
        .iter()
        .map(|s| s.data().unwrap())
        .collect()
}

fn values_of(value: &DynamicValue) -> Vec<Float128> {
    match value {
        DynamicValue::Sequence(items) | DynamicValue::Array(items) => items
            .iter()
            .map(|v| match v {
                DynamicValue::Float128(f) => *f,
                other => panic!("expected Float128, got {other:?}"),
            })
            .collect(),
        other => panic!("expected a collection, got {other:?}"),
    }
}

#[test]
fn float128_derive_writer_to_dynamic_reader() {
    let domain_id = next_domain_id();
    let pub_side = participant(domain_id);
    let sub_side = participant(domain_id);
    let support = dynamic_support();

    let topic = pub_side
        .create_topic::<F128Sample>(
            TOPIC,
            "F128Sample",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let writer = pub_side
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datawriter::<F128Sample>(&topic, reliable_writer_qos(), None, StatusMask::default())
        .unwrap();

    let dyn_topic = sub_side
        .create_topic_dynamic(
            TOPIC,
            support.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader = sub_side
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader_dynamic(
            &dyn_topic,
            support.clone(),
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(10))
        .unwrap();
    let sent = sample();
    writer.write(&sent, InstanceHandle::NIL).unwrap();

    let received: Vec<DynamicData> = take_all(&reader);
    assert_eq!(received.len(), 1);
    let data = &received[0];
    assert_eq!(data.get::<Float128>("id").unwrap(), sent.id);
    assert_eq!(data.get::<Float128>("value").unwrap(), sent.value);
    assert_eq!(values_of(data.get_value("values").unwrap()), sent.values);
    assert_eq!(values_of(data.get_value("pair").unwrap()), sent.pair);

    cleanup(vec![pub_side, sub_side]);
}

#[test]
fn float128_dynamic_writer_to_derive_reader() {
    let domain_id = next_domain_id();
    let pub_side = participant(domain_id);
    let sub_side = participant(domain_id);
    let support = dynamic_support();

    let dyn_topic = pub_side
        .create_topic_dynamic(
            TOPIC,
            support.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let writer = pub_side
        .create_publisher(PublisherQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datawriter_dynamic(
            &dyn_topic,
            support.clone(),
            reliable_writer_qos(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    let topic = sub_side
        .create_topic::<F128Sample>(
            TOPIC,
            "F128Sample",
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let reader = sub_side
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap()
        .create_datareader::<F128Sample>(
            &topic,
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();

    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(10))
        .unwrap();
    let expected = sample();
    let mut data = support.create_data();
    data.set("id", expected.id).unwrap();
    data.set("value", expected.value).unwrap();
    data.set("values", expected.values.clone()).unwrap();
    data.set(
        "pair",
        DynamicValue::Array(expected.pair.iter().map(|v| DynamicValue::Float128(*v)).collect()),
    )
    .unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_all(&reader);
    assert_eq!(received, vec![expected]);

    cleanup(vec![pub_side, sub_side]);
}
