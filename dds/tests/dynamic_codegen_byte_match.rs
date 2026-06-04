//! Byte-for-byte equivalence between the derive-codegen serialization and the
//! runtime dynamic-type serialization (`DynamicType` built from a
//! `CompleteTypeObject` via the `TypeRegistry`).
//!
//! This is the strongest guarantee that the dynamic path produces exactly the
//! same wire bytes as `#[derive(DdsType)]`. Covered: nested struct + `Vec<inner>`
//! across CDR(Final/Appendable/Mutable=PL_CDR) and XCDR2 Final/Appendable/Mutable,
//! plus optional members across all distinct optional encodings, and
//! union/bitset members.

use std::sync::Arc;

use int2dds::dcps::topic::type_support::{DdsType, SerializationFormat};
use int2dds::serialize::cdr::{
    CdrSerialize, CdrSerializer, ExtensibilityKind, PrimitiveSerialize, Xcdr2Serializer,
    XcdrDeserialize, XcdrDeserializer, XcdrSerialize,
};
use int2dds::serialize::{BufferManager, DeserializerReader};
use int2dds::xtypes::{
    serialize_dynamic_data, CompleteStructMember, CompleteStructType, CompleteTypeObject,
    DynamicData, DynamicType, DynamicTypeSupport, DynamicValue, EquivalenceHash, HasTypeObject,
    MemberFlag, PlainCollectionHeader, TryConstructKind, TypeFlag, TypeIdentifier, TypeObject,
    TypeRegistry,
};

fn hash_of(id: &TypeIdentifier) -> EquivalenceHash {
    match id {
        TypeIdentifier::CompleteTypeId(h) | TypeIdentifier::MinimalTypeId(h) => *h,
        other => panic!("expected a hash-based type identifier, got {:?}", other),
    }
}

fn concrete_cdr<T: CdrSerialize>(value: &T) -> Vec<u8> {
    let mut serializer = CdrSerializer::with_capacity(true, 256);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();
    serializer.into_bytes()
}

fn concrete_xcdr<T: XcdrSerialize>(value: &T, ext: ExtensibilityKind) -> Vec<u8> {
    let mut serializer = Xcdr2Serializer::with_capacity(true, ext, 256);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();
    serializer.into_bytes()
}

fn dynamic_bytes(data: &DynamicData, format: &SerializationFormat) -> Vec<u8> {
    serialize_dynamic_data(data, format).unwrap().to_vec()
}

fn xcdr_format(ext: ExtensibilityKind) -> SerializationFormat {
    SerializationFormat::Xcdr { extensibility_kind: ext, use_delimiters: false }
}

fn standalone_type<T: HasTypeObject>() -> Arc<DynamicType> {
    Arc::new(
        DynamicType::from_type_object(T::complete_type_object(), T::type_identifier()).unwrap(),
    )
}

/// Build a DynamicType for `O` with the nested type `I` resolved via a registry.
fn nested_types<O: HasTypeObject, I: HasTypeObject>() -> (Arc<DynamicType>, Arc<DynamicType>) {
    let mut registry = TypeRegistry::new();
    let inner_hash = hash_of(&I::type_identifier());
    registry.register_complete(inner_hash, "Inner".into(), I::complete_type_object());

    let outer_dt = Arc::new(
        DynamicType::from_type_object_with_registry(
            Arc::new(O::complete_type_object()),
            O::type_identifier(),
            &registry,
        )
        .unwrap(),
    );
    let inner_dt = standalone_type::<I>();
    (outer_dt, inner_dt)
}

fn assert_complete_object_is_struct<T: HasTypeObject>() {
    assert!(matches!(T::complete_type_object(), CompleteTypeObject::Struct(_)));
}

macro_rules! nested_family {
    ($inner:ident, $outer:ident, $ext:literal) => {
        #[derive(DdsType)]
        #[dds_type(crate_path = "int2dds", extensibility = $ext)]
        struct $inner {
            a: i32,
            b: i32,
        }

        #[derive(DdsType)]
        #[dds_type(crate_path = "int2dds", extensibility = $ext)]
        struct $outer {
            id: i32,
            child: $inner,
        }
    };
}

