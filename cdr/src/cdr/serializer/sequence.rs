use std::convert::TryFrom;

use super::primitive::PrimitiveSerialize;
use super::string::StringSerialize;
use super::CdrSerializerCommon;
use crate::cdr::prim_bulk::{extend_prim_slice, NativeBytes};
use crate::cdr::{CdrError, CdrSerializer, Xcdr2Serializer};
use crate::SerializationError;

fn checked_length(len: usize) -> Result<u32, CdrError> {
    u32::try_from(len).map_err(|_| {
        SerializationError::SerializationError("Sequence length exceeds u32::MAX".to_string())
    })
}

/// Write the length prefix and then the whole element run in one bulk copy.
///
/// Alignment is applied only when there is at least one element: CDR aligns *before* a
/// primitive, so an empty sequence contributes its 4-byte length and nothing else. This
/// matters for 8-byte elements under XCDR1, where aligning unconditionally would emit four
/// stray pad bytes that neither the generated C codec nor any other CDR implementation
/// writes.
///
/// A free function rather than a trait method so `SequenceSerialize` keeps its current
/// shape and `NativeBytes` stays crate-private.
fn write_prim_seq<S, T>(ser: &mut S, data: &[T]) -> Result<(), CdrError>
where
    S: SequenceSerialize + ?Sized,
    T: NativeBytes,
{
    let length = checked_length(data.len())?;
    ser.serialize_u32(length)?;
    if data.is_empty() {
        return Ok(());
    }
    ser.align(std::mem::size_of::<T>());
    let endianness = ser.endianness();
    extend_prim_slice(ser.buffer_mut(), data, endianness);
    Ok(())
}

