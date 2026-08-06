use speedy::Endianness;

use super::{CdrError, CdrSerializerCommon, EncodingKind, ExtensibilityKind, LcHint, MemberHeader};
use crate::serialize::core::endianness_from_bool;
use crate::serialize::{
    align_position_with_header_offset, to_bytes_u32, BufferManager, DeserializerReader,
};

/// XCDR v2 Serializer supporting PLAIN_CDR2, DELIMITED_CDR, and PL_CDR2
/// Based on DDS-XTypes specification v1.3 Extended CDR version 2
pub struct Xcdr2Serializer {
    pub(super) endianness: Endianness,
    pub(super) buffer: Vec<u8>,
    pub(super) extensibility_kind: ExtensibilityKind,
    pub(super) type_hash: Option<[u8; 14]>,
}

impl Xcdr2Serializer {
    /// Create a new XCDR serializer
    pub fn new(little_endian: bool, extensibility: ExtensibilityKind) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::new(),
            extensibility_kind: extensibility,
            type_hash: None,
        }
    }

    /// Create XCDR serializer with pre-allocated capacity
    pub fn with_capacity(
        little_endian: bool,
        extensibility: ExtensibilityKind,
        capacity: usize,
    ) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::with_capacity(capacity),
            extensibility_kind: extensibility,
            type_hash: None,
        }
    }

    /// Create XCDR serializer reusing an existing buffer (capacity preserved, content cleared)
    pub fn reuse_buffer(
        little_endian: bool,
        extensibility: ExtensibilityKind,
        mut buffer: Vec<u8>,
    ) -> Self {
        buffer.clear();
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer,
            extensibility_kind: extensibility,
            type_hash: None,
        }
    }

    /// Consume the serializer and return the internal buffer (for ownership round-trip)
    pub fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }

    /// Create XCDR serializer with type hash for type safety
    pub fn with_type_hash(
        little_endian: bool,
        extensibility: ExtensibilityKind,
        type_hash: [u8; 14],
    ) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::new(),
            extensibility_kind: extensibility,
            type_hash: Some(type_hash),
        }
    }

    /// Write XCDR encapsulation header (auto-detects version based on extensibility)
    pub fn write_encapsulation_header(&mut self) -> Result<(), CdrError> {
        let encoding_kind = match (self.extensibility_kind, self.endianness) {
            // FINAL types use PLAINCDR2 (XCDR2)
            (ExtensibilityKind::Final, Endianness::LittleEndian) => EncodingKind::PlainCdr2Le,
            (ExtensibilityKind::Final, Endianness::BigEndian) => EncodingKind::PlainCdr2Be,

            // APPENDABLE types use DELIMITED_CDR (XCDR2)
            (ExtensibilityKind::Appendable, Endianness::LittleEndian) => EncodingKind::DCdr2Le,
            (ExtensibilityKind::Appendable, Endianness::BigEndian) => EncodingKind::DCdr2Be,

            // MUTABLE types use PL_CDR2 (XCDR2)
            (ExtensibilityKind::Mutable, Endianness::LittleEndian) => EncodingKind::PlCdr2Le,
            (ExtensibilityKind::Mutable, Endianness::BigEndian) => EncodingKind::PlCdr2Be,
        };

        // Encoding identifier (2 bytes) - always in big-endian
        let encap_id_bytes = (encoding_kind as u16).to_be_bytes();
        self.buffer.extend_from_slice(&encap_id_bytes);

        // Options (2 bytes) - bit 0 indicates type hash presence
        let options = if self.type_hash.is_some() { 0x0001u16 } else { 0x0000u16 };
        self.buffer.extend_from_slice(&options.to_be_bytes());

        // Optional EquivalenceHash (14 bytes) if present
        if let Some(hash) = self.type_hash {
            self.buffer.extend_from_slice(&hash);
        }

        Ok(())
    }

    /// Write member header for MUTABLE types
    pub fn write_member_header(&mut self, member_id: u32, length: usize) -> Result<(), CdrError> {
        let header = MemberHeader::new(member_id, length);
        header.write(&mut self.buffer, self.endianness)?;
        Ok(())
    }

    /// Get current buffer position (for calculating lengths)
    pub fn position(&self) -> usize {
        self.buffer.len()
    }

    /// Reserve space for DHEADER and return position
    /// DHEADER is always 4-byte aligned per XCDR2 spec
    pub fn reserve_dheader(&mut self) -> usize {
        self.align(4);
        let pos = self.buffer.len();
        self.buffer.extend_from_slice(&[0u8; 4]); // Reserve 4 bytes for DHEADER
        pos
    }

    /// Write DHEADER at reserved position
    pub fn write_dheader_at(&mut self, position: usize, length: u32) {
        let bytes = to_bytes_u32(length, self.endianness);
        self.buffer[position..position + 4].copy_from_slice(&bytes);
    }

    /// Insert 4 zero bytes at the given position, shifting subsequent bytes right.
    /// Used when an EMHEADER's payload length overflows 16 bits and the codegen
    /// needs to retroactively switch from compact (LC=0..3) to LC=4 (NEXTINT) encoding.
    pub fn insert_nextint_slot_at(&mut self, position: usize) {
        self.buffer.splice(position..position, [0u8; 4]);
    }

    pub fn write_member_with<F>(
        &mut self,
        member_id: u32,
        must_understand: bool,
        write_value: F,
    ) -> Result<(), CdrError>
    where
        F: FnOnce(&mut Self) -> Result<(), CdrError>,
    {
        self.write_member_with_lc(member_id, must_understand, LcHint::Auto, write_value)
    }

    pub fn write_member_with_lc<F>(
        &mut self,
        member_id: u32,
        must_understand: bool,
        lc_hint: LcHint,
        write_value: F,
    ) -> Result<(), CdrError>
    where
        F: FnOnce(&mut Self) -> Result<(), CdrError>,
    {
        if member_id > 0x0FFF_FFFF {
            return Err(CdrError::InvalidMemberId(member_id));
        }
        let emh_pos = self.reserve_dheader();
        let start = self.position();
        write_value(self)?;
        let len = (self.position() - start) as u32;

        let must_bit: u32 = if must_understand { 0x8000_0000 } else { 0 };

        let (lc_word, nextint) = self.select_lc(len, lc_hint);
        let lc = lc_word >> 28;

        let emh = must_bit | lc_word | (member_id & 0x0FFF_FFFF);
        match lc {
            // LC=4: separate NEXTINT slot before payload
            4 => {
                let ni = nextint.expect("LC=4 always has NEXTINT");
                self.insert_nextint_slot_at(emh_pos + 4);
                self.write_dheader_at(emh_pos, emh);
                self.write_dheader_at(emh_pos + 4, ni);
            }
            // LC=5/6/7: NEXTINT overlaps with payload's first 4 bytes
            5 | 6 | 7 => {
                self.write_dheader_at(emh_pos, emh);
            }
            // LC=0..=3: no NEXTINT
            _ => {
                self.write_dheader_at(emh_pos, emh);
            }
        }
        Ok(())
    }

    fn select_lc(&self, len: u32, hint: LcHint) -> (u32, Option<u32>) {
        // The value already wrote its own DHEADER as its first 4 bytes; reuse it as
        // the NEXTINT (LC=5, byte-length form) instead of inserting a separate one.
        if let LcHint::Dheader = hint {
            return (5u32 << 28, None);
        }
        match len {
            1 => (0u32 << 28, None),
            2 => (1u32 << 28, None),
            4 => (2u32 << 28, None),
            8 => (3u32 << 28, None),
            _ => match hint {
                LcHint::SeqMul4 if len >= 4 && (len - 4) % 4 == 0 => {
                    (6u32 << 28, Some((len - 4) / 4))
                }
                LcHint::SeqMul8 if len >= 4 && (len - 4) % 8 == 0 => {
                    (7u32 << 28, Some((len - 4) / 8))
                }
                _ => (4u32 << 28, Some(len)),
            },
        }
    }

    /// Helper method for writing u32 (used by begin_struct)
    fn _write_u32(&mut self, value: u32) -> Result<(), CdrError> {
        self.align(4);
        let bytes = to_bytes_u32(value, self.endianness);
        self.buffer.extend_from_slice(&bytes);
        Ok(())
    }

    /// Begin struct serialization by writing a DHEADER placeholder.
    ///
    /// Per XTypes §7.4.3.1 framing follows the extensibility of the object being
    /// serialized, so the *caller* decides whether to call this (only
    /// APPENDABLE/MUTABLE are delimited). When called, a DHEADER is always
    /// reserved — independent of the serializer's top-level extensibility — to
    /// stay symmetric with the deserializer and correctly frame nested types
    /// whose extensibility differs from the message-level one.
    pub fn begin_struct(&mut self) -> Result<usize, CdrError> {
        self.align(4);
        let size_pos = self.buffer.len();
        let bytes = to_bytes_u32(0, self.endianness);
        self.buffer.extend_from_slice(&bytes); // placeholder
        Ok(size_pos)
    }

    /// End struct serialization (backpatch the DHEADER reserved by begin_struct).
    pub fn end_struct(&mut self, size_pos: usize) -> Result<(), CdrError> {
        let current_pos = self.buffer.len();
        let object_size = (current_pos - size_pos - 4) as u32;
        let size_bytes = to_bytes_u32(object_size, self.endianness);
        self.buffer[size_pos..size_pos + 4].copy_from_slice(&size_bytes);
        Ok(())
    }
}

