use log::{debug, warn};

use crate::rtps::common::parameters::{ParameterValue, PlCdrParameter};

mod builtins;
mod constants;
pub mod inline_qos_deserialize;
mod inline_qos_parameters;
pub mod inline_qos_serialize;
pub mod pl_cdr_deserialize;
pub mod pl_cdr_serialize;
pub(crate) mod reader;

pub use builtins::ParsedBuiltinTopicData;
pub use constants::{
    MAX_DATA_SIZE_BYTES, MAX_PARAMETER_ITERATIONS, PARAMETER_ALIGNMENT, PL_CDR_BE, PL_CDR_LE,
};
pub use inline_qos_parameters::InlineQosParameters;

pub use inline_qos_deserialize::InlineQosParser;
pub use inline_qos_serialize::InlineQosSerializer;
pub use pl_cdr_deserialize::PlCdrParser;
pub use pl_cdr_serialize::{
    append_pl_cdr_sentinel, combine_pl_cdr_parameters, discovery_helpers, merge_serialized_data,
    remove_pl_cdr_sentinel, PlCdrSerializer, RtpsMessageBuilder,
};

pub type Parameter<'a> = PlCdrParameter<'a>;

/// Fast endianness detection based on the encapsulation header.
pub fn detect_endianness(data: &[u8]) -> bool {
    if data.len() < 2 {
        return false; // Default to little endian
    }

    let encapsulation_kind = u16::from_be_bytes([data[0], data[1]]);
    match encapsulation_kind {
        PL_CDR_BE => true,
        PL_CDR_LE => false,
        _ => {
            // Try little endian interpretation
            let kind_le = u16::from_le_bytes([data[0], data[1]]);
            match kind_le {
                PL_CDR_BE => true,
                PL_CDR_LE => false,
                _ => {
                    warn!(
                        "Unknown encapsulation kind: 0x{:04x}, defaulting to little endian",
                        encapsulation_kind
                    );
                    false
                }
            }
        }
    }
}

/// Main parsing entry point for PL-CDR discovery data.
pub fn parse_discovery_data(data: &[u8]) -> Result<Vec<PlCdrParameter<'_>>, String> {
    // Input validation
    if data.is_empty() {
        debug!("Empty data provided to parse_discovery_data");
        return Ok(Vec::new());
    }

    if data.len() < 4 {
        debug!("Data too short for encapsulation header, attempting graceful recovery");
        // Graceful fallback: parse without header
        let parser = PlCdrParser::new(false); // Default to little endian
        return parser.parse(data);
    }

    // Input data size limit
    if data.len() > MAX_DATA_SIZE_BYTES {
        return Err("Input data too large: maximum 50MB allowed".to_string());
    }

    let big_endian = detect_endianness(data);
    let parser = PlCdrParser::new(big_endian);

    // Extract padding length from encapsulation header byte[3]
    // (XCDR standard: padding length is stored in options[1])
    let padding_len = data[3] as usize;

    // Calculate actual payload without header and trailing padding
    let payload_start = 4;
    let payload_end = data.len().saturating_sub(padding_len);

    if payload_end <= payload_start {
        debug!("Invalid payload size after removing padding");
        return Ok(Vec::new());
    }

    let payload = &data[payload_start..payload_end];

    parser.parse(payload)
}

/// Find a specific parameter by ID.
pub fn find_parameter_by_id<'a>(
    parameters: &'a [PlCdrParameter<'a>],
    id: u16,
) -> Option<&'a ParameterValue<'a>> {
    parameters.iter().find(|p| p.id as u16 == id).map(|p| &p.value)
}

/// Serialize a list of parameters to PL-CDR encoded bytes.
pub fn serialize_parameters(
    parameters: &[PlCdrParameter],
    little_endian: bool,
) -> Result<Vec<u8>, String> {
    let serializer = PlCdrSerializer::new(little_endian);
    serializer.serialize_parameters(parameters)
}
