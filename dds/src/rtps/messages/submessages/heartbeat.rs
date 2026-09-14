//! HEARTBEAT submessage for reliable communication protocol.
//!
//! This module implements the HEARTBEAT submessage sent by DataWriters to inform
//! DataReaders about available data sequence numbers. Readers use HEARTBEATs to
//! identify missing samples and request retransmission via ACKNACK.

use std::{io, mem};

use bytes::Bytes;
use speedy::{Context, Error, Readable, Writable, Writer};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        guid::GroupDigest,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::SequenceNumber,
        types::Count,
    },
    messages::submessage_header::SubmessageHeader,
};

// Group sequence number elements present only when the GroupInfoFlag is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeartbeatGroupInfo {
    pub current_gsn: SequenceNumber,
    pub first_gsn: SequenceNumber,
    pub last_gsn: SequenceNumber,
    pub writer_set: GroupDigest,
    pub secure_writer_set: GroupDigest,
}

impl HeartbeatGroupInfo {
    const OCTETS: u16 = 3 * mem::size_of::<SequenceNumber>() as u16 + 2 * 4;

    // Validity rules for the group sequence number elements.
    fn validate(&self) -> RtpsResult<()> {
        let current_gsn = self.current_gsn.to_i64();
        let first_gsn = self.first_gsn.to_i64();
        let last_gsn = self.last_gsn.to_i64();

        let is_invalid = current_gsn <= 0
            || first_gsn <= 0
            || last_gsn < 0
            || last_gsn < first_gsn - 1
            || current_gsn < first_gsn
            || current_gsn > last_gsn;
        if is_invalid {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "invalid group sequence numbers in Heartbeat",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Heartbeat {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub first_sn: SequenceNumber,
    pub last_sn: SequenceNumber,
    pub count: Count,
    pub group_info: Option<HeartbeatGroupInfo>,
}

impl Heartbeat {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
        count: Count,
        group_info: Option<HeartbeatGroupInfo>,
    ) -> Self {
        Self { reader_id, writer_id, first_sn, last_sn, count, group_info }
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        let group_info_octets =
            if self.group_info.is_some() { HeartbeatGroupInfo::OCTETS } else { 0 };

        mem::size_of::<EntityId>() as u16 /* reader_id 4 */
            + mem::size_of::<EntityId>() as u16 /* writer_id 4 */
            + mem::size_of::<SequenceNumber>() as u16 /* first_sn 8 */
            + mem::size_of::<SequenceNumber>() as u16 /* last_sn 8 */
            + mem::size_of::<Count>() as u16 /* count 4 */
            + group_info_octets
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

        let mut group_info = None;
        if submessage_header.group_info_flag() == Some(true) {
            let mut read_sequence_number = || {
                SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                    .map_err(map_speedy_err)
            };
            let current_gsn = read_sequence_number()?;
            let first_gsn = read_sequence_number()?;
            let last_gsn = read_sequence_number()?;
            let mut read_group_digest = || {
                <[u8; 4]>::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                    .map(GroupDigest::new)
                    .map_err(map_speedy_err)
            };
            let writer_set = read_group_digest()?;
            let secure_writer_set = read_group_digest()?;

            let read_group_info = HeartbeatGroupInfo {
                current_gsn,
                first_gsn,
                last_gsn,
                writer_set,
                secure_writer_set,
            };
            read_group_info.validate()?;
            group_info = Some(read_group_info);
        }

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

        Ok(Self { reader_id, writer_id, first_sn, last_sn, count, group_info })
    }
}

impl<C: Context> Writable<C> for Heartbeat {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.reader_id)?;
        writer.write_value(&self.writer_id)?;
        writer.write_value(&self.first_sn)?;
        writer.write_value(&self.last_sn)?;
        writer.write_value(&self.count)?;

        if let Some(group_info) = &self.group_info {
            writer.write_value(&group_info.current_gsn)?;
            writer.write_value(&group_info.first_gsn)?;
            writer.write_value(&group_info.last_gsn)?;
            writer.write_bytes(&group_info.writer_set.to_bytes())?;
            writer.write_bytes(&group_info.secure_writer_set.to_bytes())?;
        }
        Ok(())
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

    const GROUP_INFO_FLAG: u8 = 0x08;

    fn create_dummy_heartbeat() -> Heartbeat {
        Heartbeat {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            first_sn: SequenceNumber::new(0, 1),
            last_sn: SequenceNumber::new(0, 1),
            count: 1,
            group_info: None,
        }
    }

    fn create_dummy_group_info() -> HeartbeatGroupInfo {
        HeartbeatGroupInfo {
            current_gsn: SequenceNumber::from_i64(7),
            first_gsn: SequenceNumber::from_i64(3),
            last_gsn: SequenceNumber::from_i64(9),
            writer_set: GroupDigest::new([0xaa, 0xbb, 0xcc, 0xdd]),
            secure_writer_set: GroupDigest::new([0; 4]),
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::DATA,
            0,
            length, // submessage_length
        )
    }

    fn create_heartbeat_submessage_header(flags: u8, length: u16) -> SubmessageHeader {
        SubmessageHeader::new(SubmessageId::HEARTBEAT, flags, length)
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

    #[test]
    fn group_info_roundtrips_when_the_flag_is_set() {
        let mut heartbeat = create_dummy_heartbeat();
        heartbeat.group_info = Some(create_dummy_group_info());

        let buffer = heartbeat.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        assert_eq!(buffer.len() as u16, heartbeat.octets_to_next_header());
        let header = create_heartbeat_submessage_header(GROUP_INFO_FLAG, buffer.len() as u16);
        let deserialized = Heartbeat::deserialize(&Bytes::from(buffer), &header).unwrap();

        assert_eq!(deserialized, heartbeat);
    }

    #[test]
    fn group_info_is_absent_when_the_flag_is_clear() {
        let heartbeat = create_dummy_heartbeat();

        let buffer = heartbeat.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        assert_eq!(buffer.len() as u16, heartbeat.octets_to_next_header());
        let header = create_heartbeat_submessage_header(0, buffer.len() as u16);
        let deserialized = Heartbeat::deserialize(&Bytes::from(buffer), &header).unwrap();

        assert_eq!(deserialized.group_info, None);
    }

    #[test]
    fn group_info_violating_the_validity_rules_is_rejected() {
        let invalid_group_infos = [
            // currentGSN.value is zero or negative
            HeartbeatGroupInfo { current_gsn: SequenceNumber::ZERO, ..create_dummy_group_info() },
            // firstGSN.value is zero or negative
            HeartbeatGroupInfo { first_gsn: SequenceNumber::ZERO, ..create_dummy_group_info() },
            // lastGSN.value is negative
            HeartbeatGroupInfo {
                last_gsn: SequenceNumber::from_i64(-1),
                ..create_dummy_group_info()
            },
            // lastGSN.value < firstGSN.value - 1
            HeartbeatGroupInfo {
                first_gsn: SequenceNumber::from_i64(5),
                last_gsn: SequenceNumber::from_i64(3),
                ..create_dummy_group_info()
            },
            // currentGSN.value < firstGSN.value
            HeartbeatGroupInfo {
                current_gsn: SequenceNumber::from_i64(2),
                ..create_dummy_group_info()
            },
            // currentGSN.value > lastGSN.value
            HeartbeatGroupInfo {
                current_gsn: SequenceNumber::from_i64(10),
                ..create_dummy_group_info()
            },
        ];

        for invalid_group_info in invalid_group_infos {
            let mut heartbeat = create_dummy_heartbeat();
            heartbeat.group_info = Some(invalid_group_info);
            let buffer = heartbeat.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
            let header = create_heartbeat_submessage_header(GROUP_INFO_FLAG, buffer.len() as u16);

            let result = Heartbeat::deserialize(&Bytes::from(buffer), &header);
            assert!(result.is_err(), "{invalid_group_info:?} must be rejected");
        }
    }

    #[test]
    fn group_range_of_a_single_sample_is_accepted() {
        let mut heartbeat = create_dummy_heartbeat();
        heartbeat.group_info = Some(HeartbeatGroupInfo {
            current_gsn: SequenceNumber::from_i64(4),
            first_gsn: SequenceNumber::from_i64(4),
            last_gsn: SequenceNumber::from_i64(4),
            ..create_dummy_group_info()
        });
        let buffer = heartbeat.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_heartbeat_submessage_header(GROUP_INFO_FLAG, buffer.len() as u16);

        assert!(Heartbeat::deserialize(&Bytes::from(buffer), &header).is_ok());
    }
}
