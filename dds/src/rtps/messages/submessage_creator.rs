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
            gap,
            heartbeat::{self, Heartbeat},
            info::{InfoDestination, InfoTimestamp},
            nack_frag::NackFrag,
        },
    },
};

pub(crate) struct SubmessageCreator;

impl SubmessageCreator {
    pub(crate) fn create_info_ts_submessage(timestamp: DateTime<Utc>) -> Submessage<'static> {
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

    pub(crate) fn create_info_dst_submessage(prefix: GuidPrefix) -> Submessage<'static> {
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
        heartbeat_count: u32,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
        final_flag: bool,
        liveliness_flag: bool,
        group_info: Option<heartbeat::GroupInfo>,
    ) -> Result<Submessage<'static>, Box<dyn std::error::Error>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::HEARTBEAT);
        if liveliness_flag {
            data_header_flag.add_flag(SubmessageFlagType::LivelinessFlag, SubmessageId::HEARTBEAT);
        }
        if final_flag {
            data_header_flag.add_flag(SubmessageFlagType::FinalFlag, SubmessageId::HEARTBEAT);
        }
        if group_info.is_some() {
            data_header_flag.add_flag(SubmessageFlagType::GroupInfoFlag, SubmessageId::HEARTBEAT);
        }
        let heartbeat_data = Heartbeat::new(
            reader_entity_id,
            writer_entity_id,
            first_sn,
            last_sn,
            heartbeat_count,
            group_info,
        );

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
        acknack_count: u32,
        bitmap_base: SequenceNumber,
        is_preemptive: bool,
    ) -> Result<Submessage<'static>, Box<dyn std::error::Error>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::ACKNACK);

        // `bitmap_base` is the reader's answer about its own receive ledger, and RTPS 2.5
        // 8.3.7.1.1 has the writer read `base - 1` as a positive acknowledgement. It is carried
        // through on both branches.
        //
        // Rebasing the set onto the first entry of the missing list -- as this used to do --
        // acknowledges everything between the two whenever they differ, and they differ exactly
        // when the thing the reader is still waiting on cannot appear in that list: a sample
        // short of fragments is stamped `Received`, and a sequence number with no ledger entry
        // sits outside the heartbeat range the list was built from.
        //
        // `from_vec` covers `base..base+255` and silently ignores anything beyond, so a distant
        // sequence number simply waits for a later round as the base walks forward.
        let reader_sn_state = if missing_changes.is_empty() {
            if !is_preemptive {
                data_header_flag.add_flag(SubmessageFlagType::FinalFlag, SubmessageId::ACKNACK);
            }
            SequenceNumberSet::new_empty_with_base(bitmap_base)
        } else {
            SequenceNumberSet::from_vec(bitmap_base, missing_changes)
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
        group_info: Option<gap::GroupInfo>,
    ) -> Result<Submessage<'static>, Box<dyn std::error::Error>> {
        if gap_list.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Gap list cannot be empty",
            )));
        }

        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::GAP);
        if group_info.is_some() {
            data_header_flag.add_flag(SubmessageFlagType::GroupInfoFlag, SubmessageId::GAP);
        }

        let (gap_start, sequence_number_set) = Self::calculate_gap_sns_from_vec(gap_list);

        let gap_data = gap::Gap::new(
            reader_entity_id,
            writer_entity_id,
            gap_start,
            sequence_number_set,
            group_info,
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
        group_info: Option<gap::GroupInfo>,
    ) -> Result<Submessage<'static>, Box<dyn std::error::Error>> {
        let mut data_header_flag = SubmessageHeaderFlag::new();
        data_header_flag.add_flag(SubmessageFlagType::EndiannessFlag, SubmessageId::GAP);
        if group_info.is_some() {
            data_header_flag.add_flag(SubmessageFlagType::GroupInfoFlag, SubmessageId::GAP);
        }

        // RTPS 2.5 - 8.3.8.4.5
        // The set of sequence numbers identify in the range gapStart <= sequence_number <= gapList.base -1
        let sequence_number_set = SequenceNumberSet::new_empty_with_base(gap_end + 1);

        let gap_data = gap::Gap::new(
            reader_entity_id,
            writer_entity_id,
            gap_start,
            sequence_number_set,
            group_info,
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
        nackfrag_count: u32,
    ) -> Result<Submessage<'static>, Box<dyn std::error::Error>> {
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
        let mut sn_after_base: Vec<SequenceNumber> =
            Vec::with_capacity(gap_list.len().saturating_sub(1).min(256));
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

    // Pack the lowest missing fragment numbers into one FragmentNumberSet within
    // the 256-wide window from the first entry, draining them from frag_list.
    pub(crate) fn calculate_nackfrag_fns_from_vec(frag_list: &mut Vec<u32>) -> FragmentNumberSet {
        if frag_list.is_empty() {
            return FragmentNumberSet::new_empty_with_base(1);
        }

        let base = frag_list[0];
        let window_end = base.saturating_add(255);
        let split_at = frag_list.partition_point(|&frag| frag <= window_end);
        let window: Vec<u32> = frag_list.drain(..split_at).collect();

        FragmentNumberSet::from_vec(base, window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::guid::GroupDigest;

    fn acknack_state(
        missing_changes: Vec<SequenceNumber>,
        bitmap_base: SequenceNumber,
    ) -> SequenceNumberSet {
        let submessage = SubmessageCreator::create_acknack_submessage(
            EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            missing_changes,
            1,
            bitmap_base,
            false,
        )
        .expect("the ACKNACK submessage must build");

        match submessage.body {
            SubmessageBody::AckNack(ack_nack) => ack_nack.reader_sn_state,
            _ => panic!("expected an ACKNACK submessage"),
        }
    }

    /// The base states what the reader received; the bitmap states which sequence numbers above
    /// it are outstanding. Rebasing the set onto the first entry of the missing list discards the
    /// reader's answer and silently acknowledges everything below that entry.
    ///
    /// The two diverge whenever the first thing the reader is still waiting on does not appear in
    /// the missing list: a sample short of fragments is stamped `Received`, and a sequence number
    /// with no ledger entry falls outside the heartbeat range the list was built from.
    #[test]
    fn the_acknack_keeps_the_base_the_reader_computed() {
        let state = acknack_state(
            vec![SequenceNumber::new(0, 5), SequenceNumber::new(0, 6)],
            SequenceNumber::new(0, 3),
        );

        assert_eq!(
            state.bitmap_base(),
            SequenceNumber::new(0, 3),
            "SN 3 and 4 were never received, so base - 1 must not reach them"
        );
        assert_eq!(
            state.extract_numbers(),
            vec![SequenceNumber::new(0, 5), SequenceNumber::new(0, 6)],
            "the outstanding sequence numbers still have to be requested"
        );
    }

    /// The bitmap covers `base .. base + 255`. Sequence numbers past that window cannot be named
    /// in this ACKNACK, and `NumberSet::from_vec` drops them without a word -- so the base must
    /// still be the reader's, and the request resumes as the window walks forward.
    #[test]
    fn a_missing_sequence_number_past_the_bitmap_window_does_not_move_the_base() {
        let state = acknack_state(
            vec![SequenceNumber::new(0, 10), SequenceNumber::new(0, 400)],
            SequenceNumber::new(0, 2),
        );

        assert_eq!(state.bitmap_base(), SequenceNumber::new(0, 2));
        assert_eq!(
            state.extract_numbers(),
            vec![SequenceNumber::new(0, 10)],
            "400 is outside base..base+255 and waits for a later round"
        );
    }

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

    #[test]
    fn test_calculate_nackfrag_fns_from_vec_sparse() {
        let mut frag_list = vec![1, 5, 8];

        let fns = SubmessageCreator::calculate_nackfrag_fns_from_vec(&mut frag_list);

        assert_eq!(fns.bitmap_base(), 1);
        assert_eq!(fns.extract_numbers(), vec![1, 5, 8]);
        assert!(frag_list.is_empty());
    }

    #[test]
    fn test_calculate_nackfrag_fns_from_vec_over_256() {
        // base = 1, 1 + 255 = 256 so 257 is over the window
        let mut frag_list = vec![1, 5, 256, 257];

        let fns = SubmessageCreator::calculate_nackfrag_fns_from_vec(&mut frag_list);

        assert_eq!(fns.bitmap_base(), 1);
        assert_eq!(fns.extract_numbers(), vec![1, 5, 256]);
        assert_eq!(frag_list, vec![257]);
    }

    #[test]
    fn test_calculate_nackfrag_fns_from_vec_3_msgs() {
        // 1 -> window [1,256]; 257 -> window [257,512]; 513 -> window [513,768]
        let mut frag_list = vec![1, 257, 513];

        let mut res: Vec<FragmentNumberSet> = Vec::new();

        while !frag_list.is_empty() {
            res.push(SubmessageCreator::calculate_nackfrag_fns_from_vec(&mut frag_list));
        }

        assert_eq!(res.len(), 3);

        assert_eq!(res[0].bitmap_base(), 1);
        assert_eq!(res[0].extract_numbers(), vec![1]);

        assert_eq!(res[1].bitmap_base(), 257);
        assert_eq!(res[1].extract_numbers(), vec![257]);

        assert_eq!(res[2].bitmap_base(), 513);
        assert_eq!(res[2].extract_numbers(), vec![513]);
    }

    const HEARTBEAT_GROUP_INFO_FLAG: u8 = 0x08;
    const GAP_GROUP_INFO_FLAG: u8 = 0x02;

    fn heartbeat_submessage_flags(group_info: Option<heartbeat::GroupInfo>) -> u8 {
        SubmessageCreator::create_heartbeat_submessage(
            1,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            SequenceNumber::new(0, 1),
            SequenceNumber::new(0, 1),
            false,
            false,
            group_info,
        )
        .expect("the HEARTBEAT submessage must build")
        .header
        .flags()
    }

    fn dummy_heartbeat_group_info() -> heartbeat::GroupInfo {
        heartbeat::GroupInfo {
            current_gsn: SequenceNumber::new(0, 4),
            first_gsn: SequenceNumber::new(0, 1),
            last_gsn: SequenceNumber::new(0, 3),
            writer_set: GroupDigest::new([0xaa, 0xbb, 0xcc, 0xdd]),
            secure_writer_set: GroupDigest::new([0; 4]),
        }
    }

    fn dummy_gap_group_info() -> gap::GroupInfo {
        gap::GroupInfo {
            gap_start_gsn: SequenceNumber::new(0, 1),
            gap_end_gsn: SequenceNumber::new(0, 3),
        }
    }

    #[test]
    fn the_heartbeat_flags_group_info_only_when_it_carries_some() {
        assert_eq!(heartbeat_submessage_flags(None) & HEARTBEAT_GROUP_INFO_FLAG, 0);
        assert_eq!(
            heartbeat_submessage_flags(Some(dummy_heartbeat_group_info()))
                & HEARTBEAT_GROUP_INFO_FLAG,
            HEARTBEAT_GROUP_INFO_FLAG
        );
    }

    #[test]
    fn the_consecutive_gap_flags_group_info_only_when_it_carries_some() {
        let gap_flags = |group_info| {
            SubmessageCreator::create_gap_submessage_consecutive(
                EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
                EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
                SequenceNumber::new(0, 1),
                SequenceNumber::new(0, 3),
                group_info,
            )
            .expect("the GAP submessage must build")
            .header
            .flags()
        };

        assert_eq!(gap_flags(None) & GAP_GROUP_INFO_FLAG, 0);
        assert_eq!(
            gap_flags(Some(dummy_gap_group_info())) & GAP_GROUP_INFO_FLAG,
            GAP_GROUP_INFO_FLAG
        );
    }

    #[test]
    fn the_batched_gap_flags_group_info_only_when_it_carries_some() {
        let gap_flags = |group_info| {
            let mut gap_list = vec![SequenceNumber::new(0, 1), SequenceNumber::new(0, 3)];
            SubmessageCreator::create_gap_submessage(
                EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
                EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
                &mut gap_list,
                group_info,
            )
            .expect("the GAP submessage must build")
            .header
            .flags()
        };

        assert_eq!(gap_flags(None) & GAP_GROUP_INFO_FLAG, 0);
        assert_eq!(
            gap_flags(Some(dummy_gap_group_info())) & GAP_GROUP_INFO_FLAG,
            GAP_GROUP_INFO_FLAG
        );
    }
}
