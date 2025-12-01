mod array;
mod primitive;
mod sequence;
mod string;

use speedy::Endianness;

use super::CdrError;
use crate::serialize::core::endianness_from_bool;
use crate::serialize::BufferManager;

pub struct CdrSerializer {
    pub(super) endianness: Endianness,
    pub(super) buffer: Vec<u8>,
}

impl CdrSerializer {
    /// Create a new CDR serializer
    pub fn new(little_endian: bool) -> Self {
        Self { endianness: endianness_from_bool(little_endian), buffer: Vec::new() }
    }

    /// Create CDR serializer with pre-allocated capacity
    pub fn with_capacity(little_endian: bool, capacity: usize) -> Self {
        Self {
            endianness: endianness_from_bool(little_endian),
            buffer: Vec::with_capacity(capacity),
        }
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

        // debug!(
        //     "CDR encapsulation header written: ID=0x{:04X}, endianness={:?}",
        //     encap_id, self.endianness
        // );
        Ok(())
    }
}

impl BufferManager for CdrSerializer {
    /// Get the serialized data
    fn into_bytes(self) -> Vec<u8> {
        // debug!(
        //     "CDR serialization complete: {} bytes total (header: {:02X?})",
        //     self.buffer.len(),
        //     &self.buffer[..std::cmp::min(self.buffer.len(), 8)]
        // );
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
