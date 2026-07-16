use std::any::Any;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::serialize::cdr::ExtensibilityKind;
use int2dds::topic::type_support::{DdsType, TypeSupport};
use int2dds::xtypes::{HasTypeObject, TypeIdentifier, TypeObject};
use int2dds_ffi::data::{CdrFieldDescriptor, CdrFieldType};
use int2dds_ffi::raw_type_support::{KeyFieldInfo, KeyFieldType, RawTypeSupport};

/// Compute an InstanceHandle through the FFI *type_info* path (the primary path
/// C# generated types and Python `_dds_type_info_fields` topics take). This
/// registers only a full `TypeObject` — no flat field descriptors — so the
/// handle must come from the canonical DynamicData key delegation in
/// `RawTypeSupport::compute_key`.
fn type_info_handle<T: HasTypeObject>(ext: ExtensibilityKind, payload: &[u8]) -> InstanceHandle {
    let support = RawTypeSupport::with_type_info(
        T::dds_type_name().to_string(),
        ext,
        true,
        T::type_identifier(),
        TypeObject::Complete(T::complete_type_object()),
    );
    let data = support.deserialize(payload, None).unwrap();
    support.compute_key(data.as_ref())
}

fn raw_handle_ext(
    ext: ExtensibilityKind,
    key_fields: Vec<KeyFieldInfo>,
    all_fields: Vec<CdrFieldDescriptor>,
    payload: &[u8],
) -> InstanceHandle {
    let mut support = RawTypeSupport::new_with_key("T".to_string(), ext, true);
    support.set_key_fields(key_fields);
    support.set_all_fields(all_fields);
    let data = support.deserialize(payload, None).unwrap();
    support.compute_key(data.as_ref())
}

fn raw_handle(
    key_fields: Vec<KeyFieldInfo>,
    all_fields: Vec<CdrFieldDescriptor>,
    payload: &[u8],
) -> InstanceHandle {
    raw_handle_ext(ExtensibilityKind::Final, key_fields, all_fields, payload)
}

