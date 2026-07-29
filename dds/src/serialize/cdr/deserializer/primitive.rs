use crate::serialize::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};

use crate::serialize::{
    from_bytes_f32, from_bytes_f64, from_bytes_i16, from_bytes_i32, from_bytes_i64, from_bytes_u16,
    from_bytes_u32, from_bytes_u64,
};

impl<'a> CdrDeserializer<'a> {
    /// Deserialize boolean
    #[inline]
    pub fn deserialize_bool(&mut self) -> Result<bool, CdrError> {
        self.check_available(1)?;
        let value = self.input.read_byte(self.position) != 0;
        self.position += 1;
        Ok(value)
    }

    /// Deserialize 8-bit signed integer
    #[inline]
    pub fn deserialize_i8(&mut self) -> Result<i8, CdrError> {
        self.check_available(1)?;
        let value = self.input.read_byte(self.position) as i8;
        self.position += 1;
        Ok(value)
    }

    /// Deserialize 8-bit unsigned integer
    #[inline]
    pub fn deserialize_u8(&mut self) -> Result<u8, CdrError> {
        self.check_available(1)?;
        let value = self.input.read_byte(self.position);
        self.position += 1;
        Ok(value)
    }

    /// Deserialize 16-bit signed integer
    #[inline]
    pub fn deserialize_i16(&mut self) -> Result<i16, CdrError> {
        self.align(2);
        self.check_available(2)?;

        let bytes = self.input.read_array::<2>(self.position);
        let value = from_bytes_i16(bytes, self.endianness);
        self.position += 2;
        Ok(value)
    }

    /// Deserialize 16-bit unsigned integer
    #[inline]
    pub fn deserialize_u16(&mut self) -> Result<u16, CdrError> {
        self.align(2);
        self.check_available(2)?;

        let bytes = self.input.read_array::<2>(self.position);
        let value = from_bytes_u16(bytes, self.endianness);
        self.position += 2;
        Ok(value)
    }

    /// Deserialize 32-bit signed integer
    #[inline]
    pub fn deserialize_i32(&mut self) -> Result<i32, CdrError> {
        self.align(4);
        self.check_available(4)?;

        let bytes = self.input.read_array::<4>(self.position);
        let value = from_bytes_i32(bytes, self.endianness);
        self.position += 4;
        Ok(value)
    }

    /// Deserialize 32-bit unsigned integer
    #[inline]
    pub fn deserialize_u32(&mut self) -> Result<u32, CdrError> {
        self.align(4);
        self.check_available(4)?;

        let bytes = self.input.read_array::<4>(self.position);
        let value = from_bytes_u32(bytes, self.endianness);
        self.position += 4;
        Ok(value)
    }

    /// Deserialize 64-bit signed integer
    #[inline]
    pub fn deserialize_i64(&mut self) -> Result<i64, CdrError> {
        self.align(8);
        self.check_available(8)?;

        let bytes = self.input.read_array::<8>(self.position);
        let value = from_bytes_i64(bytes, self.endianness);
        self.position += 8;
        Ok(value)
    }

    /// Deserialize 64-bit unsigned integer
    #[inline]
    pub fn deserialize_u64(&mut self) -> Result<u64, CdrError> {
        self.align(8);
        self.check_available(8)?;

        let bytes = self.input.read_array::<8>(self.position);
        let value = from_bytes_u64(bytes, self.endianness);
        self.position += 8;
        Ok(value)
    }

    /// Deserialize 32-bit floating point
    #[inline]
    pub fn deserialize_f32(&mut self) -> Result<f32, CdrError> {
        self.align(4);
        self.check_available(4)?;

        let bytes = self.input.read_array::<4>(self.position);
        let value = from_bytes_f32(bytes, self.endianness);
        self.position += 4;
        Ok(value)
    }

    /// Deserialize 64-bit floating point
    #[inline]
    pub fn deserialize_f64(&mut self) -> Result<f64, CdrError> {
        self.align(8);
        self.check_available(8)?;

        let bytes = self.input.read_array::<8>(self.position);
        let value = from_bytes_f64(bytes, self.endianness);
        self.position += 8;
        Ok(value)
    }
}