impl BufferManager for Xcdr2Serializer {
    /// Get the serialized data
    fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    /// Get reference to the internal buffer
    fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    /// Reset the serializer for reuse
    fn reset(&mut self) {
        self.buffer.clear();
    }
}

/// XCDR v2 Deserializer
pub struct Xcdr2Deserializer<'a> {
    pub(super) endianness: Endianness,
    pub(super) data: &'a [u8],
    pub(super) position: usize,
    header_size: usize,
    /// Whether the data uses XCDR2 encoding (affects alignment rules)
    /// XCDR2 limits maximum alignment to 4 bytes, while XCDR1 allows 8 bytes
    is_xcdr2: bool,
}

impl<'a> Xcdr2Deserializer<'a> {
    /// Create a new CDR deserializer
    pub fn new(data: &'a [u8]) -> Result<Self, CdrError> {
        let (endianness, header_size, is_xcdr2) = parse_encapsulation_header(data)?;

        Ok(Self { endianness, data: &data[header_size..], position: 0, header_size, is_xcdr2 })
    }

    /// Create deserializer without encapsulation header
    /// Assumes XCDR2 encoding by default (4-byte max alignment)
    pub fn new_without_header(data: &'a [u8], little_endian: bool) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            data,
            position: 0,
            header_size: 0,
            is_xcdr2: true, // Default to XCDR2 behavior
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    // Backing slice. xcdr2 input is always contiguous (no fragment reassembly).
    pub fn get_data(&self) -> &[u8] {
        self.data
    }

    /// Align position to boundary (accounting for removed header)
    /// XCDR2 limits maximum alignment to 4 bytes to reduce padding
    /// XCDR1 (CDR) allows full alignment (up to 8 bytes for double/i64/u64)
    pub(super) fn align(&mut self, alignment: usize) {
        let actual_alignment = if self.is_xcdr2 {
            std::cmp::min(alignment, 4) // XCDR2: max 4-byte alignment
        } else {
            alignment // XCDR1: no alignment limit
        };
        align_position_with_header_offset(&mut self.position, actual_alignment, self.header_size);
    }

    /// Check if enough data is available
    #[inline]
    pub(super) fn check_available(&self, size: usize) -> Result<(), CdrError> {
        // Subtraction form: `position + size` can wrap and pass an additive check.
        let len = self.data.len();
        if self.position > len || size > len - self.position {
            Err(CdrError::InsufficientData)
        } else {
            Ok(())
        }
    }

    /// Validate a wire-declared count against remaining bytes before allocating.
    #[inline]
    pub(super) fn checked_capacity(
        &self,
        count: usize,
        min_elem_size: usize,
    ) -> Result<usize, CdrError> {
        self.check_available(count.saturating_mul(min_elem_size.max(1)))?;
        Ok(count)
    }

    /// Read DHEADER (4-byte object size for APPENDABLE/MUTABLE types)
    pub fn read_dheader(&mut self) -> Result<u32, CdrError> {
        self.deserialize_u32()
    }

    /// Skip bytes (for forward compatibility)
    pub fn skip(&mut self, bytes: usize) -> Result<(), CdrError> {
        self.check_available(bytes)?;
        self.position += bytes;
        Ok(())
    }

    /// Begin reading a struct with DHEADER (for APPENDABLE/MUTABLE types)
    /// Returns the object size and starting position for boundary checking
    pub fn begin_struct(&mut self) -> Result<(u32, usize), CdrError> {
        let object_size = self.read_dheader()?;
        let start_position = self.position;
        Ok((object_size, start_position))
    }

    /// End reading a struct with DHEADER, automatically skipping any remaining bytes
    /// This enables forward compatibility for APPENDABLE types
    pub fn end_struct(&mut self, object_size: u32, start_position: usize) -> Result<(), CdrError> {
        let current_position = self.position;
        let bytes_read = current_position - start_position;

        if bytes_read < object_size as usize {
            // Skip remaining bytes for forward compatibility (unknown fields)
            let remaining_bytes = object_size as usize - bytes_read;
            self.skip(remaining_bytes)?;
        } else if bytes_read > object_size as usize {
            // This shouldn't happen with well-formed data
            return Err(CdrError::DeserializationError(format!(
                "Object size mismatch: read {} bytes but DHEADER indicates {} bytes",
                bytes_read, object_size
            )));
        }

        Ok(())
    }

    /// Read EMHEADER for MUTABLE types
    /// Returns (member_id, member_length) on success
    /// The deserializer position is advanced past the header
    pub fn read_member_header(&mut self) -> Result<(u32, u32), CdrError> {
        use super::MemberHeader;

        self.align(4); // EMHEADER is u32, needs 4-byte alignment
        let (header, bytes_consumed) =
            MemberHeader::read(self.data, self.position, self.endianness)?;

        self.position += bytes_consumed;

        Ok((header.member_id, header.member_length))
    }

    /// Read EMHEADER and also return must_understand flag
    /// Returns (member_id, member_length, must_understand) on success
    pub fn read_member_header_full(&mut self) -> Result<(u32, u32, bool), CdrError> {
        use super::MemberHeader;

        self.align(4); // EMHEADER is u32, needs 4-byte alignment
        let (header, bytes_consumed) =
            MemberHeader::read(self.data, self.position, self.endianness)?;

        self.position += bytes_consumed;

        Ok((header.member_id, header.member_length, header.must_understand))
    }

    /// Skip to next member (for unknown member_ids in Mutable types)
    /// This enables forward compatibility
    pub fn skip_member(&mut self, member_length: u32) -> Result<(), CdrError> {
        self.skip(member_length as usize)
    }

    /// Peek at the next member header without consuming it
    /// Returns Some((member_id, member_length)) or None if at end of struct
    pub fn peek_member_header(&self, object_end_position: usize) -> Option<(u32, u32)> {
        use super::MemberHeader;

        if self.position >= object_end_position {
            return None;
        }

        if self.position + 4 > self.data.len() {
            return None;
        }

        MemberHeader::read(self.data, self.position, self.endianness)
            .ok()
            .map(|(h, _)| (h.member_id, h.member_length))
    }
}

impl<'a> DeserializerReader for Xcdr2Deserializer<'a> {
    type Error = CdrError;

