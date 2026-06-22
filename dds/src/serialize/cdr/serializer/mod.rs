pub mod array;
pub mod primitive;
pub mod sequence;
pub mod string;

use speedy::Endianness;

use crate::serialize::align_buffer;

/// Common trait for CDR serializers (CdrSerializer and Xcdr2Serializer)
/// This trait abstracts the differences between CDR v1 and XCDR v2 serialization,
/// allowing shared implementation of primitive, array, sequence, and string serialization.
pub trait CdrSerializerCommon {
    /// Get the endianness setting
    fn endianness(&self) -> Endianness;

    /// Get mutable reference to the internal buffer
    fn buffer_mut(&mut self) -> &mut Vec<u8>;

    /// Get reference to the internal buffer
    fn buffer(&self) -> &[u8];

    /// Align buffer to the specified boundary
    /// Note: CdrSerializer uses standard CDR alignment (up to 8 bytes)
    ///       Xcdr2Serializer limits alignment to 4 bytes per XCDR2 spec
    fn align(&mut self, alignment: usize);
}

// Import serializer types from their definition modules
use super::{CdrSerializer, Xcdr2Serializer};

impl CdrSerializerCommon for CdrSerializer {
    #[inline]
    fn endianness(&self) -> Endianness {
        self.endianness
    }

    #[inline]
    fn buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    #[inline]
    fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    fn align(&mut self, alignment: usize) {
        // CDR alignment is relative to data start.
        let header_size = self.header_size;
        let data_len = self.buffer.len().saturating_sub(header_size);
        let aligned_data_len = (data_len + alignment - 1) & !(alignment - 1);
        let target_len = header_size + aligned_data_len;
        if target_len > self.buffer.len() {
            self.buffer.resize(target_len, 0);
        }
    }
}

impl CdrSerializerCommon for Xcdr2Serializer {
    #[inline]
    fn endianness(&self) -> Endianness {
        self.endianness
    }

    #[inline]
    fn buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    #[inline]
    fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    fn align(&mut self, alignment: usize) {
        // XCDR2 standard: limit to 4-byte max alignment to reduce padding
        let actual_alignment = std::cmp::min(alignment, 4);
        align_buffer(&mut self.buffer, actual_alignment);
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_struct_tests {
    use crate::{
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
    fn encode_cdr<T: CdrSerialize>(value: &T) -> Vec<u8> {
        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();
        serializer.into_bytes()
    }
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
        use crate::dcps::topic::type_support::TypeSupport;

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
        assert_ne!(instance_handle, crate::common::instance_handle::InstanceHandle::NIL);
    }

    // Error Handling Tests

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
        use crate::dcps::topic::type_support::TypeSupport;

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
            let value =
                MyBitmaskValue::from(MyBitmask::Flag1) | MyBitmaskValue::from(MyBitmask::Flag7);

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
}
