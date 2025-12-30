//! HEARTBEATFRAG submessage for fragmented data reliability.
//!
//! This module implements the HEARTBEATFRAG submessage sent by DataWriters to inform
//! DataReaders about available fragments for a specific sequence number. Enables
//! reliable delivery of fragmented samples.

use std::io;

use bytes::Bytes;
use speedy::{Error, Readable, Writable};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{FragmentNumber, SequenceNumber},
        types::Count,
    },
    messages::submessage_header::SubmessageHeader,
};

#[derive(Debug, Clone, PartialEq, Eq, Writable)]
pub(crate) struct HeartbeatFrag {
    reader_id: EntityId,
    writer_id: EntityId,
    writer_sn: SequenceNumber,
    last_fragment_num: FragmentNumber,
    count: Count,
}

impl HeartbeatFrag {
    pub fn deserialize(buffer: &Bytes, submessage_header: &SubmessageHeader) -> RtpsResult<Self> {
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
        let last_fragment_num =
            FragmentNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                .map_err(map_speedy_err)?;
        let count = Count::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
            .map_err(map_speedy_err)?;

        // 8.3.7.6.3
        if writer_sn.to_i64() <= 0 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "writerSN value is zero or negative",
            ));
        };

        // 8.3.7.6.3
        if last_fragment_num == 0 {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "Last fragment number is zero",
            ));
        }

        Ok(Self { reader_id, writer_id, writer_sn, last_fragment_num, count })
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

    fn create_dummy_heartbeatfrag() -> HeartbeatFrag {
        HeartbeatFrag {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            writer_sn: SequenceNumber::new(0, 1),
            last_fragment_num: 1,
            count: 1,
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::HEARTBEAT_FRAG,
            0,
            length, // submessage_length
        )
    }

    #[test]
    fn test_data_serialization_roundtrip() {
        let data = create_dummy_heartbeatfrag();

        let buffer = data.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_dummy_submessage_header(buffer.len() as u16);
        let result = HeartbeatFrag::deserialize(&Bytes::from(buffer), &header);
        assert!(result.is_ok());

        let deserialized = result.unwrap();
        assert_eq!(data.reader_id, deserialized.reader_id);
        assert_eq!(data.writer_id, deserialized.writer_id);
        assert_eq!(data.writer_sn, deserialized.writer_sn);
        assert_eq!(data.last_fragment_num, deserialized.last_fragment_num);
        assert_eq!(data.count, deserialized.count);
    }
}
