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

        let result = self.input.copy_to_vec(self.position, length);
        self.position += length;

        Ok(result)
    }

    /// Deserialize u16 sequence with length prefix
    pub fn deserialize_u16_sequence(&mut self) -> Result<Vec<u16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(2);
        let mut result = Vec::with_capacity(self.checked_capacity(length, 2)?);
        for _ in 0..length {
            self.check_available(2)?;
            let bytes = self.input.read_array::<2>(self.position);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes = self.input.read_array::<4>(self.position);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 8)?);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes = self.input.read_array::<8>(self.position);
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
            result.push(self.input.read_byte(self.position) as i8);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize i16 sequence with length prefix
    pub fn deserialize_i16_sequence(&mut self) -> Result<Vec<i16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.align(2);
        let mut result = Vec::with_capacity(self.checked_capacity(length, 2)?);
        for _ in 0..length {
            self.check_available(2)?;
            let bytes = self.input.read_array::<2>(self.position);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes = self.input.read_array::<4>(self.position);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 8)?);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes = self.input.read_array::<8>(self.position);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
        for _ in 0..length {
            self.check_available(4)?;
            let bytes = self.input.read_array::<4>(self.position);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 8)?);
        for _ in 0..length {
            self.check_available(8)?;
            let bytes = self.input.read_array::<8>(self.position);
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
            result.push(self.input.read_byte(self.position) != 0);
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
            result.push(self.input.read_byte(self.position) as char);
            self.position += 1;
        }
        Ok(result)
    }

    /// Deserialize string sequence with length prefix
    pub fn deserialize_string_sequence(&mut self) -> Result<Vec<String>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
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
        let mut result = Vec::new();
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 2)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 8)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 2)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 8)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 8)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
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
        let mut result = Vec::with_capacity(self.checked_capacity(length, 1)?);
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

#[cfg(test)]
mod robustness_tests {
    use super::*;

    // 4-byte LE length prefix of 0xFFFFFFFF plus a near-empty body: the declared
    // element count cannot fit, so deserialization must Err rather than OOM.
    const HUGE_LEN: [u8; 8] = [0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];

    #[test]
    fn cdr_sequences_reject_huge_length() {
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u16_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u32_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u64_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_f64_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_string_sequence()
            .is_err());
    }

    #[test]
    fn xcdr2_sequences_reject_huge_length() {
        assert!(Xcdr2Deserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u32_sequence()
            .is_err());
        assert!(Xcdr2Deserializer::new_without_header(&HUGE_LEN, true)
            .deserialize_u64_sequence()
            .is_err());
    }

    #[test]
    fn generic_sequence_with_huge_length_errs_without_oom() {
        let mut d = CdrDeserializer::new_without_header(&HUGE_LEN, true);
        let r: Result<Vec<u32>, _> = d.deserialize_sequence(|de| de.deserialize_u32());
        assert!(r.is_err());
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod try_construct_seq_tests {
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
    struct TcWireSeq {
        pub values: Vec<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcSeqDiscard {
        #[dds(bound = 4)]
        pub values: Vec<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcSeqUseDefault {
        #[dds(bound = 4, try_construct = "use_default")]
        pub values: Vec<i32>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct TcSeqTrim {
        #[dds(bound = 4, try_construct = "trim")]
        pub values: Vec<i32>,
    }
    #[test]
    fn test_try_construct_discard_is_error() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqDiscard::deserialize_cdr(&mut deserializer);
        assert!(result.is_err());
    }

    #[test]
    fn test_try_construct_use_default_yields_empty_sequence() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqUseDefault::deserialize_cdr(&mut deserializer).unwrap();
        assert!(result.values.is_empty());
    }

    #[test]
    fn test_try_construct_trim_truncates_sequence() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2, 3, 4, 5, 6, 7, 8] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqTrim::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_try_construct_trim_preserves_within_bound() {
        let bytes = encode_cdr(&TcWireSeq { values: vec![1, 2] });
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = TcSeqTrim::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2]);
    }
}
