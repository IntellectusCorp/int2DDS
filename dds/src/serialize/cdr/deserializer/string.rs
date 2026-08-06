use crate::serialize::cdr::{try_vec_prealloc, CdrDeserializer, CdrError, Xcdr2Deserializer};
use log::debug;

use crate::serialize::{read_u16, read_u32, read_u8};

impl<'a> CdrDeserializer<'a> {
    /// Internal helper to read u8
    fn read_u8(&mut self) -> Result<u8, CdrError> {
        read_u8(self).map_err(|_| CdrError::InsufficientData)
    }

    /// Internal helper to read u16
    fn read_u16(&mut self) -> Result<u16, CdrError> {
        read_u16(self).map_err(|_| CdrError::InsufficientData)
    }

    /// Internal helper to read u32
    fn read_u32(&mut self) -> Result<u32, CdrError> {
        read_u32(self).map_err(|_| CdrError::InsufficientData)
    }

    /// Deserialize 8-bit character (ISO Latin-1)
    pub fn deserialize_char(&mut self) -> Result<char, CdrError> {
        let byte_val = self.read_u8()?;

        // Convert Latin-1 to char (safe conversion)
        Ok(byte_val as char)
    }

    /// Deserialize wide character (UTF-16)
    pub fn deserialize_wchar16(&mut self) -> Result<char, CdrError> {
        let code_unit = self.read_u16()?;

        // Check for UTF-16 surrogate pair (simple implementation)
        if (0xD800..=0xDFFF).contains(&code_unit) {
            return Err(CdrError::InvalidWideCharacter);
        }

        char::from_u32(code_unit as u32).ok_or(CdrError::InvalidWideCharacter)
    }

    /// Deserialize wide character (UTF-32)
    pub fn deserialize_wchar32(&mut self) -> Result<char, CdrError> {
        let code_point = self.read_u32()?;

        char::from_u32(code_point).ok_or(CdrError::InvalidWideCharacter)
    }

    /// Deserialize character array (8-bit Latin-1)
    pub fn deserialize_char_array(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.read_u32()? as usize;

        self.check_available(length)?;

        let mut chars = try_vec_prealloc(length)?;
        for _ in 0..length {
            let byte_val = self.read_u8()?;
            chars.push(byte_val as char);
        }

        // debug!("CDR deserialize_char_array: {} chars", length);
        Ok(chars)
    }

    /// Deserialize wide character string (UTF-16)
    pub fn deserialize_wstring16(&mut self) -> Result<String, CdrError> {
        let length = self.read_u32()? as usize;

        self.align(2);
        let mut utf16_chars = try_vec_prealloc(self.checked_capacity(length, 2)?)?;

        for _ in 0..length {
            utf16_chars.push(self.read_u16()?);
        }

        String::from_utf16(&utf16_chars).map_err(|_| CdrError::InvalidWideCharacter)
    }

    /// Deserialize string
    pub fn deserialize_string(&mut self) -> Result<String, CdrError> {
        let length = self.deserialize_u32()? as usize;

        if length == 0 {
            debug!("CDR deserialize_string: empty string (length: 0)");
            return Ok(String::new());
        }

        self.check_available(length)?;

        // String data (including null terminator). Borrowed on the contiguous
        // path (no copy); only chained input gathers into an owned buffer.
        let string_data = self.input.bytes(self.position, length);
        self.position += length;

        // Remove null terminator if present
        let string_bytes = if string_data.last() == Some(&0) {
            &string_data[..string_data.len() - 1]
        } else {
            &string_data[..]
        };

        // Optimized: validate UTF-8 without copying, then convert to String
        let result =
            std::str::from_utf8(string_bytes).map_err(|_| CdrError::InvalidString)?.to_string();
        // debug!(
        //     "CDR deserialize_string: '{}' (length: {}, bytes: {:02X?})",
        //     result,
        //     length,
        //     &string_bytes[..std::cmp::min(string_bytes.len(), 32)]
        // );
        Ok(result)
    }
}