nested_family!(InnerFinal, OuterFinal, "Final");
nested_family!(InnerApp, OuterApp, "Appendable");
nested_family!(InnerMut, OuterMut, "Mutable");

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct OuterFinalChildApp {
    id: i32,
    child: InnerApp,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct OuterAppChildFinal {
    id: i32,
    child: InnerFinal,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct OuterFinalChildMut {
    id: i32,
    child: InnerMut,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
#[repr(u8)]
enum Small8 {
    A,
    B,
    C,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
#[repr(i16)]
enum Small16 {
    X,
    Y,
    Z,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct EnumHolder {
    v8: Small8,
    v16: Small16,
    list: Vec<Small8>,
}

fn build_dynamic_nested(outer_dt: &Arc<DynamicType>, inner_dt: &Arc<DynamicType>) -> DynamicData {
    let mut child = DynamicData::new(inner_dt.clone());
    child.set("a", 1i32).unwrap();
    child.set("b", 2i32).unwrap();

    let mut outer = DynamicData::new(outer_dt.clone());
    outer.set("id", 7i32).unwrap();
    outer.set_value("child", DynamicValue::Struct(Box::new(child))).unwrap();
    outer
}

#[test]
fn nested_byte_match_cdr_final() {
    assert_complete_object_is_struct::<OuterFinal>();
    let (outer_dt, inner_dt) = nested_types::<OuterFinal, InnerFinal>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterFinal { id: 7, child: InnerFinal { a: 1, b: 2 } };
    assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
}

#[test]
fn nested_byte_match_xcdr_final() {
    let (outer_dt, inner_dt) = nested_types::<OuterFinal, InnerFinal>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterFinal { id: 7, child: InnerFinal { a: 1, b: 2 } };
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Final)
    );
}

#[test]
fn nested_byte_match_cdr_appendable() {
    let (outer_dt, inner_dt) = nested_types::<OuterApp, InnerApp>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterApp { id: 7, child: InnerApp { a: 1, b: 2 } };
    assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
}

#[test]
fn nested_byte_match_xcdr_appendable() {
    let (outer_dt, inner_dt) = nested_types::<OuterApp, InnerApp>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterApp { id: 7, child: InnerApp { a: 1, b: 2 } };
    let format = xcdr_format(ExtensibilityKind::Appendable);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Appendable)
    );
}

#[test]
fn nested_byte_match_xcdr_mutable() {
    let (outer_dt, inner_dt) = nested_types::<OuterMut, InnerMut>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterMut { id: 7, child: InnerMut { a: 1, b: 2 } };
    let format = xcdr_format(ExtensibilityKind::Mutable);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Mutable)
    );
}

#[test]
fn nested_byte_match_cdr_mutable() {
    let (outer_dt, inner_dt) = nested_types::<OuterMut, InnerMut>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterMut { id: 7, child: InnerMut { a: 1, b: 2 } };
    assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
}

#[test]
fn nested_byte_match_xcdr_mixed_final_outer_app_inner() {
    let (outer_dt, inner_dt) = nested_types::<OuterFinalChildApp, InnerApp>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterFinalChildApp { id: 7, child: InnerApp { a: 1, b: 2 } };
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Final)
    );
}

#[test]
fn nested_byte_match_xcdr_mixed_app_outer_final_inner() {
    let (outer_dt, inner_dt) = nested_types::<OuterAppChildFinal, InnerFinal>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterAppChildFinal { id: 7, child: InnerFinal { a: 1, b: 2 } };
    let format = xcdr_format(ExtensibilityKind::Appendable);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Appendable)
    );
}

#[test]
fn nested_byte_match_xcdr_mixed_final_outer_mut_inner() {
    let (outer_dt, inner_dt) = nested_types::<OuterFinalChildMut, InnerMut>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterFinalChildMut { id: 7, child: InnerMut { a: 1, b: 2 } };
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Final)
    );
}

#[test]
fn mixed_inner_appendable_is_delimited() {
    let (mixed_dt, app_inner_dt) = nested_types::<OuterFinalChildApp, InnerApp>();
    let mixed = build_dynamic_nested(&mixed_dt, &app_inner_dt);
    let mixed_bytes = dynamic_bytes(&mixed, &xcdr_format(ExtensibilityKind::Final));

    let (final_dt, final_inner_dt) = nested_types::<OuterFinal, InnerFinal>();
    let uniform = build_dynamic_nested(&final_dt, &final_inner_dt);
    let uniform_bytes = dynamic_bytes(&uniform, &xcdr_format(ExtensibilityKind::Final));

    // Layout: [encap(4) | id(4)] [inner DHEADER(4)] [a(4) | b(4)]. The all-Final
    // case omits the inner DHEADER; the mixed case inserts it before a,b.
    assert_eq!(mixed_bytes.len(), uniform_bytes.len() + 4, "inner DHEADER must be present");
    assert_eq!(mixed_bytes[..8], uniform_bytes[..8], "encap + id prefix unchanged");
    assert_eq!(mixed_bytes[8..12], [8, 0, 0, 0], "inner DHEADER = object size 8 (LE)");
    assert_eq!(mixed_bytes[12..], uniform_bytes[8..], "inner fields a,b unchanged");
}

