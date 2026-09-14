//! GAP submessage for indicating irrelevant sequence numbers.
//!
//! This module implements the GAP submessage sent by DataWriters to inform DataReaders
//! that certain sequence numbers are no longer relevant (e.g., unregistered instances,
//! filtered samples). This prevents readers from indefinitely waiting for those samples.

use std::{io, mem};

use bytes::Bytes;
use speedy::{Context, Error, Readable, Writable, Writer};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        sequence::{SequenceNumber, SequenceNumberSet},
    },
    messages::submessage_header::SubmessageHeader,
};

// Group sequence number range present only when the GroupInfoFlag is set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GapGroupInfo {
    pub gap_start_gsn: SequenceNumber,
    pub gap_end_gsn: SequenceNumber,
}

impl GapGroupInfo {
    const OCTETS: u16 = 2 * mem::size_of::<SequenceNumber>() as u16;

    // Validity rules for the group sequence number range.
    fn validate(&self) -> RtpsResult<()> {
        let gap_start_gsn = self.gap_start_gsn.to_i64();
        let gap_end_gsn = self.gap_end_gsn.to_i64();

        let is_invalid = gap_start_gsn <= 0 || gap_end_gsn <= 0 || gap_end_gsn < gap_start_gsn - 1;
        if is_invalid {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidSubmessageBody,
                "invalid group sequence numbers in Gap",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Gap {
    pub reader_id: EntityId,
    pub writer_id: EntityId,
    pub gap_start: SequenceNumber,
    pub gap_list: SequenceNumberSet,
    pub group_info: Option<GapGroupInfo>,
}

impl Gap {
    pub(crate) fn new(
        reader_id: EntityId,
        writer_id: EntityId,
        gap_start: SequenceNumber,
        gap_list: SequenceNumberSet,
        group_info: Option<GapGroupInfo>,
    ) -> Self {
        Self { reader_id, writer_id, gap_start, gap_list, group_info }
    }

    pub(crate) fn octets_to_next_header(&self) -> u16 {
        let group_info_octets = if self.group_info.is_some() { GapGroupInfo::OCTETS } else { 0 };

        mem::size_of::<EntityId>() as u16 /* reader_id 4 */
            + mem::size_of::<EntityId>() as u16 /* writer_id 4*/
            + mem::size_of::<SequenceNumber>() as u16 /* gap_start */
            + self.gap_list.length() /* gap_list */
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

        let mut group_info = None;
        if submessage_header.group_info_flag() == Some(true) {
            let mut read_sequence_number = || {
                SequenceNumber::read_from_stream_unbuffered_with_ctx(endianness, &mut cursor)
                    .map_err(map_speedy_err)
            };
            let gap_start_gsn = read_sequence_number()?;
            let gap_end_gsn = read_sequence_number()?;

            let read_group_info = GapGroupInfo { gap_start_gsn, gap_end_gsn };
            read_group_info.validate()?;
            group_info = Some(read_group_info);
        }

        Ok(Gap { reader_id, writer_id, gap_start, gap_list, group_info })
    }
}

impl<C: Context> Writable<C> for Gap {
    fn write_to<T: ?Sized + Writer<C>>(&self, writer: &mut T) -> Result<(), C::Error> {
        writer.write_value(&self.reader_id)?;
        writer.write_value(&self.writer_id)?;
        writer.write_value(&self.gap_start)?;
        writer.write_value(&self.gap_list)?;

        if let Some(group_info) = &self.group_info {
            writer.write_value(&group_info.gap_start_gsn)?;
            writer.write_value(&group_info.gap_end_gsn)?;
        }
        Ok(())
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

    const GROUP_INFO_FLAG: u8 = 0x02;

    fn create_dummy_gap() -> Gap {
        Gap {
            reader_id: EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            writer_id: EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            gap_start: SequenceNumber::new(0, 1),
            gap_list: SequenceNumberSet::from_vec(
                SequenceNumber::new(0, 4),
                vec![SequenceNumber::new(0, 4), SequenceNumber::new(0, 6)],
            ),
            group_info: None,
        }
    }

    fn create_dummy_group_info() -> GapGroupInfo {
        GapGroupInfo {
            gap_start_gsn: SequenceNumber::from_i64(3),
            gap_end_gsn: SequenceNumber::from_i64(9),
        }
    }

    fn create_dummy_submessage_header(length: u16) -> SubmessageHeader {
        SubmessageHeader::new(
            SubmessageId::GAP,
            0,
            length, // submessage_length
        )
    }

    fn create_gap_submessage_header(flags: u8, length: u16) -> SubmessageHeader {
        SubmessageHeader::new(SubmessageId::GAP, flags, length)
    }

    #[test]
    fn group_info_roundtrips_when_the_flag_is_set() {
        let mut gap = create_dummy_gap();
        gap.group_info = Some(create_dummy_group_info());

        let buffer = gap.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        assert_eq!(buffer.len() as u16, gap.octets_to_next_header());
        let header = create_gap_submessage_header(GROUP_INFO_FLAG, buffer.len() as u16);
        let deserialized = Gap::deserialize(&Bytes::from(buffer), &header).unwrap();

        assert_eq!(deserialized, gap);
    }

    #[test]
    fn group_info_is_absent_when_the_flag_is_clear() {
        let gap = create_dummy_gap();

        let buffer = gap.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        assert_eq!(buffer.len() as u16, gap.octets_to_next_header());
        let header = create_gap_submessage_header(0, buffer.len() as u16);
        let deserialized = Gap::deserialize(&Bytes::from(buffer), &header).unwrap();

        assert_eq!(deserialized.group_info, None);
    }

    #[test]
    fn group_info_violating_the_validity_rules_is_rejected() {
        let invalid_group_infos = [
            // gapStartGSN.value is zero or negative
            GapGroupInfo { gap_start_gsn: SequenceNumber::ZERO, ..create_dummy_group_info() },
            // gapEndGSN.value is zero or negative
            GapGroupInfo { gap_end_gsn: SequenceNumber::ZERO, ..create_dummy_group_info() },
            // gapEndGSN.value < gapStartGSN.value - 1
            GapGroupInfo {
                gap_start_gsn: SequenceNumber::from_i64(5),
                gap_end_gsn: SequenceNumber::from_i64(3),
            },
        ];

        for invalid_group_info in invalid_group_infos {
            let mut gap = create_dummy_gap();
            gap.group_info = Some(invalid_group_info);
            let buffer = gap.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
            let header = create_gap_submessage_header(GROUP_INFO_FLAG, buffer.len() as u16);

            let result = Gap::deserialize(&Bytes::from(buffer), &header);
            assert!(result.is_err(), "{invalid_group_info:?} must be rejected");
        }
    }

    #[test]
    fn empty_group_range_is_accepted() {
        let mut gap = create_dummy_gap();
        gap.group_info = Some(GapGroupInfo {
            gap_start_gsn: SequenceNumber::from_i64(4),
            gap_end_gsn: SequenceNumber::from_i64(3),
        });
        let buffer = gap.write_to_vec_with_ctx(Endianness::BigEndian).unwrap();
        let header = create_gap_submessage_header(GROUP_INFO_FLAG, buffer.len() as u16);

        assert!(Gap::deserialize(&Bytes::from(buffer), &header).is_ok());
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
