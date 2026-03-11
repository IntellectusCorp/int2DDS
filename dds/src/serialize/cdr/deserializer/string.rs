use crate::serialize::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};
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

        let mut chars = Vec::with_capacity(length);
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
        let mut utf16_chars = Vec::with_capacity(length);

        for _ in 0..length {
            utf16_chars.push(self.read_u16()?);
        }

        // No trailing align(4) here - the next field's read_u32/read_u16 etc.
        // handles its own alignment. Adding align(4) here would over-read
        // past the struct boundary when wstring is the last field.

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

        // String data (including null terminator)
        let string_data = &self.data[self.position..self.position + length];
        self.position += length;

        // Remove null terminator if present
        let string_bytes = if string_data.last() == Some(&0) {
            &string_data[..string_data.len() - 1]
        } else {
            string_data
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
        let mut chars = Vec::with_capacity(length);
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
        let mut utf16_chars = Vec::with_capacity(length);
        for _ in 0..length {
            utf16_chars.push(self.read_u16()?);
        }
        // No trailing align(4) - next field handles its own alignment.
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
