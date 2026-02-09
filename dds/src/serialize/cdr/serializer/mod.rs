pub mod array;
pub mod primitive;
pub mod sequence;
pub mod string;

use speedy::Endianness;

use crate::serialize::align_buffer;

/// Common trait for CDR serializers (CdrSerializer and Xcdr2Serializer)
/// This trait abstracts the differences between CDR v1 and XCDR v2 serialization,
/// allowing shared implementation of primitive, array, sequence, and string serialization.
pub trait CdrSerializerCommon {
    /// Get the endianness setting
    fn endianness(&self) -> Endianness;

    /// Get mutable reference to the internal buffer
    fn buffer_mut(&mut self) -> &mut Vec<u8>;

    /// Get reference to the internal buffer
    fn buffer(&self) -> &[u8];

    /// Align buffer to the specified boundary
    /// Note: CdrSerializer uses standard CDR alignment (up to 8 bytes)
    ///       Xcdr2Serializer limits alignment to 4 bytes per XCDR2 spec
    fn align(&mut self, alignment: usize);
}

// Import serializer types from their definition modules
use super::{CdrSerializer, Xcdr2Serializer};

impl CdrSerializerCommon for CdrSerializer {
    #[inline]
    fn endianness(&self) -> Endianness {
        self.endianness
    }

    #[inline]
    fn buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    #[inline]
    fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    fn align(&mut self, alignment: usize) {
        // CDR alignment is relative to data start (after 4-byte encapsulation header)
        const ENCAPSULATION_HEADER_SIZE: usize = 4;
        let data_len = self.buffer.len().saturating_sub(ENCAPSULATION_HEADER_SIZE);
        let aligned_data_len = (data_len + alignment - 1) & !(alignment - 1);
        let target_len = ENCAPSULATION_HEADER_SIZE + aligned_data_len;
        if target_len > self.buffer.len() {
            self.buffer.resize(target_len, 0);
        }
    }
}

impl CdrSerializerCommon for Xcdr2Serializer {
    #[inline]
    fn endianness(&self) -> Endianness {
        self.endianness
    }

    #[inline]
    fn buffer_mut(&mut self) -> &mut Vec<u8> {
        &mut self.buffer
    }

    #[inline]
    fn buffer(&self) -> &[u8] {
        &self.buffer
    }

    #[inline]
    fn align(&mut self, alignment: usize) {
        // XCDR2 standard: limit to 4-byte max alignment to reduce padding
        let actual_alignment = std::cmp::min(alignment, 4);
        align_buffer(&mut self.buffer, actual_alignment);
    }
}
