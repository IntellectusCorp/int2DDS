use crate::cdr::prim_bulk::{read_prim_vec, NativeBytes};
use crate::cdr::{CdrDeserializer, CdrError, Xcdr2Deserializer};

impl<'a> CdrDeserializer<'a> {
    /// Read a whole run of same-width primitives in one copy.
    ///
    /// Alignment is skipped on an empty run, mirroring the write path: CDR aligns before a
    /// primitive, so a zero-length sequence is just its 4-byte length.
    ///
    /// The copy goes through `CdrInput`, which may be a chain of fragment buffers — hence
    /// the fill closure rather than a direct slice.
    pub(super) fn read_prim_run<T: NativeBytes>(
        &mut self,
        count: usize,
    ) -> Result<Vec<T>, CdrError> {
        let elem_size = std::mem::size_of::<T>();
        if count == 0 {
            return Ok(Vec::new());
        }
        self.align(elem_size);
        // Validates the wire-declared count against the bytes actually left before allocating.
        let count = self.checked_capacity(count, elem_size)?;

        let (endianness, pos) = (self.endianness, self.position);
        let input = &self.input;
        let result = read_prim_vec::<T>(count, endianness, |dst| input.copy_to(pos, dst));
        self.position = pos + count * elem_size;
        Ok(result)
    }

    /// Read a run of octets, then map each one. Used for the element types whose Rust
    /// representation is not its wire byte (`bool` normalization, `char` widening).
    pub(super) fn read_octet_run<T>(
        &mut self,
        count: usize,
        map: impl Fn(u8) -> T,
    ) -> Result<Vec<T>, CdrError> {
        self.check_available(count)?;
        let bytes = self.input.copy_to_vec(self.position, count);
        self.position += count;
        Ok(bytes.into_iter().map(map).collect())
    }

    /// Deserialize byte sequence with length prefix
    pub fn deserialize_byte_sequence(&mut self) -> Result<Vec<u8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize u16 sequence with length prefix
    pub fn deserialize_u16_sequence(&mut self) -> Result<Vec<u16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize u32 sequence with length prefix
    pub fn deserialize_u32_sequence(&mut self) -> Result<Vec<u32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize u64 sequence with length prefix
    pub fn deserialize_u64_sequence(&mut self) -> Result<Vec<u64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i8 sequence with length prefix
    pub fn deserialize_i8_sequence(&mut self) -> Result<Vec<i8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i16 sequence with length prefix
    pub fn deserialize_i16_sequence(&mut self) -> Result<Vec<i16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i32 sequence with length prefix
    pub fn deserialize_i32_sequence(&mut self) -> Result<Vec<i32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i64 sequence with length prefix
    pub fn deserialize_i64_sequence(&mut self) -> Result<Vec<i64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize f32 sequence with length prefix
    pub fn deserialize_f32_sequence(&mut self) -> Result<Vec<f32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize f64 sequence with length prefix
    pub fn deserialize_f64_sequence(&mut self) -> Result<Vec<f64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize bool sequence with length prefix.
    /// The wire may carry any octet; anything nonzero is `true`.
    pub fn deserialize_bool_sequence(&mut self) -> Result<Vec<bool>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b != 0)
    }

    /// Deserialize char sequence with length prefix
    pub fn deserialize_char_sequence(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b as char)
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
    /// XCDR2 counterpart of `CdrDeserializer::read_prim_run`. Backed by a plain slice
    /// rather than a fragment chain, so the fill is a straight `copy_from_slice`.
    pub(super) fn read_prim_run<T: NativeBytes>(
        &mut self,
        count: usize,
    ) -> Result<Vec<T>, CdrError> {
        let elem_size = std::mem::size_of::<T>();
        if count == 0 {
            return Ok(Vec::new());
        }
        self.align(elem_size);
        let count = self.checked_capacity(count, elem_size)?;

        let (endianness, pos) = (self.endianness, self.position);
        let len = count * elem_size;
        let src = &self.data[pos..pos + len];
        let result = read_prim_vec::<T>(count, endianness, |dst| dst.copy_from_slice(src));
        self.position = pos + len;
        Ok(result)
    }

    pub(super) fn read_octet_run<T>(
        &mut self,
        count: usize,
        map: impl Fn(u8) -> T,
    ) -> Result<Vec<T>, CdrError> {
        self.check_available(count)?;
        let start = self.position;
        self.position += count;
        Ok(self.data[start..start + count].iter().map(|&b| map(b)).collect())
    }

    pub fn deserialize_byte_sequence(&mut self) -> Result<Vec<u8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_u16_sequence(&mut self) -> Result<Vec<u16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_u32_sequence(&mut self) -> Result<Vec<u32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_u64_sequence(&mut self) -> Result<Vec<u64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_i8_sequence(&mut self) -> Result<Vec<i8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_i16_sequence(&mut self) -> Result<Vec<i16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_i32_sequence(&mut self) -> Result<Vec<i32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_i64_sequence(&mut self) -> Result<Vec<i64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_f32_sequence(&mut self) -> Result<Vec<f32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_f64_sequence(&mut self) -> Result<Vec<f64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    pub fn deserialize_bool_sequence(&mut self) -> Result<Vec<bool>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b != 0)
    }

    pub fn deserialize_char_sequence(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b as char)
    }

    /// XCDR2: string is non-primitive, so the sequence is preceded by a DHEADER.
    pub fn deserialize_string_sequence(&mut self) -> Result<Vec<String>, CdrError> {
        let _dheader = self.read_dheader()?;
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
        for _ in 0..length {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }

    /// XCDR2: reads the DHEADER that precedes a sequence of non-primitive elements.
    pub fn deserialize_sequence<T, F>(&mut self, mut deserialize_fn: F) -> Result<Vec<T>, CdrError>
    where
        F: FnMut(&mut Self) -> Result<T, CdrError>,
    {
        let _dheader = self.read_dheader()?;
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
