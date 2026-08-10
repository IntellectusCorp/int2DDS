#![allow(dead_code)]
#![allow(unused_variables)]

use crate::utils::notify::{callback_handle, notify_user};
use std::{
    cmp::max,
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
    time::Instant,
};

use log::debug;

use crate::{
    common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
    core::time::Duration,
    infrastructure::status::{StatusInfo, StatusKind},
    rtps::{
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::Locator,
            sequence::{FragmentNumberSet, SequenceNumber},
        },
        entities::history::cache_change::CacheChange,
    },
};

#[derive(Clone)]
pub(crate) struct WriterProxy {
    remote_writer_guid: Guid,
    remote_group_entity_id: EntityId,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    data_max_size_serialized: u32,
    changes_from_writer: BTreeMap<SequenceNumber, ChangeFromWriter>,
    acknack_count: u32,
    nackfrag_count: u32,
    expected_sn: SequenceNumber, // Expected next sequence number from writer
    last_heartbeat_count: Option<u32>,
    last_heartbeat_at: Option<Instant>,
    last_heartbeat_frag_count: Option<u32>,
    buffered_change: BTreeSet<CacheChange>, // Changes that reader has not processed yet
    publication_builtin_topic_data: PublicationBuiltinTopicData,
    #[allow(clippy::type_complexity)]
    status_callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
}

impl PartialEq for WriterProxy {
    fn eq(&self, other: &Self) -> bool {
        self.remote_writer_guid == other.remote_writer_guid
    }
}

impl WriterProxy {
    #[allow(clippy::type_complexity)]
    pub(crate) fn new(
        remote_writer_guid: Guid,
        remote_group_entity_id: EntityId,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        data_max_size_serialized: u32,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
        status_callback: Arc<
            Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>,
        >,
    ) -> Self {
        Self {
            remote_writer_guid,
            remote_group_entity_id,
            unicast_locator_list,
            multicast_locator_list,
            data_max_size_serialized,
            changes_from_writer: BTreeMap::new(), // Initialize with empty vector
            acknack_count: 0,
            nackfrag_count: 0,
            expected_sn: SequenceNumber::UNKNOWN,
            last_heartbeat_count: None,
            last_heartbeat_frag_count: None,
            last_heartbeat_at: None,
            buffered_change: BTreeSet::new(),
            publication_builtin_topic_data,
            status_callback,
        }
    }

    pub(crate) fn increase_acknack_count(&mut self) {
        self.acknack_count = self.acknack_count.wrapping_add(1);
    }

    pub(crate) fn acknack_count(&self) -> u32 {
        self.acknack_count
    }

    pub(crate) fn increase_nackfrag_count(&mut self) {
        self.nackfrag_count = self.nackfrag_count.wrapping_add(1);
    }

    pub(crate) fn nackfrag_count(&self) -> u32 {
        self.nackfrag_count
    }

    pub(crate) fn expected_sn(&self) -> SequenceNumber {
        self.expected_sn
    }

    // For a change already received (via DATA or GAP), whether it was relevant
    // (DATA) or irrelevant (GAP). None if the change is not yet received.
    #[cfg(test)]
    pub(crate) fn received_change_is_relevant(&self, seq_num: SequenceNumber) -> Option<bool> {
        self.changes_from_writer.get(&seq_num).and_then(|change| {
            if change.status == ChangeFromWriterStatusKind::Received {
                Some(change.is_relevant)
            } else {
                None
            }
        })
    }

    pub(crate) fn last_heartbeat_count(&self) -> Option<u32> {
        self.last_heartbeat_count
    }

    pub(crate) fn set_last_heartbeat_count(&mut self, count: u32) {
        self.last_heartbeat_count = Some(count);
    }

    pub(crate) fn last_heartbeat_frag_count(&self) -> Option<u32> {
        self.last_heartbeat_frag_count
    }

    pub(crate) fn set_last_heartbeat_frag_count(&mut self, count: u32) {
        self.last_heartbeat_frag_count = Some(count);
    }

    pub(crate) fn last_heartbeat_at(&self) -> Option<Instant> {
        self.last_heartbeat_at
    }

