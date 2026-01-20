use chrono::{DateTime, Utc};

use crate::rtps::{
    common::{
        entity_id::EntityId,
        guid::GuidPrefix,
        sequence::{FragmentNumberSet, SequenceNumber, SequenceNumberSet},
    },
    messages::{
        submessage::Submessage,
        submessage_body::SubmessageBody,
        submessage_header::SubmessageHeader,
        submessage_header_flag::{SubmessageFlagType, SubmessageHeaderFlag},
        submessage_id::SubmessageId,
        submessages::{
            ack_nack::AckNack,
            heartbeat::Heartbeat,
            info::{InfoDestination, InfoTimestamp},
            nack_frag::NackFrag,
        },
    },
};

pub(crate) struct SubmessageCreator;

impl SubmessageCreator {
    pub(crate) fn create_info_ts_submessage(timestamp: DateTime<Utc>) -> Submessage {
        let mut info_ts_header_flag = SubmessageHeaderFlag::new();
        info_ts_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::INFO_TS);
        let info_ts_data = InfoTimestamp::new(timestamp);

        Submessage {
            header: SubmessageHeader::new(
                SubmessageId::INFO_TS,
                info_ts_header_flag.flag,
                info_ts_data.length(),
            ),
            body: SubmessageBody::InfoTimestamp(info_ts_data),
        }
    }

    pub(crate) fn create_info_dst_submessage(prefix: GuidPrefix) -> Submessage {
        let mut info_dst_header_flag = SubmessageHeaderFlag::new();
        info_dst_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::INFO_DST);
        let info_dst_data = InfoDestination::new(prefix);

        Submessage {
            header: SubmessageHeader::new(
                SubmessageId::INFO_DST,
                info_dst_header_flag.flag,
                info_dst_data.length(),
            ),
            body: SubmessageBody::InfoDestination(info_dst_data),
        }
    }

    pub(crate) fn create_heartbeat_submessage(
        heartbeat_count: i32,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
        final_flag: bool,
        liveliness_flag: bool,
    ) -> Result<Submessage, Box<dyn std::error::Error>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::HEARTBEAT);
        if liveliness_flag {
            data_header_flag.add_flag(SubmessageFlagType::LivelinessFlag, SubmessageId::HEARTBEAT);
        }
        if final_flag {
            data_header_flag.add_flag(SubmessageFlagType::FinalFlag, SubmessageId::HEARTBEAT);
        }
        let heartbeat_data =
            Heartbeat::new(reader_entity_id, writer_entity_id, first_sn, last_sn, heartbeat_count);

        let heartbeat_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::HEARTBEAT,
                data_header_flag.flag,
                heartbeat_data.octets_to_next_header(),
            ),
            body: SubmessageBody::Heartbeat(heartbeat_data),
        };
        Ok(heartbeat_submessage)
    }

    pub(crate) fn create_acknack_submessage(
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        missing_changes: Vec<SequenceNumber>,
        acknack_count: i32,
        bitmap_base: SequenceNumber,
        is_preemptive: bool,
    ) -> Result<Submessage, Box<dyn std::error::Error>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::ACKNACK);

        // Convert missing changes to SequenceNumberSet
        let reader_sn_state = if missing_changes.is_empty() {
            // If no missing changes, create empty set
            if !is_preemptive {
                data_header_flag.add_flag(SubmessageFlagType::FinalFlag, SubmessageId::ACKNACK);
            }
            SequenceNumberSet::new_empty_with_base(bitmap_base)
        } else {
            // If there are missing changes, create set with those sequence numbers
            let base_sn = missing_changes[0];
            SequenceNumberSet::from_vec(base_sn, missing_changes)
        };

        let acknack_data =
            AckNack::new(reader_entity_id, writer_entity_id, reader_sn_state, acknack_count);

        let acknack_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::ACKNACK,
                data_header_flag.flag,
                acknack_data.octets_to_next_header(),
            ),
            body: SubmessageBody::AckNack(acknack_data),
        };
        Ok(acknack_submessage)
    }

    pub(crate) fn create_gap_submessage(
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        gap_list: &mut Vec<SequenceNumber>,
    ) -> Result<Submessage, Box<dyn std::error::Error>> {
        if gap_list.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Gap list cannot be empty",
            )));
        }

        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::GAP);

        let (gap_start, sequence_number_set) = Self::calculate_gap_sns_from_vec(gap_list);

        let gap_data = crate::rtps::messages::submessages::gap::Gap::new(
            reader_entity_id,
            writer_entity_id,
            gap_start,
            sequence_number_set,
        );

        let gap_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::GAP,
                data_header_flag.flag,
                gap_data.octets_to_next_header(),
            ),
            body: SubmessageBody::Gap(gap_data),
        };
        Ok(gap_submessage)
    }

    pub(crate) fn create_gap_submessage_consecutive(
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        gap_start: SequenceNumber,
        gap_end: SequenceNumber,
    ) -> Result<Submessage, Box<dyn std::error::Error>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::GAP);

        // RTPS 2.5 - 8.3.8.4.5
        // The set of sequence numbers identify in the range gapStart <= sequence_number <= gapList.base -1
        let sequence_number_set = SequenceNumberSet::new_empty_with_base(gap_end + 1);

        let gap_data = crate::rtps::messages::submessages::gap::Gap::new(
            reader_entity_id,
            writer_entity_id,
            gap_start,
            sequence_number_set,
        );

        let gap_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::GAP,
                data_header_flag.flag,
                gap_data.octets_to_next_header(),
            ),
            body: SubmessageBody::Gap(gap_data),
        };
        Ok(gap_submessage)
    }

    pub(crate) fn create_nackfrag_submessage(
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        writer_sn: SequenceNumber,
        fragment_number_state: FragmentNumberSet,
        nackfrag_count: i32,
    ) -> Result<Submessage, Box<dyn std::error::Error>> {
        let mut nackfrag_header_flag = SubmessageHeaderFlag::new();
        nackfrag_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::NACK_FRAG);

        let nackfrag_data = NackFrag {
            reader_id: reader_entity_id,
            writer_id: writer_entity_id,
            writer_sn,
            fragment_number_state,
            count: nackfrag_count,
        };

        let nackfrag_submessage = Submessage {
            header: SubmessageHeader::new(
                SubmessageId::NACK_FRAG,
                nackfrag_header_flag.flag,
                nackfrag_data.octets_to_next_header(),
            ),
            body: SubmessageBody::NackFrag(nackfrag_data),
        };

        Ok(nackfrag_submessage)
    }

    fn calculate_gap_sns_from_vec(
        gap_list: &mut Vec<SequenceNumber>,
    ) -> (SequenceNumber, SequenceNumberSet) {
        let gap_start = gap_list[0];
        let mut bitmap_base = gap_start + 1;
        let mut sn_after_base: Vec<SequenceNumber> = Vec::new();
        let mut processed = 0;

        for (i, sn) in gap_list.iter().enumerate().skip(1) {
            if *sn == bitmap_base {
                // RTPS 2.5 - 8.3.8.4.5
                // The set of sequence numbers identify in the range gapStart <= sequence_number <= gapList.base -1
                bitmap_base += 1;
                processed = i;
            } else {
                // Bitmap base can handle up to the range of 256 sequence numbers
                if *sn <= bitmap_base + 255 {
                    sn_after_base.push(*sn);
                    processed = i;
                } else {
                    break;
                }
            }
        }

        let remaining = gap_list.split_off(processed + 1);
        *gap_list = remaining;

        let sequence_number_set = if sn_after_base.is_empty() {
            SequenceNumberSet::new_empty_with_base(bitmap_base)
        } else {
            SequenceNumberSet::from_vec(bitmap_base, sn_after_base)
        };

        (gap_start, sequence_number_set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_gap_sns_from_vec_empty_sns() {
        let mut gap_list = vec![SequenceNumber::new(0, 1)];

        let (gap_start, sns) = SubmessageCreator::calculate_gap_sns_from_vec(&mut gap_list);

        assert_eq!(gap_start, SequenceNumber::new(0, 1));
        assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 2));
        assert!(sns.extract_numbers().is_empty());
    }

    #[test]
    fn test_calculate_gap_sns_from_vec_sparse_sns() {
        let mut gap_list =
            vec![SequenceNumber::new(0, 1), SequenceNumber::new(0, 5), SequenceNumber::new(0, 8)];

        let (gap_start, sns) = SubmessageCreator::calculate_gap_sns_from_vec(&mut gap_list);

        assert_eq!(gap_start, SequenceNumber::new(0, 1));
        assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 2));
        assert_eq!(
            sns.extract_numbers(),
            vec![SequenceNumber::new(0, 5), SequenceNumber::new(0, 8),]
        );
    }

    #[test]
    fn test_calculate_gap_sns_from_vec_continuous_with_sparse() {
        let mut gap_list = vec![
            SequenceNumber::new(0, 1),
            SequenceNumber::new(0, 2),
            SequenceNumber::new(0, 3),
            SequenceNumber::new(0, 4),
            SequenceNumber::new(0, 7),
            SequenceNumber::new(0, 10),
        ];

        let (gap_start, sns) = SubmessageCreator::calculate_gap_sns_from_vec(&mut gap_list);

        assert_eq!(gap_start, SequenceNumber::new(0, 1));
        assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 5));
        assert_eq!(
            sns.extract_numbers(),
            vec![SequenceNumber::new(0, 7), SequenceNumber::new(0, 10),]
        );
    }

    #[test]
    fn test_calculate_gap_sns_from_vec_continuous_with_sparse_over_256() {
        let mut gap_list = vec![
            SequenceNumber::new(0, 1),
            SequenceNumber::new(0, 2),
            SequenceNumber::new(0, 3),
            SequenceNumber::new(0, 4),
            SequenceNumber::new(0, 7),
            SequenceNumber::new(0, 260), // bitmap_base = 5, 5 + 255 = 260
            SequenceNumber::new(0, 261), // this is over 256
        ];

        let (gap_start, sns) = SubmessageCreator::calculate_gap_sns_from_vec(&mut gap_list);

        assert_eq!(gap_start, SequenceNumber::new(0, 1));
        assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 5));
        assert_eq!(
            sns.extract_numbers(),
            vec![SequenceNumber::new(0, 7), SequenceNumber::new(0, 260),]
        );
        assert!(gap_list == vec![SequenceNumber::new(0, 261)]);
    }

    #[test]
    fn test_calculate_gap_sns_from_vec_sparse_with_3_msgs() {
        let mut gap_list = vec![
            SequenceNumber::new(0, 1), // bitmap_base = 2, 2 + 255 = 257 so below is over 256
            SequenceNumber::new(0, 258), // bitmap_base = 259, 259 + 255 = 514 so below is over 256
            SequenceNumber::new(0, 515),
        ];

        let mut res: Vec<(SequenceNumber, SequenceNumberSet)> = Vec::new();

        while !gap_list.is_empty() {
            res.push(SubmessageCreator::calculate_gap_sns_from_vec(&mut gap_list));
        }

        // This should make 3 rtps gap messages.
        assert_eq!(res.len(), 3);

        // GAP 1
        assert_eq!(res[0].0, SequenceNumber::new(0, 1));
        assert_eq!(res[0].1.bitmap_base(), SequenceNumber::new(0, 2));
        assert!(res[0].1.extract_numbers().is_empty());

        // GAP 2
        assert_eq!(res[1].0, SequenceNumber::new(0, 258));
        assert_eq!(res[1].1.bitmap_base(), SequenceNumber::new(0, 259));
        assert!(res[1].1.extract_numbers().is_empty());

        // GAP 3
        assert_eq!(res[2].0, SequenceNumber::new(0, 515));
        assert_eq!(res[2].1.bitmap_base(), SequenceNumber::new(0, 516));
        assert!(res[2].1.extract_numbers().is_empty());
    }

    // fn calculate_acknack_sns_from_vec(
    //     missing_changes: &mut Vec<SequenceNumber>,
    // ) -> SequenceNumberSet {
    //     // Current ACKNACK logic (from create_acknack_submessage)
    //     // NOTE: This does NOT handle 256-bit limit like GAP does
    //     if missing_changes.is_empty() {
    //         SequenceNumberSet::new_empty_with_base(SequenceNumber::new(0, 1))
    //     } else {
    //         let base_sn = missing_changes[0];
    //         let result = SequenceNumberSet::from_vec(base_sn, missing_changes.clone());
    //         missing_changes.clear(); // Current logic consumes all at once (no remaining)
    //         result
    //     }
    // }

    // #[test]
    // fn test_calculate_acknack_sns_from_vec_empty_sns() {
    //     let mut missing_changes = vec![SequenceNumber::new(0, 1)];

    //     let sns = calculate_acknack_sns_from_vec(&mut missing_changes);

    //     // Expected: same as GAP test
    //     assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 2));
    //     assert!(sns.extract_numbers().is_empty());
    // }

    // #[test]
    // fn test_calculate_acknack_sns_from_vec_sparse_sns() {
    //     let mut missing_changes =
    //         vec![SequenceNumber::new(0, 1), SequenceNumber::new(0, 5), SequenceNumber::new(0, 8)];

    //     let sns = calculate_acknack_sns_from_vec(&mut missing_changes);

    //     // Expected: same as GAP test
    //     assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 2));
    //     assert_eq!(
    //         sns.extract_numbers(),
    //         vec![SequenceNumber::new(0, 5), SequenceNumber::new(0, 8),]
    //     );
    // }

    // #[test]
    // fn test_calculate_acknack_sns_from_vec_continuous_with_sparse() {
    //     let mut missing_changes = vec![
    //         SequenceNumber::new(0, 1),
    //         SequenceNumber::new(0, 2),
    //         SequenceNumber::new(0, 3),
    //         SequenceNumber::new(0, 4),
    //         SequenceNumber::new(0, 7),
    //         SequenceNumber::new(0, 10),
    //     ];

    //     let sns = calculate_acknack_sns_from_vec(&mut missing_changes);

    //     // Expected: same as GAP test
    //     assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 5));
    //     assert_eq!(
    //         sns.extract_numbers(),
    //         vec![SequenceNumber::new(0, 7), SequenceNumber::new(0, 10),]
    //     );
    // }

    // #[test]
    // fn test_calculate_acknack_sns_from_vec_continuous_with_sparse_over_256() {
    //     let mut missing_changes = vec![
    //         SequenceNumber::new(0, 1),
    //         SequenceNumber::new(0, 2),
    //         SequenceNumber::new(0, 3),
    //         SequenceNumber::new(0, 4),
    //         SequenceNumber::new(0, 7),
    //         SequenceNumber::new(0, 260), // bitmap_base = 5, 5 + 255 = 260
    //         SequenceNumber::new(0, 261), // this is over 256
    //     ];

    //     let sns = calculate_acknack_sns_from_vec(&mut missing_changes);

    //     // Expected: same as GAP test
    //     assert_eq!(sns.bitmap_base(), SequenceNumber::new(0, 5));
    //     assert_eq!(
    //         sns.extract_numbers(),
    //         vec![SequenceNumber::new(0, 7), SequenceNumber::new(0, 260),]
    //     );
    //     assert!(missing_changes == vec![SequenceNumber::new(0, 261)]);
    // }

    // #[test]
    // fn test_calculate_acknack_sns_from_vec_sparse_with_3_msgs() {
    //     let mut missing_changes = vec![
    //         SequenceNumber::new(0, 1), // bitmap_base = 2, 2 + 255 = 257 so below is over 256
    //         SequenceNumber::new(0, 258), // bitmap_base = 259, 259 + 255 = 514 so below is over 256
    //         SequenceNumber::new(0, 515),
    //     ];

    //     let mut res: Vec<SequenceNumberSet> = Vec::new();

    //     while !missing_changes.is_empty() {
    //         res.push(calculate_acknack_sns_from_vec(&mut missing_changes));
    //     }

    //     // Expected: same as GAP test - should make 3 rtps acknack messages
    //     assert_eq!(res.len(), 3);

    //     // ACKNACK 1
    //     assert_eq!(res[0].bitmap_base(), SequenceNumber::new(0, 2));
    //     assert!(res[0].extract_numbers().is_empty());

    //     // ACKNACK 2
    //     assert_eq!(res[1].bitmap_base(), SequenceNumber::new(0, 259));
    //     assert!(res[1].extract_numbers().is_empty());

    //     // ACKNACK 3
    //     assert_eq!(res[2].bitmap_base(), SequenceNumber::new(0, 516));
    //     assert!(res[2].extract_numbers().is_empty());
    // }
}
