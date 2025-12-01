use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};

use crate::serialize::{
    to_bytes_f32, to_bytes_f64, to_bytes_i16, to_bytes_i32, to_bytes_i64, to_bytes_u16,
    to_bytes_u32, to_bytes_u64,
};

impl CdrSerializer {
    /// Serialize fixed-size byte array (no length prefix)
    pub fn serialize_byte_array(&mut self, data: &[u8]) -> Result<(), CdrError> {
        // No length prefix for fixed arrays
        self.buffer.extend_from_slice(data);

        // debug!("CDR serialize_byte_array: {} bytes (no length prefix)", data.len());
        Ok(())
    }

    /// Serialize fixed-size u16 array (no length prefix)
    pub fn serialize_u16_array(&mut self, data: &[u16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in data {
            let bytes = to_bytes_u16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u32 array (no length prefix)
    pub fn serialize_u32_array(&mut self, data: &[u32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_u32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u64 array (no length prefix)
    pub fn serialize_u64_array(&mut self, data: &[u64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_u64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i8 array (no length prefix)
    pub fn serialize_i8_array(&mut self, data: &[i8]) -> Result<(), CdrError> {
        for &value in data {
            self.buffer.push(value as u8);
        }
        Ok(())
    }

    /// Serialize fixed-size i16 array (no length prefix)
    pub fn serialize_i16_array(&mut self, data: &[i16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in data {
            let bytes = to_bytes_i16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i32 array (no length prefix)
    pub fn serialize_i32_array(&mut self, data: &[i32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_i32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i64 array (no length prefix)
    pub fn serialize_i64_array(&mut self, data: &[i64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_i64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f32 array (no length prefix)
    pub fn serialize_f32_array(&mut self, data: &[f32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in data {
            let bytes = to_bytes_f32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f64 array (no length prefix)
    pub fn serialize_f64_array(&mut self, data: &[f64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in data {
            let bytes = to_bytes_f64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }
}

// Xcdr2Serializer uses the same array serialization logic
impl Xcdr2Serializer {
    /// Serialize fixed-size byte array (no length prefix)
    pub fn serialize_byte_array(&mut self, bytes: &[u8]) -> Result<(), CdrError> {
        self.buffer.extend_from_slice(bytes);
        Ok(())
    }

    /// Serialize fixed-size u16 array (no length prefix)
    pub fn serialize_u16_array(&mut self, values: &[u16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in values {
            let bytes = to_bytes_u16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u32 array (no length prefix)
    pub fn serialize_u32_array(&mut self, values: &[u32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in values {
            let bytes = to_bytes_u32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size u64 array (no length prefix)
    pub fn serialize_u64_array(&mut self, values: &[u64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in values {
            let bytes = to_bytes_u64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i8 array (no length prefix)
    pub fn serialize_i8_array(&mut self, values: &[i8]) -> Result<(), CdrError> {
        for &value in values {
            self.buffer.push(value as u8);
        }
        Ok(())
    }

    /// Serialize fixed-size i16 array (no length prefix)
    pub fn serialize_i16_array(&mut self, values: &[i16]) -> Result<(), CdrError> {
        self.align(2);
        for &value in values {
            let bytes = to_bytes_i16(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i32 array (no length prefix)
    pub fn serialize_i32_array(&mut self, values: &[i32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in values {
            let bytes = to_bytes_i32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size i64 array (no length prefix)
    pub fn serialize_i64_array(&mut self, values: &[i64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in values {
            let bytes = to_bytes_i64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f32 array (no length prefix)
    pub fn serialize_f32_array(&mut self, values: &[f32]) -> Result<(), CdrError> {
        self.align(4);
        for &value in values {
            let bytes = to_bytes_f32(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }

    /// Serialize fixed-size f64 array (no length prefix)
    pub fn serialize_f64_array(&mut self, values: &[f64]) -> Result<(), CdrError> {
        self.align(8);
        for &value in values {
            let bytes = to_bytes_f64(value, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }
        Ok(())
    }
}
