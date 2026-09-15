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
        common::{entity_id::EntityId, guid::Guid, locator::Locator, sequence::SequenceNumber},
        entities::history::cache_change::CacheChange,
        messages::submessages::{gap, heartbeat},
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
    heartbeat_group_info: Option<heartbeat::GroupInfo>, // Group info the latest accepted Heartbeat announced.
    highest_gap_end_gsn: Option<SequenceNumber>, // Furthest group sequence number any Gap has declared unavailable.
    buffered_change: BTreeSet<CacheChange>,      // Changes that reader has not processed yet
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
            heartbeat_group_info: None,
            highest_gap_end_gsn: None,
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

    pub(crate) fn heartbeat_group_info(&self) -> Option<heartbeat::GroupInfo> {
        self.heartbeat_group_info
    }

    // The announced range moves as the writer's history rolls, so the latest one replaces it.
    pub(crate) fn record_heartbeat_group_info(&mut self, group_info: heartbeat::GroupInfo) {
        self.heartbeat_group_info = Some(group_info);
    }

    pub(crate) fn highest_gap_end_gsn(&self) -> Option<SequenceNumber> {
        self.highest_gap_end_gsn
    }

    // A later Gap can cover a shorter range, and what an earlier one declared gone stays gone.
    pub(crate) fn record_gap_group_info(&mut self, group_info: gap::GroupInfo) {
        let highest = match self.highest_gap_end_gsn {
            Some(previous) => max(previous, group_info.gap_end_gsn),
            None => group_info.gap_end_gsn,
        };

        self.highest_gap_end_gsn = Some(highest);
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

    // The first incomplete fragmented change in first_sn..=last_sn. The caller stamps the
    // NACK_FRAG with this sequence number and gets its fragments via get_ascending_missing_fn_list.
    pub(crate) fn first_incomplete_fragmented_sn(
        &self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Option<SequenceNumber> {
        if last_sn < first_sn {
            return None;
        }

        self.changes_from_writer.range(first_sn..=last_sn).find_map(|(seq_num, change)| {
            change.fragment_info.as_ref().filter(|info| !info.is_complete).map(|_| *seq_num)
        })
    }

    // Ascending list of not-yet-received fragment numbers for a sample.
    pub(crate) fn get_ascending_missing_fn_list(&self, seq_num: SequenceNumber) -> Vec<u32> {
        self.changes_from_writer
            .get(&seq_num)
            .and_then(|change| change.fragment_info.as_ref())
            .filter(|info| !info.is_complete)
            .map(|info| {
                (1..=info.total_fragments)
                    .filter(|frag| !info.received_fragments.contains(frag))
                    .collect()
            })
            .unwrap_or_default()
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
    ///
    /// Not the ACKNACK base -- use [`Self::calculate_bitmap_base`] for that. This walks only the
    /// entries that exist and reads status alone, so it steps over a sequence number the ledger
    /// has no entry for and counts a sample still short of fragments as available. Either one
    /// turns into an acknowledgement of something that never arrived.
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

    /// Fold a heartbeat into the ledger and answer with everything an ACKNACK for it needs:
    /// the `readerSNState.base` and the sequence numbers to request.
    ///
    /// Both come back together on purpose. Each of the three heartbeat handlers used to take the
    /// missing list from here and then reach elsewhere for the base, and all three reached for
    /// `expected_sn` -- a delivery cursor, which on this receive path advances once per
    /// *datagram*. One announcement arriving on four NICs moved it four sequence numbers, and
    /// the ACKNACK then claimed delivery of three samples that never existed.
    pub(crate) fn process_heartbeat(
        &mut self,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> (SequenceNumber, Vec<SequenceNumber>) {
        self.lost_changes_update(first_sn);

        self.update_changes_for_heartbeat_range(first_sn, last_sn);

        (self.calculate_bitmap_base(), self.missing_changes_for_heartbeat(first_sn, last_sn))
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

    /// The `readerSNState.base` an ACKNACK for this writer must carry: the lowest sequence
    /// number the reader has neither received nor been told is irrelevant.
    ///
    /// RTPS 2.5 8.3.7.1.1 reads `base - 1` as a positive acknowledgement of everything below it,
    /// and a writer acts on that by dropping those changes. So this is derived from the receive
    /// ledger and nothing else -- never from `expected_sn`, which is a delivery cursor that
    /// several paths advance for reasons unrelated to what arrived.
    pub(crate) fn calculate_bitmap_base(&self) -> SequenceNumber {
        // Walk forward from the oldest sequence number still on record and stop at the first one
        // that is not settled. Three things can stop the walk, and all of them mean "not
        // received":
        //
        // * an entry that is still outstanding,
        // * a sequence number with no entry at all -- a sample can arrive before any heartbeat
        //   names the range behind it, and `missing_changes_for_heartbeat` already reads that
        //   absence as missing. Skipping over the hole would acknowledge a sample nobody sent.
        // * an entry short of fragments. `mark_frag_received` stamps a change `Received` on its
        //   first fragment, so a half-arrived sample is indistinguishable by status alone.
        //   Acknowledging it lets the writer drop the sample, which strands the NACK_FRAG that
        //   was going to repair it.
        //
        // Starting at the oldest entry rather than at 1 keeps the base inside the range the
        // writer still holds: `lost_changes_update` drops what the writer discarded, and
        // reporting 1 there would name a sequence number that no longer exists while claiming
        // nothing at all had arrived.
        let Some((first_seq_num, _)) = self.changes_from_writer.first_key_value() else {
            return SequenceNumber::new(0, 1);
        };

        let mut base = *first_seq_num;
        for (seq_num, change) in self.changes_from_writer.iter() {
            if *seq_num != base {
                return base;
            }
            let settled = match change.status {
                ChangeFromWriterStatusKind::Received => {
                    change.fragment_info.as_ref().map(|info| info.is_complete).unwrap_or(true)
                }
                ChangeFromWriterStatusKind::NotAvailable(NotAvailable::Removed) => true,
                _ => false,
            };

            if !settled {
                return *seq_num;
            }
            base = seq_num.next();
        }

        base
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

        // A HEARTBEAT or HEARTBEAT_FRAG may have created this change with no
        // fragment_info, so ensure it exists before merging received fragments.
        let info = change.fragment_info.get_or_insert_with(|| FragmentInfo {
            total_fragments,
            received_fragments: std::collections::HashSet::new(),
            is_complete: false,
        });

        // A HEARTBEAT_FRAG may have seeded a smaller last-fragment number than
        // the sample's true total, so keep the max.
        info.total_fragments = info.total_fragments.max(total_fragments);
        // Reject fragment numbers outside the sample's total: an unbounded insert would let a
        // mislabeled range satisfy the completion count without the buffer holding those bytes.
        for fragment in received.into_iter().filter(|f| (1..=info.total_fragments).contains(f)) {
            info.received_fragments.insert(fragment);
        }

        info.is_complete = info.received_fragments.len() == info.total_fragments as usize;
    }

    /// Undo `mark_frag_received` for one change. Called whenever the buffer backing it is gone
    /// before delivery -- eviction, or a bail between completion and delivery: the bytes are
    /// gone, so the ledger must go back to reporting nothing received, or the reader would only
    /// re-ask for the fragments it happened to see before the buffer was lost.
    pub(crate) fn forget_fragments(&mut self, seq_num: SequenceNumber) {
        if let Some(info) = self
            .changes_from_writer
            .get_mut(&seq_num)
            .and_then(|change| change.fragment_info.as_mut())
        {
            info.received_fragments.clear();
            info.is_complete = false;
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
    use crate::rtps::common::guid::GroupDigest;
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

    /// GAP says the sequence number will never carry data, so it must not hold the base back --
    /// otherwise the reader re-requests it for as long as the writer keeps heartbeating.
    #[test]
    fn gapped_sequence_numbers_do_not_hold_the_bitmap_base_back() {
        let mut proxy = empty_writer_proxy();
        proxy.process_heartbeat(SequenceNumber::new(0, 1), SequenceNumber::new(0, 4));

        proxy.irrelevant_change_set(SequenceNumber::new(0, 1));
        proxy.irrelevant_change_set(SequenceNumber::new(0, 2));
        proxy.mark_change_received(SequenceNumber::new(0, 3), None);

        assert_eq!(proxy.calculate_bitmap_base(), SequenceNumber::new(0, 4));
    }

    /// Nothing is known about this writer yet, so the reader cannot claim any sample arrived.
    #[test]
    fn the_bitmap_base_of_an_untouched_proxy_acknowledges_nothing() {
        assert_eq!(empty_writer_proxy().calculate_bitmap_base(), SequenceNumber::new(0, 1));
    }

    /// `on_reader_cache_change_removal` deletes the ledger entry when the reader's cache evicts a
    /// sample, which punches a hole behind the base. The hole is indistinguishable from a sample
    /// that never arrived, so the base stops at it and the sequence number gets requested again.
    ///
    /// That is the safe direction and it matches what the bitmap already does --
    /// `missing_changes_for_heartbeat` reads an absent entry as missing too, so this sequence
    /// number was being nacked before the base ever looked at it. Under-claiming costs a
    /// redundant retransmit; over-claiming would let the writer drop a sample for good.
    #[test]
    fn a_ledger_entry_removed_after_delivery_is_requested_again_rather_than_acknowledged() {
        let mut proxy = empty_writer_proxy();
        proxy.process_heartbeat(SequenceNumber::new(0, 1), SequenceNumber::new(0, 3));
        for sn in 1..=3 {
            proxy.mark_change_received(SequenceNumber::new(0, sn), None);
        }
        assert_eq!(proxy.calculate_bitmap_base(), SequenceNumber::new(0, 4));

        proxy.remove_change_from_writer_by_sn(SequenceNumber::new(0, 2));

        assert_eq!(
            proxy.calculate_bitmap_base(),
            SequenceNumber::new(0, 2),
            "the base must not run past a sequence number the ledger can no longer vouch for"
        );
        assert!(proxy
            .missing_changes_for_heartbeat(SequenceNumber::new(0, 1), SequenceNumber::new(0, 3))
            .contains(&SequenceNumber::new(0, 2)));
    }

    /// `mark_frag_received` stamps a change `Received` on its *first* fragment, so a sample that
    /// is still short of data looks settled in the ledger. Acknowledging it lets the writer drop
    /// it, and the NACK_FRAG that was going to repair it can then never be answered.
    #[test]
    fn an_incomplete_fragmented_sample_holds_the_bitmap_base_back() {
        let mut proxy = empty_writer_proxy();

        proxy.mark_frag_received(SequenceNumber::new(0, 1), 4, [1, 2]);

        assert!(proxy.still_missing_fragments(SequenceNumber::new(0, 1)));
        assert_eq!(
            proxy.calculate_bitmap_base(),
            SequenceNumber::new(0, 1),
            "the sample is short of fragments, so it has not been received"
        );
    }

    /// Once the last fragment lands the sample really is received and must stop holding the base.
    #[test]
    fn a_completed_fragmented_sample_releases_the_bitmap_base() {
        let mut proxy = empty_writer_proxy();

        proxy.mark_frag_received(SequenceNumber::new(0, 1), 4, [1, 2, 3, 4]);

        assert!(proxy.all_fragments_received(SequenceNumber::new(0, 1)));
        assert_eq!(proxy.calculate_bitmap_base(), SequenceNumber::new(0, 2));
    }

    /// A sample can arrive before any heartbeat has named the range it sits in, so the ledger
    /// carries no entry at all for the sequence numbers below it. `missing_changes_for_heartbeat`
    /// already reads an absent entry as missing; the base has to agree, or it acknowledges a hole
    /// nobody ever sent.
    ///
    /// This is the reliable user-data path: `deliver_change_to_reader` marks the arriving sample
    /// received and buffers it when it is out of order, without filling in the gap behind it.
    #[test]
    fn a_sequence_number_missing_from_the_ledger_holds_the_bitmap_base_back() {
        let mut proxy = empty_writer_proxy();

        proxy.mark_change_received(SequenceNumber::new(0, 1), None);
        proxy.mark_change_received(SequenceNumber::new(0, 3), None);

        assert_eq!(
            proxy.calculate_bitmap_base(),
            SequenceNumber::new(0, 2),
            "SN 2 never arrived and has no ledger entry; the base must stop at it"
        );
    }

    /// Three heartbeat handlers each derived the ACKNACK base separately, and all three reached
    /// for the delivery cursor. Handing the base back together with the missing list is what
    /// stops a fourth caller from inventing a fourth source for it.
    #[test]
    fn process_heartbeat_answers_with_the_base_the_acknack_must_carry() {
        let mut proxy = empty_writer_proxy();

        let (base, missing) =
            proxy.process_heartbeat(SequenceNumber::new(0, 1), SequenceNumber::new(0, 3));
        assert_eq!(base, SequenceNumber::new(0, 1), "nothing has arrived yet");
        assert_eq!(
            missing,
            vec![SequenceNumber::new(0, 1), SequenceNumber::new(0, 2), SequenceNumber::new(0, 3)]
        );

        // One announcement, delivered once per NIC. Measured on a four-NIC host: the receive
        // path runs four times for the same sequence number.
        for _ in 0..4 {
            proxy.mark_change_received(SequenceNumber::new(0, 1), None);
        }

        let (base, missing) =
            proxy.process_heartbeat(SequenceNumber::new(0, 1), SequenceNumber::new(0, 3));
        assert_eq!(
            base,
            SequenceNumber::new(0, 2),
            "four copies of SN 1 still mean only SN 1 arrived"
        );
        assert_eq!(missing, vec![SequenceNumber::new(0, 2), SequenceNumber::new(0, 3)]);
    }

    /// A writer that pruned its history heartbeats a range starting above 1. Reporting base 1
    /// there names a sequence number the writer no longer holds, and claims nothing at all was
    /// received; the base belongs at the start of what the writer still has.
    #[test]
    fn bitmap_base_starts_where_the_writers_history_starts() {
        let mut proxy = empty_writer_proxy();
        proxy.process_heartbeat(SequenceNumber::new(0, 5), SequenceNumber::new(0, 7));

        assert_eq!(
            proxy.calculate_bitmap_base(),
            SequenceNumber::new(0, 5),
            "5 is the oldest sample the writer still has, and none of the range arrived"
        );
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
        assert_eq!(proxy.first_incomplete_fragmented_sn(sn, sn), Some(sn));
        assert_eq!(proxy.get_ascending_missing_fn_list(sn), vec![2, 4]);

        proxy.mark_frag_received(sn, 4, 2..3); // fragment 2
        proxy.mark_frag_received(sn, 4, 4..5); // fragment 4
        assert!(proxy.all_fragments_received(sn));
        assert_eq!(proxy.first_incomplete_fragmented_sn(sn, sn), None);
    }

    // A submessage carrying several fragments passes a multi-element range.
    #[test]
    fn test_mark_frag_received_multi_fragment_range() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.mark_frag_received(sn, 4, 1..3); // fragments 1, 2
        assert!(proxy.still_missing_fragments(sn));
        assert_eq!(proxy.first_incomplete_fragmented_sn(sn, sn), Some(sn));
        assert_eq!(proxy.get_ascending_missing_fn_list(sn), vec![3, 4]);

        proxy.mark_frag_received(sn, 4, 3..5); // fragments 3, 4
        assert!(proxy.all_fragments_received(sn));
    }

    // A HEARTBEAT that references a sequence number before any DATA_FRAG creates the change with
    // no fragment_info. A later fragment must still register so the sample is tracked as
    // fragmented and the reader can NACK_FRAG the rest.
    #[test]
    fn test_mark_frag_received_after_heartbeat_seeded_change() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.process_heartbeat(sn, sn);
        assert!(!proxy.has_fragmented_changes(sn, sn));

        proxy.mark_frag_received(sn, 4, 1..2); // fragment 1

        assert!(proxy.has_fragmented_changes(sn, sn));
        assert!(proxy.still_missing_fragments(sn));
        assert_eq!(proxy.first_incomplete_fragmented_sn(sn, sn), Some(sn));
        assert_eq!(proxy.get_ascending_missing_fn_list(sn), vec![2, 3, 4]);
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
    fn first_incomplete_fragmented_sn_names_the_sequence_number_it_answered_for() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        // Two fragmented samples in flight, each short of a *different* fragment.
        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3]); // SN 2 is missing fragment 4

        let answered_for = proxy
            .first_incomplete_fragmented_sn(sn1, sn2)
            .expect("both samples are incomplete, so something must be reported");

        assert_eq!(answered_for, sn1, "the scan settled on the first incomplete change");
        assert_eq!(proxy.get_ascending_missing_fn_list(answered_for), vec![2]);
    }

    /// Every fragment named must be one the nacked sample is actually missing.
    #[test]
    fn every_nacked_fragment_must_be_missing_from_the_sn_the_nack_carries() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3]); // SN 2 is missing fragment 4

        let nacked_sn = proxy
            .first_incomplete_fragmented_sn(sn1, sn2)
            .expect("both samples are incomplete, so a repair request is owed");

        for fragment in proxy.get_ascending_missing_fn_list(nacked_sn) {
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

        let nacked_sn = proxy
            .first_incomplete_fragmented_sn(sn1, sn2)
            .expect("SN 1 is incomplete, so a repair request is owed");

        assert_eq!(nacked_sn, sn1, "the request must name the sample that is actually short");
        assert_eq!(proxy.get_ascending_missing_fn_list(nacked_sn), vec![2]);
        assert!(proxy.still_missing_fragments(nacked_sn), "the nacked sample really is short");
    }

    /// The NACK_FRAG retry re-arms itself for as long as this reports something missing, so a
    /// completed sample has to report `None` or the timer never stops.
    #[test]
    fn a_completed_sample_reports_nothing_missing() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.mark_frag_received(sn, 4, [1, 3]);
        assert!(
            proxy.first_incomplete_fragmented_sn(sn, sn).is_some(),
            "2 and 4 are still missing"
        );

        proxy.mark_frag_received(sn, 4, [2, 4]);
        assert!(proxy.all_fragments_received(sn));
        assert_eq!(
            proxy.first_incomplete_fragmented_sn(sn, sn),
            None,
            "a whole sample must end the retry loop"
        );
    }

    /// A partially received sample is invisible to ACKNACK, so only a NACK_FRAG can recover it.
    ///
    /// `mark_frag_received` stamps the change `Received` on the first fragment, which takes it
    /// out of `missing_changes_for_heartbeat`. `handle_heartbeat_message` must therefore route on
    /// "is anything in this range short of fragments" -- not on whether the *last* sample is
    /// whole. Routing on the latter sends a plain ACKNACK that omits the short sample, nothing
    /// asks for it again, and the writer stops heartbeating once the rest is acked: permanent
    /// loss on a RELIABLE reader rather than a delay.
    #[test]
    fn a_short_earlier_sample_is_invisible_to_acknack() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is short of fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3, 4]); // SN 2 is whole

        // The two predicates the routing decision can be built from disagree here. This is the
        // case the old condition got wrong.
        assert!(
            proxy.has_fragmented_changes(sn1, sn2),
            "SN 1 is short, so a NACK_FRAG is owed for this heartbeat range"
        );
        assert!(
            proxy.all_fragments_received(sn2),
            "...while the range's last sample is whole, which is what used to force the ACKNACK \
             branch"
        );

        // And the ACKNACK branch could not have recovered SN 1 anyway.
        assert!(
            !proxy.missing_changes_for_heartbeat(sn1, sn2).contains(&sn1),
            "SN 1 is marked Received once any fragment arrives, so an ACKNACK never lists it; \
             taking the ACKNACK branch here loses the sample outright"
        );
    }

    /// The same guard through serialization, since the sequence number is a wire field.
    #[test]
    fn the_nack_frag_on_the_wire_names_the_short_sample() {
        let mut proxy = empty_writer_proxy();
        let sn1 = SequenceNumber::new(0, 1);
        let sn2 = SequenceNumber::new(0, 2);

        proxy.mark_frag_received(sn1, 4, [1, 3, 4]); // SN 1 is missing fragment 2
        proxy.mark_frag_received(sn2, 4, [1, 2, 3, 4]); // SN 2 is whole

        let nacked_sn = proxy.first_incomplete_fragmented_sn(sn1, sn2).expect("SN 1 is incomplete");
        let mut missing = proxy.get_ascending_missing_fn_list(nacked_sn);

        let reader_guid = Guid::new([1u8; 12], EntityId::SEDP_BUILTIN_PUBLICATIONS_READER);
        let writer_guid = Guid::new([2u8; 12], EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER);

        let (messages, consumed_count) = MessageCreator::create_multiple_nackfrag_msgs(
            reader_guid,
            writer_guid,
            reader_guid.entity_id(),
            writer_guid.entity_id(),
            nacked_sn,
            &mut missing,
            1,
        )
        .expect("the NACK_FRAG message must serialize");
        assert_eq!(messages.len(), 1, "a single window fits one datagram");
        assert_eq!(consumed_count, 1, "one count per NACK_FRAG submessage");

        let from_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7400);
        let mut receiver = MessageReceiver::new(writer_guid.prefix(), &from_addr);
        receiver.init(&Bytes::from(messages[0].to_vec())).expect("the message must parse back");

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

    /// Eviction destroys the buffered bytes but leaves the ledger untouched unless something
    /// retracts the arrival record too. Forgetting must reset the change to "nothing received",
    /// not just clear the completion flag, or the reader would re-ask for only the fragments it
    /// never actually has.
    #[test]
    fn an_evicted_reassembly_is_re_requested_from_the_start() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.mark_frag_received(sn, 4, [1, 2]);
        assert_eq!(proxy.get_ascending_missing_fn_list(sn), vec![3, 4]);

        proxy.forget_fragments(sn);

        assert_eq!(
            proxy.get_ascending_missing_fn_list(sn),
            vec![1, 2, 3, 4],
            "the buffer behind this entry is gone; nothing may be treated as received"
        );
        assert!(!proxy.all_fragments_received(sn));
    }

    /// A fragment number outside the sample's total must never be recorded. Left unbounded, a
    /// range that names numbers beyond `total_fragments` could fill `received_fragments` to the
    /// same count as `total_fragments` while the buffer backing it holds none of the real bytes.
    #[test]
    fn mark_frag_received_rejects_fragment_numbers_outside_the_total() {
        let mut proxy = empty_writer_proxy();
        let sn = SequenceNumber::new(0, 1);

        proxy.mark_frag_received(sn, 4, 5..9);

        assert_eq!(proxy.get_ascending_missing_fn_list(sn), vec![1, 2, 3, 4]);
        assert!(!proxy.all_fragments_received(sn));
    }

    fn heartbeat_group_info(
        current_gsn: i64,
        first_gsn: i64,
        last_gsn: i64,
    ) -> heartbeat::GroupInfo {
        heartbeat::GroupInfo {
            current_gsn: SequenceNumber::from_i64(current_gsn),
            first_gsn: SequenceNumber::from_i64(first_gsn),
            last_gsn: SequenceNumber::from_i64(last_gsn),
            writer_set: GroupDigest::EMPTY,
            secure_writer_set: GroupDigest::EMPTY,
        }
    }

    // firstGSN and lastGSN move forward as the writer's history rolls, so keeping the highest
    // pair would claim the writer still holds samples it has already dropped.
    #[test]
    fn records_the_group_info_of_the_latest_heartbeat() {
        let mut proxy = empty_writer_proxy();

        proxy.record_heartbeat_group_info(heartbeat_group_info(5, 1, 5));
        proxy.record_heartbeat_group_info(heartbeat_group_info(9, 4, 9));

        assert_eq!(proxy.heartbeat_group_info(), Some(heartbeat_group_info(9, 4, 9)));
    }

    // A Gap declares its range gone for good, and a later Gap can cover a shorter one. Replacing
    // the value would lose the proof that the group sequence numbers up to 7 will never arrive.
    #[test]
    fn keeps_the_furthest_gap_end_group_sequence_number() {
        let mut proxy = empty_writer_proxy();

        proxy.record_gap_group_info(gap::GroupInfo {
            gap_start_gsn: SequenceNumber::from_i64(3),
            gap_end_gsn: SequenceNumber::from_i64(7),
        });
        proxy.record_gap_group_info(gap::GroupInfo {
            gap_start_gsn: SequenceNumber::from_i64(4),
            gap_end_gsn: SequenceNumber::from_i64(4),
        });

        assert_eq!(proxy.highest_gap_end_gsn(), Some(SequenceNumber::from_i64(7)));
    }
}