#[test]
fn mixed_codegen_roundtrip_reads_inner_dheader() {
    let (outer_dt, inner_dt) = nested_types::<OuterFinalChildApp, InnerApp>();
    let dynamic = build_dynamic_nested(&outer_dt, &inner_dt);
    let concrete = OuterFinalChildApp { id: 7, child: InnerApp { a: 1, b: 2 } };

    let dyn_bytes = dynamic_bytes(&dynamic, &xcdr_format(ExtensibilityKind::Final));
    let concrete_bytes = concrete_xcdr(&concrete, ExtensibilityKind::Final);
    assert_eq!(dyn_bytes, concrete_bytes, "dynamic and codegen wire must be identical");

    let mut d = XcdrDeserializer::new(&dyn_bytes).unwrap();
    let back = OuterFinalChildApp::deserialize_xcdr(&mut d).unwrap();
    assert_eq!(back.id, 7);
    assert_eq!(back.child.a, 1);
    assert_eq!(back.child.b, 2);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct VecHolderFinal {
    items: Vec<InnerFinal>,
}

fn vec_holder_dynamic_type() -> (Arc<DynamicType>, Arc<DynamicType>) {
    let inner_complete = InnerFinal::complete_type_object();
    let inner_hash = EquivalenceHash::compute(&inner_complete.serialize());

    let mut registry = TypeRegistry::new();
    registry.register_complete(inner_hash, "InnerFinal".into(), inner_complete);

    let mut outer = CompleteStructType::new(
        TypeFlag::new(int2dds::xtypes::ExtensibilityKind::Final, false, false),
        "VecHolderFinal".into(),
        None,
    );
    outer.add_member(CompleteStructMember::new(
        0,
        MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
        TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader::default(),
            bound: 0,
            element_identifier: Box::new(TypeIdentifier::CompleteTypeId(inner_hash)),
        },
        "items".to_string(),
    ));

    let outer_dt = Arc::new(
        DynamicType::from_type_object_with_registry(
            Arc::new(CompleteTypeObject::Struct(outer)),
            TypeIdentifier::None,
            &registry,
        )
        .unwrap(),
    );
    let inner_dt = standalone_type::<InnerFinal>();
    (outer_dt, inner_dt)
}

fn build_dynamic_vec_holder(
    outer_dt: &Arc<DynamicType>,
    inner_dt: &Arc<DynamicType>,
) -> DynamicData {
    let make = |a: i32, b: i32| {
        let mut e = DynamicData::new(inner_dt.clone());
        e.set("a", a).unwrap();
        e.set("b", b).unwrap();
        DynamicValue::Struct(Box::new(e))
    };
    let mut outer = DynamicData::new(outer_dt.clone());
    outer.set_value("items", DynamicValue::Sequence(vec![make(10, 20), make(30, 40)])).unwrap();
    outer
}

#[test]
fn vec_of_struct_byte_match_cdr() {
    let (outer_dt, inner_dt) = vec_holder_dynamic_type();
    let dynamic = build_dynamic_vec_holder(&outer_dt, &inner_dt);
    let concrete =
        VecHolderFinal { items: vec![InnerFinal { a: 10, b: 20 }, InnerFinal { a: 30, b: 40 }] };
    assert_eq!(dynamic_bytes(&dynamic, &SerializationFormat::Cdr), concrete_cdr(&concrete));
}

#[test]
fn vec_of_struct_byte_match_xcdr_final() {
    let (outer_dt, inner_dt) = vec_holder_dynamic_type();
    let dynamic = build_dynamic_vec_holder(&outer_dt, &inner_dt);
    let concrete =
        VecHolderFinal { items: vec![InnerFinal { a: 10, b: 20 }, InnerFinal { a: 30, b: 40 }] };
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete, ExtensibilityKind::Final)
    );
}