// Xcdr2Deserializer uses the same primitive deserialization logic
impl<'a> Xcdr2Deserializer<'a> {
    /// Deserialize boolean
    #[inline]
    pub fn deserialize_bool(&mut self) -> Result<bool, CdrError> {
        self.check_available(1)?;
        let value = self.data[self.position] != 0;
        self.position += 1;
        Ok(value)
    }

    /// Deserialize 8-bit signed integer
    #[inline]
    pub fn deserialize_i8(&mut self) -> Result<i8, CdrError> {
        self.check_available(1)?;
        let value = self.data[self.position] as i8;
        self.position += 1;
        Ok(value)
    }

    /// Deserialize 8-bit unsigned integer
    #[inline]
    pub fn deserialize_u8(&mut self) -> Result<u8, CdrError> {
        self.check_available(1)?;
        let value = self.data[self.position];
        self.position += 1;
        Ok(value)
    }

    /// Deserialize 16-bit signed integer
    #[inline]
    pub fn deserialize_i16(&mut self) -> Result<i16, CdrError> {
        self.align(2);
        self.check_available(2)?;
        let bytes = [self.data[self.position], self.data[self.position + 1]];
        let value = from_bytes_i16(bytes, self.endianness);
        self.position += 2;
        Ok(value)
    }

    /// Deserialize 16-bit unsigned integer
    #[inline]
    pub fn deserialize_u16(&mut self) -> Result<u16, CdrError> {
        self.align(2);
        self.check_available(2)?;
        let bytes = [self.data[self.position], self.data[self.position + 1]];
        let value = from_bytes_u16(bytes, self.endianness);
        self.position += 2;
        Ok(value)
    }

    /// Deserialize 32-bit signed integer
    #[inline]
    pub fn deserialize_i32(&mut self) -> Result<i32, CdrError> {
        self.align(4);
        self.check_available(4)?;
        let bytes = [
            self.data[self.position],
            self.data[self.position + 1],
            self.data[self.position + 2],
            self.data[self.position + 3],
        ];
        let value = from_bytes_i32(bytes, self.endianness);
        self.position += 4;
        Ok(value)
    }

    /// Deserialize 32-bit unsigned integer
    #[inline]
    pub fn deserialize_u32(&mut self) -> Result<u32, CdrError> {
        self.align(4);
        self.check_available(4)?;
        let bytes = [
            self.data[self.position],
            self.data[self.position + 1],
            self.data[self.position + 2],
            self.data[self.position + 3],
        ];
        let value = from_bytes_u32(bytes, self.endianness);
        self.position += 4;
        Ok(value)
    }

    /// Deserialize 64-bit signed integer
    #[inline]
    pub fn deserialize_i64(&mut self) -> Result<i64, CdrError> {
        self.align(8);
        self.check_available(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[self.position..self.position + 8]);
        let value = from_bytes_i64(bytes, self.endianness);
        self.position += 8;
        Ok(value)
    }

    /// Deserialize 64-bit unsigned integer
    #[inline]
    pub fn deserialize_u64(&mut self) -> Result<u64, CdrError> {
        self.align(8);
        self.check_available(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[self.position..self.position + 8]);
        let value = from_bytes_u64(bytes, self.endianness);
        self.position += 8;
        Ok(value)
    }

    /// Deserialize 32-bit floating point
    #[inline]
    pub fn deserialize_f32(&mut self) -> Result<f32, CdrError> {
        self.align(4);
        self.check_available(4)?;
        let bytes = [
            self.data[self.position],
            self.data[self.position + 1],
            self.data[self.position + 2],
            self.data[self.position + 3],
        ];
        let value = from_bytes_f32(bytes, self.endianness);
        self.position += 4;
        Ok(value)
    }

    /// Deserialize 64-bit floating point
    #[inline]
    pub fn deserialize_f64(&mut self) -> Result<f64, CdrError> {
        self.align(8);
        self.check_available(8)?;
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[self.position..self.position + 8]);
        let value = from_bytes_f64(bytes, self.endianness);
        self.position += 8;
        Ok(value)
    }
}
