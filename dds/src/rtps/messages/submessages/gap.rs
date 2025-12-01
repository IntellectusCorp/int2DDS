//! GAP submessage for indicating irrelevant sequence numbers.
//!
//! This module implements the GAP submessage sent by DataWriters to inform DataReaders
//! that certain sequence numbers are no longer relevant (e.g., unregistered instances,
//! filtered samples). This prevents readers from indefinitely waiting for those samples.

use std::{io, mem};

use bytes::Bytes;
use speedy::{Error, Readable, Writable};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{SequenceNumber, SequenceNumberSet},
    },
    messages::submessage_header::SubmessageHeader,
};

#[derive(Debug, Clone, PartialEq, Eq, Writable)]
pub(crate) struct Gap {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub gap_start: SequenceNumber,
    pub gap_list: SequenceNumberSet,
}

impl Gap {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        gap_start: SequenceNumber,
        gap_list: SequenceNumberSet,
    ) -> Self {
        Self { reader_id, writer_id, gap_start, gap_list }
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        mem::size_of::<EntityId>() as u16 /* reader_id 4 */
            + mem::size_of::<EntityId>() as u16 /* writer_id 4*/
            + mem::size_of::<SequenceNumber>() as u16 /* gap_start */
            + self.gap_list.length() /* gap_list */
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
        let gap_start =
            SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        if gap_start <= 0 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Gap start value should be strictly positive",
            ));
        }
        let gap_list =
            SequenceNumberSet::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        if !gap_list.is_valid() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Invalid sequence number set for gap list",
            ));
        }
        Ok(Gap { reader_id, writer_id, gap_start, gap_list })
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

    fn create_dummy_gap() -> Gap {
        Gap {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            gap_start: SequenceNumber::new(0, 1),
            gap_list: SequenceNumberSet::from_vec(
                SequenceNumber::new(0, 4),
                vec![SequenceNumber::new(0, 4), SequenceNumber::new(0, 6)],
            ),
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::GAP,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_gap_serialization_roundtrip() {
        let gap = create_dummy_gap();
        let buffer = gap.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);

        let result = Gap::deserialize(&Bytes::from(buffer.to_vec()), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(gap.reader_id, deserialized.reader_id);
        assert_eq!(gap.writer_id, deserialized.writer_id);
        assert_eq!(gap.gap_start, deserialized.gap_start);
        assert_eq!(gap.gap_list, deserialized.gap_list);
    }
}
