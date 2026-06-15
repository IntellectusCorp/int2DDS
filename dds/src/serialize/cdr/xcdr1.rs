use bytes::Bytes;
use speedy::Endianness;

use super::cdr_input::CdrInput;
use super::{CdrError, ExtensibilityKind};
use crate::serialize::core::endianness_from_bool;
use crate::serialize::{
    align_position_with_header_offset, from_bytes_u16, from_bytes_u32, to_bytes_u16, to_bytes_u32,
    BufferManager, DeserializerReader,
};

pub(crate) const PID_SENTINEL: u16 = 0x3F02;
pub(crate) const PID_EXTENDED: u16 = 0x3F01;
const MAX_SHORT_MEMBER_ID: u16 = 0x3F00;
const MAX_SHORT_LENGTH: u16 = 0xFFFF;

pub struct CdrSerializer {
    pub(super) endianness: Endianness,
    pub(super) buffer: Vec<u8>,
    pub(super) header_size: usize,
    pub(super) extensibility: ExtensibilityKind,
}

impl CdrSerializer {
    pub fn new(little_endian: bool) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::new(),
            header_size: 0,
            extensibility: ExtensibilityKind::Appendable,
        }
    }

    pub fn new_mutable(little_endian: bool) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::new(),
            header_size: 0,
            extensibility: ExtensibilityKind::Mutable,
        }
    }

    pub fn with_capacity(little_endian: bool, capacity: usize) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::with_capacity(capacity),
            header_size: 0,
            extensibility: ExtensibilityKind::Appendable,
        }
    }

    pub fn with_extensibility(little_endian: bool, extensibility: ExtensibilityKind) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::new(),
            header_size: 0,
            extensibility,
        }
    }

    pub fn with_extensibility_and_buffer(
        little_endian: bool,
        extensibility: ExtensibilityKind,
        mut buffer: Vec<u8>,
    ) -> Self {
        buffer.clear();
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer,
            header_size: 0,
            extensibility,
        }
    }

    /// Consume the serializer and return the internal buffer (for ownership round-trip)
    pub fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }

    /// Write CDR encapsulation header
    pub fn write_encapsulation_header(&mut self) -> Result<(), CdrError> {
        let encap_id = match (self.extensibility, self.endianness) {
            (ExtensibilityKind::Mutable, Endianness::LittleEndian) => 0x0003u16,
            (ExtensibilityKind::Mutable, Endianness::BigEndian) => 0x0002u16,
            (_, Endianness::LittleEndian) => 0x0001u16,
            (_, Endianness::BigEndian) => 0x0000u16,
        };

        self.buffer.extend_from_slice(&encap_id.to_be_bytes());
        self.buffer.extend_from_slice(&0x0000u16.to_be_bytes());
        self.header_size = 4;

        Ok(())
    }

    pub fn get_header_size(&self) -> usize {
        self.header_size
    }

    pub fn position(&self) -> usize {
        self.buffer.len()
    }

    fn align_buffer(&mut self, alignment: usize) {
        let offset = self.buffer.len() % alignment;
        if offset != 0 {
            let padding = alignment - offset;
            self.buffer.extend(std::iter::repeat(0u8).take(padding));
        }
    }

    pub fn end_mutable_struct(&mut self) -> Result<(), CdrError> {
        self.align_buffer(4);
        self.buffer.extend_from_slice(&to_bytes_u16(PID_SENTINEL, self.endianness));
        self.buffer.extend_from_slice(&to_bytes_u16(0u16, self.endianness));
        Ok(())
    }

    pub fn write_member_with_v1<F>(
        &mut self,
        member_id: u32,
        must_understand: bool,
        write_value: F,
    ) -> Result<(), CdrError>
    where
        F: FnOnce(&mut Self) -> Result<(), CdrError>,
    {
        self.align_buffer(4);
        let header_pos = self.buffer.len();

        if member_id <= MAX_SHORT_MEMBER_ID as u32 {
            self.buffer.extend_from_slice(&[0u8; 4]);
        } else {
            self.buffer.extend_from_slice(&[0u8; 12]);
        }

        let content_start = self.buffer.len();
        write_value(self)?;
        let content_len = self.buffer.len() - content_start;

        let flags: u16 = if must_understand { 0x4000 } else { 0 };

        if member_id <= MAX_SHORT_MEMBER_ID as u32 && content_len <= MAX_SHORT_LENGTH as usize {
            let pid = flags | (member_id as u16 & 0x3FFF);
            let len = content_len as u16;
            self.buffer[header_pos..header_pos + 2]
                .copy_from_slice(&to_bytes_u16(pid, self.endianness));
            self.buffer[header_pos + 2..header_pos + 4]
                .copy_from_slice(&to_bytes_u16(len, self.endianness));
        } else {
            if member_id <= MAX_SHORT_MEMBER_ID as u32 {
                self.buffer.splice(header_pos + 4..header_pos + 4, [0u8; 8]);
            }
            let pid_ext = flags | PID_EXTENDED;
            self.buffer[header_pos..header_pos + 2]
                .copy_from_slice(&to_bytes_u16(pid_ext, self.endianness));
            self.buffer[header_pos + 2..header_pos + 4]
                .copy_from_slice(&to_bytes_u16(8u16, self.endianness));
            self.buffer[header_pos + 4..header_pos + 8]
                .copy_from_slice(&to_bytes_u32(member_id, self.endianness));
            self.buffer[header_pos + 8..header_pos + 12]
                .copy_from_slice(&to_bytes_u32(content_len as u32, self.endianness));
        }

        Ok(())
    }
}

