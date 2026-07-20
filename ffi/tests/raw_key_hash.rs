use std::any::Any;

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::serialize::cdr::ExtensibilityKind;
use int2dds::topic::type_support::{DdsType, TypeSupport};
use int2dds::xtypes::{HasTypeObject, TypeIdentifier, TypeObject};
use int2dds_ffi::raw_type_support::RawTypeSupport;

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

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedLong {
    #[dds(key)]
    id: i32,
    value: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedLongLong {
    #[dds(key)]
    id: i64,
    value: i32,
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

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedString {
    #[dds(key, bound = 128)]
    color: String,
    value: i32,
}

// ---------------------------------------------------------------------------
// type_info path (#336): keyed topics registered with only a full TypeObject —
// the primary path for C# generated types and Python `_dds_type_info_fields`.
// These must produce the same canonical InstanceHandle as the derive oracle via
// the DynamicData key delegation (the sole FFI raw-path key machinery).
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

// A 64-bit-wide key value re-validated through the dynamic delegation to lock the
// FFI raw path and the derive oracle together.
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