fn enum_holder_dynamic_type() -> Arc<DynamicType> {
    use int2dds::xtypes::HasTypeObject;
    let small8_hash = hash_of(&Small8::type_identifier());
    let small16_hash = hash_of(&Small16::type_identifier());

    let mut registry = TypeRegistry::new();
    registry.register_complete(small8_hash, "Small8".into(), Small8::complete_type_object());
    registry.register_complete(small16_hash, "Small16".into(), Small16::complete_type_object());

    let flag = || MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);
    let mut outer = CompleteStructType::new(
        TypeFlag::new(int2dds::xtypes::ExtensibilityKind::Final, false, false),
        "EnumHolder".into(),
        None,
    );
    outer.add_member(CompleteStructMember::new(
        0,
        flag(),
        TypeIdentifier::CompleteTypeId(small8_hash),
        "v8".to_string(),
    ));
    outer.add_member(CompleteStructMember::new(
        1,
        flag(),
        TypeIdentifier::CompleteTypeId(small16_hash),
        "v16".to_string(),
    ));
    outer.add_member(CompleteStructMember::new(
        2,
        flag(),
        TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader::default(),
            bound: 0,
            element_identifier: Box::new(TypeIdentifier::CompleteTypeId(small8_hash)),
        },
        "list".to_string(),
    ));

    Arc::new(
        DynamicType::from_type_object_with_registry(
            Arc::new(CompleteTypeObject::Struct(outer)),
            TypeIdentifier::None,
            &registry,
        )
        .unwrap(),
    )
}

#[test]
fn codegen_emits_repr_bit_bound() {
    use int2dds::xtypes::HasTypeObject;
    let bound = |obj: CompleteTypeObject| match obj {
        CompleteTypeObject::Enum(e) => e.header.common.bit_bound,
        other => panic!("expected enum type object, got {:?}", other),
    };
    assert_eq!(bound(Small8::complete_type_object()), 8, "#[repr(u8)] enum -> bit_bound 8");
    assert_eq!(bound(Small16::complete_type_object()), 16, "#[repr(i16)] enum -> bit_bound 16");
}

fn build_dynamic_enum_holder(dt: &Arc<DynamicType>) -> DynamicData {
    let e = |value: i32| DynamicValue::Enum { name: String::new(), value };
    let mut data = DynamicData::new(dt.clone());
    data.set_value("v8", e(2)).unwrap();
    data.set_value("v16", e(2)).unwrap();
    data.set_value("list", DynamicValue::Sequence(vec![e(0), e(1)])).unwrap();
    data
}

fn concrete_enum_holder() -> EnumHolder {
    EnumHolder { v8: Small8::C, v16: Small16::Z, list: vec![Small8::A, Small8::B] }
}

#[test]
fn enum_bit_bound_byte_match_cdr() {
    let dynamic = build_dynamic_enum_holder(&enum_holder_dynamic_type());
    assert_eq!(
        dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
        concrete_cdr(&concrete_enum_holder())
    );
}

#[test]
fn enum_bit_bound_byte_match_xcdr_final() {
    let dynamic = build_dynamic_enum_holder(&enum_holder_dynamic_type());
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&dynamic, &format),
        concrete_xcdr(&concrete_enum_holder(), ExtensibilityKind::Final)
    );
}

#[test]
fn enum_bit_bound_codegen_roundtrip() {
    let dynamic = build_dynamic_enum_holder(&enum_holder_dynamic_type());
    let bytes = dynamic_bytes(&dynamic, &xcdr_format(ExtensibilityKind::Final));
    let mut d = XcdrDeserializer::new(&bytes).unwrap();
    let back = EnumHolder::deserialize_xcdr(&mut d).unwrap();
    assert_eq!(back.v8, Small8::C);
    assert_eq!(back.v16, Small16::Z);
    assert_eq!(back.list, vec![Small8::A, Small8::B]);
}

fn enum_holder_type_object() -> TypeObject {
    let small8_hash = hash_of(&Small8::type_identifier());
    let small16_hash = hash_of(&Small16::type_identifier());
    let flag = || MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);

    let mut outer = CompleteStructType::new(
        TypeFlag::new(int2dds::xtypes::ExtensibilityKind::Final, false, false),
        "EnumHolderSupport".into(),
        None,
    );
    outer.add_member(CompleteStructMember::new(
        0,
        flag(),
        TypeIdentifier::CompleteTypeId(small8_hash),
        "v8".to_string(),
    ));
    outer.add_member(CompleteStructMember::new(
        1,
        flag(),
        TypeIdentifier::CompleteTypeId(small16_hash),
        "v16".to_string(),
    ));
    outer.add_member(CompleteStructMember::new(
        2,
        flag(),
        TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader::default(),
            bound: 0,
            element_identifier: Box::new(TypeIdentifier::CompleteTypeId(small8_hash)),
        },
        "list".to_string(),
    ));
    TypeObject::Complete(CompleteTypeObject::Struct(outer))
}

