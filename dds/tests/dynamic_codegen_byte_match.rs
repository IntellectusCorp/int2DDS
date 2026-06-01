//! Byte-for-byte equivalence between the derive-codegen serialization and the
//! runtime dynamic-type serialization (`DynamicType` built from a
//! `CompleteTypeObject` via the `TypeRegistry`).
//!
//! This is the strongest guarantee that the dynamic path produces exactly the
//! same wire bytes as `#[derive(DdsType)]`. Covered: nested struct + `Vec<inner>`
//! across CDR(Final/Appendable) and XCDR2 Final/Appendable/Mutable, plus optional
//! members across all four distinct optional encodings.

use std::sync::Arc;

use int2dds::dcps::topic::type_support::{DdsType, SerializationFormat};
use int2dds::serialize::cdr::{
    CdrSerialize, CdrSerializer, ExtensibilityKind, Xcdr2Serializer, XcdrSerialize,
};
use int2dds::serialize::{BufferManager, DeserializerReader};
use int2dds::xtypes::{
    serialize_dynamic_data, CompleteStructMember, CompleteStructType, CompleteTypeObject,
    DynamicData, DynamicType, DynamicValue, EquivalenceHash, HasTypeObject, MemberFlag,
    PlainCollectionHeader, TryConstructKind, TypeFlag, TypeIdentifier, TypeRegistry,
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