    pub(crate) fn set_last_heartbeat_at(&mut self, at: Instant) {
        self.last_heartbeat_at = Some(at);
    }

    pub(crate) fn add_new_changes_from_writer(&mut self, change_from_writer: ChangeFromWriter) {
        self.changes_from_writer.insert(change_from_writer.sequence_number, change_from_writer);
    }

    pub(crate) fn add_buffered_change(&mut self, change: CacheChange) {
        self.buffered_change.insert(change);
    }

    pub(crate) fn flush_buffered_changes(&mut self) -> Vec<CacheChange> {
        let mut flushed_changes = Vec::new();

        while let Some(change) = self.buffered_change.first() {
            if change.sequence_number <= self.expected_sn {
                let change = self.buffered_change.pop_first().unwrap();
                flushed_changes.push(change);
                self.increment_expected_sn();
            } else {
                break;
            }
        }

        flushed_changes
    }

    pub(crate) fn publication_builtin_topic_data(&self) -> PublicationBuiltinTopicData {
        self.publication_builtin_topic_data.clone()
    }

    pub(crate) fn set_publication_builtin_topic_data(&mut self, data: PublicationBuiltinTopicData) {
        self.publication_builtin_topic_data = data;
    }

    pub(crate) fn get_ownership_strength(&self) -> i32 {
        self.publication_builtin_topic_data.ownership_strength().value
    }

    pub(crate) fn get_lifespan_duration(&self) -> Duration {
        self.publication_builtin_topic_data.lifespan().duration
    }

