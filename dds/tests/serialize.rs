mod common;

use int2dds::{
    dcps::topic::type_support::{DdsType, FieldAccessor},
    serialize::{
        cdr::{
            CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
            XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
        },
        BufferManager, DeserializerReader, WChar, WString,
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

#[test]
fn test_cdr_multidim_array_row_major() {
    let value: [[u16; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let expected: &[u8] = &[0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06, 0x00];
    assert_eq!(&bytes[4..], expected);

    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = <[[u16; 3]; 2]>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_xcdr2_multidim_array_row_major() {
    let value: [[u16; 3]; 2] = [[1, 2, 3], [4, 5, 6]];

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let expected: &[u8] = &[
        0x0C, 0x00, 0x00, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04, 0x00, 0x05, 0x00, 0x06,
        0x00,
    ];
    assert_eq!(&bytes[4..], expected);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = <[[u16; 3]; 2]>::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);
}

#[test]
fn test_xcdr2_3d_array_row_major() {
    let value: [[[u8; 2]; 3]; 2] = [[[1, 2], [3, 4], [5, 6]], [[7, 8], [9, 10], [11, 12]]];

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = <[[[u8; 2]; 3]; 2]>::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result, value);

    let payload = &bytes[4..];
    let outer_dheader = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    assert_eq!(outer_dheader as usize, payload.len() - 4);
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
    let serialized = type_support.serialize(&value, None).unwrap();
    let deserialized = type_support.deserialize(&serialized, None).unwrap();
    let result = deserialized.downcast_ref::<TupleStruct>().unwrap();
    assert_eq!(result.0, value.0);

    // has_field should work with index as string
    assert!(type_support.has_field("0"));
    assert!(!type_support.has_field("1"));
    assert!(!type_support.has_field("value"));
}

// =============================================================================
// EMHEADER (Member Header) Tests for Mutable Types
// =============================================================================

use int2dds::serialize::cdr::MemberHeader;
use speedy::Endianness;

#[test]
fn test_emheader_roundtrip_small_length() {
    for (length, expected_lc) in [(1usize, 0u8), (2, 1), (4, 2), (8, 3)] {
        let header = MemberHeader::new(42, length);
        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::LittleEndian).unwrap();

        assert_eq!(buffer.len(), 4, "length={length} must encode as 4 bytes");

        let word = u32::from_le_bytes(buffer[..4].try_into().unwrap());
        assert_eq!((word >> 28) & 0x07, expected_lc as u32);
        assert_eq!(word & 0x0FFF_FFFF, 42);
        assert_eq!(word & 0x8000_0000, 0);

        let (read_header, bytes_consumed) =
            MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();
        assert_eq!(bytes_consumed, 4);
        assert_eq!(read_header.member_id, 42);
        assert_eq!(read_header.member_length as usize, length);
        assert!(!read_header.must_understand);
    }
}

#[test]
fn test_emheader_roundtrip_large_length() {
    // Test EMHEADER with large length (> 64KB, requires LC=4 extended header)
    let large_length: usize = 100_000; // 100KB
    let header = MemberHeader::new(123, large_length);

    let mut buffer = Vec::new();
    header.write(&mut buffer, Endianness::LittleEndian).unwrap();

    // Verify header is 8 bytes for large lengths (4 + 4 for extended length)
    assert_eq!(buffer.len(), 8);

    // Read it back
    let (read_header, bytes_consumed) =
        MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();

    assert_eq!(bytes_consumed, 8);
    assert_eq!(read_header.member_id, 123);
    assert_eq!(read_header.member_length, large_length as u32);
}

#[test]
fn test_emheader_must_understand_flag() {
    // Test must_understand flag (bit 31)
    let header = MemberHeader { member_id: 10, member_length: 50, must_understand: true };

    let mut buffer = Vec::new();
    header.write(&mut buffer, Endianness::LittleEndian).unwrap();

    // Read it back
    let (read_header, _) = MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();

    assert_eq!(read_header.member_id, 10);
    assert_eq!(read_header.member_length, 50);
    assert!(read_header.must_understand);
}

#[test]
fn test_emheader_big_endian() {
    // Test big-endian encoding/decoding
    let header = MemberHeader::new(255, 1000);

    let mut buffer = Vec::new();
    header.write(&mut buffer, Endianness::BigEndian).unwrap();

    let (read_header, _) = MemberHeader::read(&buffer, 0, Endianness::BigEndian).unwrap();

    assert_eq!(read_header.member_id, 255);
    assert_eq!(read_header.member_length, 1000);
}

// =============================================================================
// Mutable Struct Tests
// =============================================================================

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableStruct {
    #[dds(id = 1)]
    pub id: u32,
    #[dds(id = 2)]
    pub name: String,
    #[dds(id = 3)]
    pub value: f64,
}

#[test]
fn test_mutable_struct_xcdr2() {
    let value = MutableStruct { id: 42, name: "test_mutable".to_string(), value: 3.14159 };

    // Serialize
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Verify encapsulation header is PL_CDR2_LE (0x000B)
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x0B);

    // Deserialize
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableStruct::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result.id, 42);
    assert_eq!(result.name, "test_mutable");
    assert!((result.value - 3.14159).abs() < 1e-10);
}

