//! DATAFRAG submessage for transmitting fragmented data samples.
//!
//! This module implements the DATAFRAG submessage used to transmit large data samples
//! that exceed the maximum transport message size. Samples are fragmented and
//! reassembled at the receiver.

use crate::rtps::common::time::RtpsTime;
use bytes::Bytes;
use speedy::{Context, Error, Readable, Writable, Writer};
use std::time::Instant;
use std::{collections::HashSet, io, sync::Arc};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        parameters::ParameterList,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{FragmentNumber, SequenceNumber},
        types::SerializedData,
    },
    messages::submessage_header::SubmessageHeader,
};

const EXTRA_FLAGS: u16 = 0; // 9.4.5.3.2 - extraFlags
const OCTETS_TO_INLINE_QOS: u16 = 28; // 9.4.5.3.3 - octetsToInlineQos

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataFrag {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub writer_sn: SequenceNumber,
    pub fragment_starting_num: FragmentNumber,
    pub fragments_in_submessage: u16,
    pub fragment_size: u16, // fragmentSize is the unit size determined by Writer when splitting the sample. This value must always be the same for the same Writer and the same sample
    pub sample_size: u32,
    inline_qos: Option<ParameterList>,
    serialized_data: SerializedData,
}

impl DataFrag {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        writer_sn: SequenceNumber,
        fragment_starting_num: FragmentNumber,
        fragments_in_submessage: u16,
        fragment_size: u16,
        sample_size: u32,
    ) -> Self {
        Self {
            reader_id,
            writer_id,
            writer_sn,
            fragment_starting_num,
            fragments_in_submessage,
            fragment_size,
            sample_size,
            inline_qos: None,
            serialized_data: Arc::from(vec![]),
        }
    }

    pub(crate) fn add_serialized_data(&mut self, serialized_data: SerializedData) {
        self.serialized_data = serialized_data;
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        2  /* extra_flags */
         + 2  /* octets_to_inline_qos */
         + 4  /* reader_id */
         + 4  /* writer_id */
         + 8  /* writer_sn */
         + 4  /* fragment_starting_num */
         + 2  /* fragments_in_submessage */
         + 2  /* fragment_size */
         + 4  /* sample_size */
         + match &self.inline_qos {
            Some(param_list) => {
                param_list.length() as u16
            },
            None => 0,
         }  /* inline_qos */
         + self.serialized_data.len() as u16
    }

    pub(crate) fn serialized_data(&self) -> &[u8] {
        &self.serialized_data
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

        let _extra_flags = u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
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

        // 8.3.7.3.3
        if writer_sn.to_i64() <= 0 || writer_sn == SequenceNumber::UNKNOWN {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "writerSN value should be strictly positive or not SEQUENCENUMBER_UNKNOWN",
            ));
        }

        let fragment_starting_num =
            FragmentNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let fragments_in_submessage =
            u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let fragment_size = u16::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let sample_size = u32::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;

        // // 8.3.7.3.3 Validity
        // let total_fragments_num = (sample_size / fragment_size as u32)
        //     + if sample_size % fragment_size as u32 > 0 { 1 } else { 0 };

        // if fragment_starting_num <= 0 || fragment_starting_num > total_fragments_num {
        //     return Err(RtpsError::new(
        //         RtpsErrorCode::InvalidSubmessageBody,
        //         "FragmentStartingNum is not strictly positive (1, 2, ...) or exceeds the total number of fragments",
        //     ));
        // }

        // Temporarily commented out because message is not received in message validity check
        // 8.3.7.3.3 Validity
        // if fragment_size > sample_size as u16 {
        //     return Err(RtpsError::new(
        //         RtpsErrorCode::InvalidSubmessageBody,
        //         "Fragment size exceeds sample size",
        //     ));
        // }

        // 9.4.5.3.3 - should always use the octetsToInlineQos to skip any submessage headers it does not expect or understand
        cursor.set_position(
            4 + octets_to_inline_qos as u64, // extraFlags + octetsToInlineQos
        );

        let inline_qos = if inline_qos_flag {
            Some(
                ParameterList::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                    .map_err(map_speedy_err)?,
            )
        } else {
            None
        };

        let start_pos = cursor.position() as usize;
        let serialized_data_bytes = buffer.slice(start_pos..);

        // 8.3.7.3.3 Validity
        // allow up to 3 extra bytes for RTPS submessage 4-byte alignment padding
        let expected_data_size = (fragments_in_submessage as u32 * fragment_size as u32) as usize;
        if serialized_data_bytes.len() > expected_data_size + 3 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Serialized data size exceeds the expected size based on fragments_in_submessage and fragment_size",
            ));
        }

        // truncate padding bytes - only keep actual fragment data
        let actual_len = std::cmp::min(serialized_data_bytes.len(), expected_data_size);
        let serialized_data = Arc::from(serialized_data_bytes[..actual_len].to_vec());

        Ok(Self {
            reader_id,
            writer_id,
            writer_sn,
            fragment_starting_num,
            fragments_in_submessage,
            fragment_size,
            sample_size,
            inline_qos,
            serialized_data,
        })
    }
}

