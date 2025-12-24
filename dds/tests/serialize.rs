mod common;

use int2dds::{
    dcps::topic::type_support::DdsType,
    serialize::{
        cdr::{
            CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
            XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
        },
        BufferManager, WChar, WString,
    },
};
use std::collections::HashMap;

// Primitive Types Tests - CDR

#[test]
fn test_cdr_bool() {
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    true.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = bool::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, true);

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    false.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = bool::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, false);
}

#[test]
fn test_cdr_i32() {
    let values: [i32; 4] = [-2147483648, -1, 0, 2147483647];
    for value in values {
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }
}

#[test]
fn test_cdr_f64() {
    let values: [f64; 5] = [0.0, -1.0, 1.0, f64::MIN, f64::MAX];
    for value in values {
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = f64::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }
}

// String Tests - CDR

#[test]
fn test_cdr_string() {
    // empty string
    let value = String::new();
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = String::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);

    // ascii string
    let value = "Hello, World!".to_string();
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = String::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// Vec (Sequence) Tests - CDR

#[test]
fn test_cdr_vec_i32() {
    let value: Vec<i32> = vec![1, 2, 3, 4, 5];

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = Vec::<i32>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// Option Tests - CDR

#[test]
fn test_cdr_option() {
    // None
    let value: Option<i32> = None;
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = Option::<i32>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);

    // Some
    let value: Option<i32> = Some(42);
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = Option::<i32>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// Array Tests - CDR

#[test]
fn test_cdr_array_i32() {
    let value: [i32; 5] = [1, 2, 3, 4, 5];

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = <[i32; 5]>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// HashMap Tests - CDR

#[test]
fn test_cdr_hashmap() {
    let mut value: HashMap<String, i32> = HashMap::new();
    value.insert("one".to_string(), 1);
    value.insert("two".to_string(), 2);
    value.insert("three".to_string(), 3);

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = HashMap::<String, i32>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// WChar/WString Tests - CDR

#[test]
fn test_cdr_wchar() {
    let values: [char; 3] = ['A', 'Z', '0'];
    for c in values {
        let value = WChar::from(c);

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = WChar::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.as_char(), c);
    }
}

#[test]
fn test_cdr_wstring() {
    let value = WString::from("Hello, World!");

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = WString::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.as_str(), "Hello, World!");
}

// Endianness Tests - CDR

#[test]
fn test_cdr_little_endian() {
    let value: u32 = 0x12345678;

    let mut serializer = CdrSerializer::new(true); // little endian
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x01);

    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = u32::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_cdr_big_endian() {
    let value: u32 = 0x12345678;

    let mut serializer = CdrSerializer::new(false); // big endian
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x00);

    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = u32::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// XCDR2 Tests

#[test]
fn test_xcdr2_primitives() {
    // bool
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    true.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = bool::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, true);

    // i32
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    42i32.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, 42);
}

#[test]
fn test_xcdr2_string() {
    let value = "Hello, XCDR2!".to_string();

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = String::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// XCDR2 Extensibility Tests

#[test]
fn test_xcdr2_extensibility_final() {
    let value: i32 = 42;

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    // PLAIN_CDR2_LE = 0x0007
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x07);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_xcdr2_extensibility_appendable() {
    let value: i32 = 42;

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    // DCDR2_LE = 0x0009
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x09);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_xcdr2_extensibility_mutable() {
    let value: i32 = 42;

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    // PL_CDR2_LE = 0x000B
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x0B);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// Complex Struct Tests with DdsType derive

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct SimpleStruct {
    pub x: i32,
    pub y: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct AppendableStruct {
    pub id: u32,
    pub name: String,
    pub values: Vec<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct NestedStruct {
    pub inner: SimpleStruct,
    pub count: u32,
}

#[test]
fn test_simple_struct_cdr() {
    let value = SimpleStruct { x: 10, y: 20 };

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = SimpleStruct::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_appendable_struct_xcdr2() {
    let value = AppendableStruct { id: 1, name: "test".to_string(), values: vec![1, 2, 3] };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = AppendableStruct::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_nested_struct_cdr() {
    let value = NestedStruct { inner: SimpleStruct { x: 100, y: 200 }, count: 5 };

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = NestedStruct::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

// Keyed Struct Tests

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct KeyedStruct {
    #[dds(key)]
    pub id: u32,
    pub name: String,
    pub value: f64,
}

#[test]
fn test_keyed_struct_cdr() {
    use int2dds::dcps::topic::type_support::TypeSupport;

    let value = KeyedStruct { id: 42, name: "test".to_string(), value: 3.14 };

    // Full serialization/deserialization
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = KeyedStruct::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);

    // Key serialization via TypeSupport
    let type_support = KeyedStruct::get_type_support();
    let key_bytes = type_support.serialize_key(&value).unwrap();
    assert!(!key_bytes.is_empty());

    // Key deserialization via TypeSupport
    let key_result = type_support.deserialize_key(&key_bytes).unwrap();
    let key_struct = key_result.downcast_ref::<KeyedStruct>().unwrap();
    assert_eq!(key_struct.id, value.id);

    // Compute key (instance handle)
    let instance_handle = type_support.compute_key(&value);
    assert!(type_support.is_compute_key_provided());
    assert_ne!(instance_handle, int2dds::common::instance_handle::InstanceHandle::NIL);
}

// Error Handling Tests

#[test]
fn test_cdr_insufficient_data() {
    // Empty data should fail
    let result = CdrDeserializer::new(&[]);
    assert!(result.is_err());

    // Only 2 bytes (incomplete header)
    let result = CdrDeserializer::new(&[0x00, 0x01]);
    assert!(result.is_err());
}

#[test]
fn test_cdr_invalid_encapsulation() {
    // Invalid encapsulation identifier
    let data = [0xFF, 0xFF, 0x00, 0x00];
    let result = CdrDeserializer::new(&data);
    assert!(result.is_err());
}

// Tuple Struct Tests

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TupleStruct(pub u8);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct MultiFieldTuple(pub u32, pub String);

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct AppendableTuple(pub i32);

#[test]
fn test_tuple_struct_cdr() {
    let value = TupleStruct(42);

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TupleStruct::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.0, value.0);
}

#[test]
fn test_tuple_struct_xcdr2() {
    let value = TupleStruct(123);

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = TupleStruct::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.0, value.0);
}

#[test]
fn test_multi_field_tuple_cdr() {
    let value = MultiFieldTuple(12345, "hello".to_string());

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MultiFieldTuple::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.0, value.0);
    assert_eq!(result.1, value.1);
}

#[test]
fn test_appendable_tuple_xcdr2() {
    let value = AppendableTuple(-42);

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = AppendableTuple::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.0, value.0);
}

#[test]
fn test_tuple_struct_type_support() {
    use int2dds::dcps::topic::type_support::TypeSupport;

    let value = TupleStruct(99);
    let type_support = TupleStruct::get_type_support();

    // Serialize and deserialize
    let serialized = type_support.serialize(&value).unwrap();
    let deserialized = type_support.deserialize(&serialized).unwrap();
    let result = deserialized.downcast_ref::<TupleStruct>().unwrap();
    assert_eq!(result.0, value.0);

    // has_field should work with index as string
    assert!(type_support.has_field("0"));
    assert!(!type_support.has_field("1"));
    assert!(!type_support.has_field("value"));
}
