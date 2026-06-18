use std::convert::TryFrom;

use super::primitive::PrimitiveSerialize;
use super::string::StringSerialize;
use super::CdrSerializerCommon;
use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::serialize::{
    to_bytes_f32, to_bytes_f64, to_bytes_i16, to_bytes_i32, to_bytes_i64, to_bytes_u16,
    to_bytes_u32, to_bytes_u64, SerializationError,
};

fn checked_length(len: usize) -> Result<u32, CdrError> {
    u32::try_from(len).map_err(|_| {
        SerializationError::SerializationError("Sequence length exceeds u32::MAX".to_string())
    })
}

/// Trait for sequence serialization (with length prefix)
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait SequenceSerialize: CdrSerializerCommon + PrimitiveSerialize + StringSerialize {
    /// Serialize byte array with length prefix (for sequences)
    fn serialize_byte_sequence(&mut self, data: &[u8]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.buffer_mut().reserve(data.len());
        self.buffer_mut().extend_from_slice(data);
        Ok(())
    }

    /// Serialize u16 sequence with length prefix
    fn serialize_u16_sequence(&mut self, data: &[u16]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(2);
        self.buffer_mut().reserve(data.len() * 2);
        for &value in data {
            let bytes = to_bytes_u16(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize u32 sequence with length prefix
    fn serialize_u32_sequence(&mut self, data: &[u32]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        self.buffer_mut().reserve(data.len() * 4);
        for &value in data {
            let bytes = to_bytes_u32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize u64 sequence with length prefix
    fn serialize_u64_sequence(&mut self, data: &[u64]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        self.buffer_mut().reserve(data.len() * 8);
        for &value in data {
            let bytes = to_bytes_u64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i8 sequence with length prefix
    fn serialize_i8_sequence(&mut self, data: &[i8]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.buffer_mut().reserve(data.len());
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize i16 sequence with length prefix
    fn serialize_i16_sequence(&mut self, data: &[i16]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(2);
        self.buffer_mut().reserve(data.len() * 2);
        for &value in data {
            let bytes = to_bytes_i16(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i32 sequence with length prefix
    fn serialize_i32_sequence(&mut self, data: &[i32]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        self.buffer_mut().reserve(data.len() * 4);
        for &value in data {
            let bytes = to_bytes_i32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i64 sequence with length prefix
    fn serialize_i64_sequence(&mut self, data: &[i64]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        self.buffer_mut().reserve(data.len() * 8);
        for &value in data {
            let bytes = to_bytes_i64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize f32 sequence with length prefix
    fn serialize_f32_sequence(&mut self, data: &[f32]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        self.buffer_mut().reserve(data.len() * 4);
        for &value in data {
            let bytes = to_bytes_f32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize f64 sequence with length prefix
    fn serialize_f64_sequence(&mut self, data: &[f64]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        self.buffer_mut().reserve(data.len() * 8);
        for &value in data {
            let bytes = to_bytes_f64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize bool sequence with length prefix
    fn serialize_bool_sequence(&mut self, data: &[bool]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.buffer_mut().reserve(data.len());
        for &value in data {
            self.buffer_mut().push(if value { 1 } else { 0 });
        }
        Ok(())
    }

    /// Serialize char sequence with length prefix
    fn serialize_char_sequence(&mut self, data: &[char]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.buffer_mut().reserve(data.len());
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize string sequence with length prefix
    fn serialize_string_sequence(&mut self, data: &[String]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        for value in data {
            self.serialize_string(value)?;
        }
        Ok(())
    }
}

// Implement SequenceSerialize for both serializer types
impl SequenceSerialize for CdrSerializer {}
impl SequenceSerialize for Xcdr2Serializer {}

// Generic serialize_sequence and serialize_optional methods need to stay as inherent impl
// because they use generic parameters with Self type bounds
impl CdrSerializer {
    /// Serialize sequence of values with length prefix
    pub fn serialize_sequence<T, F>(
        &mut self,
        sequence: &[T],
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        let length = checked_length(sequence.len())?;
        self.serialize_u32(length)?;
        for item in sequence {
            serialize_fn(self, item)?;
        }
        Ok(())
    }

    /// Serialize optional value
    pub fn serialize_optional<T, F>(
        &mut self,
        value: &Option<T>,
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        match value {
            Some(val) => {
                self.serialize_bool(true)?;
                serialize_fn(self, val)?;
            }
            None => self.serialize_bool(false)?,
        }
        Ok(())
    }
}

impl Xcdr2Serializer {
    /// Serialize sequence of non-primitive values with length prefix (XCDR2).
    pub fn serialize_sequence<T, F>(
        &mut self,
        values: &[T],
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        for value in values {
            serialize_fn(self, value)?;
        }
        Ok(())
    }

    /// Serialize optional value
    pub fn serialize_optional<T, F>(
        &mut self,
        value: &Option<T>,
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        match value {
            Some(val) => {
                self.serialize_bool(true)?;
                serialize_fn(self, val)?;
            }
            None => self.serialize_bool(false)?,
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_sequence_tests {
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
}
