// Bitmask bit names (Read/Write/Admin) must byte-match the XML <bit_value> labels,
// so they intentionally keep their non-UPPER_CASE spelling in the derived constants.
#![allow(non_upper_case_globals)]

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use common::*;
use int2dds::dcps::domain::domain_participant::DomainParticipant;
use int2dds::dcps::publication::{data_writer::DataWriter, publisher::Publisher};
use int2dds::dcps::subscription::{data_reader::DataReader, subscriber::Subscriber};
use int2dds::dcps::topic::topic::Topic;
use int2dds::{
    common::instance_handle::InstanceHandle,
    config::xml::XmlTypeRegistry,
    dcps::{
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
        topic::type_support::DdsType,
    },
    serialize::{cdr::PrimitiveSerialize, DeserializerReader, WString},
    topic::qos::TopicQos,
    xtypes::{
        CompleteStructMember, CompleteStructType, CompleteTypeObject, DynamicData, DynamicTypeKind,
        DynamicTypeSupport, DynamicValue, ExtensibilityKind, HasTypeObject, MemberFlag,
        TryConstructKind, TypeFlag, TypeIdentifier, TypeObject,
    },
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
    support: &Arc<DynamicTypeSupport>,
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

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct AllPrimitives {
    f_bool: bool,
    f_byte: u8,
    f_char: char,
    f_i8: i8,
    f_i16: i16,
    f_i32: i32,
    f_i64: i64,
    f_u16: u16,
    f_u32: u32,
    f_u64: u64,
    f_f32: f32,
    f_f64: f64,
}

#[test]
fn byte_identical_primitives_vs_derive() {
    let registry = load(
        r#"<types><struct name="AllPrimitives">
             <member name="f_bool" type="boolean"/>
             <member name="f_byte" type="byte"/>
             <member name="f_char" type="char8"/>
             <member name="f_i8" type="int8"/>
             <member name="f_i16" type="int16"/>
             <member name="f_i32" type="int32"/>
             <member name="f_i64" type="int64"/>
             <member name="f_u16" type="uint16"/>
             <member name="f_u32" type="uint32"/>
             <member name="f_u64" type="uint64"/>
             <member name="f_f32" type="float32"/>
             <member name="f_f64" type="float64"/>
           </struct></types>"#,
    );
    let from_xml = registry.get_type_object("AllPrimitives").unwrap();
    let from_derive = AllPrimitives::complete_type_object();
    assert_eq!(from_xml.serialize(), from_derive.serialize());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable", autoid = "Hash")]
struct AnnotatedType {
    #[dds(id = 10, key)]
    sensor_id: u32,
    #[dds(hashid = "crc")]
    checksum: u32,
    #[dds(must_understand)]
    flags: u16,
}

#[test]
fn byte_identical_annotations_vs_derive() {
    let registry = load(
        r#"<types><struct name="AnnotatedType" extensibility="mutable" autoid="hash">
             <member name="sensor_id" type="uint32" id="10" key="true"/>
             <member name="checksum" type="uint32" hashid="crc"/>
             <member name="flags" type="uint16" mustUnderstand="true"/>
           </struct></types>"#,
    );
    let from_xml = registry.get_type_object("AnnotatedType").unwrap();
    let from_derive = AnnotatedType::complete_type_object();
    assert_eq!(from_xml.serialize(), from_derive.serialize());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", type_name = "sensors::Thing")]
struct ScopedThing {
    value: i32,
}

#[test]
fn byte_identical_type_name_override_vs_xml() {
    // The TypeObject's QualifiedTypeName must reflect the type_name override
    // (the ROS2 mangling path relies on this), byte-identical to the XML module path.
    let registry = load(
        r#"<types><module name="sensors">
             <struct name="Thing"><member name="value" type="int32"/></struct>
           </module></types>"#,
    );
    let from_xml = registry.get_type_object("sensors::Thing").unwrap();
    let from_derive = ScopedThing::complete_type_object();
    assert_eq!(from_xml.serialize(), from_derive.serialize());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct HolderOfScoped {
    thing: ScopedThing,
}

#[test]
fn nested_member_ref_matches_registered_id() {
    // A type_name override must NOT desync the name-based nested id scheme: a referencing
    // struct's member id (derived from the Rust type ident) must equal the id under which
    // the referenced type registers itself in the nested set.
    let mut scoped_set = Vec::new();
    ScopedThing::collect_nested_type_objects(&mut scoped_set);
    let scoped_reg_id = scoped_set[0].0.clone();

    let member_id = match HolderOfScoped::complete_type_object() {
        CompleteTypeObject::Struct(s) => s.member_seq[0].common.member_type_id.clone(),
        other => panic!("expected struct, got {other:?}"),
    };
    assert_eq!(
        member_id, scoped_reg_id,
        "nested member ref must resolve to the referenced type's registered nested id"
    );
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", nested)]
struct NestedAnnotated {
    x: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", data_representation(XCDR2))]
struct Xcdr2Only {
    x: i32,
}

#[test]
fn byte_identical_nested_vs_derive() {
    let registry = load(
        r#"<types><struct name="NestedAnnotated" nested="true">
             <member name="x" type="int32"/>
           </struct></types>"#,
    );
    let from_xml = registry.get_type_object("NestedAnnotated").unwrap();
    let from_derive = NestedAnnotated::complete_type_object();
    assert_eq!(from_xml.serialize(), from_derive.serialize());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct StringsAndCollections {
    name: String,
    note: WString,
    values: Vec<i32>,
    tags: Vec<String>,
    samples: [f64; 4],
    raw: [u8; 3],
}

#[test]
fn byte_identical_strings_collections_vs_derive() {
    let registry = load(
        r#"<types><struct name="StringsAndCollections">
             <member name="name" type="string"/>
             <member name="note" type="wstring"/>
             <member name="values" type="int32" sequenceMaxLength="-1"/>
             <member name="tags" type="string" sequenceMaxLength="-1"/>
             <member name="samples" type="float64" arrayDimensions="4"/>
             <member name="raw" type="byte" arrayDimensions="3"/>
           </struct></types>"#,
    );
    let from_xml = registry.get_type_object("StringsAndCollections").unwrap();
    let from_derive = StringsAndCollections::complete_type_object();
    assert_eq!(from_xml.serialize(), from_derive.serialize());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
enum Color {
    Red,
    Green = 5,
    #[dds(default_literal)]
    Blue,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", nested)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct Holder {
    color: Color,
    origin: Point,
    path: Vec<Point>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
#[repr(i32)]
enum Shape {
    Circle(f64),
    Rect(Point),
    Count(u32),
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", bitmask, bit_bound = 8)]
enum Permissions {
    #[dds(position = 0)]
    Read,
    #[dds(position = 1)]
    Write,
    #[dds(position = 5)]
    Admin,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", bitset)]
struct PackedHeader {
    #[dds(bitfield = 4)]
    version: u8,
    #[dds(bitfield = 12)]
    length: u16,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct BaseMsg {
    id: i32,
    label: u32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
struct DerivedMsg {
    #[dds(parent)]
    base: BaseMsg,
    value: f64,
}

#[test]
fn byte_identical_inheritance_vs_derive() {
    // Derive embeds the parent as a named member alongside base_type; the XML mirrors that.
    let registry = load(
        r#"<types>
             <struct name="BaseMsg">
               <member name="id" type="int32"/>
               <member name="label" type="uint32"/>
             </struct>
             <struct name="DerivedMsg" baseType="BaseMsg">
               <member name="base" type="nonBasic" nonBasicTypeName="BaseMsg"/>
               <member name="value" type="float64"/>
             </struct>
           </types>"#,
    );
    assert_eq!(
        registry.get_type_object("DerivedMsg").unwrap().serialize(),
        DerivedMsg::complete_type_object().serialize()
    );
}

#[test]
fn standard_inheritance_flattens_parent_members() {
    // baseType alone = standard XTypes inheritance: parent members flattened ahead of child's.
    let registry = load(
        r#"<types>
             <struct name="Animal"><member name="legs" type="int32"/></struct>
             <struct name="Mammal" baseType="Animal"><member name="fur" type="boolean"/></struct>
             <struct name="Dog" baseType="Mammal"><member name="name" type="string"/></struct>
           </types>"#,
    );
    let support = registry.get("Dog").unwrap();
    let data = support.create_data();
    let names: Vec<String> =
        data.dynamic_type().members().unwrap().iter().map(|m| m.name.to_string()).collect();
    assert_eq!(names, ["legs", "fur", "name"]);
}

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
fn unknown_base_type_rejected() {
    let mut registry = XmlTypeRegistry::new();
    let err = registry
        .load_str(r#"<types><struct name="T" baseType="Ghost"><member name="x" type="int32"/></struct></types>"#)
        .unwrap_err();
    assert!(format!("{err:?}").contains("unknown base type 'Ghost'"));
}

#[test]
fn inheritance_cycle_rejected() {
    let mut registry = XmlTypeRegistry::new();
    let err = registry
        .load_str(
            r#"<types>
                 <struct name="A" baseType="B"><member name="a" type="int32"/></struct>
                 <struct name="B" baseType="A"><member name="b" type="int32"/></struct>
               </types>"#,
        )
        .unwrap_err();
    assert!(format!("{err:?}").contains("inheritance cycle"));
}

#[test]
fn byte_identical_bitmask_vs_derive() {
    let registry = load(
        r#"<types><bitmask name="Permissions" bit_bound="8">
             <bit_value name="Read"/>
             <bit_value name="Write"/>
             <bit_value name="Admin" position="5"/>
           </bitmask></types>"#,
    );
    assert_eq!(
        registry.get_type_object("Permissions").unwrap().serialize(),
        Permissions::complete_type_object().serialize()
    );
}

#[test]
fn byte_identical_bitset_vs_derive() {
    let registry = load(
        r#"<types><bitset name="PackedHeader">
             <bitfield name="version" bit_bound="4" type="byte"/>
             <bitfield name="length" bit_bound="12" type="uint16"/>
           </bitset></types>"#,
    );
    assert_eq!(
        registry.get_type_object("PackedHeader").unwrap().serialize(),
        PackedHeader::complete_type_object().serialize()
    );
}

const UNION_XML: &str = r#"<types>
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
    </types>"#;

#[test]
fn byte_identical_union_vs_derive() {
    let registry = load(UNION_XML);
    assert_eq!(
        registry.get_type_object("Shape").unwrap().serialize(),
        Shape::complete_type_object().serialize()
    );
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
fn union_enum_discriminator_resolves_labels() {
    // Enum-typed discriminator with case labels written as enum literal names.
    let registry = load(
        r#"<types>
             <enum name="Mode">
               <enumerator name="IDLE"/>
               <enumerator name="ACTIVE" value="5"/>
             </enum>
             <union name="Command">
               <discriminator type="nonBasic" nonBasicTypeName="Mode"/>
               <case><caseDiscriminator value="IDLE"/><member name="sleep_ms" type="uint32"/></case>
               <case><caseDiscriminator value="ACTIVE"/><member name="speed" type="float64"/></case>
             </union>
           </types>"#,
    );
    let obj = registry.get_type_object("Command").unwrap();
    let CompleteTypeObject::Union(u) = obj else { panic!("expected union") };
    assert_eq!(u.member_seq[0].common.label_seq, vec![0]);
    assert_eq!(u.member_seq[1].common.label_seq, vec![5]);
    registry.get("Command").unwrap();
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
fn byte_identical_enum_and_nested_vs_derive() {
    let registry = load(NESTED_XML);
    assert_eq!(
        registry.get_type_object("Color").unwrap().serialize(),
        Color::complete_type_object().serialize()
    );
    assert_eq!(
        registry.get_type_object("Holder").unwrap().serialize(),
        Holder::complete_type_object().serialize()
    );
}

#[test]
fn byte_identical_idl_alias_names_vs_derive() {
    let registry = load(
        r#"<types><struct name="AllPrimitives">
             <member name="f_bool" type="boolean"/>
             <member name="f_byte" type="octet"/>
             <member name="f_char" type="char"/>
             <member name="f_i8" type="int8"/>
             <member name="f_i16" type="short"/>
             <member name="f_i32" type="long"/>
             <member name="f_i64" type="longLong"/>
             <member name="f_u16" type="unsignedShort"/>
             <member name="f_u32" type="unsignedLong"/>
             <member name="f_u64" type="unsignedLongLong"/>
             <member name="f_f32" type="float"/>
             <member name="f_f64" type="double"/>
           </struct></types>"#,
    );
    assert_eq!(
        registry.get_type_object("AllPrimitives").unwrap().serialize(),
        AllPrimitives::complete_type_object().serialize()
    );
}

#[test]
fn byte_identical_data_representation_vs_derive() {
    let registry = load(
        r#"<types><struct name="Xcdr2Only" data_representation="xcdr2">
             <member name="x" type="int32"/>
           </struct></types>"#,
    );
    assert_eq!(
        registry.get_type_object("Xcdr2Only").unwrap().serialize(),
        Xcdr2Only::complete_type_object().serialize()
    );
}

#[test]
fn typedef_flattens_to_derive_equivalent() {
    let registry = load(
        r#"<types>
             <typedef name="Ids" type="int32" sequenceMaxLength="-1"/>
             <typedef name="Tags" type="string" sequenceMaxLength="-1"/>
             <typedef name="Samples" type="float64" arrayDimensions="4"/>
             <typedef name="Raw" type="byte" arrayDimensions="3"/>
             <struct name="StringsAndCollections">
               <member name="name" type="string"/>
               <member name="note" type="wstring"/>
               <member name="values" type="nonBasic" nonBasicTypeName="Ids"/>
               <member name="tags" type="nonBasic" nonBasicTypeName="Tags"/>
               <member name="samples" type="nonBasic" nonBasicTypeName="Samples"/>
               <member name="raw" type="nonBasic" nonBasicTypeName="Raw"/>
             </struct>
           </types>"#,
    );
    assert_eq!(
        registry.get_type_object("StringsAndCollections").unwrap().serialize(),
        StringsAndCollections::complete_type_object().serialize()
    );
    assert!(registry.get_type_object("Ids").is_none());
    assert_eq!(registry.type_names(), ["StringsAndCollections"]);
}

#[test]
fn typedef_chain_through_struct_alias_matches_derive() {
    let registry = load(
        r#"<types>
             <enum name="Color">
               <enumerator name="Red"/>
               <enumerator name="Green" value="5"/>
               <enumerator name="Blue" default_literal="true"/>
             </enum>
             <struct name="Point" nested="true">
               <member name="x" type="int32"/>
               <member name="y" type="int32"/>
             </struct>
             <typedef name="Position" type="nonBasic" nonBasicTypeName="Point"/>
             <typedef name="Path" type="nonBasic" nonBasicTypeName="Position" sequenceMaxLength="-1"/>
             <struct name="Holder">
               <member name="color" type="nonBasic" nonBasicTypeName="Color"/>
               <member name="origin" type="nonBasic" nonBasicTypeName="Position"/>
               <member name="path" type="nonBasic" nonBasicTypeName="Path"/>
             </struct>
           </types>"#,
    );
    assert_eq!(
        registry.get_type_object("Holder").unwrap().serialize(),
        Holder::complete_type_object().serialize()
    );
    registry.get("Holder").unwrap();
}

#[test]
fn relative_module_references_resolve() {
    let registry = load(
        r#"<types>
             <enum name="Mode"><enumerator name="IDLE"/></enum>
             <module name="geo">
               <struct name="Point" nested="true"><member name="x" type="int32"/></struct>
               <module name="inner">
                 <struct name="Holder">
                   <member name="p" type="nonBasic" nonBasicTypeName="Point"/>
                   <member name="m" type="nonBasic" nonBasicTypeName="Mode"/>
                 </struct>
               </module>
             </module>
           </types>"#,
    );

    let content_id = |name: &str| {
        TypeIdentifier::CompleteTypeId(
            TypeObject::Complete(registry.get_type_object(name).unwrap().clone()).compute_hash(),
        )
    };
    let holder = registry.get_type_object("geo::inner::Holder").unwrap();
    let CompleteTypeObject::Struct(s) = holder else { panic!("expected struct") };
    assert_eq!(s.member_seq[0].common.member_type_id, content_id("geo::Point"));
    assert_eq!(s.member_seq[1].common.member_type_id, content_id("Mode"));
    registry.get("geo::inner::Holder").unwrap();
}

#[test]
fn cycles_require_external() {
    let cyclic = |external: &str| {
        format!(
            r#"<types>
                 <struct name="A" nested="true">
                   <member name="b" type="nonBasic" nonBasicTypeName="B"/>
                 </struct>
                 <struct name="B" nested="true">
                   <member name="a" type="nonBasic" nonBasicTypeName="A"{external}/>
                 </struct>
               </types>"#
        )
    };
    let mut registry = XmlTypeRegistry::new();
    let err = registry.load_str(&cyclic("")).unwrap_err();
    assert!(format!("{err:?}").contains("circular reference"));

    let mut registry = XmlTypeRegistry::new();
    registry.load_str(&cyclic(r#" external="true""#)).unwrap();

    let mut registry = XmlTypeRegistry::new();
    let err = registry
        .load_str(
            r#"<types>
                 <typedef name="X" type="nonBasic" nonBasicTypeName="Y"/>
                 <typedef name="Y" type="nonBasic" nonBasicTypeName="X"/>
               </types>"#,
        )
        .unwrap_err();
    assert!(format!("{err:?}").contains("circular typedef"));
}

#[test]
fn lenient_load_skips_vendor_extras() {
    let xml = r#"<dds>
         <profiles><participant profile_name="x"/></profiles>
         <types>
           <const name="MAX" type="uint32" value="3"/>
           <struct name="T" useVector="true">
             <member name="x" type="int32" transferMode="p"/>
           </struct>
         </types>
       </dds>"#;
    let mut strict = XmlTypeRegistry::new();
    assert!(strict.load_str(xml).is_err());

    let mut registry = XmlTypeRegistry::new();
    registry.load_str_lenient(xml).unwrap();
    registry.get("T").unwrap();
}

fn temp_xml_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("int2dds_xml_inc_{}_{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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
fn unknown_reference_rejected_at_load() {
    let mut registry = XmlTypeRegistry::new();
    let err = registry
        .load_str(
            r#"<types><struct name="T">
                 <member name="p" type="nonBasic" nonBasicTypeName="Missing"/>
               </struct></types>"#,
        )
        .unwrap_err();
    assert!(format!("{err:?}").contains("references unknown type 'Missing'"));
}

#[test]
fn primitives_without_rust_counterpart() {
    let registry = load(
        r#"<types><struct name="Extra">
             <member name="a" type="uint8"/>
             <member name="b" type="char16" optional="true"/>
             <member name="c" type="float128"/>
           </struct></types>"#,
    );
    let mut expected = CompleteStructType::new(
        TypeFlag::new(ExtensibilityKind::Appendable, false, false),
        "Extra".to_string(),
        None,
    );
    let flags = MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);
    let optional = MemberFlag::new(TryConstructKind::Discard, false, true, false, false, false);
    expected.add_member(CompleteStructMember::new(
        0,
        flags,
        TypeIdentifier::Uint8,
        "a".to_string(),
    ));
    expected.add_member(CompleteStructMember::new(
        1,
        optional,
        TypeIdentifier::Char16,
        "b".to_string(),
    ));
    expected.add_member(CompleteStructMember::new(
        2,
        flags,
        TypeIdentifier::Float128,
        "c".to_string(),
    ));
    let expected = CompleteTypeObject::Struct(expected);
    assert_eq!(registry.get_type_object("Extra").unwrap(), &expected);
}

#[test]
fn module_qualified_lookup() {
    let registry = load(
        r#"<types><module name="sensors">
             <struct name="T"><member name="x" type="int32"/></struct>
           </module></types>"#,
    );
    assert!(registry.get_type_object("sensors::T").is_some());
    assert!(registry.get_type_object("T").is_none());
    assert_eq!(registry.type_names(), ["sensors::T"]);
}

#[test]
fn redefinition_policy() {
    let xml = r#"<types><struct name="T"><member name="x" type="int32"/></struct></types>"#;
    let mut registry = load(xml);
    registry.load_str(xml).unwrap();
    assert_eq!(registry.type_names().len(), 1);

    let conflicting = r#"<types><struct name="T"><member name="x" type="int64"/></struct></types>"#;
    let err = registry.load_str(conflicting).unwrap_err();
    assert!(format!("{err:?}").contains("redefined with different content"));
}

#[test]
fn get_unknown_type_fails() {
    let registry = XmlTypeRegistry::new();
    assert!(registry.get("Nope").is_err());
}

#[test]
fn const_bounds_match_literal_bounds() {
    let with_const = load(
        r#"<types>
             <const name="MAX_ITEMS" type="int32" value="16"/>
             <const name="ROWS" type="uint32" value="3"/>
             <struct name="T">
               <member name="names" type="string" sequenceMaxLength="MAX_ITEMS"/>
               <member name="grid" type="float64" arrayDimensions="ROWS, 2"/>
             </struct>
           </types>"#,
    );
    let with_literal = load(
        r#"<types><struct name="T">
             <member name="names" type="string" sequenceMaxLength="16"/>
             <member name="grid" type="float64" arrayDimensions="3, 2"/>
           </struct></types>"#,
    );
    assert_eq!(
        with_const.get_type_object("T").unwrap().serialize(),
        with_literal.get_type_object("T").unwrap().serialize(),
    );
}

#[test]
fn forward_dcl_does_not_block_definition() {
    let registry = load(
        r#"<types>
             <forward_dcl name="Point" kind="struct"/>
             <struct name="Line">
               <member name="start" type="nonBasic" nonBasicTypeName="Point"/>
             </struct>
             <struct name="Point">
               <member name="x" type="int32"/>
               <member name="y" type="int32"/>
             </struct>
           </types>"#,
    );
    assert!(registry.get("Line").is_ok());
    assert!(registry.get_type_object("Point").is_some());
}

#[test]
fn include_resolves_relative_to_including_file() {
    let dir = temp_xml_dir("compose");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(
        dir.join("sub").join("base.xml"),
        r#"<types><struct name="Point">
             <member name="x" type="int32"/><member name="y" type="int32"/>
           </struct></types>"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("main.xml"),
        r#"<dds>
             <include file="sub/base.xml"/>
             <types><struct name="Line">
               <member name="start" type="nonBasic" nonBasicTypeName="Point"/>
             </struct></types>
           </dds>"#,
    )
    .unwrap();
    let registry = XmlTypeRegistry::from_file(dir.join("main.xml")).unwrap();
    assert!(registry.get_type_object("Point").is_some());
    assert!(registry.get("Line").is_ok());
}

#[test]
fn include_cycle_is_safe() {
    let dir = temp_xml_dir("cycle");
    std::fs::write(
        dir.join("a.xml"),
        r#"<dds><include file="b.xml"/>
             <types><struct name="A"><member name="x" type="int32"/></struct></types>
           </dds>"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("b.xml"),
        r#"<dds><include file="a.xml"/>
             <types><struct name="B"><member name="y" type="int32"/></struct></types>
           </dds>"#,
    )
    .unwrap();
    let registry = XmlTypeRegistry::from_file(dir.join("a.xml")).unwrap();
    assert!(registry.get_type_object("A").is_some());
    assert!(registry.get_type_object("B").is_some());
}

#[test]
fn include_diamond_loads_each_file_once() {
    let dir = temp_xml_dir("diamond");
    std::fs::write(
        dir.join("common.xml"),
        r#"<types><struct name="Common"><member name="v" type="int32"/></struct></types>"#,
    )
    .unwrap();
    for (file, name) in [("left.xml", "Left"), ("right.xml", "Right")] {
        std::fs::write(
            dir.join(file),
            format!(
                r#"<dds><include file="common.xml"/>
                     <types><struct name="{name}">
                       <member name="c" type="nonBasic" nonBasicTypeName="Common"/>
                     </struct></types>
                   </dds>"#
            ),
        )
        .unwrap();
    }
    std::fs::write(
        dir.join("top.xml"),
        r#"<dds><include file="left.xml"/><include file="right.xml"/>
             <types><struct name="Top">
               <member name="l" type="nonBasic" nonBasicTypeName="Left"/>
             </struct></types>
           </dds>"#,
    )
    .unwrap();
    let registry = XmlTypeRegistry::from_file(dir.join("top.xml")).unwrap();
    for name in ["Common", "Left", "Right", "Top"] {
        assert!(registry.get_type_object(name).is_some(), "{name} missing");
    }
    // The shared file is loaded once despite being included from two parents.
    assert_eq!(registry.type_names().iter().filter(|n| n.as_str() == "Common").count(), 1);
}

#[test]
fn load_str_ignores_include() {
    let registry = load(
        r#"<types><include file="other.xml"/>
             <struct name="T"><member name="x" type="int32"/></struct>
           </types>"#,
    );
    assert!(registry.get_type_object("T").is_some());
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