impl BufferManager for CdrSerializer {
    fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    fn as_bytes(&self) -> &[u8] {
        &self.buffer
    }

    fn reset(&mut self) {
        self.buffer.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlCdrMemberHeader {
    Short { pid: u16, length: u16, must_understand: bool },
    Long { member_id: u32, length: u32, must_understand: bool },
    Sentinel,
}

pub struct CdrDeserializer<'a> {
    pub(super) endianness: Endianness,
    pub(super) input: CdrInput<'a>,
    pub(super) position: usize,
}

impl<'a> CdrDeserializer<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, CdrError> {
        if data.len() < 4 {
            return Err(CdrError::InsufficientData);
        }

        let encap_id = u16::from_be_bytes([data[0], data[1]]);

        let endianness = match encap_id {
            0x0000 | 0x0002 => Endianness::BigEndian,
            0x0001 | 0x0003 => Endianness::LittleEndian,
            _ => return Err(CdrError::InvalidEncapsulation(encap_id)),
        };

        Ok(Self { endianness, input: CdrInput::Contiguous(&data[4..]), position: 0 })
    }

    pub fn new_without_header(data: &'a [u8], little_endian: bool) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            input: CdrInput::Contiguous(data),
            position: 0,
        }
    }

    // Fragment receive path: deserialize directly across fragment chunks with no
    // contiguous reassembly buffer. Reads the 4-byte encapsulation header from the
    // chunks (like `new`), then exposes the body via `skip` so positions stay
    // body-relative.
    pub fn new_chained(chunks: &'a [Bytes]) -> Result<Self, CdrError> {
        let total: usize = chunks.iter().map(|c| c.len()).sum();
        if total < 4 {
            return Err(CdrError::InsufficientData);
        }

        let header = CdrInput::Chained { chunks, skip: 0 }.read_array::<4>(0);
        let encap_id = u16::from_be_bytes([header[0], header[1]]);

        let endianness = match encap_id {
            0x0000 | 0x0002 => Endianness::BigEndian,
            0x0001 | 0x0003 => Endianness::LittleEndian,
            _ => return Err(CdrError::InvalidEncapsulation(encap_id)),
        };

        Ok(Self { endianness, input: CdrInput::Chained { chunks, skip: 4 }, position: 0 })
    }

    pub(super) fn align(&mut self, alignment: usize) {
        align_position_with_header_offset(&mut self.position, alignment, 0);
    }

    #[inline]
    pub(super) fn check_available(&self, size: usize) -> Result<(), CdrError> {
        if self.position + size > self.input.len() {
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

    pub fn read_parameter_header(&mut self) -> Result<PlCdrMemberHeader, CdrError> {
        self.align(4);
        self.check_available(4)?;

        let pid_bytes = self.input.read_array::<2>(self.position);
        let pid = from_bytes_u16(pid_bytes, self.endianness);

        let len_bytes = self.input.read_array::<2>(self.position + 2);
        let length = from_bytes_u16(len_bytes, self.endianness);

        let raw_pid = pid & 0x3FFF;

        if raw_pid == (PID_SENTINEL & 0x3FFF) {
            self.position += 4;
            return Ok(PlCdrMemberHeader::Sentinel);
        }

        let must_understand = (pid & 0x4000) != 0;

        if raw_pid == (PID_EXTENDED & 0x3FFF) {
            self.position += 4;
            self.check_available(8)?;

            let id_bytes = self.input.read_array::<4>(self.position);
            let member_id = from_bytes_u32(id_bytes, self.endianness);

            let mlen_bytes = self.input.read_array::<4>(self.position + 4);
            let member_length = from_bytes_u32(mlen_bytes, self.endianness);

            self.position += 8;

            return Ok(PlCdrMemberHeader::Long {
                member_id,
                length: member_length,
                must_understand,
            });
        }

        self.position += 4;
        Ok(PlCdrMemberHeader::Short { pid: raw_pid, length, must_understand })
    }

    pub fn is_at_sentinel(&self) -> bool {
        if self.position + 4 > self.input.len() {
            return false;
        }
        let pid_bytes = self.input.read_array::<2>(self.position);
        let pid = from_bytes_u16(pid_bytes, self.endianness);
        (pid & 0x3FFF) == (PID_SENTINEL & 0x3FFF)
    }

    pub fn skip(&mut self, n: usize) -> Result<(), CdrError> {
        self.check_available(n)?;
        self.position += n;
        Ok(())
    }
}

impl<'a> DeserializerReader for CdrDeserializer<'a> {
    type Error = CdrError;

    fn check_available(&self, size: usize) -> Result<(), Self::Error> {
        self.check_available(size)
    }

    fn copy_bytes_at(&self, offset: usize, out: &mut [u8]) {
        self.input.copy_to(offset, out);
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

#[cfg(test)]
mod chained_tests {
    use super::*;

    // Split a byte buffer into chunks of the given sizes.
    fn split(buf: &[u8], sizes: &[usize]) -> Vec<Bytes> {
        let mut out = Vec::new();
        let mut pos = 0;
        for &n in sizes {
            out.push(Bytes::copy_from_slice(&buf[pos..pos + n]));
            pos += n;
        }
        assert_eq!(pos, buf.len(), "chunk sizes must cover the whole buffer");
        out
    }

    // A hand-built little-endian CDR buffer: 4-byte encap header + body holding a
    // u64, a length-prefixed byte sequence, padding, then a second u64. The second
    // u64 forces 8-byte alignment after an odd position.
    fn sample_buffer() -> Vec<u8> {
        let mut buf = vec![0x00, 0x01, 0x00, 0x00]; // CDR_LE encapsulation
        buf.extend_from_slice(&0x0102030405060708u64.to_le_bytes()); // body 0..8
        buf.extend_from_slice(&3u32.to_le_bytes()); // seq length, body 8..12
        buf.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // seq bytes, body 12..15
        buf.push(0x00); // pad to 8-byte body alignment, body 15
        buf.extend_from_slice(&0x1112131415161718u64.to_le_bytes()); // body 16..24
        buf
    }

    fn read_fields(de: &mut CdrDeserializer) -> (u64, Vec<u8>, u64) {
        let a = de.deserialize_u64().unwrap();
        let seq = de.deserialize_byte_sequence().unwrap();
        let b = de.deserialize_u64().unwrap();
        (a, seq, b)
    }

    #[test]
    fn chained_matches_contiguous_with_header_and_alignment() {
        let buf = sample_buffer();

        let mut contig = CdrDeserializer::new(&buf).unwrap();
        let expected = read_fields(&mut contig);
        assert_eq!(expected.0, 0x0102030405060708);
        assert_eq!(expected.1, vec![0xAA, 0xBB, 0xCC]);
        assert_eq!(expected.2, 0x1112131415161718);

        // Awkward split: the first u64 (absolute bytes 4..12) spans three chunks.
        let chunks = split(&buf, &[5, 6, 4, 8, 5]);
        let mut chained = CdrDeserializer::new_chained(&chunks).unwrap();
        let got = read_fields(&mut chained);

        assert_eq!(got, expected);
    }
}
