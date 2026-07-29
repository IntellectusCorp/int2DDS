use speedy::Endianness;

use crate::rtps::builtin::data::content_filtered_topic::ContentFilterInfo;

/// Serializer specifically designed for inline QoS parameters in RTPS DATA messages
pub struct InlineQosSerializer {
    endianness: Endianness,
}

impl InlineQosSerializer {
    /// * `big_endian` - true for big endian, false for little endian
    pub fn new(big_endian: bool) -> Self {
        Self {
            endianness: if big_endian { Endianness::BigEndian } else { Endianness::LittleEndian },
        }
    }

    /// Write u32 with endianness consideration
    fn write_u32(&self, buffer: &mut Vec<u8>, value: u32) {
        let bytes = match self.endianness {
            Endianness::LittleEndian => value.to_le_bytes(),
            Endianness::BigEndian => value.to_be_bytes(),
        };
        buffer.extend_from_slice(&bytes);
    }

    /// Write i32 with endianness consideration
    fn write_i32(&self, buffer: &mut Vec<u8>, value: i32) {
        let bytes = match self.endianness {
            Endianness::LittleEndian => value.to_le_bytes(),
            Endianness::BigEndian => value.to_be_bytes(),
        };
        buffer.extend_from_slice(&bytes);
    }

    /// Serialize ContentFilterInfo to raw bytes (DDS-RTPS 2.5 section 9.6.4.1)
    pub fn serialize_content_filter_info(
        &self,
        content_filter_info: &ContentFilterInfo,
    ) -> Result<Vec<u8>, String> {
        // Pre-calculate exact buffer size to avoid reallocations
        // Format: 4 (numBitmaps) + 4*num_bitmaps + 4 (numSignatures) + 16*num_signatures
        let num_bitmaps = content_filter_info.filter_result.len();
        let num_signatures = content_filter_info.filter_signatures.len();
        let capacity = 8 + (4 * num_bitmaps) + (16 * num_signatures);

        let mut buffer: Vec<u8> = Vec::with_capacity(capacity);

        // numBitmaps
        self.write_u32(&mut buffer, num_bitmaps as u32);

        // bitmaps
        for &bitmap in &content_filter_info.filter_result {
            self.write_i32(&mut buffer, bitmap);
        }

        // numSignatures
        self.write_u32(&mut buffer, num_signatures as u32);

        // signatures (128-bit MD5)
        for signature in &content_filter_info.filter_signatures {
            buffer.extend_from_slice(signature);
        }

        Ok(buffer)
    }
}

// Wire-format snapshots: bytes must stay identical across refactors that
// claim to preserve wire compatibility (DDS-RTPS 2.5 section 9.6.4.1).
#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::builtin::data::content_filtered_topic::ContentFilterInfo;

    #[test]
    fn serialize_content_filter_info_little_endian_single_entry() {
        let info = ContentFilterInfo {
            filter_result: vec![0x12345678],
            filter_signatures: vec![[0xAA; 16]],
        };
        let bytes = InlineQosSerializer::new(false).serialize_content_filter_info(&info).unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&1u32.to_le_bytes()); // numBitmaps
        expected.extend_from_slice(&0x12345678i32.to_le_bytes()); // bitmap[0]
        expected.extend_from_slice(&1u32.to_le_bytes()); // numSignatures
        expected.extend_from_slice(&[0xAA; 16]); // signature[0]
        assert_eq!(bytes, expected);
    }

    #[test]
    fn serialize_content_filter_info_little_endian_empty() {
        let info = ContentFilterInfo { filter_result: vec![], filter_signatures: vec![] };
        let bytes = InlineQosSerializer::new(false).serialize_content_filter_info(&info).unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&0u32.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(bytes, expected);
    }

    #[test]
    fn serialize_content_filter_info_little_endian_two_entries() {
        let info = ContentFilterInfo {
            filter_result: vec![-1, 1],
            filter_signatures: vec![[0x11; 16], [0x22; 16]],
        };
        let bytes = InlineQosSerializer::new(false).serialize_content_filter_info(&info).unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&2u32.to_le_bytes());
        expected.extend_from_slice(&(-1i32).to_le_bytes());
        expected.extend_from_slice(&1i32.to_le_bytes());
        expected.extend_from_slice(&2u32.to_le_bytes());
        expected.extend_from_slice(&[0x11; 16]);
        expected.extend_from_slice(&[0x22; 16]);
        assert_eq!(bytes, expected);
    }

    #[test]
    fn serialize_content_filter_info_big_endian_single_entry() {
        let info =
            ContentFilterInfo { filter_result: vec![1], filter_signatures: vec![[0xAA; 16]] };
        let bytes = InlineQosSerializer::new(true).serialize_content_filter_info(&info).unwrap();

        let mut expected = Vec::new();
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&1i32.to_be_bytes());
        expected.extend_from_slice(&1u32.to_be_bytes());
        expected.extend_from_slice(&[0xAA; 16]);
        assert_eq!(bytes, expected);
    }
}
