use crate::serialize::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};

use crate::serialize::{to_bytes_u16, to_bytes_u32};

impl CdrSerializer {
    /// Serialize 8-bit character (ISO Latin-1)
    pub fn serialize_char(&mut self, value: char) -> Result<(), CdrError> {
        // CDR char is limited to 8-bit Latin-1
        if value as u32 > 255 {
            return Err(CdrError::InvalidCharacter);
        }
        self.buffer.push(value as u8);
        // debug!("CDR serialize_char: '{}' (0x{:02X})", value, value as u8);
        Ok(())
    }

    /// Serialize wide character (UTF-16)
    pub fn serialize_wchar16(&mut self, value: char) -> Result<(), CdrError> {
        self.align(2);

        // UTF-16 encoding (only BMP supported)
        if value as u32 > 0xFFFF {
            return Err(CdrError::InvalidWideCharacter);
        }

        let bytes = to_bytes_u16(value as u16, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        // debug!("CDR serialize_wchar16: '{}' (0x{:04X})", value, value as u16);
        Ok(())
    }

    /// Serialize wide character (UTF-32)
    pub fn serialize_wchar32(&mut self, value: char) -> Result<(), CdrError> {
        self.align(4);

        let bytes = to_bytes_u32(value as u32, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        // debug!("CDR serialize_wchar32: '{}' (0x{:08X})", value, value as u32);
        Ok(())
    }

    /// Serialize character array (8-bit Latin-1)
    pub fn serialize_char_array(&mut self, data: &[char]) -> Result<(), CdrError> {
        // Length prefix
        self.serialize_u32(data.len() as u32)?;

        // Character data (8-bit each)
        for &c in data {
            if c as u32 > 255 {
                return Err(CdrError::InvalidCharacter);
            }
            self.buffer.push(c as u8);
        }

        // No alignment needed for char arrays (1-byte elements)
        // Next field will align as needed based on its own alignment requirement

        // debug!("CDR serialize_char_array: {} chars", data.len());
        Ok(())
    }

    /// Serialize wide character string (UTF-16)
    pub fn serialize_wstring16(&mut self, value: &str) -> Result<(), CdrError> {
        // UTF-16 encoding
        let utf16_chars: Vec<u16> = value.encode_utf16().collect();

        // Length (code units, not bytes)
        self.serialize_u32(utf16_chars.len() as u32)?;

        // UTF-16 data
        self.align(2);
        for code_unit in utf16_chars {
            let bytes = to_bytes_u16(code_unit, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }

        // debug!("CDR serialize_wstring16: '{}' ({} code units)", value, utf16_len);
        Ok(())
    }

    /// Serialize string
    pub fn serialize_string(&mut self, value: &str) -> Result<(), CdrError> {
        let bytes = value.as_bytes();

        // String length (including null terminator)
        self.serialize_u32((bytes.len() + 1) as u32)?;

        // String data
        self.buffer.extend_from_slice(bytes);
        self.buffer.push(0); // null terminator

        self.align(4);

        // debug!(
        //     "CDR serialize_string: '{}' (length: {}, bytes: {:02X?})",
        //     value,
        //     bytes.len() + 1,
        //     &bytes[..std::cmp::min(bytes.len(), 32)]
        // );
        Ok(())
    }
}

// Xcdr2Serializer uses the same string serialization logic
impl Xcdr2Serializer {
    /// Serialize 8-bit character (ISO Latin-1)
    pub fn serialize_char(&mut self, value: char) -> Result<(), CdrError> {
        if value as u32 > 255 {
            return Err(CdrError::InvalidCharacter);
        }
        self.buffer.push(value as u8);
        Ok(())
    }

    /// Serialize wide character (UTF-16)
    pub fn serialize_wchar16(&mut self, value: char) -> Result<(), CdrError> {
        self.align(2);
        let code_unit = value as u16;
        let bytes = to_bytes_u16(code_unit, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize wide character (UTF-32)
    pub fn serialize_wchar32(&mut self, value: char) -> Result<(), CdrError> {
        self.align(4);
        let code_point = value as u32;
        let bytes = to_bytes_u32(code_point, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Serialize character array (8-bit Latin-1)
    pub fn serialize_char_array(&mut self, chars: &[char]) -> Result<(), CdrError> {
        self.serialize_u32(chars.len() as u32)?;
        for &ch in chars {
            if ch as u32 > 255 {
                return Err(CdrError::InvalidCharacter);
            }
            self.buffer.push(ch as u8);
        }
        Ok(())
    }

    /// Serialize wide string (UTF-16)
    pub fn serialize_wstring16(&mut self, value: &str) -> Result<(), CdrError> {
        let utf16_chars: Vec<u16> = value.encode_utf16().collect();
        let utf16_len = utf16_chars.len();

        self.serialize_u32(utf16_len as u32)?;
        self.align(2);

        for &code_unit in &utf16_chars {
            let bytes = to_bytes_u16(code_unit, self.endianness);
            self.buffer.extend_from_slice(&bytes);
        }

        Ok(())
    }

    /// Serialize string
    pub fn serialize_string(&mut self, value: &str) -> Result<(), CdrError> {
        let bytes = value.as_bytes();
        self.serialize_u32((bytes.len() + 1) as u32)?;
        self.buffer.extend_from_slice(bytes);
        self.buffer.push(0);
        self.align(4);
        Ok(())
    }
}