    fn check_available(&self, size: usize) -> Result<(), Self::Error> {
        self.check_available(size)
    }

    fn copy_bytes_at(&self, offset: usize, out: &mut [u8]) {
        out.copy_from_slice(&self.data[offset..offset + out.len()]);
    }

    fn get_position(&self) -> usize {
        self.position
    }

    fn set_position(&mut self, position: usize) {
        self.position = position;
    }

    fn get_endianness(&self) -> Endianness {
        self.endianness
    }

    fn align(&mut self, alignment: usize) {
        self.align(alignment);
    }
}

fn parse_encapsulation_header(data: &[u8]) -> Result<(Endianness, usize, bool), CdrError> {
    if data.len() < 4 {
        return Err(CdrError::InsufficientData);
    }

    let encap_id = u16::from_be_bytes([data[0], data[1]]);

    let (endianness, is_xcdr2) = match encap_id {
        0x0000 | 0x0002 => (Endianness::BigEndian, false),
        0x0001 | 0x0003 => (Endianness::LittleEndian, false),
        0x0006 | 0x0008 | 0x000A => (Endianness::BigEndian, true),
        0x0007 | 0x0009 | 0x000B => (Endianness::LittleEndian, true),
        _ => return Err(CdrError::InvalidEncapsulation(encap_id)),
    };

    Ok((endianness, 4, is_xcdr2))
}

#[cfg(test)]
#[allow(unused_imports)]
mod xcdr2_tests {
    use crate::serialize::cdr::MemberHeader;
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
    use speedy::Endianness;
    use std::collections::HashMap;
    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct SimpleStruct {
        pub x: i32,
        pub y: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct AppendableStruct {
        pub id: u32,
        pub name: String,
        pub values: Vec<i32>,
    }

    #[test]
    fn test_xcdr2_extensibility_final() {
        let value: i32 = 42;

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // PLAIN_CDR2_LE = 0x0007
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x07);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_extensibility_appendable() {
        let value: i32 = 42;

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // DCDR2_LE = 0x0009
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x09);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_extensibility_mutable() {
        let value: i32 = 42;

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        // PL_CDR2_LE = 0x000B
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x0B);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = i32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    // Complex Struct Tests with DdsType derive

    #[test]
    fn test_emheader_roundtrip_small_length() {
        for (length, expected_lc) in [(1usize, 0u8), (2, 1), (4, 2), (8, 3)] {
            let header = MemberHeader::new(42, length);
            let mut buffer = Vec::new();
            header.write(&mut buffer, Endianness::LittleEndian).unwrap();

            assert_eq!(buffer.len(), 4, "length={length} must encode as 4 bytes");

            let word = u32::from_le_bytes(buffer[..4].try_into().unwrap());
            assert_eq!((word >> 28) & 0x07, expected_lc as u32);
            assert_eq!(word & 0x0FFF_FFFF, 42);
            assert_eq!(word & 0x8000_0000, 0);

            let (read_header, bytes_consumed) =
                MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();
            assert_eq!(bytes_consumed, 4);
            assert_eq!(read_header.member_id, 42);
            assert_eq!(read_header.member_length as usize, length);
            assert!(!read_header.must_understand);
        }
    }

    #[test]
    fn test_emheader_roundtrip_large_length() {
        // Test EMHEADER with large length (> 64KB, requires LC=4 extended header)
        let large_length: usize = 100_000; // 100KB
        let header = MemberHeader::new(123, large_length);

        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::LittleEndian).unwrap();

        // Verify header is 8 bytes for large lengths (4 + 4 for extended length)
        assert_eq!(buffer.len(), 8);

        // Read it back
        let (read_header, bytes_consumed) =
            MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();

        assert_eq!(bytes_consumed, 8);
        assert_eq!(read_header.member_id, 123);
        assert_eq!(read_header.member_length, large_length as u32);
    }

