//! Canonical RTPS KeyHash golden vectors — anchored on the **spec**, not on the
//! derive macro's own output.
//!
//! These vectors are copied verbatim from DDSI-RTPS v2.5 §9.6.4.8 (Examples 1–3).
//! They are the independent oracle for the canonical KeyHash engine (issue A1 /
//! #334 / #336 / #339): PLAIN_CDR2 Big-Endian with max alignment 4, key members
//! ordered by memberId, recursive key projection into nested aggregated types,
//! and the raw-vs-MD5 decision made on the KeyHolder's **maximum** serialized
//! size (not the actual serialized length).
//!
//! Example 3 is expected to FAIL until nested @key projection + max-size land —
//! that failure is the proof the current implementation diverges from the
//! standard. Do not "fix" these vectors to match the code; fix the code to match
//! these vectors.

use std::any::Any;

use int2dds::serialize::DeserializerReader;
use int2dds::topic::type_support::{DdsType, TypeSupport};
use int2dds::xtypes::{
    deserialize_dynamic_data, DynamicTypeSupport, HasTypeObject, TypeIdentifier, TypeObject,
    TypeRegistry,
};

// ---------------------------------------------------------------------------
// Example 1 — single @key long, fits in 16 bytes → raw, zero-padded.
//   @final struct Foo { @key long id; long x; long y; };  id = 0x12345678
//   KeyHash = { 12 34 56 78, 00 x12 }
// ---------------------------------------------------------------------------
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct Example1 {
    #[dds(key)]
    id: i32,
    x: i32,
    y: i32,
}

#[test]
fn spec_example1_single_long_raw() {
    let obj = Example1 { id: 0x1234_5678, x: 10, y: 20 };
    let handle = Example1::get_type_support().compute_key(&obj as &dyn Any);
    assert_eq!(
        *handle.value(),
        [0x12, 0x34, 0x56, 0x78, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        "Example 1 KeyHash must be the raw 4-byte big-endian id zero-padded to 16"
    );
}

// ---------------------------------------------------------------------------
// Example 2 — @key string<12> + @key long long, max size 28 > 16 → MD5.
//   Step-4 PLAIN_CDR2 BE stream (long long aligned to 4, NOT 8):
//     00 00 00 05 42 4c 55 45 00 00 00 00 12 34 56 78 9a bc de f0
//   KeyHash = MD5(stream) = f9 1a 59 e3 2e 45 35 d9 a6 9c d5 d9 f5 b6 e3 6e
// Catches F1 (must align the long long to 4, not 8).
// ---------------------------------------------------------------------------
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct Example2 {
    #[dds(key, bound = 12)]
    label: String,
    #[dds(key)]
    id: i64,
    x: i32,
    y: i32,
}

#[test]
fn spec_example2_string_and_longlong_md5() {
    let obj =
        Example2 { label: "BLUE".to_string(), id: 0x1234_5678_9abc_def0u64 as i64, x: 10, y: 20 };
    let handle = Example2::get_type_support().compute_key(&obj as &dyn Any);
    assert_eq!(
        *handle.value(),
        [
            0xf9, 0x1a, 0x59, 0xe3, 0x2e, 0x45, 0x35, 0xd9, 0xa6, 0x9c, 0xd5, 0xd9, 0xf5, 0xb6,
            0xe3, 0x6e
        ],
        "Example 2 KeyHash must be MD5 of the align-4 PLAIN_CDR2 BE stream"
    );
}

// ---------------------------------------------------------------------------
// Example 3 — nested @key projection + memberId reorder, max size 21 > 16 → MD5.
//   @mutable struct Nested { @key long m_long; long u; long w; };
//   @mutable struct Foo {
//     @id(40) @key string<12> label;
//     @id(30) @key Nested m_nested;   // reordered BEFORE label (30 < 40)
//     @id(20) long x; @id(10) long y;
//   };
//   NestedKeyHolder keeps only m_long. Step-4 BE stream (13 bytes):
//     12 34 56 78 00 00 00 05 42 4c 55 45 00
//   KeyHash = MD5(stream) = 37 4b 96 e2 e7 27 23 7f 01 6c c4 ce bb 6e b7 1e
// Catches C1 (memberId order), C2 (nested key-only projection) AND F2
// (actual length 13 <= 16 but MUST be MD5 because max size 21 > 16).
// ---------------------------------------------------------------------------
#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct Example3Nested {
    #[dds(key)]
    m_long: i32,
    u: i32,
    w: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct Example3Foo {
    #[dds(id = 40, key, bound = 12)]
    label: String,
    #[dds(id = 30, key)]
    m_nested: Example3Nested,
    #[dds(id = 20)]
    x: i32,
    #[dds(id = 10)]
    y: i32,
}

const EXAMPLE3_KEYHASH: [u8; 16] = [
    0x37, 0x4b, 0x96, 0xe2, 0xe7, 0x27, 0x23, 0x7f, 0x01, 0x6c, 0xc4, 0xce, 0xbb, 0x6e, 0xb7, 0x1e,
];

fn example3_obj() -> Example3Foo {
    Example3Foo {
        label: "BLUE".to_string(),
        m_nested: Example3Nested { m_long: 0x1234_5678, u: 10, w: 20 },
        x: 100,
        y: 200,
    }
}

// Derive path.
#[test]
#[ignore = "RED until derive nested @key projection + max-size land (A1): dynamic path is green (see below)"]
fn spec_example3_derive_nested_key_projection_and_reorder_md5() {
    let handle = Example3Foo::get_type_support().compute_key(&example3_obj() as &dyn Any);
    assert_eq!(
        *handle.value(),
        EXAMPLE3_KEYHASH,
        "Example 3 KeyHash must project nested @key (m_long only), order by memberId \
         (m_nested before label), and MD5 because max size 21 > 16"
    );
}

// Dynamic (DynamicData / type_info) path — the path every FFI/C#/Python
// InstanceHandle flows through. Builds the DynamicType from the type's own
// TypeObject plus the nested dependency, then computes the key.
#[test]
fn spec_example3_dynamic_nested_key_projection_and_reorder_md5() {
    let bytes = example3_obj().serialize().unwrap();

    let nested_to = TypeObject::Complete(Example3Nested::complete_type_object());
    let nested_id = TypeIdentifier::CompleteTypeId(nested_to.compute_hash());
    let mut registry = TypeRegistry::new();
    registry.register_type_object_with_id(&nested_id, nested_to);
    let dts = DynamicTypeSupport::from_type_object_with_registry(
        TypeObject::Complete(Example3Foo::complete_type_object()),
        &registry,
    )
    .unwrap();

    let dyn_data = deserialize_dynamic_data(&bytes, dts.dynamic_type()).unwrap();
    let handle = dts.compute_key(&dyn_data);
    assert_eq!(
        *handle.value(),
        EXAMPLE3_KEYHASH,
        "Dynamic-path Example 3 KeyHash must match the spec MD5"
    );
}