    pub(crate) fn has_fragmented_changes(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> bool {
        if last_sn < first_sn {
            return false;
        }

        self.changes_from_writer.range(first_sn..=last_sn).any(|(_, change)| {
            change.fragment_info.as_ref().map(|info| !info.is_complete).unwrap_or(false)
        })
    }

    pub(crate) fn still_missing_fragments(&self, seq_num: SequenceNumber) -> bool {
        self.changes_from_writer
            .get(&seq_num)
            .and_then(|change| change.fragment_info.as_ref())
            .is_some_and(|info| !info.is_complete)
    }

    pub(crate) fn all_fragments_received(&self, seq_num: SequenceNumber) -> bool {
        self.changes_from_writer
            .get(&seq_num)
            .and_then(|change| change.fragment_info.as_ref())
            .is_some_and(|info| info.is_complete)
    }

    /// Reports the first incomplete change in `first_sn..=last_sn` with the sequence number it
    /// belongs to.
    ///
    /// The caller stamps the NACK_FRAG with that sequence number and `handle_nack_frag` resolves
    /// the fragment numbers against it, so the two must not be derived separately.
    ///
    /// The set covers `base..base+256` -- one bitmap window. `base` is the lowest missing
    /// fragment, so each round advances.
    pub(crate) fn calculate_missing_fragments(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Option<(SequenceNumber, FragmentNumberSet)> {
        if last_sn < first_sn {
            return None;
        }

        self.changes_from_writer.range(first_sn..=last_sn).find_map(|(seq_num, change)| {
            change.fragment_info.as_ref().and_then(|info| {
                if info.is_complete {
                    return None;
                }

                let mut missing_fragments = Vec::new();
                let mut base_fragment = None;

                for fragment_num in 1..=info.total_fragments {
                    if !info.received_fragments.contains(&fragment_num) {
                        if base_fragment.is_none() {
                            base_fragment = Some(fragment_num);
                        }
                        missing_fragments.push(fragment_num);
                    }
                }

                if !missing_fragments.is_empty() {
                    base_fragment.map(|base| {
                        (*seq_num, FragmentNumberSet::from_vec(base, missing_fragments))
                    })
                } else {
                    None
                }
            })
        })
    }

    pub(crate) fn remote_writer_guid(&self) -> Guid {
        self.remote_writer_guid
    }

    pub(crate) fn unicast_locator_list(&self) -> &[Locator] {
        &self.unicast_locator_list
    }

    pub(crate) fn increment_expected_sn(&mut self) {
        self.expected_sn += 1;
    }

    pub(crate) fn set_expected_sn(&mut self, seq_num: SequenceNumber) {
        self.expected_sn = seq_num;
    }

    /// Get the maximum available sequence number from the writer, which indicates there are no missing or unknown changes before this sequence number.
    pub(crate) fn available_changes_max(&self) -> SequenceNumber {
        self.changes_from_writer
            .iter()
            .take_while(|(_, v)| {
                v.status == ChangeFromWriterStatusKind::Received
                    || v.status == ChangeFromWriterStatusKind::NotAvailable(NotAvailable::Removed)
            })
            .last()
            .map(|(k, _)| *k)
            .unwrap_or(SequenceNumber::UNKNOWN)
    }

    /// Get the maximum sequence number from changes_from_writer (regardless of status).
    pub(crate) fn changes_from_writer_max(&self) -> SequenceNumber {
        self.changes_from_writer
            .last_key_value()
            .map(|(k, _)| *k)
            .unwrap_or(SequenceNumber::UNKNOWN)
    }

    pub(crate) fn irrelevant_change_set(&mut self, a_seq_num: SequenceNumber) {
        let change = self.changes_from_writer.entry(a_seq_num).or_insert(ChangeFromWriter {
            sequence_number: a_seq_num,
            status: ChangeFromWriterStatusKind::Received,
            is_relevant: false,
            fragment_info: None,
        });

        change.status = ChangeFromWriterStatusKind::Received;
        change.is_relevant = false;
    }

    #[allow(clippy::unnecessary_filter_map)]
    pub(crate) fn lost_changes_update(&mut self, first_available_seq_num: SequenceNumber) {
        self.expected_sn = max(self.expected_sn, first_available_seq_num);

        let keys: Vec<SequenceNumber> = self
            .changes_from_writer
            .range(..first_available_seq_num)
            .filter_map(|(seq_num, change_from_writer)| {
                if change_from_writer.status == ChangeFromWriterStatusKind::Missing {
                    debug!("Sample Lost!: {}", change_from_writer.sequence_number);
                    self.on_sample_lost();
                }
                Some(*seq_num)
            })
            .collect();

        for key in keys {
            self.changes_from_writer.remove(&key);
        }
    }

    pub(crate) fn process_heartbeat(
        &mut self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Vec<SequenceNumber> {
        self.lost_changes_update(first_sn);

        self.update_changes_for_heartbeat_range(first_sn, last_sn);

        self.missing_changes_for_heartbeat(first_sn, last_sn)
    }

    fn update_changes_for_heartbeat_range(
        &mut self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) {
        if last_sn < first_sn {
            return;
        }

        let mut current_sn = first_sn;
        while current_sn <= last_sn {
            let change = self.changes_from_writer.entry(current_sn).or_insert(ChangeFromWriter {
                sequence_number: current_sn,
                status: ChangeFromWriterStatusKind::Missing,
                is_relevant: true,
                fragment_info: None,
            });
            if change.status == ChangeFromWriterStatusKind::Unknown {
                change.status = ChangeFromWriterStatusKind::Missing;
            }
            current_sn = current_sn.next();
        }
    }

    pub(crate) fn missing_changes_for_heartbeat(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Vec<SequenceNumber> {
        if last_sn < first_sn {
            return Vec::new();
        }

        let mut missing = Vec::new();
        let mut current_sn = first_sn;

        while current_sn <= last_sn {
            // Check change state for the corresponding sequence number
            match self.changes_from_writer.get(&current_sn) {
                Some(change) if change.status != ChangeFromWriterStatusKind::Missing => {}
                _ => {
                    missing.push(current_sn);
                }
            }
            current_sn = current_sn.next();
        }

        missing
    }

    /// Update change state to Received when Data submessage is received
    pub(crate) fn mark_change_received(
        &mut self,
        seq_num: SequenceNumber,
        fragment_info: Option<FragmentInfo>,
    ) {
        self.changes_from_writer
            .entry(seq_num)
            .or_insert_with(|| ChangeFromWriter {
                sequence_number: seq_num,
                status: ChangeFromWriterStatusKind::Received,
                is_relevant: true,
                fragment_info,
            })
            .status = ChangeFromWriterStatusKind::Received;
    }

    pub(crate) fn remove_change_from_writer_by_sn(&mut self, seq_num: SequenceNumber) {
        self.changes_from_writer.remove(&seq_num);
    }

    pub(crate) fn calculate_bitmap_base(&self) -> SequenceNumber {
        let last_received = self.available_changes_max();
        if last_received == SequenceNumber::UNKNOWN {
            SequenceNumber::new(0, 1)
        } else {
            last_received.next()
        }
    }

    pub(crate) fn mark_frag_received(
        &mut self,
        seq_num: SequenceNumber,
        total_fragments: u32,
        received: impl IntoIterator<Item = u32>,
    ) {
        let change = self.changes_from_writer.entry(seq_num).or_insert_with(|| ChangeFromWriter {
            sequence_number: seq_num,
            status: ChangeFromWriterStatusKind::Received,
            is_relevant: true,
            fragment_info: Some(FragmentInfo {
                total_fragments,
                received_fragments: std::collections::HashSet::new(),
                is_complete: false,
            }),
        });

        // Merge this submessage's fragment numbers into the accumulated set
        if let Some(info) = &mut change.fragment_info {
            // A HEARTBEAT_FRAG may have seeded this entry with a smaller
            // last-fragment number than the sample's true total; keep the max.
            info.total_fragments = info.total_fragments.max(total_fragments);
            for fragment in received {
                info.received_fragments.insert(fragment);
            }

            // Update completion status
            info.is_complete = info.received_fragments.len() == info.total_fragments as usize;
        }
    }

    pub(crate) fn on_sample_lost(&self) {
        // Lift the callback out before calling it: the listener it reaches may
        // re-enter this entity, and an unwind through the call would poison the slot.
        if let Some(callback) = callback_handle(&self.status_callback) {
            notify_user("writer_proxy", || callback(StatusKind::SAMPLE_LOST, None));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeFromWriterStatusKind {
    Unknown,
    Missing,
    Received,
    NotAvailable(NotAvailable),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotAvailable {
    Filtered,
    Removed,
    Unspecified,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeFromWriter {
    pub(crate) sequence_number: SequenceNumber,
    pub(crate) status: ChangeFromWriterStatusKind,
    pub(crate) is_relevant: bool,
    pub(crate) fragment_info: Option<FragmentInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FragmentInfo {
    pub(crate) total_fragments: u32,
    pub(crate) received_fragments: std::collections::HashSet<u32>,
    pub(crate) is_complete: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::messages::{
        message_creator::MessageCreator,
        message_receiver::{MessageReceiver, TypedSubmessage},
    };
    use bytes::Bytes;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn empty_writer_proxy() -> WriterProxy {
        let pub_data = PublicationBuiltinTopicData::default();
        WriterProxy::new(
            pub_data.endpoint_guid(),
            pub_data.endpoint_guid().entity_id(),
            Vec::new(),
            Vec::new(),
            0,
            pub_data,
            Arc::new(Mutex::new(None)),
        )
    }

    // Single-fragment submessages arriving out of order must accumulate;
    // missing set and completion track the union of received ranges.
    #[test]
    fn test_mark_frag_received_accumulates_out_of_order() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.mark_frag_received(sn, 4, 3..4); // fragment 3
        proxy.mark_frag_received(sn, 4, 1..2); // fragment 1
        assert!(proxy.still_missing_fragments(sn));
        assert_eq!(
            proxy.calculate_missing_fragments(sn, sn),
            Some((sn, FragmentNumberSet::from_vec(2, vec![2, 4]))),
        );

        proxy.mark_frag_received(sn, 4, 2..3); // fragment 2
        proxy.mark_frag_received(sn, 4, 4..5); // fragment 4
        assert!(proxy.all_fragments_received(sn));
        assert_eq!(proxy.calculate_missing_fragments(sn, sn), None);
    }

    // A submessage carrying several fragments passes a multi-element range.
    #[test]
    fn test_mark_frag_received_multi_fragment_range() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.mark_frag_received(sn, 4, 1..3); // fragments 1, 2
        assert!(proxy.still_missing_fragments(sn));
        assert_eq!(
            proxy.calculate_missing_fragments(sn, sn),
            Some((sn, FragmentNumberSet::from_vec(3, vec![3, 4]))),
        );

        proxy.mark_frag_received(sn, 4, 3..5); // fragments 3, 4
        assert!(proxy.all_fragments_received(sn));
    }

    fn holds_fragment(proxy: &WriterProxy, seq_num: SequenceNumber, fragment: u32) -> bool {
        proxy
            .changes_from_writer
            .get(&seq_num)
            .and_then(|change| change.fragment_info.as_ref())
            .is_some_and(|info| info.received_fragments.contains(&fragment))
    }

    /// A heartbeat covers a range, so the scan may settle on any change in it. The answer has to
    /// say which one.
    #[test]
    fn calculate_missing_fragments_names_the_sequence_number_it_answered_for() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        // Two fragmented samples in flight, each short of a *different* fragment.
        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3]); // SN 2 is missing fragment 4

        let (answered_for, missing) = proxy
            .calculate_missing_fragments(sn1, sn2)
            .expect("both samples are incomplete, so something must be reported");

        assert_eq!(answered_for, sn1, "the scan settled on the first incomplete change");
        assert_eq!(missing, FragmentNumberSet::from_vec(2, vec![2]));
    }

    /// Every fragment named must be one the nacked sample is actually missing.
    #[test]
    fn every_nacked_fragment_must_be_missing_from_the_sn_the_nack_carries() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3]); // SN 2 is missing fragment 4

        let (nacked_sn, missing) = proxy
            .calculate_missing_fragments(sn1, sn2)
            .expect("both samples are incomplete, so a repair request is owed");

        for fragment in missing.extract_numbers() {
            assert!(
                !holds_fragment(&proxy, nacked_sn, fragment),
                "NACK_FRAG is stamped writer_sn={nacked_sn:?} but names fragment {fragment}, \
                 which that sample already holds"
            );
        }
    }

    /// A short sample behind a complete one must still be nacked; naming the complete sample
    /// leaves the short one unrepairable.
    #[test]
    fn an_earlier_short_sample_is_nacked_even_when_the_last_one_is_whole() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3, 4]); // SN 2 is whole