#[test]
fn test_mutable_struct_big_endian() {
    let value = MutableStruct { id: 100, name: "big_endian".to_string(), value: 2.71828 };

    // Serialize in big-endian
    let mut serializer = XcdrSerializer::new(false, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Verify encapsulation header is PL_CDR2_BE (0x000A)
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x0A);

    // Deserialize
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableStruct::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result.id, 100);
    assert_eq!(result.name, "big_endian");
    assert!((result.value - 2.71828).abs() < 1e-10);
}

// =============================================================================
// Mutable Struct with Nested Types
// =============================================================================

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableNestedStruct {
    #[dds(id = 1)]
    pub header: SimpleStruct,
    #[dds(id = 2)]
    pub data: Vec<i32>,
    #[dds(id = 3)]
    pub count: u32,
}

#[test]
fn test_mutable_nested_struct_xcdr2() {
    let value = MutableNestedStruct {
        header: SimpleStruct { x: 10, y: 20 },
        data: vec![1, 2, 3, 4, 5],
        count: 5,
    };

    // Serialize
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Deserialize
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableNestedStruct::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result.header.x, 10);
    assert_eq!(result.header.y, 20);
    assert_eq!(result.data, vec![1, 2, 3, 4, 5]);
    assert_eq!(result.count, 5);
}

// =============================================================================
// Optional Field Tests (Mutable types)
// =============================================================================

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableWithOptional {
    #[dds(id = 1)]
    pub required_field: u32,
    #[dds(id = 2, optional)]
    pub optional_string: Option<String>,
    #[dds(id = 3, optional)]
    pub optional_value: Option<f64>,
}

#[test]
fn test_mutable_optional_all_present() {
    let value = MutableWithOptional {
        required_field: 123,
        optional_string: Some("hello".to_string()),
        optional_value: Some(99.9),
    };

    // Serialize
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Deserialize
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result.required_field, 123);
    assert_eq!(result.optional_string, Some("hello".to_string()));
    assert_eq!(result.optional_value, Some(99.9));
}

#[test]
fn test_mutable_optional_none_values() {
    let value =
        MutableWithOptional { required_field: 456, optional_string: None, optional_value: None };

    // Serialize
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Deserialize
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result.required_field, 456);
    assert_eq!(result.optional_string, None);
    assert_eq!(result.optional_value, None);
}

#[test]
fn test_mutable_optional_mixed() {
    let value = MutableWithOptional {
        required_field: 789,
        optional_string: Some("partial".to_string()),
        optional_value: None,
    };

    // Serialize
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Deserialize
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result.required_field, 789);
    assert_eq!(result.optional_string, Some("partial".to_string()));
    assert_eq!(result.optional_value, None);
}

