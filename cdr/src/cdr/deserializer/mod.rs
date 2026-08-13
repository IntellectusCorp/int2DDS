mod array;
mod primitive;
mod sequence;
mod string;

use speedy::Endianness;

use super::CdrError;
use crate::core::endianness_from_bool;
use crate::{align_position_with_header_offset, DeserializerReader};

/// Deprecated. Use `cdr::CdrDeserializer` (re-export of `xcdr1::CdrDeserializer`).
#[allow(dead_code)]
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

#[cfg(test)]
#[allow(unused_imports)]
mod cdr_error_tests {
    use crate::{
        cdr::{
            CdrDeserialize, CdrDeserializer, CdrSerialize, CdrSerializer, ExtensibilityKind,
            XcdrDeserialize, XcdrDeserializer, XcdrSerialize, XcdrSerializer,
        },
        BufferManager, DeserializerReader, WChar, WString,
    };
    use std::collections::HashMap;
    #[test]
    fn test_cdr_insufficient_data() {
        // Empty data should fail
        let result = CdrDeserializer::new(&[]);
        assert!(result.is_err());

        // Only 2 bytes (incomplete header)
        let result = CdrDeserializer::new(&[0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn test_cdr_invalid_encapsulation() {
        // Invalid encapsulation identifier
        let data = [0xFF, 0xFF, 0x00, 0x00];
        let result = CdrDeserializer::new(&data);
        assert!(result.is_err());
    }

    // Tuple Struct Tests
}
