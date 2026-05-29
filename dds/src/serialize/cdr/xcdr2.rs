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

    /// Begin struct serialization (write DHEADER placeholder if needed)
    pub fn begin_struct(&mut self) -> Result<usize, CdrError> {
        match self.extensibility_kind {
            ExtensibilityKind::Final => Ok(0), // No header needed
            ExtensibilityKind::Appendable | ExtensibilityKind::Mutable => {
                // Align BEFORE recording size_pos so that backpatching in
                // end_struct writes to the actual DHEADER position, not into
                // alignment padding bytes.
                self.align(4);
                let size_pos = self.buffer.len();
                let bytes = to_bytes_u32(0, self.endianness);
                self.buffer.extend_from_slice(&bytes); // placeholder
                Ok(size_pos)
            }
        }
    }

    /// End struct serialization (backpatch size if needed)
    pub fn end_struct(&mut self, size_pos: usize) -> Result<(), CdrError> {
        match self.extensibility_kind {
            ExtensibilityKind::Final => Ok(()), // No backpatching needed
            ExtensibilityKind::Appendable | ExtensibilityKind::Mutable => {
                if size_pos > 0 {
                    let current_pos = self.buffer.len();
                    let object_size = (current_pos - size_pos - 4) as u32;

                    // Backpatch the size
                    let size_bytes = to_bytes_u32(object_size, self.endianness);

                    self.buffer[size_pos..size_pos + 4].copy_from_slice(&size_bytes);
                }
                Ok(())
            }
        }
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
        if self.position + size > self.data.len() {
            Err(CdrError::InsufficientData)
        } else {
            Ok(())
        }
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

    fn get_data(&self) -> &[u8] {
        self.data
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