    #[test]
    fn test_emheader_must_understand_flag() {
        // Test must_understand flag (bit 31)
        let header = MemberHeader { member_id: 10, member_length: 50, must_understand: true };

        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::LittleEndian).unwrap();

        // Read it back
        let (read_header, _) = MemberHeader::read(&buffer, 0, Endianness::LittleEndian).unwrap();

        assert_eq!(read_header.member_id, 10);
        assert_eq!(read_header.member_length, 50);
        assert!(read_header.must_understand);
    }

    #[test]
    fn test_emheader_big_endian() {
        // Test big-endian encoding/decoding
        let header = MemberHeader::new(255, 1000);

        let mut buffer = Vec::new();
        header.write(&mut buffer, Endianness::BigEndian).unwrap();

        let (read_header, _) = MemberHeader::read(&buffer, 0, Endianness::BigEndian).unwrap();

        assert_eq!(read_header.member_id, 255);
        assert_eq!(read_header.member_length, 1000);
    }

    // =============================================================================
    // Mutable Struct Tests
    // =============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableStruct {
        #[dds(id = 1)]
        pub id: u32,
        #[dds(id = 2)]
        pub name: String,
        #[dds(id = 3)]
        pub value: f64,
    }

    #[test]
    fn test_mutable_struct_xcdr2() {
        let value = MutableStruct { id: 42, name: "test_mutable".to_string(), value: 3.14159 };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Verify encapsulation header is PL_CDR2_LE (0x000B)
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x0B);

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.id, 42);
        assert_eq!(result.name, "test_mutable");
        assert!((result.value - 3.14159).abs() < 1e-10);
    }

    #[test]
    fn test_mutable_struct_big_endian() {
        let value = MutableStruct { id: 100, name: "big_endian".to_string(), value: 2.71828 };

        // Serialize in big-endian
        let mut serializer = XcdrSerializer::new(false, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Verify encapsulation header is PL_CDR2_BE (0x000A)
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x0A);

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.id, 100);
        assert_eq!(result.name, "big_endian");
        assert!((result.value - 2.71828).abs() < 1e-10);
    }

    // =============================================================================
    // Mutable Struct with Nested Types
    // =============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableNestedStruct {
        #[dds(id = 1)]
        pub header: SimpleStruct,
        #[dds(id = 2)]
        pub data: Vec<i32>,
        #[dds(id = 3)]
        pub count: u32,
    }

    #[test]
    fn test_mutable_nested_struct_xcdr2() {
        let value = MutableNestedStruct {
            header: SimpleStruct { x: 10, y: 20 },
            data: vec![1, 2, 3, 4, 5],
            count: 5,
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableNestedStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.header.x, 10);
        assert_eq!(result.header.y, 20);
        assert_eq!(result.data, vec![1, 2, 3, 4, 5]);
        assert_eq!(result.count, 5);
    }

    // =============================================================================
    // Optional Field Tests (Mutable types)
    // =============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableWithOptional {
        #[dds(id = 1)]
        pub required_field: u32,
        #[dds(id = 2, optional)]
        pub optional_string: Option<String>,
        #[dds(id = 3, optional)]
        pub optional_value: Option<f64>,
    }

    #[test]
    fn test_mutable_optional_all_present() {
        let value = MutableWithOptional {
            required_field: 123,
            optional_string: Some("hello".to_string()),
            optional_value: Some(99.9),
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.required_field, 123);
        assert_eq!(result.optional_string, Some("hello".to_string()));
        assert_eq!(result.optional_value, Some(99.9));
    }

    #[test]
    fn test_mutable_optional_none_values() {
        let value = MutableWithOptional {
            required_field: 456,
            optional_string: None,
            optional_value: None,
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.required_field, 456);
        assert_eq!(result.optional_string, None);
        assert_eq!(result.optional_value, None);
    }

    #[test]
    fn test_mutable_optional_mixed() {
        let value = MutableWithOptional {
            required_field: 789,
            optional_string: Some("partial".to_string()),
            optional_value: None,
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Deserialize
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithOptional::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result.required_field, 789);
        assert_eq!(result.optional_string, Some("partial".to_string()));
        assert_eq!(result.optional_value, None);
    }

    // =============================================================================
    // DHEADER Tests (Appendable/Mutable struct size header)
    // =============================================================================

    #[test]
    fn test_appendable_struct_dheader() {
        // Appendable structs use DHEADER for forward compatibility
        let value = AppendableStruct {
            id: 999,
            name: "dheader_test".to_string(),
            values: vec![10, 20, 30],
        };

        // Serialize
        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Appendable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        // Verify encapsulation header is DCDR2_LE (0x0009)
        assert_eq!(bytes[0], 0x00);
        assert_eq!(bytes[1], 0x09);

        // After encap header (4 bytes), there should be a DHEADER (4 bytes)
        // The DHEADER contains the object size
        let dheader_bytes = &bytes[4..8];
        let dheader_size = u32::from_le_bytes([
            dheader_bytes[0],
            dheader_bytes[1],
            dheader_bytes[2],
            dheader_bytes[3],
        ]);

        // Object size should match remaining data
        let object_data_len = bytes.len() - 8; // Total - encap header - dheader
        assert_eq!(dheader_size as usize, object_data_len);

        // Deserialize should still work
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = AppendableStruct::deserialize_xcdr(&mut deserializer).unwrap();

        assert_eq!(result, value);
    }

    // ============================================================================
    // Bounded WString Tests
    // ============================================================================

    #[derive(DdsType)]
    #[dds_type(extensibility = "Mutable")]
    struct HashIdStruct {
        #[dds(hashid)]
        pub name: String,
        #[dds(hashid = "custom_field")]
        pub value: u32,
    }

    #[test]
    fn test_hashid_mutable_roundtrip() {
        let original = HashIdStruct { name: "test_name".to_string(), value: 42 };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        original.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = HashIdStruct::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.name, "test_name");
        assert_eq!(result.value, 42);
    }

    // ============================================================================
    // @autoid Tests
    // ============================================================================

    #[derive(DdsType)]
    #[dds_type(extensibility = "Mutable", autoid = "Hash")]
    struct AutoIdHashStruct {
        pub field_a: u32,
        pub field_b: String,
    }

    #[test]
    fn test_autoid_hash_mutable_roundtrip() {
        let original = AutoIdHashStruct { field_a: 123, field_b: "autoid_test".to_string() };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        original.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = AutoIdHashStruct::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.field_a, 123);
        assert_eq!(result.field_b, "autoid_test");
    }

    #[derive(DdsType)]
    #[dds_type(extensibility = "Mutable", autoid = "Sequential")]
    struct AutoIdSeqStruct {
        pub first: u32,
        pub second: f64,
    }

    #[test]
    fn test_autoid_sequential_mutable_roundtrip() {
        let original = AutoIdSeqStruct { first: 999, second: 1.5 };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        original.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = AutoIdSeqStruct::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.first, 999);
        assert_eq!(result.second, 1.5);
    }

    // ============================================================================
    // Struct Inheritance Tests
    // ============================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableWithSeqU32 {
        #[dds(id = 0)]
        pub values: Vec<u32>,
    }

    #[test]
    fn test_lc6_u32_sequence_emheader() {
        use crate::serialize::cdr::MemberHeader;

        let value = MutableWithSeqU32 { values: vec![1, 2, 3] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let payload = &bytes[4..];

        let emheader_bytes = &payload[4..];
        let (header, _) =
            MemberHeader::read(emheader_bytes, 0, speedy::Endianness::LittleEndian).unwrap();

        assert_eq!(header.member_id, 0);

        let emh_word = u32::from_le_bytes([
            emheader_bytes[0],
            emheader_bytes[1],
            emheader_bytes[2],
            emheader_bytes[3],
        ]);
        let lc = (emh_word >> 28) & 0x07;
        assert_eq!(lc, 6);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqU32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2, 3]);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct MutableWithSeqF64 {
        #[dds(id = 0)]
        pub data: Vec<f64>,
    }

    #[test]
    fn test_lc7_f64_sequence_emheader() {
        let value = MutableWithSeqF64 { data: vec![1.0, 2.0] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();

        let bytes = serializer.into_bytes();
        let payload = &bytes[4..];

        let emheader_bytes = &payload[4..];
        let emh_word = u32::from_le_bytes([
            emheader_bytes[0],
            emheader_bytes[1],
            emheader_bytes[2],
            emheader_bytes[3],
        ]);
        let lc = (emh_word >> 28) & 0x07;
        assert_eq!(lc, 7);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqF64::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.data, vec![1.0, 2.0]);
    }

    #[test]
    fn test_lc6_u32_sequence_wire_bytes() {
        // Mutable Vec<u32> XCDR2: NEXTINT overlaps with sequence length (LC=6, no extra slot).
        let value = MutableWithSeqU32 { values: vec![1, 2, 3] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // 4 encap + 4 struct DHEADER + 4 EMHEADER + 4 NEXTINT/length + 3*4 elements = 28
        assert_eq!(bytes.len(), 28, "LC=6 wire must not contain an extra NEXTINT slot");
        // Encap header PL_CDR2_LE
        assert_eq!(&bytes[0..2], &[0x00, 0x0B]);
        // Struct DHEADER = content size after itself = 20
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 20);
        // EMHEADER: M=0, LC=6, ID=0 → 0x6000_0000
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]), 0x6000_0000);
        // NEXTINT (= sequence length) = 3
        assert_eq!(u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]), 3);
        // Elements
        assert_eq!(u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]), 1);
        assert_eq!(u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]), 2);
        assert_eq!(u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]), 3);

        // Round-trip
        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqU32::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.values, vec![1, 2, 3]);
    }

    #[test]
    fn test_lc7_f64_sequence_wire_bytes() {
        // Mutable Vec<f64> XCDR2: NEXTINT overlaps with sequence length (LC=7, no extra slot).
        let value = MutableWithSeqF64 { data: vec![1.0, 2.0] };

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Mutable);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // 4 encap + 4 struct DHEADER + 4 EMHEADER + 4 NEXTINT/length + 2*8 elements = 32
        assert_eq!(bytes.len(), 32, "LC=7 wire must not contain an extra NEXTINT slot");
        // Struct DHEADER content = 24
        assert_eq!(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]), 24);
        // EMHEADER: LC=7, ID=0 → 0x7000_0000
        assert_eq!(u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]), 0x7000_0000);
        // NEXTINT (= length) = 2
        assert_eq!(u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]), 2);
        // Elements
        assert_eq!(
            f64::from_le_bytes([
                bytes[16], bytes[17], bytes[18], bytes[19], bytes[20], bytes[21], bytes[22],
                bytes[23],
            ]),
            1.0
        );
        assert_eq!(
            f64::from_le_bytes([
                bytes[24], bytes[25], bytes[26], bytes[27], bytes[28], bytes[29], bytes[30],
                bytes[31],
            ]),
            2.0
        );

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = MutableWithSeqF64::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result.data, vec![1.0, 2.0]);
    }

    #[test]
    fn test_xcdr2_hashmap_dheader_roundtrip() {
        // XCDR2 HashMap: DHEADER + length + pairs
        let mut value: HashMap<String, i32> = HashMap::new();
        value.insert("alpha".to_string(), 10);
        value.insert("beta".to_string(), 20);

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        // DHEADER must equal (total - encap header - dheader) = total - 8
        let dheader = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(dheader as usize, bytes.len() - 8);
        // Map length follows DHEADER
        let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(len, 2);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = HashMap::<String, i32>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_btreemap_dheader_roundtrip() {
        use std::collections::BTreeMap;

        let mut value: BTreeMap<String, i32> = BTreeMap::new();
        value.insert("alpha".to_string(), 10);
        value.insert("beta".to_string(), 20);

        let mut serializer = XcdrSerializer::new(true, ExtensibilityKind::Final);
        serializer.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut serializer).unwrap();
        let bytes = serializer.into_bytes();

        let dheader = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        assert_eq!(dheader as usize, bytes.len() - 8);
        let len = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        assert_eq!(len, 2);

        let mut deserializer = XcdrDeserializer::new(&bytes).unwrap();
        let result = BTreeMap::<String, i32>::deserialize_xcdr(&mut deserializer).unwrap();
        assert_eq!(result, value);
    }

    #[test]
    fn test_xcdr2_primitive_map_omits_dheader() {
        use std::collections::BTreeMap;

        let mut hash: HashMap<i32, i32> = HashMap::new();
        hash.insert(1, 10);
        hash.insert(2, 20);
        let mut tree: BTreeMap<i32, i32> = BTreeMap::new();
        tree.insert(1, 10);
        tree.insert(2, 20);

        let serialize = |run: &dyn Fn(&mut XcdrSerializer)| {
            let mut s = XcdrSerializer::new(true, ExtensibilityKind::Final);
            s.write_encapsulation_header().unwrap();
            run(&mut s);
            s.into_bytes()
        };
        let hash_bytes = serialize(&|s| hash.serialize_xcdr(s).unwrap());
        let tree_bytes = serialize(&|s| tree.serialize_xcdr(s).unwrap());

        for bytes in [&hash_bytes, &tree_bytes] {
            let count = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            assert_eq!(count, 2, "primitive map must write count, not a DHEADER");
            assert_eq!(bytes.len(), 24, "primitive map must not contain a DHEADER slot");
        }

        let mut d = XcdrDeserializer::new(&hash_bytes).unwrap();
        assert_eq!(HashMap::<i32, i32>::deserialize_xcdr(&mut d).unwrap(), hash);
        let mut d = XcdrDeserializer::new(&tree_bytes).unwrap();
        assert_eq!(BTreeMap::<i32, i32>::deserialize_xcdr(&mut d).unwrap(), tree);
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct ThreeU64Mutable {
        a: u64,
        b: u64,
        c: u64,
    }

    #[test]
    fn xcdr2_mutable_three_u64_serialize_into_round_trip() {
        use crate::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = ThreeU64Mutable {
            a: 0xAAAA_AAAA_AAAA_AAAA,
            b: 0xBBBB_BBBB_BBBB_BBBB,
            c: 0xCCCC_CCCC_CCCC_CCCC,
        };
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Mutable,
            use_delimiters: true,
        };
        let ts = ThreeU64Mutable::get_type_support();

        let mut buf = Vec::new();
        ts.serialize_into(&value, &mut buf, Some(&format)).unwrap();

        let got = ts.deserialize(&buf, Some(&format)).unwrap();
        let got = got.downcast_ref::<ThreeU64Mutable>().unwrap();
        assert_eq!(*got, value);
    }

    #[test]
    fn xcdr2_mutable_three_u64_serialize_into_matches_serialize() {
        use crate::dcps::topic::type_support::{SerializationFormat, TypeSupport};

        let value = ThreeU64Mutable { a: 1, b: 2, c: 3 };
        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Mutable,
            use_delimiters: true,
        };
        let ts = ThreeU64Mutable::get_type_support();

        let serialized = ts.serialize(&value, Some(&format)).unwrap();
        let mut buf = Vec::new();
        ts.serialize_into(&value, &mut buf, Some(&format)).unwrap();

        assert_eq!(&buf[..], &serialized[..]);
    }
}