fn field(name: &str, ty: CdrFieldType, is_key: bool) -> CdrFieldDescriptor {
    CdrFieldDescriptor { name: name.to_string(), field_type: ty, is_key }
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedLong {
    #[dds(key)]
    id: i32,
    value: i32,
}

#[test]
fn raw_long_key_matches_derive_and_spec_vector() {
    let inst = KeyedLong { id: 5, value: 99 };
    let oracle = KeyedLong::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = raw_handle(
        vec![KeyFieldInfo { field_index: 0, field_type: KeyFieldType::Int32 }],
        vec![field("id", CdrFieldType::Int32, true), field("value", CdrFieldType::Int32, false)],
        &bytes,
    );

    assert_eq!(handle, oracle);
    assert_eq!(*handle.value(), [0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn raw_long_key_big_endian_payload_matches() {
    let mut payload = vec![0x00, 0x00, 0x00, 0x00];
    payload.extend_from_slice(&5i32.to_be_bytes());
    payload.extend_from_slice(&99i32.to_be_bytes());

    let handle = raw_handle(
        vec![KeyFieldInfo { field_index: 0, field_type: KeyFieldType::Int32 }],
        vec![field("id", CdrFieldType::Int32, true), field("value", CdrFieldType::Int32, false)],
        &payload,
    );

    assert_eq!(*handle.value(), [0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn raw_long_key_xcdr2_appendable_delimited_matches() {
    let oracle =
        KeyedLong::get_type_support().compute_key(&KeyedLong { id: 5, value: 99 } as &dyn Any);

    let mut payload = vec![0x00, 0x09, 0x00, 0x00];
    payload.extend_from_slice(&8u32.to_le_bytes());
    payload.extend_from_slice(&5i32.to_le_bytes());
    payload.extend_from_slice(&99i32.to_le_bytes());

    let handle = raw_handle_ext(
        ExtensibilityKind::Appendable,
        vec![KeyFieldInfo { field_index: 0, field_type: KeyFieldType::Int32 }],
        vec![field("id", CdrFieldType::Int32, true), field("value", CdrFieldType::Int32, false)],
        &payload,
    );

    assert_eq!(handle, oracle);
    assert_eq!(*handle.value(), [0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn raw_mutable_bails_to_nil() {
    let mut payload = vec![0x00, 0x0B, 0x00, 0x00];
    payload.extend_from_slice(&8u32.to_le_bytes());
    payload.extend_from_slice(&5i32.to_le_bytes());
    payload.extend_from_slice(&99i32.to_le_bytes());

    let handle = raw_handle_ext(
        ExtensibilityKind::Mutable,
        vec![KeyFieldInfo { field_index: 0, field_type: KeyFieldType::Int32 }],
        vec![field("id", CdrFieldType::Int32, true), field("value", CdrFieldType::Int32, false)],
        &payload,
    );

    assert_eq!(handle, InstanceHandle::NIL);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedLongLong {
    #[dds(key)]
    id: i64,
    value: i32,
}

#[test]
fn raw_long_long_key_not_truncated() {
    let inst = KeyedLongLong { id: 0x0000_0001_0000_0005, value: 7 };
    let oracle = KeyedLongLong::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = raw_handle(
        vec![KeyFieldInfo { field_index: 0, field_type: KeyFieldType::Int64 }],
        vec![field("id", CdrFieldType::Int64, true), field("value", CdrFieldType::Int32, false)],
        &bytes,
    );

    assert_eq!(handle, oracle);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct MultiKey {
    #[dds(key)]
    a: i16,
    #[dds(key)]
    b: i32,
    c: i32,
}

#[test]
fn raw_multi_key_all_fields_and_alignment() {
    let inst = MultiKey { a: 3, b: 258, c: 1 };
    let oracle = MultiKey::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = raw_handle(
        vec![
            KeyFieldInfo { field_index: 0, field_type: KeyFieldType::Int16 },
            KeyFieldInfo { field_index: 1, field_type: KeyFieldType::Int32 },
        ],
        vec![
            field("a", CdrFieldType::Int16, true),
            field("b", CdrFieldType::Int32, true),
            field("c", CdrFieldType::Int32, false),
        ],
        &bytes,
    );

    assert_eq!(handle, oracle);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedString {
    #[dds(key, bound = 128)]
    color: String,
    value: i32,
}

#[test]
fn raw_bounded_string_key_matches_derive() {
    let inst = KeyedString { color: "BLUE".to_string(), value: 4 };
    let oracle = KeyedString::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = raw_handle(
        vec![KeyFieldInfo { field_index: 0, field_type: KeyFieldType::String }],
        vec![
            field("color", CdrFieldType::String, true),
            field("value", CdrFieldType::Int32, false),
        ],
        &bytes,
    );

    assert_eq!(handle, oracle);
}

// ---------------------------------------------------------------------------
// type_info path (#336): keyed topics registered with only a full TypeObject —
// the primary path for C# generated types and Python `_dds_type_info_fields`.
// These must produce the same canonical InstanceHandle as the derive oracle via
// the DynamicData key delegation, not the flat CdrFieldType parser.
// ---------------------------------------------------------------------------

#[test]
fn type_info_long_key_matches_derive() {
    let inst = KeyedLong { id: 5, value: 99 };
    let oracle = KeyedLong::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = type_info_handle::<KeyedLong>(ExtensibilityKind::Final, &bytes);
    assert_eq!(handle, oracle);
    assert_eq!(*handle.value(), [0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
}

#[test]
fn type_info_long_long_key_not_truncated() {
    let inst = KeyedLongLong { id: 0x0000_0001_0000_0005, value: 7 };
    let oracle = KeyedLongLong::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = type_info_handle::<KeyedLongLong>(ExtensibilityKind::Final, &bytes);
    assert_eq!(handle, oracle);
}

#[test]
fn type_info_multi_key_matches_derive() {
    let inst = MultiKey { a: 3, b: 258, c: 1 };
    let oracle = MultiKey::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = type_info_handle::<MultiKey>(ExtensibilityKind::Final, &bytes);
    assert_eq!(handle, oracle);
}

#[test]
fn type_info_bounded_string_key_matches_derive() {
    let inst = KeyedString { color: "BLUE".to_string(), value: 4 };
    let oracle = KeyedString::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = type_info_handle::<KeyedString>(ExtensibilityKind::Final, &bytes);
    assert_eq!(handle, oracle);
}

// A 64-bit-wide key value the flat CdrFieldType parser handles, re-validated
// through the dynamic delegation to lock the two paths together.
#[test]
fn type_info_double_key_matches_derive() {
    let inst = KeyedDouble { id: 42.5, value: 7 };
    let oracle = KeyedDouble::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let handle = type_info_handle::<KeyedDouble>(ExtensibilityKind::Final, &bytes);
    assert_eq!(handle, oracle);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedDouble {
    #[dds(key)]
    id: f64,
    value: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct NestedKeyPart {
    a: i32,
    b: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct CompositeKeyed {
    #[dds(key)]
    loc: NestedKeyPart,
    value: i32,
}

// Composite (nested-struct) key members through the registry-backed key path. The
// nested member is referenced by content-hash `CompleteTypeId` (matching the derive
// macro) and the nested `TypeObject` is supplied as a dependency, so
// `with_type_info_and_deps` builds a `TypeRegistry`, resolves the child, and
// `serialize_key_cdr` recurses into the nested key members — yielding the same
// InstanceHandle as native Rust (canonical big-endian CDR of a=7, b=9).
#[test]
fn type_info_composite_nested_key_matches_derive() {
    let inst = CompositeKeyed { loc: NestedKeyPart { a: 7, b: 9 }, value: 1 };
    let oracle = CompositeKeyed::get_type_support().compute_key(&inst as &dyn Any);
    let bytes = inst.serialize().unwrap();

    let nested_to = TypeObject::Complete(NestedKeyPart::complete_type_object());
    let nested_id = TypeIdentifier::CompleteTypeId(nested_to.compute_hash());
    let support = RawTypeSupport::with_type_info_and_deps(
        CompositeKeyed::dds_type_name().to_string(),
        ExtensibilityKind::Final,
        true,
        CompositeKeyed::type_identifier(),
        TypeObject::Complete(CompositeKeyed::complete_type_object()),
        vec![(nested_id, nested_to)],
    );
    let data = support.deserialize(&bytes, None).unwrap();
    let handle = support.compute_key(data.as_ref());
    assert_eq!(handle, oracle);
    assert_eq!(*handle.value(), [0, 0, 0, 7, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0]);
}
