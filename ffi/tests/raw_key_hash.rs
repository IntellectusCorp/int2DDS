use std::any::Any;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::serialize::cdr::ExtensibilityKind;
use int2dds::topic::type_support::{DdsType, TypeSupport};
use int2dds_ffi::data::{CdrFieldDescriptor, CdrFieldType};
use int2dds_ffi::raw_type_support::{KeyFieldInfo, KeyFieldType, RawTypeSupport};

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
