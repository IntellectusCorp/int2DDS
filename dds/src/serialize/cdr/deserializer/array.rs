use crate::serialize::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};

use crate::serialize::{
    from_bytes_f32, from_bytes_f64, from_bytes_i16, from_bytes_i32, from_bytes_i64, from_bytes_u16,
    from_bytes_u32, from_bytes_u64,
};

impl<'a> CdrDeserializer<'a> {
    /// Deserialize fixed-size byte array (no length prefix)
    pub fn deserialize_byte_array(&mut self, size: usize) -> Result<Vec<u8>, CdrError> {
        self.check_available(size)?;

        let result = self.data[self.position..self.position + size].to_vec();
        self.position += size;

        Ok(result)
    }

    /// Deserialize fixed-size u16 array (no length prefix)
    pub fn deserialize_u16_array(&mut self, size: usize) -> Result<Vec<u16>, CdrError> {
        self.align(2);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2].try_into().unwrap();
            let value = from_bytes_u16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    /// Deserialize fixed-size u32 array (no length prefix)
    pub fn deserialize_u32_array(&mut self, size: usize) -> Result<Vec<u32>, CdrError> {
        self.align(4);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4].try_into().unwrap();
            let value = from_bytes_u32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    /// Deserialize fixed-size u64 array (no length prefix)
    pub fn deserialize_u64_array(&mut self, size: usize) -> Result<Vec<u64>, CdrError> {
        self.align(8);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8].try_into().unwrap();
            let value = from_bytes_u64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    /// Deserialize fixed-size i8 array (no length prefix)
    pub fn deserialize_i8_array(&mut self, size: usize) -> Result<Vec<i8>, CdrError> {
        self.check_available(size)?;
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.data[self.position] as i8);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize fixed-size i16 array (no length prefix)
    pub fn deserialize_i16_array(&mut self, size: usize) -> Result<Vec<i16>, CdrError> {
        self.align(2);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2].try_into().unwrap();
            let value = from_bytes_i16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    /// Deserialize fixed-size i32 array (no length prefix)
    pub fn deserialize_i32_array(&mut self, size: usize) -> Result<Vec<i32>, CdrError> {
        self.align(4);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4].try_into().unwrap();
            let value = from_bytes_i32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    /// Deserialize fixed-size i64 array (no length prefix)
    pub fn deserialize_i64_array(&mut self, size: usize) -> Result<Vec<i64>, CdrError> {
        self.align(8);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8].try_into().unwrap();
            let value = from_bytes_i64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    /// Deserialize fixed-size f32 array (no length prefix)
    pub fn deserialize_f32_array(&mut self, size: usize) -> Result<Vec<f32>, CdrError> {
        self.align(4);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4].try_into().unwrap();
            let value = from_bytes_f32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    /// Deserialize fixed-size f64 array (no length prefix)
    pub fn deserialize_f64_array(&mut self, size: usize) -> Result<Vec<f64>, CdrError> {
        self.align(8);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8].try_into().unwrap();
            let value = from_bytes_f64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    /// Deserialize fixed-size bool array (no length prefix)
    pub fn deserialize_bool_array(&mut self, size: usize) -> Result<Vec<bool>, CdrError> {
        self.check_available(size)?;
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.data[self.position] != 0);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize fixed-size char array (no length prefix)
    pub fn deserialize_char_array_fixed(&mut self, size: usize) -> Result<Vec<char>, CdrError> {
        self.check_available(size)?;
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.data[self.position] as char);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize fixed-size string array (no length prefix)
    pub fn deserialize_string_array(&mut self, size: usize) -> Result<Vec<String>, CdrError> {
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }
}

// Xcdr2Deserializer uses the same array deserialization logic
impl<'a> Xcdr2Deserializer<'a> {
    pub fn deserialize_byte_array(&mut self, size: usize) -> Result<Vec<u8>, CdrError> {
        self.check_available(size)?;
        let result = self.data[self.position..self.position + size].to_vec();
        self.position += size;
        Ok(result)
    }

    pub fn deserialize_u16_array(&mut self, size: usize) -> Result<Vec<u16>, CdrError> {
        self.align(2);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2].try_into().unwrap();
            let value = from_bytes_u16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    pub fn deserialize_u32_array(&mut self, size: usize) -> Result<Vec<u32>, CdrError> {
        self.align(4);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4].try_into().unwrap();
            let value = from_bytes_u32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    pub fn deserialize_u64_array(&mut self, size: usize) -> Result<Vec<u64>, CdrError> {
        self.align(8);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8].try_into().unwrap();
            let value = from_bytes_u64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    pub fn deserialize_i8_array(&mut self, size: usize) -> Result<Vec<i8>, CdrError> {
        self.check_available(size)?;
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.data[self.position] as i8);
            self.position += 1;
        }
        Ok(result)
    }

    pub fn deserialize_i16_array(&mut self, size: usize) -> Result<Vec<i16>, CdrError> {
        self.align(2);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(2)?;
            let bytes: [u8; 2] = self.data[self.position..self.position + 2].try_into().unwrap();
            let value = from_bytes_i16(bytes, self.endianness);
            result.push(value);
            self.position += 2;
        }
        Ok(result)
    }

    pub fn deserialize_i32_array(&mut self, size: usize) -> Result<Vec<i32>, CdrError> {
        self.align(4);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4].try_into().unwrap();
            let value = from_bytes_i32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    pub fn deserialize_i64_array(&mut self, size: usize) -> Result<Vec<i64>, CdrError> {
        self.align(8);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8].try_into().unwrap();
            let value = from_bytes_i64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    pub fn deserialize_f32_array(&mut self, size: usize) -> Result<Vec<f32>, CdrError> {
        self.align(4);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(4)?;
            let bytes: [u8; 4] = self.data[self.position..self.position + 4].try_into().unwrap();
            let value = from_bytes_f32(bytes, self.endianness);
            result.push(value);
            self.position += 4;
        }
        Ok(result)
    }

    pub fn deserialize_f64_array(&mut self, size: usize) -> Result<Vec<f64>, CdrError> {
        self.align(8);
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            self.check_available(8)?;
            let bytes: [u8; 8] = self.data[self.position..self.position + 8].try_into().unwrap();
            let value = from_bytes_f64(bytes, self.endianness);
            result.push(value);
            self.position += 8;
        }
        Ok(result)
    }

    pub fn deserialize_bool_array(&mut self, size: usize) -> Result<Vec<bool>, CdrError> {
        self.check_available(size)?;
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.data[self.position] != 0);
            self.position += 1;
        }
        Ok(result)
    }

    pub fn deserialize_char_array_fixed(&mut self, size: usize) -> Result<Vec<char>, CdrError> {
        self.check_available(size)?;
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.data[self.position] as char);
            self.position += 1;
        }
        Ok(result)
    }

    pub fn deserialize_string_array(&mut self, size: usize) -> Result<Vec<String>, CdrError> {
        let mut result = Vec::with_capacity(size);
        for _ in 0..size {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }
}
