//! The read half of the CDR wire rules, written once.
//!
//! The two codecs differ on the read path in exactly two places: how far a member is
//! aligned ([`DeserializerReader::align`], which XCDR2 caps at 4) and whether a
//! collection of non-primitive elements carries a DHEADER
//! ([`CdrRead::read_collection_dheader`]). Everything below is shared. Each
//! deserializer re-exposes it through inherent forwarders, so the public call surface
//! is unchanged and the trait itself never leaves the crate.

use std::borrow::Cow;

use crate::cdr::prim_bulk::{read_prim_vec, NativeBytes};
use crate::cdr::CdrError;
use crate::{
    from_bytes_f32, from_bytes_f64, from_bytes_i16, from_bytes_i32, from_bytes_i64, from_bytes_u16,
    from_bytes_u32, from_bytes_u64, DeserializerReader,
};

pub(crate) trait CdrRead: DeserializerReader<Error = CdrError> + Sized {
    /// The bytes at `offset`, borrowed when the input is one contiguous slice and
    /// gathered into an owned buffer only when it is a chain of fragment buffers.
    ///
    /// Both implementations index directly, so callers must `check_available` first.
    fn bytes_at(&self, offset: usize, len: usize) -> Cow<'_, [u8]>;

    /// XCDR2 precedes a collection of non-primitive elements with a DHEADER; XCDR1
    /// does not.
    fn read_collection_dheader(&mut self) -> Result<(), CdrError> {
        Ok(())
    }

    /// Validate a wire-declared count against remaining bytes before allocating.
    #[inline]
    fn checked_capacity(&self, count: usize, min_elem_size: usize) -> Result<usize, CdrError> {
        self.check_available(count.saturating_mul(min_elem_size.max(1)))?;
        Ok(count)
    }

    /// Align to `N`, then take the next `N` bytes. Every scalar read below is this
    /// followed by a byte-order conversion.
    #[inline]
    fn read_bytes<const N: usize>(&mut self) -> Result<[u8; N], CdrError> {
        self.align(N);
        self.check_available(N)?;
        let position = self.get_position();
        let mut bytes = [0u8; N];
        self.copy_bytes_at(position, &mut bytes);
        self.set_position(position + N);
        Ok(bytes)
    }

    #[inline]
    fn deserialize_bool(&mut self) -> Result<bool, CdrError> {
        Ok(self.read_bytes::<1>()?[0] != 0)
    }

    #[inline]
    fn deserialize_i8(&mut self) -> Result<i8, CdrError> {
        Ok(self.read_bytes::<1>()?[0] as i8)
    }

    #[inline]
    fn deserialize_u8(&mut self) -> Result<u8, CdrError> {
        Ok(self.read_bytes::<1>()?[0])
    }