/// Trait for sequence serialization (with length prefix)
/// Provides default implementations that work for both CdrSerializer and Xcdr2Serializer
pub trait SequenceSerialize: CdrSerializerCommon + PrimitiveSerialize + StringSerialize {
    /// Serialize byte array with length prefix (for sequences)
    fn serialize_byte_sequence(&mut self, data: &[u8]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize u16 sequence with length prefix
    fn serialize_u16_sequence(&mut self, data: &[u16]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize u32 sequence with length prefix
    fn serialize_u32_sequence(&mut self, data: &[u32]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize u64 sequence with length prefix
    fn serialize_u64_sequence(&mut self, data: &[u64]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize i8 sequence with length prefix
    fn serialize_i8_sequence(&mut self, data: &[i8]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize i16 sequence with length prefix
    fn serialize_i16_sequence(&mut self, data: &[i16]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize i32 sequence with length prefix
    fn serialize_i32_sequence(&mut self, data: &[i32]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize i64 sequence with length prefix
    fn serialize_i64_sequence(&mut self, data: &[i64]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize f32 sequence with length prefix
    fn serialize_f32_sequence(&mut self, data: &[f32]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize f64 sequence with length prefix
    fn serialize_f64_sequence(&mut self, data: &[f64]) -> Result<(), CdrError> {
        write_prim_seq(self, data)
    }

    /// Serialize bool sequence with length prefix.
    ///
    /// Not routed through the bulk path: `bool` is excluded from `NativeBytes` because an
    /// arbitrary octet is not a valid `bool`. Rust guarantees `bool` is one byte holding
    /// exactly 0 or 1, so the cast below is what the bulk copy would produce anyway.
    fn serialize_bool_sequence(&mut self, data: &[bool]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        let buffer = self.buffer_mut();
        buffer.reserve(data.len());
        buffer.extend(data.iter().map(|&b| b as u8));
        Ok(())
    }

    /// Serialize char sequence with length prefix
    fn serialize_char_sequence(&mut self, data: &[char]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        self.buffer_mut().reserve(data.len());
        for &value in data {
            self.buffer_mut().push(value as u8);
        }
        Ok(())
    }

    /// Serialize string sequence with length prefix
    fn serialize_string_sequence(&mut self, data: &[String]) -> Result<(), CdrError> {
        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        for value in data {
            self.serialize_string(value)?;
        }
        Ok(())
    }
}

// Implement SequenceSerialize for both serializer types
impl SequenceSerialize for CdrSerializer {}
impl SequenceSerialize for Xcdr2Serializer {
    /// String is non-primitive, so `sequence<string>` carries a DHEADER (DDS-XTypes 7.4.3.5.4).
    fn serialize_string_sequence(&mut self, data: &[String]) -> Result<(), CdrError> {
        let dheader_pos = self.reserve_dheader();
        let content_start = self.position();

        let length = checked_length(data.len())?;
        self.serialize_u32(length)?;
        for value in data {
            self.serialize_string(value)?;
        }

        let content_size = (self.position() - content_start) as u32;
        self.write_dheader_at(dheader_pos, content_size);
        Ok(())
    }
}

// Generic serialize_sequence and serialize_optional methods need to stay as inherent impl
// because they use generic parameters with Self type bounds
impl CdrSerializer {
    /// Serialize sequence of values with length prefix
    pub fn serialize_sequence<T, F>(
        &mut self,
        sequence: &[T],
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        let length = checked_length(sequence.len())?;
        self.serialize_u32(length)?;
        for item in sequence {
            serialize_fn(self, item)?;
        }
        Ok(())
    }

    /// Serialize optional value
    pub fn serialize_optional<T, F>(
        &mut self,
        value: &Option<T>,
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        match value {
            Some(val) => {
                self.serialize_bool(true)?;
                serialize_fn(self, val)?;
            }
            None => self.serialize_bool(false)?,
        }
        Ok(())
    }
}

impl Xcdr2Serializer {
    /// Serialize sequence of non-primitive values with DHEADER + length prefix (XCDR2).
    pub fn serialize_sequence<T, F>(
        &mut self,
        values: &[T],
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        let dheader_pos = self.reserve_dheader();
        let content_start = self.position();

        let length = checked_length(values.len())?;
        self.serialize_u32(length)?;
        for value in values {
            serialize_fn(self, value)?;
        }

        let content_size = (self.position() - content_start) as u32;
        self.write_dheader_at(dheader_pos, content_size);
        Ok(())
    }

    /// Serialize optional value
    pub fn serialize_optional<T, F>(
        &mut self,
        value: &Option<T>,
        mut serialize_fn: F,
    ) -> Result<(), CdrError>
    where
        F: FnMut(&mut Self, &T) -> Result<(), CdrError>,
    {
        match value {
            Some(val) => {
                self.serialize_bool(true)?;
                serialize_fn(self, val)?;
            }
            None => self.serialize_bool(false)?,
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_sequence_tests {
    use crate::{
        cdr::{
            CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
            XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
        },
        BufferManager, DeserializerReader, WChar, WString,
    };
    use std::collections::HashMap;
    #[test]
    fn test_cdr_vec_i32() {
        let value: Vec<i32> = vec![1, 2, 3, 4, 5];

        let mut serializer = CdrSerializer::new(true);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_cdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let mut deserializer = CdrDeserializer::new(&bytes).unwrap();
        let result = Vec::<i32>::deserialize_cdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    // Array Tests - CDR

    /// An empty sequence is its 4-byte length and nothing else, whatever the element width.
    ///
    /// CDR aligns *before* writing a primitive, so with no elements there is nothing to
    /// align to. This used to emit four stray pad bytes for 8-byte elements under XCDR1,
    /// which the generated C codec never wrote — so an empty `sequence<double>` followed by
    /// another member did not round-trip between int2DDS and int2DDS-ffi.
    #[test]
    fn empty_sequence_emits_length_only() {
        use crate::cdr::SequenceSerialize;

        let mut s = CdrSerializer::new(true);
        s.write_encapsulation_header().unwrap();
        s.serialize_f64_sequence(&[]).unwrap();
        assert_eq!(s.into_bytes().len(), 8, "XCDR1 empty sequence<double>");

        let mut s = CdrSerializer::new(true);
        s.write_encapsulation_header().unwrap();
        s.serialize_u16_sequence(&[]).unwrap();
        assert_eq!(s.into_bytes().len(), 8, "XCDR1 empty sequence<unsigned short>");

        let mut s = XcdrSerializer::new(true, ExtensibilityKind::Final);
        s.write_encapsulation_header().unwrap();
        s.serialize_f64_sequence(&[]).unwrap();
        assert_eq!(s.into_bytes().len(), 8, "XCDR2 empty sequence<double>");
    }

    /// The empty case must skip alignment on the read side too, or the writer and reader
    /// disagree about where the next member starts.
    #[test]
    fn empty_sequence_round_trips_with_a_following_member() {
        use crate::cdr::{PrimitiveSerialize, SequenceSerialize};

        let mut s = CdrSerializer::new(true);
        s.write_encapsulation_header().unwrap();
        s.serialize_f64_sequence(&[]).unwrap();
        s.serialize_u32(0xDEAD_BEEF).unwrap();
        let bytes = s.into_bytes();
        assert_eq!(bytes.len(), 12);

        let mut d = CdrDeserializer::new(&bytes).unwrap();
        assert!(d.deserialize_f64_sequence().unwrap().is_empty());
        assert_eq!(d.deserialize_u32().unwrap(), 0xDEAD_BEEF);
    }

    /// Every bulk-eligible element type must round-trip in both encodings and both byte
    /// orders — the swap path is only exercised when the stream order differs from the host.
    #[test]
    fn primitive_sequences_round_trip_in_both_byte_orders() {
        use crate::cdr::SequenceSerialize;

        for little_endian in [true, false] {
            macro_rules! case {
                ($ser:ident, $de:ident, $values:expr) => {{
                    let values = $values;
                    let mut s = CdrSerializer::new(little_endian);
                    s.write_encapsulation_header().unwrap();
                    s.$ser(&values).unwrap();
                    let bytes = s.into_bytes();
                    let mut d = CdrDeserializer::new(&bytes).unwrap();
                    assert_eq!(d.$de().unwrap(), values, concat!(stringify!($ser), " xcdr1"));

                    let mut s = XcdrSerializer::new(little_endian, ExtensibilityKind::Final);
                    s.write_encapsulation_header().unwrap();
                    s.$ser(&values).unwrap();
                    let bytes = s.into_bytes();
                    let mut d = XcdrDeserializer::new(&bytes).unwrap();
                    assert_eq!(d.$de().unwrap(), values, concat!(stringify!($ser), " xcdr2"));
                }};
            }

            case!(serialize_byte_sequence, deserialize_byte_sequence, vec![1u8, 2, 255, 0, 7]);
            case!(serialize_i8_sequence, deserialize_i8_sequence, vec![-128i8, -1, 0, 1, 127]);
            case!(serialize_u16_sequence, deserialize_u16_sequence, vec![0u16, 1, 0x1234, 0xFFFF]);
            case!(
                serialize_i16_sequence,
                deserialize_i16_sequence,
                vec![i16::MIN, -1, 0, i16::MAX]
            );
            case!(
                serialize_u32_sequence,
                deserialize_u32_sequence,
                vec![0u32, 0x0102_0304, u32::MAX]
            );
            case!(
                serialize_i32_sequence,
                deserialize_i32_sequence,
                vec![i32::MIN, -1, 0, i32::MAX]
            );
            case!(
                serialize_u64_sequence,
                deserialize_u64_sequence,
                vec![0u64, 0x0102_0304_0506_0708, u64::MAX]
            );
            case!(
                serialize_i64_sequence,
                deserialize_i64_sequence,
                vec![i64::MIN, -1, 0, i64::MAX]
            );
            case!(serialize_f32_sequence, deserialize_f32_sequence, vec![0.0f32, -1.5, 3.25e30]);
            case!(serialize_f64_sequence, deserialize_f64_sequence, vec![0.0f64, -1.5, 3.25e300]);
            case!(
                serialize_bool_sequence,
                deserialize_bool_sequence,
                vec![true, false, true, true]
            );
        }
    }

    /// Bulk reads must reject a length the remaining bytes cannot cover, before allocating.
    #[test]
    fn bulk_read_rejects_length_beyond_available_bytes() {
        // 4-byte LE length of 0xFFFFFFFF followed by almost nothing.
        let wire = [0xFFu8, 0xFF, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];
        assert!(CdrDeserializer::new_without_header(&wire, true)
            .deserialize_u64_sequence()
            .is_err());
        assert!(CdrDeserializer::new_without_header(&wire, true)
            .deserialize_byte_sequence()
            .is_err());
        assert!(crate::cdr::Xcdr2Deserializer::new_without_header(&wire, true)
            .deserialize_f64_sequence()
            .is_err());
    }
}
