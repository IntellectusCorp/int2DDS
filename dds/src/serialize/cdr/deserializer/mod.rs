mod array;
mod primitive;
mod sequence;
mod string;

use speedy::Endianness;

use super::CdrError;
use crate::serialize::core::endianness_from_bool;
use crate::serialize::{align_position_with_header_offset, DeserializerReader};

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
