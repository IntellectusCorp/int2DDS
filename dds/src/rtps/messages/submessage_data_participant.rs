use crate::rtps::common::{
    parameters::ParameterList,
    rtps_error_code::{RtpsError, RtpsErrorCode},
};
use speedy::{Endianness, Readable, Writable};

#[allow(dead_code)]
pub(crate) fn serialize_submessage_participant_data(
    parameter_list: &ParameterList,
    endianness: Endianness,
) -> Result<Vec<u8>, RtpsError> {
    let mut data: Vec<u8> = Vec::new();

    // encapsulation kind (correct byte order for little-endian)
    match endianness {
        Endianness::LittleEndian => data.extend([0x03, 0x00]), // PL_CDR_LE = 0x0003
        Endianness::BigEndian => data.extend([0x00, 0x02]),    // PL_CDR_BE = 0x0002
    }

    // encapsulation options
    data.extend([0x00, 0x00]);

    // parameter list
    match parameter_list.write_to_vec_with_ctx(endianness) {
        Ok(bytes) => data.extend(bytes),
        Err(e) => {
            return Err(RtpsError::new(
                RtpsErrorCode::SerializationError,
                format!("Failed to serialize parameter list: {:?}", e),
            ));
        }
    }

    Ok(data)
}

pub(crate) fn deserialize_submessage_participant_data(
    buffer: &[u8],
) -> Result<(ParameterList, Endianness), RtpsError> {
    if buffer.len() < 4 {
        return Err(RtpsError::new(
            RtpsErrorCode::DeserializationError,
            "Buffer too short for participant data",
        ));
    }

    // encapsulation kind (read in correct byte order)
    let encapsulation_kind = u16::from_le_bytes([buffer[0], buffer[1]]);
    let endianness = match encapsulation_kind {
        0x0003 => Endianness::LittleEndian,
        0x0002 => Endianness::BigEndian,
        _ => {
            return Err(RtpsError::new(
                RtpsErrorCode::DeserializationError,
                format!("Invalid encapsulation kind: 0x{:04x}", encapsulation_kind),
            ));
        }
    };

    // parameter list starts after 4 bytes
    let parameter_list = match ParameterList::read_from_buffer_with_ctx(endianness, &buffer[4..]) {
        Ok(list) => list,
        Err(e) => {
            return Err(RtpsError::new(
                RtpsErrorCode::DeserializationError,
                format!("Failed to deserialize parameter list: {:?}", e),
            ));
        }
    };

    Ok((parameter_list, endianness))
}
