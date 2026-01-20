//! HEARTBEAT submessage for reliable communication protocol.
//!
//! This module implements the HEARTBEAT submessage sent by DataWriters to inform
//! DataReaders about available data sequence numbers. Readers use HEARTBEATs to
//! identify missing samples and request retransmission via ACKNACK.

use std::{io, mem};

use bytes::Bytes;
use speedy::{Error, Readable, Writable};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::SequenceNumber,
        types::Count,
    },
    messages::submessage_header::SubmessageHeader,
};

#[derive(Debug, Clone, PartialEq, Eq, Writable)]
pub(crate) struct Heartbeat {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub first_sn: SequenceNumber,
    pub last_sn: SequenceNumber,
    pub count: Count,
}

impl Heartbeat {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
        count: Count,
    ) -> Self {
        Self { reader_id, writer_id, first_sn, last_sn, count }
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        mem::size_of::<EntityId>() as u16 /* reader_id 4 */
            + mem::size_of::<EntityId>() as u16 /* writer_id 4 */
            + mem::size_of::<SequenceNumber>() as u16 /* first_sn 8 */
            + mem::size_of::<SequenceNumber>() as u16 /* last_sn 8 */
            + mem::size_of::<Count>() as u16 /* count 4 */
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
        let first_sn =
            SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let last_sn = SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;
        let count = Count::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;

        // 8.3.7.5.3
        if first_sn.to_i64() <= 0 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "firstSN value is zero or negative",
            ));
        };

        /*
            Disable the following validity checks to partially adapt RTPS 2.5 and implement preemptive heartbeat.
        */

        // 8.3.7.5.3
        // if last_sn.to_i64() <= 0 {
        //     return Err(RtpsError::new(
        //         RtpsErrorCode::InvalidSubmessageBody,
        //         "lastSN value is zero or negative",
        //     ));
        // }

        // 8.3.7.5.3
        // if last_sn < first_sn {
        //     return Err(RtpsError::new(
        //         RtpsErrorCode::InvalidSubmessageBody,
        //         "Last SequenceNumber cannot be less than First SequenceNumber in Heartbeat",
        //     ));
        // }

        Ok(Self { reader_id, writer_id, first_sn, last_sn, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::{
        entity_id::EntityId, entity_kind::EntityKind, sequence::SequenceNumber,
    };
    use crate::rtps::messages::submessage_header::SubmessageHeader;
    use crate::rtps::messages::submessage_id::SubmessageId;
    use speedy::Endianness;
    use speedy::Writable;

    fn create_dummy_heartbeat() -> Heartbeat {
        Heartbeat {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            first_sn: SequenceNumber::new(0, 1),
            last_sn: SequenceNumber::new(0, 1),
            count: 1,
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::DATA,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_data_serialization_roundtrip() {
        let data = create_dummy_heartbeat();

        let buffer = data.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);
        let result = Heartbeat::deserialize(&Bytes::from(buffer), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(data.reader_id, deserialized.reader_id);
        assert_eq!(data.writer_id, deserialized.writer_id);
        assert_eq!(data.first_sn, deserialized.first_sn);
        assert_eq!(data.last_sn, deserialized.last_sn);
        assert_eq!(data.count, deserialized.count);
    }
}
