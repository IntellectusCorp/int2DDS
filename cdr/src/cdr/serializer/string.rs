use super::primitive::PrimitiveSerialize;
use super::CdrSerializerCommon;
use crate::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::{to_bytes_u16, to_bytes_u32};

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