// =============================================================================
// DHEADER Tests (Appendable/Mutable struct size header)
// =============================================================================

#[test]
fn test_appendable_struct_dheader() {
    // Appendable structs use DHEADER for forward compatibility
    let value =
        AppendableStruct { id: 999, name: "dheader_test".to_string(), values: vec![10, 20, 30] };

    // Serialize
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    // Verify encapsulation header is DCDR2_LE (0x0009)
    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x09);

    // After encap header (4 bytes), there should be a DHEADER (4 bytes)
    // The DHEADER contains the object size
    let dheader_bytes = &bytes[4..8];
    let dheader_size = u32::from_le_bytes([
        dheader_bytes[0],
        dheader_bytes[1],
        dheader_bytes[2],
        dheader_bytes[3],
    ]);

    // Object size should match remaining data
    let object_data_len = bytes.len() - 8; // Total - encap header - dheader
    assert_eq!(dheader_size as usize, object_data_len);

    // Deserialize should still work
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = AppendableStruct::deserialize_xcdr(&mut deserializer).unwrap();

    assert_eq!(result, value);
}

// ============================================================================
// Bounded WString Tests
// ============================================================================

#[derive(DdsType)]
struct BoundedWStringStruct {
    #[dds(bound = 5)]
    pub ws: WString,
}

#[test]
fn test_bounded_wstring_within_bound() {
    let value = BoundedWStringStruct { ws: WString::from("Hello") };
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    CdrSerialize::serialize_cdr(&value, &mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = BoundedWStringStruct::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.ws.as_str(), "Hello");
}

// ============================================================================
// @external (Box<T>) Tests
// ============================================================================

#[test]
fn test_box_cdr_roundtrip() {
    let value: Box<u32> = Box::new(42);
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = Box::<u32>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(*result, 42);
}

#[test]
fn test_box_xcdr_roundtrip() {
    let value: Box<String> = Box::new("test_external".to_string());
    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = Box::<String>::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(*result, "test_external");
}

#[test]
fn test_box_nested_struct_cdr() {
    #[derive(DdsType)]
    struct InnerData {
        pub x: i32,
        pub y: f64,
    }

    let value: Box<InnerData> = Box::new(InnerData { x: 100, y: 3.14 });
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = Box::<InnerData>::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.x, 100);
    assert_eq!(result.y, 3.14);
}

// ============================================================================
// @hashid Tests
// ============================================================================

#[derive(DdsType)]
#[dds_type(extensibility = "Mutable")]
struct HashIdStruct {
    #[dds(hashid)]
    pub name: String,
    #[dds(hashid = "custom_field")]
    pub value: u32,
}

#[test]
fn test_hashid_mutable_roundtrip() {
    let original = HashIdStruct { name: "test_name".to_string(), value: 42 };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    original.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = HashIdStruct::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.name, "test_name");
    assert_eq!(result.value, 42);
}

// ============================================================================
// @autoid Tests
// ============================================================================

#[derive(DdsType)]
#[dds_type(extensibility = "Mutable", autoid = "Hash")]
struct AutoIdHashStruct {
    pub field_a: u32,
    pub field_b: String,
}

#[test]
fn test_autoid_hash_mutable_roundtrip() {
    let original = AutoIdHashStruct { field_a: 123, field_b: "autoid_test".to_string() };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    original.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = AutoIdHashStruct::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.field_a, 123);
    assert_eq!(result.field_b, "autoid_test");
}

#[derive(DdsType)]
#[dds_type(extensibility = "Mutable", autoid = "Sequential")]
struct AutoIdSeqStruct {
    pub first: u32,
    pub second: f64,
}

