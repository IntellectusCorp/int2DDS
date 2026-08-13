use super::string::StringSerialize;
use super::CdrSerializerCommon;
use crate::cdr::prim_bulk::{extend_prim_slice, NativeBytes};
use crate::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};

/// Append a whole fixed-size array in one bulk copy.
///
/// Same shape as the sequence path minus the length prefix; see `write_prim_seq` for why
/// alignment is skipped on an empty run.
fn write_prim_array<S, T>(ser: &mut S, data: &[T]) -> Result<(), CdrError>
where
    S: ArraySerialize + ?Sized,
    T: NativeBytes,
{
    if data.is_empty() {
        return Ok(());
    }
    ser.align(std::mem::size_of::<T>());
    let endianness = ser.endianness();
    extend_prim_slice(ser.buffer_mut(), data, endianness);
    Ok(())
}

/// Trait for fixed-size array serialization (no length prefix)
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait ArraySerialize: CdrSerializerCommon + StringSerialize {
    /// Serialize fixed-size byte array (no length prefix)
    fn serialize_byte_array(&mut self, data: &[u8]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size u16 array (no length prefix)
    fn serialize_u16_array(&mut self, data: &[u16]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size u32 array (no length prefix)
    fn serialize_u32_array(&mut self, data: &[u32]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size u64 array (no length prefix)
    fn serialize_u64_array(&mut self, data: &[u64]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size i8 array (no length prefix)
    fn serialize_i8_array(&mut self, data: &[i8]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size i16 array (no length prefix)
    fn serialize_i16_array(&mut self, data: &[i16]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size i32 array (no length prefix)
    fn serialize_i32_array(&mut self, data: &[i32]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size i64 array (no length prefix)
    fn serialize_i64_array(&mut self, data: &[i64]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size f32 array (no length prefix)
    fn serialize_f32_array(&mut self, data: &[f32]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size f64 array (no length prefix)
    fn serialize_f64_array(&mut self, data: &[f64]) -> Result<(), CdrError> {
        write_prim_array(self, data)
    }

    /// Serialize fixed-size bool array (no length prefix).
    /// See `serialize_bool_sequence` for why `bool` stays off the bulk path.
    fn serialize_bool_array(&mut self, data: &[bool]) -> Result<(), CdrError> {
        let buffer = self.buffer_mut();
        buffer.reserve(data.len());
        buffer.extend(data.iter().map(|&b| b as u8));
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
