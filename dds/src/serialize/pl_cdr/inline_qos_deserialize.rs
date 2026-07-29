use log::{debug, warn};
use smallvec::SmallVec;
use speedy::Endianness;

use crate::rtps::{
    builtin::data::content_filtered_topic::ContentFilterInfo,
    common::parameters::{u16_to_parameter_id, Parameter, ParameterId, ParameterList},
};

use super::reader::PlCdrReader;

const MAX_INLINE_QOS_PARAMETERS: usize = 100;

/// Parser specifically designed for inline QoS parameters in RTPS DATA messages
pub struct InlineQosParser {
    endianness: Endianness,
}

impl InlineQosParser {
    /// * `big_endian` - true for big endian, false for little endian
    pub fn new(big_endian: bool) -> Self {
        Self {
            endianness: if big_endian { Endianness::BigEndian } else { Endianness::LittleEndian },
        }
    }

    pub fn parse_inline_qos_to_parameter_list(
        &self,
        parameter_list: ParameterList,
    ) -> ParameterList {
        parameter_list
    }

    /// Parse raw bytes into ParameterList, returns bytes consumed
    fn parse_raw_bytes_internal_with_size(
        &self,
        data: &[u8],
        mut parameters: Option<&mut ParameterList>,
    ) -> Result<usize, String> {
        if data.is_empty() {
            return Ok(0);
        }

        let mut reader = PlCdrReader::new(data, self.endianness);
        let mut iteration_count = 0;

        while !reader.finished() && iteration_count < MAX_INLINE_QOS_PARAMETERS {
            iteration_count += 1;

            let param_id = match reader.read_u16() {
                Ok(id) => id,
                Err(_) => break,
            };

            if param_id == ParameterId::PidSentinel as u16 {
                let _ = reader.read_u16();
                if let Some(ref mut parameters) = parameters {
                    parameters
                        .add_parameter(Parameter::new(ParameterId::PidSentinel, SmallVec::new()));
                }
                break;
            }

            let param_length = match reader.read_u16() {
                Ok(len) => len,
                Err(_) => break,
            };

            let param_data = match reader.read_bytes(param_length as usize) {
                Ok(bytes) => bytes,
                Err(_) => break,
            };

            reader.align_parameters();

            if param_id == ParameterId::PidPad as u16 {
                continue;
            }

            let param_id_enum = u16_to_parameter_id(param_id);

            if let Some(ref mut parameters) = parameters {
                if param_id_enum == ParameterId::PidContentFilterInfo {
                    debug!(
                        "Parsing PID_CONTENT_FILTER_INFO, data length: {} bytes",
                        param_data.len()
                    );

                    match self.parse_content_filter_info(param_data) {
                        Ok(content_filter_info) => {
                            debug!(
                                "Successfully parsed content filter info: {} bitmaps, {} signatures",
                                content_filter_info.filter_result.len(),
                                content_filter_info.filter_signatures.len()
                            );
                            for (i, bitmap) in content_filter_info.filter_result.iter().enumerate()
                            {
                                debug!("  bitmap[{}]: 0x{:08X}", i, bitmap);
                            }
                            for (i, sig) in content_filter_info.filter_signatures.iter().enumerate()
                            {
                                debug!("  signature[{}]: {:02X?}", i, sig);
                            }
                            parameters
                                .add_parameter(Parameter::new(param_id_enum, param_data.to_vec()));
                        }
                        Err(e) => {
                            warn!(
                                "Failed to parse content filter info: {}, storing as raw bytes",
                                e
                            );
                            parameters
                                .add_parameter(Parameter::new(param_id_enum, param_data.to_vec()));
                        }
                    }
                } else {
                    parameters.add_parameter(Parameter::new(param_id_enum, param_data.to_vec()));
                }
            }
        }

        Ok(reader.position())
    }

    /// Parse inline QoS data and return both the parameter list and the number of bytes consumed
    pub fn parse_inline_qos(&self, data: &[u8]) -> Result<(ParameterList, usize), String> {
        let mut param_list = ParameterList::default();
        let consumed_bytes =
            self.parse_raw_bytes_internal_with_size(data, Some(&mut param_list))?;
        Ok((param_list, consumed_bytes))
    }

    /// Parse Content Filter Info (DDS-RTPS 2.5 spec 9.6.4.1)
    pub(crate) fn parse_content_filter_info(
        &self,
        data: &[u8],
    ) -> Result<ContentFilterInfo, String> {
        let mut reader = PlCdrReader::new(data, self.endianness);

        let num_bitmaps = reader.read_u32()? as usize;
        let mut filter_result = Vec::new();
        for _ in 0..num_bitmaps {
            let bitmap = reader.read_i32()?;
            filter_result.push(bitmap);
        }

        let num_signatures = reader.read_u32()? as usize;
        let expected_bitmaps =
            (num_signatures / 32) + if !num_signatures.is_multiple_of(32) { 1 } else { 0 };
        if num_bitmaps != expected_bitmaps {
            return Err(format!(
                "Invalid content filter info: numBitmaps={} but expected {} for numSignatures={}",
                num_bitmaps, expected_bitmaps, num_signatures
            ));
        }

        let mut filter_signatures = Vec::new();
        for _ in 0..num_signatures {
            let bytes = reader.read_bytes(16)?;
            let mut signature = [0u8; 16];
            signature.copy_from_slice(bytes);
            filter_signatures.push(signature);
        }

        Ok(ContentFilterInfo { filter_result, filter_signatures })
    }
}