#[test]
fn test_autoid_sequential_mutable_roundtrip() {
    let original = AutoIdSeqStruct { first: 999, second: 1.5 };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    original.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = AutoIdSeqStruct::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.first, 999);
    assert_eq!(result.second, 1.5);
}

// ============================================================================
// Struct Inheritance Tests
// ============================================================================

#[derive(DdsType)]
struct ParentStruct {
    pub parent_id: u32,
    pub parent_name: String,
}

#[derive(DdsType)]
struct ChildStruct {
    #[dds(parent)]
    pub base: ParentStruct,
    pub child_value: f64,
}

#[test]
fn test_struct_inheritance_cdr() {
    let value = ChildStruct {
        base: ParentStruct { parent_id: 42, parent_name: "parent".to_string() },
        child_value: 3.14,
    };

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = ChildStruct::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.base.parent_id, 42);
    assert_eq!(result.base.parent_name, "parent");
    assert_eq!(result.child_value, 3.14);
}

#[derive(DdsType)]
#[dds_type(extensibility = "Appendable")]
struct AppendableParent2 {
    pub x: i32,
}

#[derive(DdsType)]
#[dds_type(extensibility = "Appendable")]
struct AppendableChild2 {
    #[dds(parent)]
    pub base: AppendableParent2,
    pub y: f64,
}

#[test]
fn test_struct_inheritance_xcdr_appendable() {
    let value = AppendableChild2 { base: AppendableParent2 { x: 100 }, y: 2.71 };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    let payload = &bytes[4..];
    let dheader = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    assert_eq!(dheader, 12);
    let x_val = i32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
    assert_eq!(x_val, 100);
    let y_val = f64::from_le_bytes(payload[8..16].try_into().unwrap());
    assert!((y_val - 2.71).abs() < 1e-15);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = AppendableChild2::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.base.x, 100);
    assert_eq!(result.y, 2.71);
}

// ============================================================================
// XCDR1 MUTABLE (PL_CDR v1) Tests
// ============================================================================

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableV1Simple {
    pub x: u32,
    pub y: u16,
}

#[test]
fn test_xcdr1_mutable_roundtrip() {
    let value = MutableV1Simple { x: 42, y: 7 };

    let mut serializer = CdrSerializer::new_mutable(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    assert_eq!(bytes[0], 0x00);
    assert_eq!(bytes[1], 0x03);

    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MutableV1Simple::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.x, 42);
    assert_eq!(result.y, 7);
}

#[test]
fn test_xcdr1_mutable_wire_format() {
    let value = MutableV1Simple { x: 0x12345678, y: 0xABCD };

    let mut serializer = CdrSerializer::new_mutable(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let payload = &bytes[4..];

    let pid0 = u16::from_le_bytes([payload[0], payload[1]]);
    let len0 = u16::from_le_bytes([payload[2], payload[3]]);
    assert_eq!(pid0 & 0x3FFF, 0);
    assert_eq!(len0, 4);
    assert_eq!(u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]), 0x12345678);

    let pid1 = u16::from_le_bytes([payload[8], payload[9]]);
    let len1 = u16::from_le_bytes([payload[10], payload[11]]);
    assert_eq!(pid1 & 0x3FFF, 1);
    assert_eq!(len1, 2);
    assert_eq!(u16::from_le_bytes([payload[12], payload[13]]), 0xABCD);

    let sentinel_offset = 16;
    let sentinel_pid = u16::from_le_bytes([payload[sentinel_offset], payload[sentinel_offset + 1]]);
    assert_eq!(sentinel_pid & 0x3FFF, 0x3F02 & 0x3FFF);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableV1WithOptional {
    pub required_val: u32,
    #[dds(optional)]
    pub optional_val: Option<u16>,
}

#[test]
fn test_xcdr1_mutable_optional_present() {
    let value = MutableV1WithOptional { required_val: 10, optional_val: Some(20) };

    let mut serializer = CdrSerializer::new_mutable(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MutableV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.required_val, 10);
    assert_eq!(result.optional_val, Some(20));
}