// Xcdr2Deserializer uses the same string deserialization logic
impl<'a> Xcdr2Deserializer<'a> {
    /// Internal helper to read u8
    fn read_u8(&mut self) -> Result<u8, CdrError> {
        use crate::serialize::read_u8;
        read_u8(self).map_err(|_| CdrError::InsufficientData)
    }

    /// Internal helper to read u16
    fn read_u16(&mut self) -> Result<u16, CdrError> {
        use crate::serialize::read_u16;
        read_u16(self).map_err(|_| CdrError::InsufficientData)
    }

    /// Internal helper to read u32
    fn read_u32(&mut self) -> Result<u32, CdrError> {
        use crate::serialize::read_u32;
        read_u32(self).map_err(|_| CdrError::InsufficientData)
    }

    /// Deserialize 8-bit character (ISO Latin-1)
    pub fn deserialize_char(&mut self) -> Result<char, CdrError> {
        let byte_val = self.read_u8()?;
        Ok(byte_val as char)
    }

    /// Deserialize wide character (UTF-16)
    pub fn deserialize_wchar16(&mut self) -> Result<char, CdrError> {
        let code_unit = self.read_u16()?;
        if (0xD800..=0xDFFF).contains(&code_unit) {
            return Err(CdrError::InvalidWideCharacter);
        }
        char::from_u32(code_unit as u32).ok_or(CdrError::InvalidWideCharacter)
    }

    /// Deserialize wide character (UTF-32)
    pub fn deserialize_wchar32(&mut self) -> Result<char, CdrError> {
        let code_point = self.read_u32()?;
        char::from_u32(code_point).ok_or(CdrError::InvalidWideCharacter)
    }

    /// Deserialize character array (8-bit Latin-1)
    pub fn deserialize_char_array(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.read_u32()? as usize;
        self.check_available(length)?;
        let mut chars = try_vec_prealloc(length)?;
        for _ in 0..length {
            let byte_val = self.read_u8()?;
            chars.push(byte_val as char);
        }
        Ok(chars)
    }

    /// Deserialize wide character string (UTF-16)
    pub fn deserialize_wstring16(&mut self) -> Result<String, CdrError> {
        let length = self.read_u32()? as usize;
        self.align(2);
        let mut utf16_chars = try_vec_prealloc(self.checked_capacity(length, 2)?)?;
        for _ in 0..length {
            utf16_chars.push(self.read_u16()?);
        }
        String::from_utf16(&utf16_chars).map_err(|_| CdrError::InvalidWideCharacter)
    }

    /// Deserialize string
    pub fn deserialize_string(&mut self) -> Result<String, CdrError> {
        let length = self.deserialize_u32()? as usize;
        if length == 0 {
            return Ok(String::new());
        }
        self.check_available(length)?;
        let string_data = &self.data[self.position..self.position + length];
        self.position += length;
        let string_bytes = if string_data.last() == Some(&0) {
            &string_data[..string_data.len() - 1]
        } else {
            string_data
        };
        let result =
            std::str::from_utf8(string_bytes).map_err(|_| CdrError::InvalidString)?.to_string();
        Ok(result)
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod try_construct_string_tests {
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
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcWireString {
        pub s: String,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcStringTrim {
        #[dds(bound = 5, try_construct = "trim")]
        pub s: String,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcStringUseDefault {
        #[dds(bound = 5, try_construct = "use_default")]
        pub s: String,
    }
    #[test]
    fn test_try_construct_string_trim() {
        let bytes = encode_cdr(&TcWireString { s: "Hello World!".to_string() });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcStringTrim::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.s, "Hello");
    }

    #[test]
    fn test_try_construct_string_use_default() {
        let bytes = encode_cdr(&TcWireString { s: "Hello World!".to_string() });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcStringUseDefault::deserialize_cdr(&mut deserializer).unwrap();
        assert!(result.s.is_empty());
    }
}
