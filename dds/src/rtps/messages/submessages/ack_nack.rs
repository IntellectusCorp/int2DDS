//! ACKNACK submessage for acknowledging received data and requesting retransmissions.
//!
//! This module implements the ACKNACK submessage sent by DataReaders to acknowledge
//! received samples and request retransmission of missing samples in reliable
//! communication mode.

use bytes::Bytes;
use log::debug;
use speedy::{Error, Readable, Writable};
use std::{io, mem};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{SequenceNumber, SequenceNumberSet},
        types::Count,
    },
    messages::submessage_header::SubmessageHeader,
};

#[derive(Debug, Clone, PartialEq, Eq, Writable)]
pub(crate) struct AckNack {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub reader_sn_state: SequenceNumberSet,
    pub count: Count,
}

impl AckNack {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        reader_sn_state: SequenceNumberSet,
        count: Count,
    ) -> Self {
        Self { reader_id, writer_id, reader_sn_state, count }
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        mem::size_of::<EntityId>() as u16 /* reader_id 4*/
            + mem::size_of::<EntityId>() as u16 /* writer_id 4*/
            + self.reader_sn_state.length() /* reader_sn_state 12*/
            + mem::size_of::<Count>() as u16 /* count 4*/
    }

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
        let reader_sn_state =
            SequenceNumberSet::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;

        if reader_sn_state.bitmap_base() == SequenceNumber::from_i64(0)
            && reader_sn_state.num_bits() == 0
        {
            debug!("Received preemptive ACKNACK with empty SequenceNumberSet");
        } else if !reader_sn_state.is_valid() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Invalid sequence number set for reader SN state",
            ));
        }

        let count = Count::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;

        Ok(Self { reader_id, writer_id, reader_sn_state, count })
    }
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::rtps::common::{
        entity_id::EntityId, entity_kind::EntityKind, sequence::SequenceNumber,
    };
    use crate::rtps::messages::{submessage_header::SubmessageHeader, submessage_id::SubmessageId};
    use speedy::Endianness;
    use speedy::Writable;

    fn create_dummy_acknack() -> AckNack {
        AckNack {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            reader_sn_state: SequenceNumberSet::from_vec(
                SequenceNumber::new(0, 1),
                vec![SequenceNumber::new(0, 2), SequenceNumber::new(0, 3)],
            ),
            count: 1,
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::ACKNACK,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_acknack_serialization_roundtrip() {
        let acknack = create_dummy_acknack();
        let buffer = acknack.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = AckNack::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(acknack.reader_id, deserialized.reader_id);
        assert_eq!(acknack.writer_id, deserialized.writer_id);
        assert_eq!(acknack.reader_sn_state, deserialized.reader_sn_state);
        assert_eq!(acknack.count, deserialized.count);
    }
}