#[test]
fn test_xcdr1_mutable_optional_absent() {
    let value = MutableV1WithOptional { required_val: 99, optional_val: None };

    let mut serializer = CdrSerializer::new_mutable(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MutableV1WithOptional::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.required_val, 99);
    assert_eq!(result.optional_val, None);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableV1WithId {
    #[dds(id = 100)]
    pub a: u32,
    #[dds(id = 200)]
    pub b: u64,
}

#[test]
fn test_xcdr1_mutable_explicit_ids() {
    let value = MutableV1WithId { a: 1, b: 2 };

    let mut serializer = CdrSerializer::new_mutable(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();

    let payload = &bytes[4..];
    let pid0 = u16::from_le_bytes([payload[0], payload[1]]);
    assert_eq!(pid0 & 0x3FFF, 100);
    let pid1_offset = 4 + 4;
    let pid1 = u16::from_le_bytes([payload[pid1_offset], payload[pid1_offset + 1]]);
    assert_eq!(pid1 & 0x3FFF, 200);

    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MutableV1WithId::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.a, 1);
    assert_eq!(result.b, 2);
}

// ============================================================================
// Bitmask Tests
// ============================================================================

#[allow(non_upper_case_globals)]
mod bitmask_tests {
    use super::*;

    #[derive(DdsType)]
    #[dds_type(bitmask, bit_bound = 8)]
    #[repr(u8)]
    enum MyBitmask {
        #[dds(position = 0)]
        Flag0 = 1,
        #[dds(position = 1)]
        Flag1 = 2,
        #[dds(position = 7)]
        Flag7 = 128,
    }

    #[test]
    fn test_bitmask_cdr_roundtrip() {
        let value = MyBitmaskValue::from(MyBitmask::Flag0);
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MyBitmaskValue::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, MyBitmaskValue(1));
        assert!(result.contains(MyBitmaskValue::Flag0));
        assert!(!result.contains(MyBitmaskValue::Flag1));
    }

    #[test]
    fn test_bitmask_combined_flags() {
        let mut value = MyBitmaskValue::empty();
        value.set(MyBitmaskValue::Flag0);
        value.set(MyBitmaskValue::Flag7);
        assert!(value.contains(MyBitmaskValue::Flag0));
        assert!(!value.contains(MyBitmaskValue::Flag1));
        assert!(value.contains(MyBitmaskValue::Flag7));
        assert_eq!(value.bits(), 0b1000_0001);

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = MyBitmaskValue::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_bitmask_bitwise_ops() {
        let a = MyBitmaskValue::from(MyBitmask::Flag0);
        let b = MyBitmaskValue::from(MyBitmask::Flag1);
        let combined = a | b;
        assert!(combined.contains(MyBitmaskValue::Flag0));
        assert!(combined.contains(MyBitmaskValue::Flag1));
        assert_eq!(combined.bits(), 0b11);

        let masked = combined & MyBitmaskValue::from(MyBitmask::Flag0);
        assert!(masked.contains(MyBitmaskValue::Flag0));
        assert!(!masked.contains(MyBitmaskValue::Flag1));
    }

    #[test]
    fn test_bitmask_xcdr_roundtrip() {
        let value = MyBitmaskValue::from(MyBitmask::Flag1) | MyBitmaskValue::from(MyBitmask::Flag7);

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MyBitmaskValue::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }
}

// ============================================================================
// Bitset Tests
// ============================================================================

#[derive(DdsType)]
#[dds_type(bitset)]
struct MyBitset {
    #[dds(bitfield = 3)]
    pub a: u8,
    #[dds(bitfield = 10)]
    pub b: u16,
    #[dds(bitfield = 12)]
    pub c: u32,
}

#[test]
fn test_bitset_cdr_roundtrip() {
    let value = MyBitset { a: 5, b: 1023, c: 4095 };

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MyBitset::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.a, 5);
    assert_eq!(result.b, 1023);
    assert_eq!(result.c, 4095);
}