fn enum_holder_registry() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    registry.register_complete(
        hash_of(&Small8::type_identifier()),
        "Small8".into(),
        Small8::complete_type_object(),
    );
    registry.register_complete(
        hash_of(&Small16::type_identifier()),
        "Small16".into(),
        Small16::complete_type_object(),
    );
    registry
}

#[test]
fn support_with_registry_resolves_nested_enum_width() {
    let support = DynamicTypeSupport::from_type_object_with_registry(
        enum_holder_type_object(),
        &enum_holder_registry(),
    )
    .unwrap();
    let dynamic = build_dynamic_enum_holder(support.dynamic_type());
    assert_eq!(
        dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
        concrete_cdr(&concrete_enum_holder())
    );
}

#[test]
fn support_without_registry_does_not_resolve_nested_enum_width() {
    let support = DynamicTypeSupport::from_type_object(enum_holder_type_object()).unwrap();
    let dynamic = build_dynamic_enum_holder(support.dynamic_type());
    let codegen = concrete_cdr(&concrete_enum_holder());

    match serialize_dynamic_data(&dynamic, &SerializationFormat::Cdr) {
        Ok(bytes) => assert_ne!(bytes.to_vec(), codegen),
        Err(_) => {}
    }
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct OptFinal {
    id: i32,
    #[dds(optional)]
    opt: Option<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct OptApp {
    id: i32,
    #[dds(optional)]
    opt: Option<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct OptMut {
    id: i32,
    #[dds(optional)]
    opt: Option<i32>,
}

fn build_dynamic_opt(dt: &Arc<DynamicType>, opt: Option<i32>) -> DynamicData {
    let mut data = DynamicData::new(dt.clone());
    data.set("id", 5i32).unwrap();
    if let Some(v) = opt {
        data.set("opt", v).unwrap();
    }
    data
}

#[test]
fn optional_byte_match_cdr_final() {
    let dt = standalone_type::<OptFinal>();
    for opt in [Some(9i32), None] {
        let dynamic = build_dynamic_opt(&dt, opt);
        let concrete = OptFinal { id: 5, opt };
        assert_eq!(
            dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
            concrete_cdr(&concrete),
            "CDR optional mismatch for {:?}",
            opt
        );
    }
}

#[test]
fn optional_byte_match_xcdr_final() {
    let dt = standalone_type::<OptFinal>();
    let format = xcdr_format(ExtensibilityKind::Final);
    for opt in [Some(9i32), None] {
        let dynamic = build_dynamic_opt(&dt, opt);
        let concrete = OptFinal { id: 5, opt };
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Final),
            "XCDR2 Final optional mismatch for {:?}",
            opt
        );
    }
}

#[test]
fn optional_byte_match_cdr_mutable() {
    let dt = standalone_type::<OptMut>();
    for opt in [Some(9i32), None] {
        let dynamic = build_dynamic_opt(&dt, opt);
        let concrete = OptMut { id: 5, opt };
        assert_eq!(
            dynamic_bytes(&dynamic, &SerializationFormat::Cdr),
            concrete_cdr(&concrete),
            "CDR (PL_CDR) mutable optional mismatch for {:?}",
            opt
        );
    }
}

#[test]
fn optional_byte_match_xcdr_appendable() {
    let dt = standalone_type::<OptApp>();
    let format = xcdr_format(ExtensibilityKind::Appendable);
    for opt in [Some(9i32), None] {
        let dynamic = build_dynamic_opt(&dt, opt);
        let concrete = OptApp { id: 5, opt };
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Appendable),
            "XCDR2 Appendable optional mismatch for {:?}",
            opt
        );
    }
}

#[test]
fn optional_byte_match_xcdr_mutable() {
    let dt = standalone_type::<OptMut>();
    let format = xcdr_format(ExtensibilityKind::Mutable);
    for opt in [Some(9i32), None] {
        let dynamic = build_dynamic_opt(&dt, opt);
        let concrete = OptMut { id: 5, opt };
        assert_eq!(
            dynamic_bytes(&dynamic, &format),
            concrete_xcdr(&concrete, ExtensibilityKind::Mutable),
            "XCDR2 Mutable optional mismatch for {:?}",
            opt
        );
    }
}

