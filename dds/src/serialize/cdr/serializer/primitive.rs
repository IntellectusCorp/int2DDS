use super::CdrSerializerCommon;
use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::serialize::{
    to_bytes_f32, to_bytes_f64, to_bytes_i16, to_bytes_i32, to_bytes_i64, to_bytes_u16,
    to_bytes_u32, to_bytes_u64,
};

/// Trait for primitive type serialization
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait PrimitiveSerialize: CdrSerializerCommon {
    /// Serialize boolean
    #[inline]
    fn serialize_bool(&mut self, value: bool) -> Result<(), CdrError> {
        self.buffer_mut().push(if value { 1 } else { 0 });
        Ok(())
    }

    /// Serialize 8-bit signed integer
    #[inline]
    fn serialize_i8(&mut self, value: i8) -> Result<(), CdrError> {
        self.buffer_mut().push(value as u8);
        Ok(())
    }

    /// Serialize 8-bit unsigned integer
    #[inline]
    fn serialize_u8(&mut self, value: u8) -> Result<(), CdrError> {
        self.buffer_mut().push(value);
        Ok(())
    }

    /// Serialize 16-bit signed integer
    #[inline]
    fn serialize_i16(&mut self, value: i16) -> Result<(), CdrError> {
        self.align(2);
        let bytes = to_bytes_i16(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 16-bit unsigned integer
    #[inline]
    fn serialize_u16(&mut self, value: u16) -> Result<(), CdrError> {
        self.align(2);
        let bytes = to_bytes_u16(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit signed integer
    #[inline]
    fn serialize_i32(&mut self, value: i32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_i32(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit unsigned integer
    #[inline]
    fn serialize_u32(&mut self, value: u32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_u32(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit signed integer
    #[inline]
    fn serialize_i64(&mut self, value: i64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_i64(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit unsigned integer
    #[inline]
    fn serialize_u64(&mut self, value: u64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_u64(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit floating point
    #[inline]
    fn serialize_f32(&mut self, value: f32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_f32(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit floating point
    #[inline]
    fn serialize_f64(&mut self, value: f64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_f64(value, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }
}

// Implement PrimitiveSerialize for both serializer types
impl PrimitiveSerialize for CdrSerializer {}
impl PrimitiveSerialize for Xcdr2Serializer {}

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_primitive_tests {
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
}
