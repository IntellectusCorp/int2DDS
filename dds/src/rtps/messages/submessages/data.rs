//! DATA submessage for transmitting user data samples.
//!
//! This module implements the DATA submessage which carries serialized user data
//! samples from DataWriters to DataReaders. DATA submessages include sequence numbers,
//! optional inline QoS, and the serialized payload.

use bytes::Bytes;
use speedy::{Context, Error, Readable, Writable, Writer};
use std::{io, sync::Arc};

use crate::rtps::common::{
    parameters::ParameterList, rtps_error_code::RtpsResult, sequence::SequenceNumber,
    types::SerializedData,
};
use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode},
    },
    messages::submessage_header::SubmessageHeader,
};
use crate::serialize::pl_cdr::InlineQosParser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Data {
    extra_flags: u16,
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub writer_sn: SequenceNumber,
    inline_qos: Option<ParameterList>,
    serialized_data: SerializedData,
    octets_to_inline_qos: u16,
}

impl Data {
    pub(crate) fn new(reader_id: EntityId, writer_id: EntityId, writer_sn: SequenceNumber) -> Self {
        Self {
            extra_flags: 0,
            reader_id,
            writer_id,
            writer_sn,
            inline_qos: None,
            serialized_data: Arc::<[u8]>::from([]),
            octets_to_inline_qos: 16,
        }
    }

    pub(crate) fn inline_qos(&self) -> Option<ParameterList> {
        self.inline_qos.clone()
    }

    pub(crate) fn serialized_data(&self) -> SerializedData {
        self.serialized_data.clone()
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        2  /* extra_flags */
         + 2  /* octets_to_inline_qos */
         + 4  /* reader_id */
         + 4  /* writer_id */
         + 8  /* writer_sn */
         + match &self.inline_qos {
            Some(param_list) => {
                param_list.length() as u16
            },
            None => 0,
         }  /* inline_qos */
         + self.serialized_data.len() as u16
    }

    /// Set inline QoS parameter list for DATA submessage and calculate size
    pub(crate) fn set_inline_qos_list(&mut self, param_list: ParameterList) {
        self.inline_qos = Some(param_list);
    }

    pub(crate) fn add_serialized_data(&mut self, serialized_data: SerializedData) {
        self.serialized_data = serialized_data;
    }

    pub(crate) fn deserialize(
        buffer: &Bytes,
        submessage_header: &SubmessageHeader,
    ) -> RtpsResult<Self> {
        let mut cursor = io::Cursor::new(&buffer);
        let map_speedy_err = |p: Error| RtpsError::new(RtpsErrorCode::Io, p.to_string());

        // Extract flags from the submessage header
        let endianness = submessage_header
            .endianness_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;
        let inline_qos_flag = submessage_header
            .inline_qos_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;
        let data_flag = submessage_header
            .data_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;
        let key_flag = submessage_header
            .key_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;

        // 9.4.5.3.1 - This is an invalid combination in this version of the protocol.
        if data_flag && key_flag {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageHeader,
                "DataFlag and KeyFlag cannot be set together",
            ));
        }

        let extra_flags = u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let octets_to_inline_qos =
            u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let reader_id = EntityId::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let writer_id = EntityId::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let writer_sn =
            SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;

        // 8.3.7.2.3 - writerSN value should be strictly positive or not SEQUENCENUMBER_UNKNOWN
        if writer_sn <= 0 || writer_sn == SequenceNumber::UNKNOWN {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "writerSN value should be strictly positive or not SEQUENCENUMBER_UNKNOWN",
            ));
        }

        let inline_qos = if inline_qos_flag {
            // Current cursor position is already at the inline QoS start
            let inline_qos_start = cursor.position() as usize;

            if inline_qos_start < buffer.len() {
                // Read all remaining data as potential inline QoS
                let inline_qos_data = &buffer[inline_qos_start..];

                // Use the specialized inline QoS parser
                let big_endian = match endianness {
                    speedy::Endianness::BigEndian => true,
                    speedy::Endianness::LittleEndian => false,
                };
                let inline_parser = InlineQosParser::new(big_endian);

                match inline_parser.parse_inline_qos(inline_qos_data) {
                    Ok((param_list, consumed_bytes)) => {
                        // debug!(
                        //     "Successfully parsed {} inline QoS parameters",
                        //     param_list.parameters().len()
                        // );
                        cursor.set_position(inline_qos_start as u64 + consumed_bytes as u64);
                        Some(param_list)
                    }
                    Err(_) => {
                        // debug!("Failed to parse inline QoS: {}", e);
                        cursor.set_position(buffer.len() as u64);
                        None
                    }
                }
            } else {
                // debug!("Invalid inline QoS start position: {}", inline_qos_start);
                None
            }
        } else {
            cursor.set_position(20); // extraFlags(2) + octetsToInlineQos(2) + reader_id(4) + writer_id(4) + writer_sn(8)
            None
        };

        let serialized_data: Arc<[u8]> = if data_flag || key_flag {
            let start_pos = cursor.position() as usize;
            Arc::from(&buffer[start_pos..])
        } else {
            Arc::new([])
        };

        Ok(Self {
            reader_id,
            writer_id,
            writer_sn,
            inline_qos,
            serialized_data,
            extra_flags,
            octets_to_inline_qos,
        })
    }
}

impl<C: Context> Writable<C> for Data {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_u16(self.extra_flags)?;
        writer.write_u16(self.octets_to_inline_qos)?;
        writer.write_value(&self.reader_id)?;
        writer.write_value(&self.writer_id)?;
        writer.write_value(&self.writer_sn)?;
        if let Some(ref inline_qos_list) = self.inline_qos {
            writer.write_value(inline_qos_list)?;
        }
        writer.write_bytes(&self.serialized_data)?;

        Ok(())
    }
}
