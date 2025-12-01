use speedy::Endianness;

use crate::{
    rtps::{
        builtin::data::content_filtered_topic::ContentFilterInfo,
        common::parameters::{ParameterId, ParameterList},
    },
    serialize::core::PooledBuffer,
};

use super::PARAMETER_ALIGNMENT;

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

    /// Serialize inline QoS parameters from ParameterList to raw bytes
    pub fn serialize_parameter_list(&self, parameters: &ParameterList) -> Result<Vec<u8>, String> {
        let params = parameters.parameters();
        if params.is_empty() {
            // debug!("Empty parameter list - returning empty bytes");
            return Ok(Vec::new());
        }

        let estimated_capacity: usize = params
            .iter()
            .take_while(|p| p.parameter_id() != ParameterId::PidSentinel)
            .map(|p| {
                let value_len = p.value().len();
                let padding =
                    (PARAMETER_ALIGNMENT - (value_len % PARAMETER_ALIGNMENT)) % PARAMETER_ALIGNMENT;
                4 + value_len + padding // 4 = param_id (2) + length (2)
            })
            .sum::<usize>()
            + 4; // sentinel

        // Use buffer pool for inline QoS serialization
        let mut buffer = PooledBuffer::with_capacity(estimated_capacity);

        for param in &params {
            if param.parameter_id() == ParameterId::PidSentinel {
                break;
            }

            self.write_u16(&mut buffer, param.parameter_id() as u16);
            let param_value = param.value();
            self.write_u16(&mut buffer, param_value.len() as u16);
            buffer.extend_from_slice(param_value);

            // Add padding to align to 4-byte boundary
            self.add_padding(&mut buffer, param_value.len());
        }

        // Add sentinel parameter
        self.write_u16(&mut buffer, ParameterId::PidSentinel as u16);
        self.write_u16(&mut buffer, 0); // length = 0

        // debug!("Completed inline QoS serialization: {} bytes", buffer.len());
        Ok(buffer.into_vec())
    }

    /// Add padding to align to 4-byte boundary
    fn add_padding(&self, buffer: &mut Vec<u8>, data_len: usize) {
        let padding =
            (PARAMETER_ALIGNMENT - (data_len % PARAMETER_ALIGNMENT)) % PARAMETER_ALIGNMENT;
        for _ in 0..padding {
            buffer.push(0);
        }
    }

    /// Write u16 with endianness consideration
    fn write_u16(&self, buffer: &mut Vec<u8>, value: u16) {
        let bytes = match self.endianness {
            Endianness::LittleEndian => value.to_le_bytes(),
            Endianness::BigEndian => value.to_be_bytes(),
        };
        buffer.extend_from_slice(&bytes);
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

        // Use buffer pool for content filter info serialization
        let mut buffer = PooledBuffer::with_capacity(capacity);

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

        Ok(buffer.into_vec())
    }
}
