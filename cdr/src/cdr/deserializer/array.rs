use crate::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};

// Fixed-size arrays carry no length prefix, so each of these is the sequence body read
// with the element count supplied by the caller. The bulk machinery is shared with
// deserializer/sequence.rs — see `read_prim_run` there.

impl<'a> CdrDeserializer<'a> {
    /// Deserialize fixed-size byte array (no length prefix)
    pub fn deserialize_byte_array(&mut self, size: usize) -> Result<Vec<u8>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size u16 array (no length prefix)
    pub fn deserialize_u16_array(&mut self, size: usize) -> Result<Vec<u16>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size u32 array (no length prefix)
    pub fn deserialize_u32_array(&mut self, size: usize) -> Result<Vec<u32>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size u64 array (no length prefix)
    pub fn deserialize_u64_array(&mut self, size: usize) -> Result<Vec<u64>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i8 array (no length prefix)
    pub fn deserialize_i8_array(&mut self, size: usize) -> Result<Vec<i8>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i16 array (no length prefix)
    pub fn deserialize_i16_array(&mut self, size: usize) -> Result<Vec<i16>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i32 array (no length prefix)
    pub fn deserialize_i32_array(&mut self, size: usize) -> Result<Vec<i32>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i64 array (no length prefix)
    pub fn deserialize_i64_array(&mut self, size: usize) -> Result<Vec<i64>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size f32 array (no length prefix)
    pub fn deserialize_f32_array(&mut self, size: usize) -> Result<Vec<f32>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size f64 array (no length prefix)
    pub fn deserialize_f64_array(&mut self, size: usize) -> Result<Vec<f64>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size bool array (no length prefix)
    pub fn deserialize_bool_array(&mut self, size: usize) -> Result<Vec<bool>, CdrError> {
        self.read_octet_run(size, |b| b != 0)
    }

    /// Deserialize fixed-size char array (no length prefix)
    pub fn deserialize_char_array_fixed(&mut self, size: usize) -> Result<Vec<char>, CdrError> {
        self.read_octet_run(size, |b| b as char)
    }

    /// Deserialize fixed-size string array (no length prefix)
    pub fn deserialize_string_array(&mut self, size: usize) -> Result<Vec<String>, CdrError> {
        let mut result = Vec::with_capacity(self.checked_capacity(size, 4)?);
        for _ in 0..size {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }
}

// Xcdr2Deserializer uses the same array deserialization logic
impl<'a> Xcdr2Deserializer<'a> {
    pub fn deserialize_byte_array(&mut self, size: usize) -> Result<Vec<u8>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_u16_array(&mut self, size: usize) -> Result<Vec<u16>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_u32_array(&mut self, size: usize) -> Result<Vec<u32>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_u64_array(&mut self, size: usize) -> Result<Vec<u64>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_i8_array(&mut self, size: usize) -> Result<Vec<i8>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_i16_array(&mut self, size: usize) -> Result<Vec<i16>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_i32_array(&mut self, size: usize) -> Result<Vec<i32>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_i64_array(&mut self, size: usize) -> Result<Vec<i64>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_f32_array(&mut self, size: usize) -> Result<Vec<f32>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_f64_array(&mut self, size: usize) -> Result<Vec<f64>, CdrError> {
        self.read_prim_run(size)
    }

    pub fn deserialize_bool_array(&mut self, size: usize) -> Result<Vec<bool>, CdrError> {
        self.read_octet_run(size, |b| b != 0)
    }

    pub fn deserialize_char_array_fixed(&mut self, size: usize) -> Result<Vec<char>, CdrError> {
        self.read_octet_run(size, |b| b as char)
    }

    pub fn deserialize_string_array(&mut self, size: usize) -> Result<Vec<String>, CdrError> {
        let mut result = Vec::with_capacity(self.checked_capacity(size, 4)?);
        for _ in 0..size {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }
}