#[test]
fn test_bitset_zero_values() {
    let value = MyBitset { a: 0, b: 0, c: 0 };

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = MyBitset::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.a, 0);
    assert_eq!(result.b, 0);
    assert_eq!(result.c, 0);
}

#[test]
fn test_bitset_xcdr_roundtrip() {
    let value = MyBitset { a: 7, b: 512, c: 2048 };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MyBitset::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.a, 7);
    assert_eq!(result.b, 512);
    assert_eq!(result.c, 2048);
}

#[derive(DdsType)]
#[dds_type(bitset)]
struct SmallBitset {
    #[dds(bitfield = 1)]
    pub flag: u8,
    #[dds(bitfield = 3)]
    pub mode: u8,
}

#[test]
fn test_small_bitset_u8_wire_type() {
    let value = SmallBitset { flag: 1, mode: 5 };

    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    // 4 bytes encap header + 1 byte u8 = 5 bytes total
    assert_eq!(bytes.len(), 5);

    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = SmallBitset::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.flag, 1);
    assert_eq!(result.mode, 5);
}

// ============================================================================
// KeyHash BE-CDR Golden Tests
// ============================================================================

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct SingleU32Key {
    #[dds(key)]
    pub id: u32,
    pub data: f64,
}

#[test]
fn test_keyhash_u32_big_endian() {
    use int2dds::dcps::topic::type_support::TypeSupport;

    let value = SingleU32Key { id: 42, data: 1.0 };
    let type_support = SingleU32Key::get_type_support();
    let key_bytes = type_support.serialize_key(&value).unwrap();

    assert_eq!(&*key_bytes, &[0x00, 0x00, 0x00, 0x2A]);

    let instance_handle = type_support.compute_key(&value);
    let handle_bytes = instance_handle.value();
    let mut expected = [0u8; 16];
    expected[..4].copy_from_slice(&[0x00, 0x00, 0x00, 0x2A]);
    assert_eq!(handle_bytes, &expected);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct MultiKeyStruct {
    #[dds(key)]
    pub a: u16,
    #[dds(key)]
    pub b: u32,
    pub c: f64,
}

#[test]
fn test_keyhash_multi_key_big_endian_order() {
    use int2dds::dcps::topic::type_support::TypeSupport;

    let value = MultiKeyStruct { a: 1, b: 2, c: 99.0 };
    let type_support = MultiKeyStruct::get_type_support();
    let key_bytes = type_support.serialize_key(&value).unwrap();

    assert_eq!(&*key_bytes, &[0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,]);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct LargeKeyStruct {
    #[dds(key)]
    pub a: u64,
    #[dds(key)]
    pub b: u64,
    #[dds(key)]
    pub c: u64,
}

#[test]
fn test_keyhash_large_key_uses_md5() {
    use int2dds::dcps::topic::type_support::TypeSupport;

    let value = LargeKeyStruct { a: 1, b: 2, c: 3 };
    let type_support = LargeKeyStruct::get_type_support();
    let key_bytes = type_support.serialize_key(&value).unwrap();

    assert_eq!(key_bytes.len(), 24);

    let instance_handle = type_support.compute_key(&value);
    let expected_md5 = md5::compute(&*key_bytes);
    assert_eq!(instance_handle.value(), &expected_md5.0);
}

// ============================================================================
// LC 6/7 EMHEADER Optimization Tests
// ============================================================================

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableWithSeqU32 {
    #[dds(id = 0)]
    pub values: Vec<u32>,
}

#[test]
fn test_lc6_u32_sequence_emheader() {
    use int2dds::serialize::cdr::MemberHeader;

    let value = MutableWithSeqU32 { values: vec![1, 2, 3] };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let payload = &bytes[4..];

    let dheader_size = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let emheader_bytes = &payload[4..];
    let (header, _) =
        MemberHeader::read(emheader_bytes, 0, speedy::Endianness::LittleEndian).unwrap();

    assert_eq!(header.member_id, 0);

    let emh_word = u32::from_le_bytes([
        emheader_bytes[0],
        emheader_bytes[1],
        emheader_bytes[2],
        emheader_bytes[3],
    ]);
    let lc = (emh_word >> 28) & 0x07;
    assert_eq!(lc, 6);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableWithSeqU32::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.values, vec![1, 2, 3]);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
struct MutableWithSeqF64 {
    #[dds(id = 0)]
    pub data: Vec<f64>,
}

#[test]
fn test_lc7_f64_sequence_emheader() {
    use int2dds::serialize::cdr::MemberHeader;

    let value = MutableWithSeqF64 { data: vec![1.0, 2.0] };

    let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_xcdr(&mut serializer).unwrap();

    let bytes = serializer.into_bytes();
    let payload = &bytes[4..];

    let emheader_bytes = &payload[4..];
    let emh_word = u32::from_le_bytes([
        emheader_bytes[0],
        emheader_bytes[1],
        emheader_bytes[2],
        emheader_bytes[3],
    ]);
    let lc = (emh_word >> 28) & 0x07;
    assert_eq!(lc, 7);

    let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
    let result = MutableWithSeqF64::deserialize_xcdr(&mut deserializer).unwrap();
    assert_eq!(result.data, vec![1.0, 2.0]);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcWireSeq {
    pub values: Vec<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcSeqDiscard {
    #[dds(bound = 4)]
    pub values: Vec<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcSeqUseDefault {
    #[dds(bound = 4, try_construct = "use_default")]
    pub values: Vec<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcSeqTrim {
    #[dds(bound = 4, try_construct = "trim")]
    pub values: Vec<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcWireString {
    pub s: String,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcStringTrim {
    #[dds(bound = 5, try_construct = "trim")]
    pub s: String,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct TcStringUseDefault {
    #[dds(bound = 5, try_construct = "use_default")]
    pub s: String,
}

fn encode_cdr<T: CdrSerialize>(value: &T) -> Vec<u8> {
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    value.serialize_cdr(&mut serializer).unwrap();
    serializer.into_bytes()
}

#[test]
fn test_try_construct_discard_is_error() {
    let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TcSeqDiscard::deserialize_cdr(&mut deserializer);
    assert!(result.is_err());
}

#[test]
fn test_try_construct_use_default_yields_empty_sequence() {
    let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TcSeqUseDefault::deserialize_cdr(&mut deserializer).unwrap();
    assert!(result.values.is_empty());
}

#[test]
fn test_try_construct_trim_truncates_sequence() {
    let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TcSeqTrim::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.values, vec![1, 2, 3, 4]);
}

#[test]
fn test_try_construct_trim_preserves_within_bound() {
    let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2] });
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TcSeqTrim::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.values, vec![1, 2]);
}

#[test]
fn test_try_construct_string_trim() {
    let bytes = encode_cdr(&TcWireString { s: "Hello World!".to_string() });
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TcStringTrim::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.s, "Hello");
}

#[test]
fn test_try_construct_string_use_default() {
    let bytes = encode_cdr(&TcWireString { s: "Hello World!".to_string() });
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = TcStringUseDefault::deserialize_cdr(&mut deserializer).unwrap();
    assert!(result.s.is_empty());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct NsLocalCache {
    pub sent_field: i32,
    #[dds(non_serialized)]
    pub local_cache: i64,
    pub other_sent: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct NsTwinWireOnly {
    pub sent_field: i32,
    pub other_sent: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct NsWithDefault {
    pub a: i32,
    #[dds(non_serialized, default = 42)]
    pub cached: i64,
}

#[test]
fn test_non_serialized_wire_omits_field() {
    let value = NsLocalCache { sent_field: 1, local_cache: 9999, other_sent: 2 };
    let bytes_full = encode_cdr(&value);
    let bytes_twin = encode_cdr(&NsTwinWireOnly { sent_field: 1, other_sent: 2 });
    assert_eq!(bytes_full, bytes_twin);
}

#[test]
fn test_non_serialized_roundtrip_restores_default() {
    let value = NsLocalCache { sent_field: 7, local_cache: 9999, other_sent: 3 };
    let bytes = encode_cdr(&value);
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = NsLocalCache::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.sent_field, 7);
    assert_eq!(result.other_sent, 3);
    assert_eq!(result.local_cache, 0);
}

#[test]
fn test_non_serialized_with_default_literal() {
    let value = NsWithDefault { a: 5, cached: 999 };
    let bytes = encode_cdr(&value);
    let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
    let result = NsWithDefault::deserialize_cdr(&mut deserializer).unwrap();
    assert_eq!(result.a, 5);
    assert_eq!(result.cached, 42);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
#[repr(i32)]
enum EnumWithValue {
    First,
    #[dds(value = 100)]
    HundredLit,
    Third,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", ignore_literal_names)]
#[repr(i32)]
enum EnumWithDefaultLiteral {
    Alpha,
    #[dds(default_literal)]
    Beta,
    Gamma,
}

#[test]
fn test_enum_value_attribute_roundtrip() {
    use int2dds::xtypes::HasTypeObject;
    let obj = EnumWithValue::complete_type_object();
    let literals = match obj {
        int2dds::xtypes::CompleteTypeObject::Enum(e) => e.literal_seq,
        _ => panic!("expected enum complete type"),
    };
    assert_eq!(literals[0].common.value, 0);
    assert_eq!(literals[1].common.value, 100);
    assert_eq!(literals[2].common.value, 101);
}

#[test]
fn test_enum_default_literal_flag() {
    use int2dds::xtypes::HasTypeObject;
    let obj = EnumWithDefaultLiteral::complete_type_object();
    let literals = match obj {
        int2dds::xtypes::CompleteTypeObject::Enum(e) => e.literal_seq,
        _ => panic!("expected enum complete type"),
    };
    assert!(!literals[0].common.flags.is_default());
    assert!(literals[1].common.flags.is_default());
    assert!(!literals[2].common.flags.is_default());
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", alias, extensibility = "Final")]
struct MyIntSequence(pub Vec<i32>);

#[test]
fn test_alias_emits_tk_alias_type_object() {
    use int2dds::xtypes::{CompleteTypeObject, HasTypeObject};
    let obj = MyIntSequence::complete_type_object();
    match obj {
        CompleteTypeObject::Alias(_) => {}
        _ => panic!("expected Alias TypeObject"),
    }
}

#[test]
fn test_alias_wire_matches_base_type() {
    let alias_value = MyIntSequence(vec![1, 2, 3]);
    let base_value: Vec<i32> = vec![1, 2, 3];
    let alias_bytes = encode_cdr(&alias_value);
    let mut serializer = CdrSerializer::new(true);
    serializer.write_encapsulation_header().unwrap();
    base_value.serialize_cdr(&mut serializer).unwrap();
    let base_bytes = serializer.into_bytes();
    assert_eq!(alias_bytes, base_bytes);
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", nested, extensibility = "Final")]
struct NestedOnly {
    pub v: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Final")]
struct NotNested {
    pub v: i32,
}

#[test]
fn test_nested_flag_in_type_object() {
    use int2dds::xtypes::{CompleteTypeObject, HasTypeObject};
    let nested = match NestedOnly::complete_type_object() {
        CompleteTypeObject::Struct(s) => s.struct_flags.is_nested(),
        _ => panic!("expected struct"),
    };
    let not_nested = match NotNested::complete_type_object() {
        CompleteTypeObject::Struct(s) => s.struct_flags.is_nested(),
        _ => panic!("expected struct"),
    };
    assert!(nested);
    assert!(!not_nested);
}