    #[inline]
    fn deserialize_i16(&mut self) -> Result<i16, CdrError> {
        let bytes = self.read_bytes::<2>()?;
        Ok(from_bytes_i16(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_u16(&mut self) -> Result<u16, CdrError> {
        let bytes = self.read_bytes::<2>()?;
        Ok(from_bytes_u16(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_i32(&mut self) -> Result<i32, CdrError> {
        let bytes = self.read_bytes::<4>()?;
        Ok(from_bytes_i32(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_u32(&mut self) -> Result<u32, CdrError> {
        let bytes = self.read_bytes::<4>()?;
        Ok(from_bytes_u32(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_i64(&mut self) -> Result<i64, CdrError> {
        let bytes = self.read_bytes::<8>()?;
        Ok(from_bytes_i64(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_u64(&mut self) -> Result<u64, CdrError> {
        let bytes = self.read_bytes::<8>()?;
        Ok(from_bytes_u64(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_f32(&mut self) -> Result<f32, CdrError> {
        let bytes = self.read_bytes::<4>()?;
        Ok(from_bytes_f32(bytes, self.get_endianness()))
    }

    #[inline]
    fn deserialize_f64(&mut self) -> Result<f64, CdrError> {
        let bytes = self.read_bytes::<8>()?;
        Ok(from_bytes_f64(bytes, self.get_endianness()))
    }

    /// Deserialize 8-bit character (ISO Latin-1)
    fn deserialize_char(&mut self) -> Result<char, CdrError> {
        Ok(self.deserialize_u8()? as char)
    }

    /// Deserialize wide character (UTF-16)
    fn deserialize_wchar16(&mut self) -> Result<char, CdrError> {
        let code_unit = self.deserialize_u16()?;
        if (0xD800..=0xDFFF).contains(&code_unit) {
            return Err(CdrError::InvalidWideCharacter);
        }
        char::from_u32(code_unit as u32).ok_or(CdrError::InvalidWideCharacter)
    }

    /// Deserialize wide character (UTF-32)
    fn deserialize_wchar32(&mut self) -> Result<char, CdrError> {
        let code_point = self.deserialize_u32()?;
        char::from_u32(code_point).ok_or(CdrError::InvalidWideCharacter)
    }

    /// Deserialize wide character string (UTF-16). The length prefix counts code
    /// units, not bytes.
    fn deserialize_wstring16(&mut self) -> Result<String, CdrError> {
        let length = self.deserialize_u32()? as usize;

        self.align(2);
        let mut utf16_chars = Vec::with_capacity(self.checked_capacity(length, 2)?);
        for _ in 0..length {
            utf16_chars.push(self.deserialize_u16()?);
        }

        String::from_utf16(&utf16_chars).map_err(|_| CdrError::InvalidWideCharacter)
    }

    /// Deserialize string
    fn deserialize_string(&mut self) -> Result<String, CdrError> {
        let length = self.deserialize_u32()? as usize;

        if length == 0 {
            return Ok(String::new());
        }

        self.check_available(length)?;

        let position = self.get_position();
        let result = {
            let string_data = self.bytes_at(position, length);
            let string_bytes = if string_data.last() == Some(&0) {
                &string_data[..string_data.len() - 1]
            } else {
                &string_data[..]
            };
            std::str::from_utf8(string_bytes).map_err(|_| CdrError::InvalidString)?.to_string()
        };
        self.set_position(position + length);

        Ok(result)
    }

    /// Read a whole run of same-width primitives in one copy.
    ///
    /// Alignment is skipped on an empty run, mirroring the write path: CDR aligns before a
    /// primitive, so a zero-length sequence is just its 4-byte length.
    ///
    /// The copy goes through `copy_bytes_at`, which may stitch across fragment chunk
    /// boundaries — hence the fill closure rather than a direct slice.
    fn read_prim_run<T: NativeBytes>(&mut self, count: usize) -> Result<Vec<T>, CdrError> {
        let elem_size = std::mem::size_of::<T>();
        if count == 0 {
            return Ok(Vec::new());
        }
        self.align(elem_size);
        // Validates the wire-declared count against the bytes actually left before allocating.
        let count = self.checked_capacity(count, elem_size)?;

        let endianness = self.get_endianness();
        let position = self.get_position();
        let result = read_prim_vec::<T>(count, endianness, |dst| self.copy_bytes_at(position, dst));
        self.set_position(position + count * elem_size);
        Ok(result)
    }

    /// Read a run of octets, then map each one. Used for the element types whose Rust
    /// representation is not its wire byte (`bool` normalization, `char` widening).
    fn read_octet_run<T>(
        &mut self,
        count: usize,
        map: impl Fn(u8) -> T,
    ) -> Result<Vec<T>, CdrError> {
        self.check_available(count)?;
        let position = self.get_position();
        let result = self.bytes_at(position, count).iter().map(|&b| map(b)).collect();
        self.set_position(position + count);
        Ok(result)
    }

    /// Deserialize character array (8-bit Latin-1) with length prefix
    fn deserialize_char_array(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b as char)
    }

    /// Deserialize byte sequence with length prefix
    fn deserialize_byte_sequence(&mut self) -> Result<Vec<u8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize u16 sequence with length prefix
    fn deserialize_u16_sequence(&mut self) -> Result<Vec<u16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize u32 sequence with length prefix
    fn deserialize_u32_sequence(&mut self) -> Result<Vec<u32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize u64 sequence with length prefix
    fn deserialize_u64_sequence(&mut self) -> Result<Vec<u64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i8 sequence with length prefix
    fn deserialize_i8_sequence(&mut self) -> Result<Vec<i8>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i16 sequence with length prefix
    fn deserialize_i16_sequence(&mut self) -> Result<Vec<i16>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i32 sequence with length prefix
    fn deserialize_i32_sequence(&mut self) -> Result<Vec<i32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize i64 sequence with length prefix
    fn deserialize_i64_sequence(&mut self) -> Result<Vec<i64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize f32 sequence with length prefix
    fn deserialize_f32_sequence(&mut self) -> Result<Vec<f32>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize f64 sequence with length prefix
    fn deserialize_f64_sequence(&mut self) -> Result<Vec<f64>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_prim_run(length)
    }

    /// Deserialize bool sequence with length prefix.
    /// The wire may carry any octet; anything nonzero is `true`.
    fn deserialize_bool_sequence(&mut self) -> Result<Vec<bool>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b != 0)
    }

    /// Deserialize char sequence with length prefix
    fn deserialize_char_sequence(&mut self) -> Result<Vec<char>, CdrError> {
        let length = self.deserialize_u32()? as usize;
        self.read_octet_run(length, |b| b as char)
    }

    /// Deserialize string sequence with length prefix
    fn deserialize_string_sequence(&mut self) -> Result<Vec<String>, CdrError> {
        self.read_collection_dheader()?;
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(self.checked_capacity(length, 4)?);
        for _ in 0..length {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }

    /// Deserialize sequence of values with length prefix
    fn deserialize_sequence<T, F>(&mut self, mut deserialize_fn: F) -> Result<Vec<T>, CdrError>
    where
        F: FnMut(&mut Self) -> Result<T, CdrError>,
    {
        self.read_collection_dheader()?;
        let length = self.deserialize_u32()? as usize;
        let mut result = Vec::with_capacity(self.checked_capacity(length, 1)?);
        for _ in 0..length {
            result.push(deserialize_fn(self)?);
        }
        Ok(result)
    }

    /// Deserialize optional value
    fn deserialize_optional<T, F>(&mut self, mut deserialize_fn: F) -> Result<Option<T>, CdrError>
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

    /// Deserialize fixed-size byte array (no length prefix)
    fn deserialize_byte_array(&mut self, size: usize) -> Result<Vec<u8>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size u16 array (no length prefix)
    fn deserialize_u16_array(&mut self, size: usize) -> Result<Vec<u16>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size u32 array (no length prefix)
    fn deserialize_u32_array(&mut self, size: usize) -> Result<Vec<u32>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size u64 array (no length prefix)
    fn deserialize_u64_array(&mut self, size: usize) -> Result<Vec<u64>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i8 array (no length prefix)
    fn deserialize_i8_array(&mut self, size: usize) -> Result<Vec<i8>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i16 array (no length prefix)
    fn deserialize_i16_array(&mut self, size: usize) -> Result<Vec<i16>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i32 array (no length prefix)
    fn deserialize_i32_array(&mut self, size: usize) -> Result<Vec<i32>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size i64 array (no length prefix)
    fn deserialize_i64_array(&mut self, size: usize) -> Result<Vec<i64>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size f32 array (no length prefix)
    fn deserialize_f32_array(&mut self, size: usize) -> Result<Vec<f32>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size f64 array (no length prefix)
    fn deserialize_f64_array(&mut self, size: usize) -> Result<Vec<f64>, CdrError> {
        self.read_prim_run(size)
    }

    /// Deserialize fixed-size bool array (no length prefix)
    fn deserialize_bool_array(&mut self, size: usize) -> Result<Vec<bool>, CdrError> {
        self.read_octet_run(size, |b| b != 0)
    }

    /// Deserialize fixed-size char array (no length prefix)
    fn deserialize_char_array_fixed(&mut self, size: usize) -> Result<Vec<char>, CdrError> {
        self.read_octet_run(size, |b| b as char)
    }

    /// Deserialize fixed-size string array (no length prefix)
    fn deserialize_string_array(&mut self, size: usize) -> Result<Vec<String>, CdrError> {
        self.read_collection_dheader()?;
        let mut result = Vec::with_capacity(self.checked_capacity(size, 4)?);
        for _ in 0..size {
            result.push(self.deserialize_string()?);
        }
        Ok(result)
    }
}

/// Re-expose [`CdrRead`] as inherent methods so the call surface — 69 sites in
/// `int2dds` plus the derive macro's output — does not have to import the trait.
///
/// The three arms are one list each of the no-argument readers, the readers that take
/// a fixed element count, and the two that take a closure.
macro_rules! forward_cdr_read {
    ($deserializer:ident) => {
        impl<'a> $deserializer<'a> {
            forward_cdr_read!(@nullary
                deserialize_bool -> bool,
                deserialize_i8 -> i8,
                deserialize_u8 -> u8,
                deserialize_i16 -> i16,
                deserialize_u16 -> u16,
                deserialize_i32 -> i32,
                deserialize_u32 -> u32,
                deserialize_i64 -> i64,
                deserialize_u64 -> u64,
                deserialize_f32 -> f32,
                deserialize_f64 -> f64,
                deserialize_char -> char,
                deserialize_wchar16 -> char,
                deserialize_wchar32 -> char,
                deserialize_wstring16 -> String,
                deserialize_string -> String,
                deserialize_char_array -> Vec<char>,
                deserialize_byte_sequence -> Vec<u8>,
                deserialize_u16_sequence -> Vec<u16>,
                deserialize_u32_sequence -> Vec<u32>,
                deserialize_u64_sequence -> Vec<u64>,
                deserialize_i8_sequence -> Vec<i8>,
                deserialize_i16_sequence -> Vec<i16>,
                deserialize_i32_sequence -> Vec<i32>,
                deserialize_i64_sequence -> Vec<i64>,
                deserialize_f32_sequence -> Vec<f32>,
                deserialize_f64_sequence -> Vec<f64>,
                deserialize_bool_sequence -> Vec<bool>,
                deserialize_char_sequence -> Vec<char>,
                deserialize_string_sequence -> Vec<String>,
            );

            forward_cdr_read!(@counted
                deserialize_byte_array -> Vec<u8>,
                deserialize_u16_array -> Vec<u16>,
                deserialize_u32_array -> Vec<u32>,
                deserialize_u64_array -> Vec<u64>,
                deserialize_i8_array -> Vec<i8>,
                deserialize_i16_array -> Vec<i16>,
                deserialize_i32_array -> Vec<i32>,
                deserialize_i64_array -> Vec<i64>,
                deserialize_f32_array -> Vec<f32>,
                deserialize_f64_array -> Vec<f64>,
                deserialize_bool_array -> Vec<bool>,
                deserialize_char_array_fixed -> Vec<char>,
                deserialize_string_array -> Vec<String>,
            );

            forward_cdr_read!(@closure deserialize_sequence -> Vec<T>);
            forward_cdr_read!(@closure deserialize_optional -> Option<T>);
        }
    };

    (@nullary $($method:ident -> $ret:ty),+ $(,)?) => {
        $(
            #[inline]
            pub fn $method(&mut self) -> ::std::result::Result<$ret, $crate::cdr::CdrError> {
                $crate::cdr::deserializer::read::CdrRead::$method(self)
            }
        )+
    };

    (@counted $($method:ident -> $ret:ty),+ $(,)?) => {
        $(
            #[inline]
            pub fn $method(
                &mut self,
                size: usize,
            ) -> ::std::result::Result<$ret, $crate::cdr::CdrError> {
                $crate::cdr::deserializer::read::CdrRead::$method(self, size)
            }
        )+
    };

    (@closure $method:ident -> $ret:ty) => {
        #[inline]
        pub fn $method<T, F>(
            &mut self,
            deserialize_fn: F,
        ) -> ::std::result::Result<$ret, $crate::cdr::CdrError>
        where
            F: FnMut(&mut Self) -> ::std::result::Result<T, $crate::cdr::CdrError>,
        {
            $crate::cdr::deserializer::read::CdrRead::$method(self, deserialize_fn)
        }
    };
}

pub(crate) use forward_cdr_read;
