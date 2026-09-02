mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use int2dds::common::instance_handle::InstanceHandle;
use int2dds::config::xml::XmlTypeRegistry;
use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::{data_writer::DataWriter, publisher::Publisher};
use int2dds::dcps::subscription::{data_reader::DataReader, subscriber::Subscriber};
use int2dds::dcps::topic::topic::Topic;
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
use int2dds::{
    topic::qos::TopicQos,
    xtypes::{DynamicData, DynamicTypeKind, DynamicValue},
};

fn load(xml: &str) -> XmlTypeRegistry {
    let mut registry = XmlTypeRegistry::new();
    registry.load_str(xml).unwrap();
    registry
}

struct LoopbackGuard {
    _participant: DomainParticipant,
    _topic: Topic,
    _publisher: Publisher,
    _subscriber: Subscriber,
}

// Matched dynamic writer/reader pair on a fresh domain; the guard keeps entities alive.
fn dynamic_loopback(
    topic_name: &str,
    support: &Arc<int2dds::xtypes::DynamicTypeSupport>,
) -> (DataWriter<DynamicData>, DataReader<DynamicData>, LoopbackGuard) {
    let factory = DomainParticipantFactory::get_instance();
    let participant = factory
        .create_participant(
            next_domain_id(),
            DomainParticipantQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let topic = participant
        .create_topic_dynamic(
            topic_name,
            support.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        participant.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let writer = publisher
        .create_datawriter_dynamic(
            &topic,
            support.clone(),
            DataWriterQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let subscriber = participant
        .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
        .unwrap();
    let reader = subscriber
        .create_datareader_dynamic(
            &topic,
            support.clone(),
            DataReaderQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    wait_for_reader_status(&reader, StatusMask::SUBSCRIPTION_MATCHED, Duration::from_seconds(5))
        .unwrap();
    wait_for_writer_status(&writer, StatusMask::PUBLICATION_MATCHED, Duration::from_seconds(5))
        .unwrap();
    (
        writer,
        reader,
        LoopbackGuard {
            _participant: participant,
            _topic: topic,
            _publisher: publisher,
            _subscriber: subscriber,
        },
    )
}

// Waits for and takes exactly one sample, returning its deserialized data.
fn take_one(reader: &DataReader<DynamicData>) -> DynamicData {
    wait_for_reader_status(reader, StatusMask::DATA_AVAILABLE, Duration::from_seconds(5)).unwrap();
    let samples = reader
        .take(
            10,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        )
        .unwrap();
    assert_eq!(samples.len(), 1);
    samples[0].data().unwrap()
}

const NESTED_XML: &str = r#"<types>
      <enum name="Color">
        <enumerator name="Red"/>
        <enumerator name="Green" value="5"/>
        <enumerator name="Blue" default_literal="true"/>
      </enum>
      <struct name="Point" nested="true">
        <member name="x" type="int32"/>
        <member name="y" type="int32"/>
      </struct>
      <struct name="Holder">
        <member name="color" type="nonBasic" nonBasicTypeName="Color"/>
        <member name="origin" type="nonBasic" nonBasicTypeName="Point"/>
        <member name="path" type="nonBasic" nonBasicTypeName="Point" sequenceMaxLength="-1"/>
      </struct>
    </types>"#;

#[test]
fn xml_inheritance_pubsub_loopback() {
    let registry = load(
        r#"<types>
             <struct name="Animal"><member name="legs" type="int32" key="true"/></struct>
             <struct name="Dog" baseType="Animal"><member name="name" type="string"/></struct>
           </types>"#,
    );
    let support = Arc::new(registry.get("Dog").unwrap());

    let (writer, reader, guard) = dynamic_loopback("DogTopic", &support);

    let mut data = support.create_data();
    // 'legs' is an inherited (flattened) parent member; 'name' is the child member.
    data.set("legs", 4i32).unwrap();
    data.set("name", "rex").unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    assert_eq!(received.get::<i32>("legs").unwrap(), 4);
    assert_eq!(received.get::<String>("name").unwrap(), "rex");

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}

#[test]
fn xml_bitmask_bitset_pubsub_loopback() {
    let registry = load(
        r#"<types>
             <bitmask name="Permissions" bit_bound="8">
               <bit_value name="Read"/>
               <bit_value name="Write"/>
               <bit_value name="Admin" position="5"/>
             </bitmask>
             <bitset name="PackedHeader">
               <bitfield name="version" bit_bound="4" type="byte"/>
               <bitfield name="length" bit_bound="12" type="uint16"/>
             </bitset>
             <struct name="Record">
               <member name="id" type="int32" key="true"/>
               <member name="perms" type="nonBasic" nonBasicTypeName="Permissions"/>
               <member name="hdr" type="nonBasic" nonBasicTypeName="PackedHeader"/>
             </struct>
           </types>"#,
    );
    let support = Arc::new(registry.get("Record").unwrap());

    let (writer, reader, guard) = dynamic_loopback("RecordTopic", &support);

    let mut data = support.create_data();
    data.set("id", 7i32).unwrap();
    // Read(bit0) | Admin(bit5) = 0b100001 = 33
    data.set_value("perms", DynamicValue::Bitmask(0b10_0001)).unwrap();
    // version=3 (bits 0..4), length=10 (bits 4..16) => (10 << 4) | 3 = 163
    data.set_value("hdr", DynamicValue::Bitset((10u64 << 4) | 3)).unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    assert_eq!(received.get::<i32>("id").unwrap(), 7);
    match received.get_value("perms").unwrap() {
        DynamicValue::Bitmask(bits) => assert_eq!(*bits, 0b10_0001),
        other => panic!("expected bitmask, got {other:?}"),
    }
    match received.get_value("hdr").unwrap() {
        DynamicValue::Bitset(bits) => assert_eq!(*bits, (10u64 << 4) | 3),
        other => panic!("expected bitset, got {other:?}"),
    }

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}

#[test]
fn xml_union_pubsub_loopback() {
    let registry = load(
        r#"<types>
             <struct name="Point" nested="true">
               <member name="x" type="int32"/>
               <member name="y" type="int32"/>
             </struct>
             <union name="Shape">
               <discriminator type="int32"/>
               <case><caseDiscriminator value="0"/><member name="Circle" type="float64"/></case>
               <case><caseDiscriminator value="1"/><member name="Rect" type="nonBasic" nonBasicTypeName="Point"/></case>
               <case><caseDiscriminator value="2"/><member name="Count" type="uint32"/></case>
             </union>
             <struct name="Event">
               <member name="seq" type="int32" key="true"/>
               <member name="shape" type="nonBasic" nonBasicTypeName="Shape"/>
             </struct>
           </types>"#,
    );
    let support = Arc::new(registry.get("Event").unwrap());

    let (writer, reader, guard) = dynamic_loopback("EventTopic", &support);

    let mut data = support.create_data();
    data.set("seq", 1i32).unwrap();
    data.set_value(
        "shape",
        DynamicValue::Union {
            discriminator: Box::new(DynamicValue::Int32(2)),
            value: Box::new(DynamicValue::Uint32(99)),
        },
    )
    .unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    assert_eq!(received.get::<i32>("seq").unwrap(), 1);
    match received.get_value("shape").unwrap() {
        DynamicValue::Union { discriminator, value } => {
            assert!(matches!(**discriminator, DynamicValue::Int32(2)));
            assert!(matches!(**value, DynamicValue::Uint32(99)));
        }
        other => panic!("expected union, got {other:?}"),
    }

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}

#[test]
fn xml_nested_type_via_type_lookup_cross_participant() {
    // SEDP advertises TypeInformation only; the consumer pulls the whole nested closure via TypeLookup.
    run_cross_participant_nested();
}

fn run_cross_participant_nested() {
    // Consumer has no type definition; it obtains the nested 'origin' (Point) over the wire.
    let domain_id = next_domain_id();
    let factory = DomainParticipantFactory::get_instance();

    let registry = load(NESTED_XML);
    let support = Arc::new(registry.get("Holder").unwrap());

    let producer = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();
    let topic = producer
        .create_topic_dynamic(
            "XmlHolderTopic",
            support.clone(),
            TopicQos::default(),
            None,
            StatusMask::default(),
        )
        .unwrap();
    let publisher =
        producer.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
    let _writer = publisher
        .create_datawriter_dynamic(
            &topic,
            support.clone(),
            DataWriterQos {
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration { sec: 1, nanosec: 0 },
                },
                ..Default::default()
            },
            None,
            StatusMask::default(),
        )
        .unwrap();

    let consumer = factory
        .create_participant(domain_id, DomainParticipantQos::default(), None, StatusMask::default())
        .unwrap();

    let mut consumer_support = None;
    for _ in 0..100 {
        if let Ok(s) = consumer.create_dynamic_type_support_from_discovered_type("XmlHolderTopic") {
            consumer_support = Some(s);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let consumer_support = consumer_support
        .expect("consumer must fetch the Holder closure via TypeLookup and build type support");

    // 'origin' resolves to a TypeRef only if Point's TypeObject crossed the wire.
    let dynamic_type = consumer_support.dynamic_type();
    let struct_desc = dynamic_type.as_struct().expect("Holder must be a struct");
    let origin = struct_desc
        .members()
        .iter()
        .find(|m| &*m.name == "origin")
        .expect("'origin' member present");
    assert!(
        matches!(origin.member_type, DynamicTypeKind::TypeRef(_)),
        "nested 'origin' member must resolve to a TypeRef over the wire, got {:?}",
        origin.member_type
    );

    producer.delete_contained_entities().unwrap();
    factory.delete_participant(producer).unwrap();
    consumer.delete_contained_entities().unwrap();
    factory.delete_participant(consumer).unwrap();
}

#[test]
fn xml_dynamic_pubsub_loopback() {
    let registry = load(
        r#"<dds><types>
             <struct name="SensorData" extensibility="final">
               <member name="sensor_id" type="int32" key="true"/>
               <member name="temperature" type="float64"/>
               <member name="active" type="boolean"/>
             </struct>
           </types></dds>"#,
    );
    let support = Arc::new(registry.get("SensorData").unwrap());

    let (writer, reader, guard) = dynamic_loopback("SensorTopic", &support);

    let mut data = support.create_data();
    data.set("sensor_id", 7i32).unwrap();
    data.set("temperature", 21.5f64).unwrap();
    data.set("active", true).unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    let sensor_id: i32 = received.get("sensor_id").unwrap();
    let temperature: f64 = received.get("temperature").unwrap();
    let active: bool = received.get("active").unwrap();
    assert_eq!(sensor_id, 7);
    assert_eq!(temperature, 21.5);
    assert!(active);

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}

#[test]
fn xml_collections_pubsub_loopback() {
    let registry = load(
        r#"<types><struct name="LogRecord">
             <member name="name" type="string" key="true"/>
             <member name="note" type="wstring" stringMaxLength="32"/>
             <member name="values" type="int32" sequenceMaxLength="-1"/>
             <member name="tags" type="string" sequenceMaxLength="8"/>
             <member name="samples" type="float64" arrayDimensions="3"/>
           </struct></types>"#,
    );
    let support = Arc::new(registry.get("LogRecord").unwrap());

    let (writer, reader, guard) = dynamic_loopback("LogTopic", &support);

    let mut data = support.create_data();
    data.set("name", "logger-1").unwrap();
    data.set("note", DynamicValue::WString("주의".to_string())).unwrap();
    data.set("values", vec![10i32, -20, 30]).unwrap();
    data.set("tags", vec!["a".to_string(), "bb".to_string()]).unwrap();
    data.set(
        "samples",
        DynamicValue::Array(vec![
            DynamicValue::Float64(1.5),
            DynamicValue::Float64(-2.5),
            DynamicValue::Float64(0.0),
        ]),
    )
    .unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    let name: String = received.get("name").unwrap();
    let note: String = received.get("note").unwrap();
    let values: Vec<i32> = received.get("values").unwrap();
    let tags: Vec<String> = received.get("tags").unwrap();
    let array_samples: Vec<f64> = received.get("samples").unwrap();
    assert_eq!(name, "logger-1");
    assert_eq!(note, "주의");
    assert_eq!(values, [10, -20, 30]);
    assert_eq!(tags, ["a", "bb"]);
    assert_eq!(array_samples, [1.5, -2.5, 0.0]);

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}

#[test]
fn xml_map_pubsub_loopback() {
    let registry = load(
        r#"<types><struct name="MapRecord">
             <member name="id" type="int32" key="true"/>
             <member name="lookup" type="string" key_type="int32" mapMaxLength="16"/>
           </struct></types>"#,
    );
    let support = Arc::new(registry.get("MapRecord").unwrap());

    let (writer, reader, guard) = dynamic_loopback("MapTopic", &support);

    let mut lookup = HashMap::new();
    lookup.insert(1i32, "one".to_string());
    lookup.insert(2i32, "two".to_string());
    let mut data = support.create_data();
    data.set("id", 9i32).unwrap();
    data.set("lookup", lookup).unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    assert_eq!(received.get::<i32>("id").unwrap(), 9);
    match received.get_value("lookup").unwrap() {
        DynamicValue::Map(entries) => {
            let mut out: Vec<(i32, String)> = entries
                .iter()
                .map(|(k, v)| match (k, v) {
                    (DynamicValue::Int32(k), DynamicValue::String(s)) => (*k, s.clone()),
                    other => panic!("unexpected entry {other:?}"),
                })
                .collect();
            out.sort();
            assert_eq!(out, [(1, "one".to_string()), (2, "two".to_string())]);
        }
        other => panic!("expected map, got {other:?}"),
    }

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}

#[test]
fn xml_nested_enum_pubsub_loopback() {
    let registry = load(NESTED_XML);
    let support = Arc::new(registry.get("Holder").unwrap());
    let point_support = registry.get("Point").unwrap();

    let (writer, reader, guard) = dynamic_loopback("HolderTopic", &support);

    let mut origin = point_support.create_data();
    origin.set("x", 3i32).unwrap();
    origin.set("y", -4i32).unwrap();
    let mut p1 = point_support.create_data();
    p1.set("x", 10i32).unwrap();
    p1.set("y", 20i32).unwrap();

    let mut data = support.create_data();
    data.set_value("color", DynamicValue::Enum { name: String::new(), value: 5 }).unwrap();
    data.set_value("origin", DynamicValue::Struct(Box::new(origin))).unwrap();
    data.set_value("path", DynamicValue::Sequence(vec![DynamicValue::Struct(Box::new(p1))]))
        .unwrap();
    writer.write(&data, InstanceHandle::NIL).unwrap();

    let received = take_one(&reader);
    match received.get_value("color").unwrap() {
        DynamicValue::Enum { name, value } => {
            assert_eq!(*value, 5);
            assert_eq!(name, "Green");
        }
        other => panic!("expected enum, got {other:?}"),
    }
    let x: i32 = received.get_nested("origin.x").unwrap();
    let y: i32 = received.get_nested("origin.y").unwrap();
    assert_eq!((x, y), (3, -4));
    match received.get_value("path").unwrap() {
        DynamicValue::Sequence(items) => {
            assert_eq!(items.len(), 1);
            match &items[0] {
                DynamicValue::Struct(p) => {
                    assert_eq!(p.get::<i32>("x").unwrap(), 10);
                    assert_eq!(p.get::<i32>("y").unwrap(), 20);
                }
                other => panic!("expected struct, got {other:?}"),
            }
        }
        other => panic!("expected sequence, got {other:?}"),
    }

    guard._participant.delete_contained_entities().unwrap();
    DomainParticipantFactory::get_instance().delete_participant(guard._participant).unwrap();
}
