use super::primitive::PrimitiveSerialize;
use super::CdrSerializerCommon;
use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::serialize::{to_bytes_u16, to_bytes_u32};

/// Trait for string and character serialization
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait StringSerialize: CdrSerializerCommon + PrimitiveSerialize {
    /// Serialize 8-bit character (ISO Latin-1)
    fn serialize_char(&mut self, value: char) -> Result<(), CdrError> {
        // CDR char is limited to 8-bit Latin-1
        if value as u32 > 255 {
            return Err(CdrError::InvalidCharacter);
        }
        self.buffer_mut().push(value as u8);
        Ok(())
    }

    /// Serialize wide character (UTF-16)
    fn serialize_wchar16(&mut self, value: char) -> Result<(), CdrError> {
        self.align(2);

        // UTF-16 encoding (only BMP supported)
        if value as u32 > 0xFFFF {
            return Err(CdrError::InvalidWideCharacter);
        }

        let bytes = to_bytes_u16(value as u16, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize wide character (UTF-32)
    fn serialize_wchar32(&mut self, value: char) -> Result<(), CdrError> {
        self.align(4);

        let bytes = to_bytes_u32(value as u32, self.endianness());
        self.buffer_mut().extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize character array (8-bit Latin-1)
    fn serialize_char_array(&mut self, data: &[char]) -> Result<(), CdrError> {
        // Length prefix
        self.serialize_u32(data.len() as u32)?;

        // Character data (8-bit each)
        for &c in data {
            if c as u32 > 255 {
                return Err(CdrError::InvalidCharacter);
            }
            self.buffer_mut().push(c as u8);
        }

        Ok(())
    }

    /// Serialize wide character string (UTF-16)
    fn serialize_wstring16(&mut self, value: &str) -> Result<(), CdrError> {
        // Length (code units, not bytes) - count first without allocating
        let utf16_len = value.encode_utf16().count();
        self.serialize_u32(utf16_len as u32)?;

        // UTF-16 data - iterate directly without intermediate Vec
        self.align(2);
        for code_unit in value.encode_utf16() {
            let bytes = to_bytes_u16(code_unit, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }

        Ok(())
    }

    /// Serialize string
    fn serialize_string(&mut self, value: &str) -> Result<(), CdrError> {
        let bytes = value.as_bytes();

        // String length (including null terminator)
        self.serialize_u32((bytes.len() + 1) as u32)?;

        // String data
        self.buffer_mut().extend_from_slice(bytes);
        self.buffer_mut().push(0); // null terminator

        Ok(())
    }
}

// Implement StringSerialize for both serializer types
impl StringSerialize for CdrSerializer {}
impl StringSerialize for Xcdr2Serializer {}

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_string_tests {
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
}
