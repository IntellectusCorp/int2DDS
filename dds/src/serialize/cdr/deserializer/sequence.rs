use crate::serialize::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};

use crate::serialize::{
    from_bytes_f32, from_bytes_f64, from_bytes_i16, from_bytes_i32, from_bytes_i64, from_bytes_u16,
    from_bytes_u32, from_bytes_u64,
};

impl<'a> CdrDeserializer<'a> {
    /// Deserialize byte sequence with length prefix
    pub fn deserialize_byte_sequence(&mut self) -> Result<Vec<u8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;

        let result = self.data[self.position..self.position + length].to_vec();
        self.position += length;

        Ok(result)
    }

    /// Deserialize u16 sequence with length prefix
    pub fn deserialize_u16_sequence(&mut self) -> Result<Vec<u16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(2);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_u16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    /// Deserialize u32 sequence with length prefix
    pub fn deserialize_u32_sequence(&mut self) -> Result<Vec<u32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(4);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_u32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    /// Deserialize u64 sequence with length prefix
    pub fn deserialize_u64_sequence(&mut self) -> Result<Vec<u64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(8);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_u64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    /// Deserialize i8 sequence with length prefix
    pub fn deserialize_i8_sequence(&mut self) -> Result<Vec<i8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.data[self.position] as i8);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize i16 sequence with length prefix
    pub fn deserialize_i16_sequence(&mut self) -> Result<Vec<i16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(2);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_i16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    /// Deserialize i32 sequence with length prefix
    pub fn deserialize_i32_sequence(&mut self) -> Result<Vec<i32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(4);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_i32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    /// Deserialize i64 sequence with length prefix
    pub fn deserialize_i64_sequence(&mut self) -> Result<Vec<i64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(8);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_i64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    /// Deserialize f32 sequence with length prefix
    pub fn deserialize_f32_sequence(&mut self) -> Result<Vec<f32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(4);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_f32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    /// Deserialize f64 sequence with length prefix
    pub fn deserialize_f64_sequence(&mut self) -> Result<Vec<f64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(8);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_f64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    /// Deserialize bool sequence with length prefix
    pub fn deserialize_bool_sequence(&mut self) -> Result<Vec<bool>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.data[self.position] != 0);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize char sequence with length prefix
    pub fn deserialize_char_sequence(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.data[self.position] as char);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize string sequence with length prefix
    pub fn deserialize_string_sequence(&mut self) -> Result<Vec<String>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }

    /// Deserialize sequence of values with length prefix
    pub fn deserialize_sequence<T, F>(&mut self, mut deserialize_fn: F) -> Result<Vec<T>, CdrError>
    where
        F: FnMut(&mut Self) -> Result<T, CdrError>,
    {
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(deserialize_fn(self)?);
        }
        Ok(result)
    }

    /// Deserialize optional value
    pub fn deserialize_optional<T, F>(
        &mut self,
        mut deserialize_fn: F,
    ) -> Result<Option<T>, CdrError>
    where
        F: FnMut(&mut Self) -> Result<T, CdrError>,
    {
        let has_value = self.deserialize_bool()?;
        if has_value {
            Ok(Some(deserialize_fn(self)?))
        } else {
            Ok(None)
        }
    }
}

// Xcdr2Deserializer uses the same sequence deserialization logic
impl<'a> Xcdr2Deserializer<'a> {
    pub fn deserialize_byte_sequence(&mut self) -> Result<Vec<u8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let result = self.data[self.position..self.position + length].to_vec();
        self.position += length;
        Ok(result)
    }

    pub fn deserialize_u16_sequence(&mut self) -> Result<Vec<u16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(2);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_u16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    pub fn deserialize_u32_sequence(&mut self) -> Result<Vec<u32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(4);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_u32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    pub fn deserialize_u64_sequence(&mut self) -> Result<Vec<u64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(8);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_u64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    pub fn deserialize_i8_sequence(&mut self) -> Result<Vec<i8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.data[self.position] as i8);
            self.position += 1;
        }
        Ok(result)
    }

    pub fn deserialize_i16_sequence(&mut self) -> Result<Vec<i16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(2);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_i16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    pub fn deserialize_i32_sequence(&mut self) -> Result<Vec<i32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(4);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_i32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    pub fn deserialize_i64_sequence(&mut self) -> Result<Vec<i64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(8);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_i64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    pub fn deserialize_f32_sequence(&mut self) -> Result<Vec<f32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(4);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_f32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    pub fn deserialize_f64_sequence(&mut self) -> Result<Vec<f64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(8);
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8]
                .try_into()
                .map_err(|_| CdrError::SliceConversionError)?;
            let value = from_bytes_f64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    pub fn deserialize_bool_sequence(&mut self) -> Result<Vec<bool>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.data[self.position] != 0);
            self.position += 1;
        }
        Ok(result)
    }

    pub fn deserialize_char_sequence(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.check_available(length)?;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.data[self.position] as char);
            self.position += 1;
        }
        Ok(result)
    }

    pub fn deserialize_string_sequence(&mut self) -> Result<Vec<String>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }

    pub fn deserialize_sequence<T, F>(&mut self, mut deserialize_fn: F) -> Result<Vec<T>, CdrError>
    where
        F: FnMut(&mut Self) -> Result<T, CdrError>,
    {
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(length);
        for _ in 0..length {
            result.push(deserialize_fn(self)?);
        }
        Ok(result)
    }

    pub fn deserialize_optional<T, F>(
        &mut self,
        mut deserialize_fn: F,
    ) -> Result<Option<T>, CdrError>
    where
        F: FnMut(&mut Self) -> Result<T, CdrError>,
    {
        let has_value = self.deserialize_bool()?;
        if has_value {
            Ok(Some(deserialize_fn(self)?))
        } else {
            Ok(None)
        }
    }
}