        let (nacked_sn, missing) = proxy
            .calculate_missing_fragments(sn1, sn2)
            .expect("SN 1 is incomplete, so a repair request is owed");

        assert_eq!(nacked_sn, sn1, "the request must name the sample that is actually short");
        assert_eq!(missing, FragmentNumberSet::from_vec(2, vec![2]));
        assert!(proxy.still_missing_fragments(nacked_sn), "the nacked sample really is short");
    }

    /// The same guard through serialization, since the sequence number is a wire field.
    #[test]
    fn the_nack_frag_on_the_wire_names_the_short_sample() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3, 4]); // SN 2 is whole

        let (nacked_sn, missing) =
            proxy.calculate_missing_fragments(sn1, sn2).expect("SN 1 is incomplete");
        proxy.increase_nackfrag_count();

        let reader_guid = Guid::new([1u8; 12], EntityId::SEDP_BUILTIN_PUBLICATIONS_READER);
        let writer_guid = Guid::new([2u8; 12], EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER);

        let buffer = MessageCreator::create_nackfrag_msg(
            reader_guid,
            writer_guid,
            reader_guid.entity_id(),
            writer_guid.entity_id(),
            nacked_sn,
            missing,
            proxy.nackfrag_count(),
            None,
        )
        .expect("the NACK_FRAG message must serialize");

        let from_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7400);
        let mut receiver = MessageReceiver::new(writer_guid.prefix(), &from_addr);
        receiver.init(&Bytes::from(buffer.to_vec())).expect("the message must parse back");

        let submessages = receiver.parse_submessages();
        let nack_frags: Vec<_> = submessages
            .iter()
            .filter_map(|submessage| match submessage {
                TypedSubmessage::NackFrag(_, nack_frag) => Some(*nack_frag),
                _ => None,
            })
            .collect();

        assert_eq!(nack_frags.len(), 1, "one bitmap window per round");
        assert_eq!(nack_frags[0].writer_sn, sn1, "the wire must name the short sample");
        assert_eq!(nack_frags[0].fragment_number_state.extract_numbers(), vec![2]);
    }
}
