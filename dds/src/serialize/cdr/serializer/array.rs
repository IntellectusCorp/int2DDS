use super::string::StringSerialize;
use super::CdrSerializerCommon;
use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::serialize::{
    to_bytes_f32, to_bytes_f64, to_bytes_i16, to_bytes_i32, to_bytes_i64, to_bytes_u16,
    to_bytes_u32, to_bytes_u64,
};

/// Trait for fixed-size array serialization (no length prefix)
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait ArraySerialize: CdrSerializerCommon + StringSerialize {
    /// Serialize fixed-size byte array (no length prefix)
    fn serialize_byte_array(&mut self, data: &[u8]) -> Result<(), CdrError> {
        self.buffer_mut().extend_from_slice(data);
        Ok(())
    }

    /// Serialize fixed-size u16 array (no length prefix)
    fn serialize_u16_array(&mut self, data: &[u16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in data {
            let bytes = to_bytes_u16(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u32 array (no length prefix)
    fn serialize_u32_array(&mut self, data: &[u32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_u32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u64 array (no length prefix)
    fn serialize_u64_array(&mut self, data: &[u64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_u64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i8 array (no length prefix)
    fn serialize_i8_array(&mut self, data: &[i8]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize fixed-size i16 array (no length prefix)
    fn serialize_i16_array(&mut self, data: &[i16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in data {
            let bytes = to_bytes_i16(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i32 array (no length prefix)
    fn serialize_i32_array(&mut self, data: &[i32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_i32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i64 array (no length prefix)
    fn serialize_i64_array(&mut self, data: &[i64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_i64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f32 array (no length prefix)
    fn serialize_f32_array(&mut self, data: &[f32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_f32(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f64 array (no length prefix)
    fn serialize_f64_array(&mut self, data: &[f64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_f64(value, self.endianness());
            self.buffer_mut().extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size bool array (no length prefix)
    fn serialize_bool_array(&mut self, data: &[bool]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer_mut().push(if value { 1 } else { 0 });
        }
        Ok(())
    }

    /// Serialize fixed-size char array (no length prefix)
    fn serialize_char_array_fixed(&mut self, data: &[char]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize fixed-size string array (no length prefix)
    fn serialize_string_array(&mut self, data: &[String]) -> Result<(), CdrError> {
        for value in data {
            self.serialize_string(value)?;
        }
        Ok(())
    }
}

// Implement ArraySerialize for both serializer types
impl ArraySerialize for CdrSerializer {}
impl ArraySerialize for Xcdr2Serializer {}
