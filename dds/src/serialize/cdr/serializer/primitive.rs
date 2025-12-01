use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::serialize::{
    to_bytes_f32, to_bytes_f64, to_bytes_i16, to_bytes_i32, to_bytes_i64, to_bytes_u16,
    to_bytes_u32, to_bytes_u64,
};

impl CdrSerializer {
    /// Serialize boolean
    pub fn serialize_bool(&mut self, value: bool) -> Result<(), CdrError> {
        self.buffer.push(if value { 1 } else { 0 });
        // debug!("CDR serialize_bool: {} (byte: 0x{:02X})", value, if value { 1 } else { 0 });
        Ok(())
    }

    /// Serialize 8-bit signed integer
    pub fn serialize_i8(&mut self, value: i8) -> Result<(), CdrError> {
        self.buffer.push(value as u8);
        Ok(())
    }

    /// Serialize 8-bit unsigned integer
    pub fn serialize_u8(&mut self, value: u8) -> Result<(), CdrError> {
        self.buffer.push(value);
        Ok(())
    }

    /// Serialize 16-bit signed integer
    pub fn serialize_i16(&mut self, value: i16) -> Result<(), CdrError> {
        self.align(2);
        let bytes = to_bytes_i16(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 16-bit unsigned integer
    pub fn serialize_u16(&mut self, value: u16) -> Result<(), CdrError> {
        self.align(2);
        let bytes = to_bytes_u16(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit signed integer
    pub fn serialize_i32(&mut self, value: i32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_i32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit unsigned integer
    pub fn serialize_u32(&mut self, value: u32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_u32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        // debug!("CDR serialize_u32: {} (bytes: {:02X?})", value, bytes);
        Ok(())
    }

    /// Serialize 64-bit signed integer
    pub fn serialize_i64(&mut self, value: i64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_i64(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit unsigned integer
    pub fn serialize_u64(&mut self, value: u64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_u64(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit floating point
    pub fn serialize_f32(&mut self, value: f32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_f32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit floating point
    pub fn serialize_f64(&mut self, value: f64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_f64(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }
}

// Xcdr2Serializer uses the same primitive serialization logic
impl Xcdr2Serializer {
    /// Serialize boolean
    pub fn serialize_bool(&mut self, value: bool) -> Result<(), CdrError> {
        self.buffer.push(if value { 1 } else { 0 });
        Ok(())
    }

    /// Serialize 8-bit signed integer
    pub fn serialize_i8(&mut self, value: i8) -> Result<(), CdrError> {
        self.buffer.push(value as u8);
        Ok(())
    }

    /// Serialize 8-bit unsigned integer
    pub fn serialize_u8(&mut self, value: u8) -> Result<(), CdrError> {
        self.buffer.push(value);
        Ok(())
    }

    /// Serialize 16-bit signed integer
    pub fn serialize_i16(&mut self, value: i16) -> Result<(), CdrError> {
        self.align(2);
        let bytes = to_bytes_i16(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 16-bit unsigned integer
    pub fn serialize_u16(&mut self, value: u16) -> Result<(), CdrError> {
        self.align(2);
        let bytes = to_bytes_u16(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit signed integer
    pub fn serialize_i32(&mut self, value: i32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_i32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit unsigned integer
    pub fn serialize_u32(&mut self, value: u32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_u32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit signed integer
    pub fn serialize_i64(&mut self, value: i64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_i64(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit unsigned integer
    pub fn serialize_u64(&mut self, value: u64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_u64(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 32-bit floating point
    pub fn serialize_f32(&mut self, value: f32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_f32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize 64-bit floating point
    pub fn serialize_f64(&mut self, value: f64) -> Result<(), CdrError> {
        self.align(8);
        let bytes = to_bytes_f64(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }
}
