use std::convert::TryFrom;

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

impl CdrSerializer {
    /// Serialize byte array with length prefix (for sequences)
    pub fn serialize_byte_sequence(&mut self, data: &[u8]) -> Result<(), CdrError> {
        // Sequence length
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;

        // Sequence data
        self.buffer.extend_from_slice(data);

        // No alignment needed for byte sequences (1-byte elements)
        // Next field will align as needed based on its own alignment requirement

        Ok(())
    }

    /// Serialize u16 sequence with length prefix
    pub fn serialize_u16_sequence(&mut self, data: &[u16]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(2);
        for &value in data {
            let bytes = to_bytes_u16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize u32 sequence with length prefix
    pub fn serialize_u32_sequence(&mut self, data: &[u32]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        for &value in data {
            let bytes = to_bytes_u32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize u64 sequence with length prefix
    pub fn serialize_u64_sequence(&mut self, data: &[u64]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        for &value in data {
            let bytes = to_bytes_u64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i8 sequence with length prefix
    pub fn serialize_i8_sequence(&mut self, data: &[i8]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        for &value in data {
            self.buffer.push(value as u8);
        }
        Ok(())
    }

    /// Serialize i16 sequence with length prefix
    pub fn serialize_i16_sequence(&mut self, data: &[i16]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(2);
        for &value in data {
            let bytes = to_bytes_i16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i32 sequence with length prefix
    pub fn serialize_i32_sequence(&mut self, data: &[i32]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        for &value in data {
            let bytes = to_bytes_i32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i64 sequence with length prefix
    pub fn serialize_i64_sequence(&mut self, data: &[i64]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        for &value in data {
            let bytes = to_bytes_i64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize f32 sequence with length prefix
    pub fn serialize_f32_sequence(&mut self, data: &[f32]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        for &value in data {
            let bytes = to_bytes_f32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize f64 sequence with length prefix
    pub fn serialize_f64_sequence(&mut self, data: &[f64]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        for &value in data {
            let bytes = to_bytes_f64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

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

// Xcdr2Serializer uses the same sequence serialization logic
impl Xcdr2Serializer {
    /// Serialize byte sequence with length prefix
    pub fn serialize_byte_sequence(&mut self, bytes: &[u8]) -> Result<(), CdrError> {
        let length = checked_length(bytes.len())?;
        self.serialize_u32(length)?;
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// Serialize u16 sequence with length prefix
    pub fn serialize_u16_sequence(&mut self, values: &[u16]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(2);
        for &value in values {
            let bytes = to_bytes_u16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize u32 sequence with length prefix
    pub fn serialize_u32_sequence(&mut self, values: &[u32]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        for &value in values {
            let bytes = to_bytes_u32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize u64 sequence with length prefix
    pub fn serialize_u64_sequence(&mut self, values: &[u64]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        for &value in values {
            let bytes = to_bytes_u64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i8 sequence with length prefix
    pub fn serialize_i8_sequence(&mut self, values: &[i8]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        for &value in values {
            self.buffer.push(value as u8);
        }
        Ok(())
    }

    /// Serialize i16 sequence with length prefix
    pub fn serialize_i16_sequence(&mut self, values: &[i16]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(2);
        for &value in values {
            let bytes = to_bytes_i16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i32 sequence with length prefix
    pub fn serialize_i32_sequence(&mut self, values: &[i32]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        for &value in values {
            let bytes = to_bytes_i32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize i64 sequence with length prefix
    pub fn serialize_i64_sequence(&mut self, values: &[i64]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        for &value in values {
            let bytes = to_bytes_i64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize f32 sequence with length prefix
    pub fn serialize_f32_sequence(&mut self, values: &[f32]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(4);
        for &value in values {
            let bytes = to_bytes_f32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize f64 sequence with length prefix
    pub fn serialize_f64_sequence(&mut self, values: &[f64]) -> Result<(), CdrError> {
        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        self.align(8);
        for &value in values {
            let bytes = to_bytes_f64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize sequence of values with length prefix
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
}