fn final_holder_with_member<I: HasTypeObject>(member_name: &str) -> Arc<DynamicType> {
    let inner_complete = I::complete_type_object();
    let inner_hash = EquivalenceHash::compute(&inner_complete.serialize());

    let mut registry = TypeRegistry::new();
    registry.register_complete(inner_hash, "Inner".into(), inner_complete);

    let mut outer = CompleteStructType::new(
        TypeFlag::new(int2dds::xtypes::ExtensibilityKind::Final, false, false),
        "Holder".into(),
        None,
    );
    outer.add_member(CompleteStructMember::new(
        0,
        MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false),
        TypeIdentifier::CompleteTypeId(inner_hash),
        member_name.to_string(),
    ));
    Arc::new(
        DynamicType::from_type_object_with_registry(
            Arc::new(CompleteTypeObject::Struct(outer)),
            TypeIdentifier::None,
            &registry,
        )
        .unwrap(),
    )
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
#[repr(i32)]
enum UnionFinal {
    A(i32),
    B(i32),
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
#[repr(i32)]
enum UnionMut {
    A(i32),
    B(i32),
}

fn union_data(dt: &Arc<DynamicType>, disc: i32, value: i32) -> DynamicData {
    let mut data = DynamicData::new(dt.clone());
    data.set_value(
        "u",
        DynamicValue::Union {
            discriminator: Box::new(DynamicValue::Int32(disc)),
            value: Box::new(DynamicValue::Int32(value)),
        },
    )
    .unwrap();
    data
}

#[test]
fn union_byte_match_cdr_final() {
    let dt = final_holder_with_member::<UnionFinal>("u");
    assert_eq!(
        dynamic_bytes(&union_data(&dt, 0, 11), &SerializationFormat::Cdr),
        concrete_cdr(&UnionFinal::A(11)),
        "union case A"
    );
    assert_eq!(
        dynamic_bytes(&union_data(&dt, 1, 22), &SerializationFormat::Cdr),
        concrete_cdr(&UnionFinal::B(22)),
        "union case B"
    );
}

#[test]
fn union_byte_match_xcdr_final() {
    let dt = final_holder_with_member::<UnionFinal>("u");
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&union_data(&dt, 0, 11), &format),
        concrete_xcdr(&UnionFinal::A(11), ExtensibilityKind::Final),
        "union case A"
    );
    assert_eq!(
        dynamic_bytes(&union_data(&dt, 1, 22), &format),
        concrete_xcdr(&UnionFinal::B(22), ExtensibilityKind::Final),
        "union case B"
    );
}

#[test]
fn union_byte_match_cdr_mutable() {
    let dt = final_holder_with_member::<UnionMut>("u");
    assert_eq!(
        dynamic_bytes(&union_data(&dt, 1, 22), &SerializationFormat::Cdr),
        concrete_cdr(&UnionMut::B(22)),
    );
}

#[test]
fn union_byte_match_xcdr_mutable() {
    let dt = final_holder_with_member::<UnionMut>("u");
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&union_data(&dt, 1, 22), &format),
        concrete_xcdr(&UnionMut::B(22), ExtensibilityKind::Final),
    );
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", bitset)]
struct BitsetByteMatch {
    #[dds(bitfield = 3)]
    a: u8,
    #[dds(bitfield = 5)]
    b: u8,
}

fn bitset_data(dt: &Arc<DynamicType>, packed: u64) -> DynamicData {
    let mut data = DynamicData::new(dt.clone());
    data.set_value("s", DynamicValue::Bitset(packed)).unwrap();
    data
}

#[test]
fn bitset_byte_match_cdr() {
    let dt = final_holder_with_member::<BitsetByteMatch>("s");
    let packed = 5u64 | (9u64 << 3); // a = 5 (3 bits), b = 9 (5 bits)
    assert_eq!(
        dynamic_bytes(&bitset_data(&dt, packed), &SerializationFormat::Cdr),
        concrete_cdr(&BitsetByteMatch { a: 5, b: 9 }),
    );
}

#[test]
fn bitset_byte_match_xcdr_final() {
    let dt = final_holder_with_member::<BitsetByteMatch>("s");
    let packed = 5u64 | (9u64 << 3);
    let format = xcdr_format(ExtensibilityKind::Final);
    assert_eq!(
        dynamic_bytes(&bitset_data(&dt, packed), &format),
        concrete_xcdr(&BitsetByteMatch { a: 5, b: 9 }, ExtensibilityKind::Final),
    );
}
