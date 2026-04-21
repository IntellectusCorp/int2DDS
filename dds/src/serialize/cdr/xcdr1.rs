use speedy::Endianness;

use super::CdrError;
use crate::serialize::core::endianness_from_bool;
use crate::serialize::{align_position_with_header_offset, BufferManager, DeserializerReader};

/// XCDR v1 (CDR) Serializer
/// Legacy RTPS 2.x serialization for FINAL types only
pub struct CdrSerializer {
    pub(super) endianness: Endianness,
    pub(super) buffer: Vec<u8>,
    /// Size of encapsulation header (0 if no header written, 4 after header is written)
    /// Used for alignment calculation relative to data stream start
    pub(super) header_size: usize,
}

impl CdrSerializer {
    /// Create a new CDR serializer
    pub fn new(little_endian: bool) -> Self {
        Self { endianness: endianness_from_bool(little_endian), buffer: Vec::new(), header_size: 0 }
    }

    /// Create CDR serializer with pre-allocated capacity
    pub fn with_capacity(little_endian: bool, capacity: usize) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::with_capacity(capacity),
            header_size: 0,
        }
    }

    /// Create CDR serializer reusing an existing buffer (capacity preserved, content cleared)
    pub fn reuse_buffer(little_endian: bool, mut buffer: Vec<u8>) -> Self {
        buffer.clear();
        Self { endianness: endianness_from_bool(little_endian), buffer, header_size: 0 }
    }

    /// Consume the serializer and return the internal buffer (for ownership round-trip)
    pub fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }

    /// Write CDR encapsulation header
    pub fn write_encapsulation_header(&mut self) -> Result<(), CdrError> {
        // CDR encapsulation identifier
        let encap_id = match self.endianness {
            Endianness::LittleEndian => 0x0001u16, // CDR_LE
            Endianness::BigEndian => 0x0000u16,    // CDR_BE
        };

        // Always write encapsulation header in big-endian
        self.buffer.extend_from_slice(&encap_id.to_be_bytes());

        // Options (2 bytes) - reserved, always 0x0000
        self.buffer.extend_from_slice(&0x0000u16.to_be_bytes());

        // Mark that header has been written (4 bytes)
        self.header_size = 4;

        Ok(())
    }

    /// Get the header size (for alignment calculations)
    pub fn get_header_size(&self) -> usize {
        self.header_size
    }
}

impl BufferManager for CdrSerializer {
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

/// XCDR v1 (CDR) Deserializer
pub struct CdrDeserializer<'a> {
    pub(super) endianness: Endianness,
    pub(super) data: &'a [u8],
    pub(super) position: usize,
}

impl<'a> CdrDeserializer<'a> {
    /// Create a new CDR deserializer
    pub fn new(data: &'a [u8]) -> Result<Self, CdrError> {
        if data.len() < 4 {
            return Err(CdrError::InsufficientData);
        }

        // Read encapsulation header
        let encap_id = u16::from_be_bytes([data[0], data[1]]);

        let endianness = match encap_id {
            0x0000 => Endianness::BigEndian,    // CDR_BE
            0x0001 => Endianness::LittleEndian, // CDR_LE
            _ => return Err(CdrError::InvalidEncapsulation(encap_id)),
        };

        Ok(Self {
            endianness,
            data: &data[4..], // Skip encapsulation header
            position: 0,
        })
    }

    /// Create deserializer without encapsulation header
    pub fn new_without_header(data: &'a [u8], little_endian: bool) -> Self {
        Self { endianness: endianness_from_bool(little_endian), data, position: 0 }
    }

    /// Align position to boundary
    /// CDR alignment is relative to data start (after encapsulation header)
    pub(super) fn align(&mut self, alignment: usize) {
        align_position_with_header_offset(&mut self.position, alignment, 0);
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
}

impl<'a> DeserializerReader for CdrDeserializer<'a> {
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