impl<C: Context> Writable<C> for DataFrag {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_u16(EXTRA_FLAGS)?;
        writer.write_u16(OCTETS_TO_INLINE_QOS)?;
        writer.write_value(&self.reader_id)?;
        writer.write_value(&self.writer_id)?;
        writer.write_value(&self.writer_sn)?;
        writer.write_value(&self.fragment_starting_num)?;
        writer.write_value(&self.fragments_in_submessage)?;
        writer.write_value(&self.fragment_size)?;
        writer.write_value(&self.sample_size)?;
        if let Some(ref inline_qos) = self.inline_qos {
            writer.write_value(inline_qos)?;
        }
        writer.write_bytes(self.serialized_data.as_ref())?;

        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct FragmentBuffer {
    pub sequence_number: SequenceNumber,
    pub total_size: u32,
    pub payload: Vec<u8>,
    pub received_fragments: HashSet<u32>,
    pub total_fragments: u32,
    pub fragment_size: u16,
    pub source_timestamp: Option<RtpsTime>,
    pub created_at: Instant,
    pub last_updated: Instant,
}

#[allow(dead_code)]
impl FragmentBuffer {
    pub(crate) fn new(
        sequence_number: SequenceNumber,
        total_size: u32,
        fragment_size: u16,
    ) -> Self {
        let total_fragments = (total_size / fragment_size as u32)
            + if !total_size.is_multiple_of(fragment_size as u32) { 1 } else { 0 };
        let now = Instant::now();

        Self {
            sequence_number,
            total_size,
            payload: vec![0u8; total_size as usize],
            received_fragments: std::collections::HashSet::new(),
            total_fragments,
            fragment_size,
            source_timestamp: None,
            created_at: now,
            last_updated: now,
        }
    }

    pub(crate) fn all_fragments_received(&self) -> bool {
        self.received_fragments.len() == self.total_fragments as usize
    }

    pub(crate) fn mark_fragment_received(&mut self, fragment_num: u32) {
        self.received_fragments.insert(fragment_num);
    }

    pub(crate) fn copy_fragment_data(&mut self, fragment_num: u32, data: &[u8]) -> bool {
        // Calculate fragment_offset: (fragment_num - 1) * fragment_size
        let fragment_offset = ((fragment_num - 1) * self.fragment_size as u32) as usize;

        let actual_data_size = data.len();

        // For the last fragment, adjust to actual data size
        let max_available_size = self.payload.len() - fragment_offset;
        let copy_size = std::cmp::min(actual_data_size, max_available_size);
        let payload_end = fragment_offset + copy_size;

        let payload_ok = payload_end <= self.payload.len();

        if payload_ok {
            self.payload[fragment_offset..payload_end].copy_from_slice(&data[..copy_size]);
            self.received_fragments.insert(fragment_num);
            self.last_updated = Instant::now();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::{
        common::{entity_id::EntityId, entity_kind::EntityKind, sequence::SequenceNumber},
        messages::{submessage_header::SubmessageHeader, submessage_id::SubmessageId},
    };

    use speedy::{Endianness, Writable};

    fn create_dummy_datafrag() -> DataFrag {
        DataFrag {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            writer_sn: SequenceNumber::new(0, 1),
            fragment_starting_num: 1,
            fragments_in_submessage: 1,
            fragment_size: 1,
            sample_size: 2,
            inline_qos: None,
            serialized_data: Arc::from(vec![0 as u8]),
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::DATA_FRAG,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_datafrag_serialization_roundtrip() {
        let datafrag = create_dummy_datafrag();

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();

        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(datafrag.reader_id, deserialized.reader_id);
        assert_eq!(datafrag.writer_id, deserialized.writer_id);
        assert_eq!(datafrag.writer_sn, deserialized.writer_sn);
        assert_eq!(datafrag.fragment_starting_num, deserialized.fragment_starting_num);
        assert_eq!(datafrag.fragments_in_submessage, deserialized.fragments_in_submessage);
        assert_eq!(datafrag.fragment_size, deserialized.fragment_size);
        assert_eq!(datafrag.sample_size, deserialized.sample_size);
        assert_eq!(datafrag.inline_qos, deserialized.inline_qos);
        assert_eq!(&datafrag.serialized_data[..], &deserialized.serialized_data[..]);
    }

    #[test]
    fn test_datafrag_invalid_sequence_number() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.writer_sn = SequenceNumber::UNKNOWN;

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();

        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }

    #[test]
    fn test_datafrag_invalid_serialized_data_too_large() {
        let mut datafrag = create_dummy_datafrag();
        datafrag.fragments_in_submessage = 1;
        datafrag.fragment_size = 3;
        datafrag.sample_size = 3;

        // serialized data of 5 bytes when only 3 are expected
        datafrag.serialized_data = Arc::from(vec![1, 2, 3, 4, 5]);

        let buffer = datafrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();

        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = DataFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_err());
        if let Err(err) = result {
            assert_eq!(err.code, RtpsErrorCode::InvalidSubmessageBody);
        }
    }
}
