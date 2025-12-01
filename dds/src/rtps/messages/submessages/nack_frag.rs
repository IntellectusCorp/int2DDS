//! NACKFRAG submessage for requesting missing fragments.
//!
//! This module implements the NACKFRAG submessage sent by DataReaders to request
//! retransmission of missing fragments for a specific sequence number in fragmented
//! data communication.

use bytes::Bytes;
use speedy::{Error, Readable, Writable};
use std::io;

use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{FragmentNumberSet, SequenceNumber},
        types::Count,
    },
    messages::submessage_header::SubmessageHeader,
};

#[derive(Debug, Clone, PartialEq, Eq, Writable)]
pub(crate) struct NackFrag {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub writer_sn: SequenceNumber,
    pub fragment_number_state: FragmentNumberSet,
    pub count: Count,
}

impl NackFrag {
    pub(crate) fn deserialize(
        buffer: &Bytes,
        submessage_header: &SubmessageHeader,
    ) -> RtpsResult<Self> {
        let mut cursor = io::Cursor::new(&buffer);
        let map_speedy_err = |p: Error| RtpsError::new(RtpsErrorCode::Io, p.to_string());

        let endianness = submessage_header
            .endianness_flag()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::UnsupportedSubmessageType, None))?;

        let reader_id = EntityId::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let writer_id = EntityId::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let writer_sn =
            SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        if writer_sn <= 0 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "writerSN value should be strictly positive",
            ));
        }
        let fragment_number_state =
            FragmentNumberSet::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        if !fragment_number_state.is_valid() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Invalid fragment number set",
            ));
        }

        let count = Count::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;

        Ok(Self { reader_id, writer_id, writer_sn, fragment_number_state, count })
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        4 /* reader_id 4*/
         + 4 /* writer_id 4*/
         + 8 /* writer_sn 8*/
         + self.fragment_number_state.length() /* 12 */
         + 4 /* count 4*/
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::rtps::common::sequence::SequenceNumber;
    use crate::rtps::common::{entity_id::EntityId, entity_kind::EntityKind};
    use crate::rtps::messages::submessage_header::SubmessageHeader;
    use crate::rtps::messages::submessage_id::SubmessageId;
    use speedy::{Endianness, Writable};

    fn create_dummy_nackfrag() -> NackFrag {
        NackFrag {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            writer_sn: SequenceNumber::new(0, 1),
            fragment_number_state: FragmentNumberSet::from_vec(1, vec![2, 3]),
            count: 1,
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::NACK_FRAG,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_nackfrag_serialization_roundtrip() {
        let nackfrag = create_dummy_nackfrag();
        let buffer = nackfrag.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = NackFrag::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(nackfrag.reader_id, deserialized.reader_id);
        assert_eq!(nackfrag.writer_id, deserialized.writer_id);
        assert_eq!(nackfrag.writer_sn, deserialized.writer_sn);
        assert_eq!(nackfrag.fragment_number_state, deserialized.fragment_number_state);
        assert_eq!(nackfrag.count, deserialized.count);
    }
}
