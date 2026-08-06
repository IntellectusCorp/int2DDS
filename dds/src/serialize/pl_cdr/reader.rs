use speedy::Endianness;

use super::constants::PARAMETER_ALIGNMENT;

/// Streaming reader for PL-CDR encoded parameter sequences.
///
/// The reader makes no progress guarantee of its own: a read that fails leaves the position
/// untouched, `read_bytes(0)` advances by nothing, and aligning an already aligned position
/// is a no-op. A loop driving this reader over untrusted bytes therefore has to stop on an
/// error, or consume at least one byte before it continues, or it will never end.
pub(crate) struct PlCdrReader<'a> {
    data: &'a [u8],
    position: usize,
    endianness: Endianness,
}

impl<'a> PlCdrReader<'a> {
    pub(crate) fn new(data: &'a [u8], endianness: Endianness) -> Self {
        Self { data, position: 0, endianness }
    }

    #[inline]
    pub(crate) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    #[inline]
    pub(crate) fn position(&self) -> usize {
        self.position
    }

    #[inline]
    pub(crate) fn finished(&self) -> bool {
        self.position >= self.data.len()
    }

    pub(crate) fn read_u16(&mut self) -> Result<u16, String> {
        const BYTES: usize = 2;
        if self.remaining() < BYTES {
            return Err(format!(
                "insufficient data for u16: need {BYTES} bytes, have {}",
                self.remaining()
            ));
        }

        let bytes = [self.data[self.position], self.data[self.position + 1]];
        self.position += BYTES;

        Ok(match self.endianness {
            Endianness::LittleEndian => u16::from_le_bytes(bytes),
            Endianness::BigEndian => u16::from_be_bytes(bytes),
        })
    }

    pub(crate) fn read_u32(&mut self) -> Result<u32, String> {
        const BYTES: usize = 4;
        if self.remaining() < BYTES {
            return Err(format!(
                "insufficient data for u32: need {BYTES} bytes, have {}",
                self.remaining()
            ));
        }

        let bytes = [
            self.data[self.position],
            self.data[self.position + 1],
            self.data[self.position + 2],
            self.data[self.position + 3],
        ];
        self.position += BYTES;

        Ok(match self.endianness {
            Endianness::LittleEndian => u32::from_le_bytes(bytes),
            Endianness::BigEndian => u32::from_be_bytes(bytes),
        })
    }

    pub(crate) fn read_i32(&mut self) -> Result<i32, String> {
        const BYTES: usize = 4;
        if self.remaining() < BYTES {
            return Err(format!(
                "insufficient data for i32: need {BYTES} bytes, have {}",
                self.remaining()
            ));
        }

        let bytes = [
            self.data[self.position],
            self.data[self.position + 1],
            self.data[self.position + 2],
            self.data[self.position + 3],
        ];
        self.position += BYTES;

        Ok(match self.endianness {
            Endianness::LittleEndian => i32::from_le_bytes(bytes),
            Endianness::BigEndian => i32::from_be_bytes(bytes),
        })
    }

    pub(crate) fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], String> {
        if self.remaining() < len {
            return Err(format!("insufficient data for {len} bytes, have {}", self.remaining()));
        }

        let bytes = &self.data[self.position..self.position + len];
        self.position += len;
        Ok(bytes)
    }

    #[inline]
    pub(crate) fn align_parameters(&mut self) {
        self.align_to(PARAMETER_ALIGNMENT);
    }

    #[inline]
    fn align_to(&mut self, alignment: usize) {
        let aligned = (self.position + (alignment - 1)) & !(alignment - 1);
        self.position = aligned.min(self.data.len());
    }
}
