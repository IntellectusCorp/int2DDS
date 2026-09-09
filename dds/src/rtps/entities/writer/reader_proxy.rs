#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use log::debug;

use crate::{
    common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    rtps::{
        builtin::data::content_filtered_topic::FilterSignature,
        common::{entity_id::EntityId, guid::Guid, locator::Locator, sequence::SequenceNumber},
        entities::history::{
            cache_change::CacheChange, history_cache::HistoryCache,
            writer_history::WriterHistoryCache,
        },
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ReaderProxy {
    remote_reader_guid: Guid,
    remote_group_entity_id: EntityId,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    requested_changes: Vec<SequenceNumber>, // List of sequence numbers requested again by Reader through ACKNACK or NACK message
    requested_fragments: BTreeMap<SequenceNumber, BTreeSet<u32>>, // Fragment numbers requested again by Reader through NACK_FRAG, grouped by sequence number
    highest_sent_change_sn: SequenceNumber, // Sequence number of the last CacheChange recorded as sent to Reader
    max_acked_sn: SequenceNumber,
    expects_inline_qos: bool, // false
    is_active: bool,
    last_acknack_count: Option<u32>,
    last_acknack_at: Option<Instant>,
    last_nackfrag_count: Option<u32>,
    last_nackfrag_at: Option<Instant>,
    content_filter_signatures: Option<Vec<FilterSignature>>, // Content filter signatures for this reader
    subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    last_irrelevant_sn: SequenceNumber, // Sequence numbers <= this value are irrelevant for this reader and should be responded with GAP.
    is_first_hb_sent: bool,             // has stateful writer sent first heartbeat to this reader
}

use std::hash::{Hash, Hasher};

impl PartialEq for ReaderProxy {
    fn eq(&self, other: &Self) -> bool {
        self.remote_reader_guid == other.remote_reader_guid
    }
}

impl Eq for ReaderProxy {}

impl Hash for ReaderProxy {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.remote_reader_guid.hash(state);
    }
}

impl ReaderProxy {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        remote_reader_guid: Guid,
        remote_group_entity_id: EntityId,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        highest_sent_change_sn: SequenceNumber,
        max_acked_sn: SequenceNumber,
        expects_inline_qos: bool,
        is_active: bool,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
        last_irrelevant_sn: SequenceNumber,
    ) -> Self {
        Self {
            remote_reader_guid,
            remote_group_entity_id,
            unicast_locator_list,
            multicast_locator_list,
            highest_sent_change_sn,
            requested_changes: Vec::new(),
            requested_fragments: BTreeMap::new(),
            max_acked_sn,
            expects_inline_qos,
            is_active,
            last_acknack_count: None,
            last_acknack_at: None,
            last_nackfrag_count: None,
            last_nackfrag_at: None,
            content_filter_signatures: None,
            subscription_builtin_topic_data,
            last_irrelevant_sn,
            is_first_hb_sent: false,
        }
    }

    pub(crate) fn subscription_builtin_topic_data(&self) -> &SubscriptionBuiltinTopicData {
        &self.subscription_builtin_topic_data
    }

    pub(crate) fn is_reliable(&self) -> bool {
        self.subscription_builtin_topic_data.is_reliable()
    }

    pub(crate) fn set_subscription_builtin_topic_data(
        &mut self,
        data: SubscriptionBuiltinTopicData,
    ) {
        self.subscription_builtin_topic_data = data;
    }

    pub(crate) fn remote_reader_guid(&self) -> Guid {
        self.remote_reader_guid
    }

    pub(crate) fn remote_group_entity_id(&self) -> EntityId {
        self.remote_group_entity_id
    }

    pub(crate) fn highest_sent_change_sn(&self) -> SequenceNumber {
        self.highest_sent_change_sn
    }

    pub(crate) fn acked_changes_set(&mut self, committed_seq_num: SequenceNumber) {
        // Update cached max_acked_sn
        if committed_seq_num > self.max_acked_sn {
            self.max_acked_sn = committed_seq_num;
        }
    }

    pub(crate) fn max_acked_sn(&self) -> SequenceNumber {
        self.max_acked_sn
    }

    pub(crate) fn next_requested_change(&self) -> SequenceNumber {
        self.requested_changes.iter().min().cloned().unwrap_or(SequenceNumber::UNKNOWN)
    }

    pub(crate) fn next_unsent_change(&self, history_cache: &WriterHistoryCache) -> SequenceNumber {
        let start_sn = std::cmp::max(self.highest_sent_change_sn, self.last_irrelevant_sn);
        history_cache
            .next_change_after(start_sn)
            .map(|change| change.sequence_number())
            .unwrap_or(SequenceNumber::UNKNOWN)
    }

    pub(crate) fn requested_changes(&self) -> &[SequenceNumber] {
        &self.requested_changes
    }

    pub(crate) fn requested_changes_set(&mut self, req_seq_num_set: Vec<SequenceNumber>) {
        self.requested_changes = Vec::new();
        for seq_num in req_seq_num_set {
            self.requested_changes.push(seq_num);
        }
    }

    pub(crate) fn remove_cached_sn_on_cache_change_removal(&mut self, deleted_sn: SequenceNumber) {
        self.requested_changes.retain(|sn| *sn != deleted_sn);
        self.requested_fragments.remove(&deleted_sn);
    }

    pub(crate) fn empty_requested_changes(&mut self) {
        self.requested_changes = Vec::new();
    }

    // Records `fragment_nums`, the fragments a NACK_FRAG requested for `seq_num`.
    // Its bitmap states the receive status only for [window_start, window_start + window_num_bits).
    pub(crate) fn requested_fragments_add(
        &mut self,
        seq_num: SequenceNumber,
        window_start: u32,       // First fragment number the bitmap covers.
        window_num_bits: u32,    // Number of bits in the bitmap
        fragment_nums: Vec<u32>, // Fragment numbers the bitmap still requests, all in [window_start, window_start + window_num_bits)
    ) {
        // First fragment number after the range the bitmap covers.
        let window_end = window_start.saturating_add(window_num_bits);

        // Pending fragments of this sample, accumulated across earlier NACK_FRAGs.
        let missing_fragments = self.requested_fragments.entry(seq_num).or_default();

        // Clears every old entry inside [window_start, window_end)
        missing_fragments
            .retain(|fragment_num| *fragment_num < window_start || *fragment_num >= window_end);

        // Then puts back the ones the bitmap still requests.
        missing_fragments.extend(fragment_nums);
    }

    pub(crate) fn take_requested_fragments(&mut self) -> BTreeMap<SequenceNumber, BTreeSet<u32>> {
        std::mem::take(&mut self.requested_fragments)
    }

    pub(crate) fn unsent_changes(&self, history_cache: &WriterHistoryCache) -> bool {
        self.next_unsent_change(history_cache) != SequenceNumber::UNKNOWN
    }

    pub(crate) fn unacked_changes(&self, history_cache: &WriterHistoryCache) -> bool {
        history_cache.get_seq_num_max().is_some_and(|max_sn| max_sn > self.max_acked_sn)
    }

    pub(crate) fn unicast_locator_list(&self) -> &[Locator] {
        &self.unicast_locator_list
    }

    pub(crate) fn unicast_locator_list_mut(&mut self) -> &mut Vec<Locator> {
        &mut self.unicast_locator_list
    }

    pub(crate) fn multicast_locator_list(&self) -> &[Locator] {
        &self.multicast_locator_list
    }

    pub(crate) fn multicast_locator_list_mut(&mut self) -> &mut Vec<Locator> {
        &mut self.multicast_locator_list
    }

    pub(crate) fn is_active(&self) -> bool {
        self.is_active
    }

    pub(crate) fn is_first_hb_sent(&self) -> bool {
        self.is_first_hb_sent
    }

    pub(crate) fn set_first_hb_sent(&mut self) {
        self.is_first_hb_sent = true;
    }

    pub(crate) fn set_highest_sent_change_sn(&mut self, highest_sent_change_sn: SequenceNumber) {
        if highest_sent_change_sn > self.highest_sent_change_sn {
            self.highest_sent_change_sn = highest_sent_change_sn;
        }
    }

    pub(crate) fn last_acknack_count(&self) -> Option<u32> {
        self.last_acknack_count
    }

    pub(crate) fn set_last_acknack_count(&mut self, last_acknack_count: u32) {
        self.last_acknack_count = Some(last_acknack_count);
    }

    pub(crate) fn last_acknack_at(&self) -> Option<Instant> {
        self.last_acknack_at
    }

    pub(crate) fn set_last_acknack_at(&mut self, at: Instant) {
        self.last_acknack_at = Some(at);
    }

    pub(crate) fn last_nackfrag_count(&self) -> Option<u32> {
        self.last_nackfrag_count
    }

    pub(crate) fn set_last_nackfrag_count(&mut self, last_nackfrag_count: u32) {
        self.last_nackfrag_count = Some(last_nackfrag_count);
    }

    pub(crate) fn last_nackfrag_at(&self) -> Option<Instant> {
        self.last_nackfrag_at
    }

    pub(crate) fn set_last_nackfrag_at(&mut self, at: Instant) {
        self.last_nackfrag_at = Some(at);
    }

    pub(crate) fn content_filter_signatures(&self) -> Option<&Vec<FilterSignature>> {
        self.content_filter_signatures.as_ref()
    }

    pub(crate) fn set_content_filter_signatures(
        &mut self,
        signatures: Option<Vec<FilterSignature>>,
    ) {
        self.content_filter_signatures = signatures;
    }

    pub(crate) fn last_irrelevant_sn(&self) -> SequenceNumber {
        self.last_irrelevant_sn
    }

    pub(crate) fn extend_last_irrelevant_sn(&mut self, sn: SequenceNumber) {
        if sn > self.last_irrelevant_sn {
            debug!("Extending last_irrelevant_sn from {} to {}", self.last_irrelevant_sn, sn);
            self.last_irrelevant_sn = sn;
        }
    }

    // Whether this change belongs to a coherent set whose first sequence number falls
    // inside this reader's GAP range, so the change must be GAPped instead of sent as DATA.
    pub(crate) fn is_change_in_gapped_coherent_set(&self, change: &CacheChange) -> bool {
        // Set member: carries its set's first sequence number inline.
        if let Some(set_start) = change.presentation_info().coherent_set {
            return set_start != SequenceNumber::UNKNOWN && set_start <= self.last_irrelevant_sn;
        }

        // Set end marker: it has no set start of its own. Its members were suppressed one by
        // one while advancing the irrelevant horizon, so the marker closes the GAPped run when
        // it directly follows that horizon.
        if change.is_coherent_end_marker() {
            return change.sequence_number() == self.last_irrelevant_sn + 1;
        }

        false
    }

    /// Generate ContentFilterInfo for this ReaderProxy
    /// Returns None if no content filter signatures are set
    pub(crate) fn generate_content_filter_info(
        &self,
    ) -> Option<crate::rtps::builtin::data::content_filtered_topic::ContentFilterInfo> {
        self.content_filter_signatures.as_ref().map(|signatures| {
            use crate::rtps::builtin::data::content_filtered_topic::ContentFilterInfo;

            let num_signatures = signatures.len();
            let num_bitmaps = num_signatures.div_ceil(32);
            let mut filter_result = vec![0i32; num_bitmaps];

            // Set all bits to 1 (all filters passed) - MSB first
            for idx in 0..num_signatures {
                let bitmap_idx = idx / 32;
                let bit_pos = idx % 32;
                filter_result[bitmap_idx] |= 1 << (31 - bit_pos);
            }

            ContentFilterInfo { filter_result, filter_signatures: signatures.clone() }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::instance_handle::InstanceHandle;
    use crate::rtps::common::entity_kind::EntityKind;
    use crate::rtps::common::types::ChangeKind;
    use crate::rtps::entities::history::cache_change::{CacheChange, PresentationInfo};

    fn create_reader_proxy(last_irrelevant_sn: i64) -> ReaderProxy {
        ReaderProxy::new(
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY)),
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::from_i64(last_irrelevant_sn),
        )
    }

    fn writer_guid() -> Guid {
        Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY))
    }

    // Coherent set member: a Data carrying its set's first sequence number as PID_COHERENT_SET.
    fn create_member(seq: i64, set_start: i64) -> CacheChange {
        let mut change = CacheChange::new(
            ChangeKind::Alive,
            writer_guid(),
            InstanceHandle::default(),
            SequenceNumber::from_i64(seq),
            vec![1],
            None,
        );
        change.set_presentation_info(PresentationInfo {
            coherent_set: Some(SequenceNumber::from_i64(set_start)),
            ..Default::default()
        });
        change
    }

    // Coherent set end marker: a payload-less Alive Data without a coherent set id.
    fn create_end_marker(seq: i64) -> CacheChange {
        CacheChange::new(
            ChangeKind::Alive,
            writer_guid(),
            InstanceHandle::default(),
            SequenceNumber::from_i64(seq),
            Vec::new(),
            None,
        )
    }

    #[test]
    fn member_with_set_start_at_or_below_horizon_is_gapped() {
        let change = create_member(5, 1);
        let proxy = create_reader_proxy(4);

        assert!(proxy.is_change_in_gapped_coherent_set(&change));
    }

    #[test]
    fn member_with_set_start_above_horizon_is_not_gapped() {
        let change = create_member(14, 14);
        let proxy = create_reader_proxy(13);

        assert!(!proxy.is_change_in_gapped_coherent_set(&change));
    }

    #[test]
    fn non_coherent_change_is_not_gapped() {
        let mut change = CacheChange::new(
            ChangeKind::Alive,
            writer_guid(),
            InstanceHandle::default(),
            SequenceNumber::from_i64(5),
            vec![1],
            None,
        );
        change.set_presentation_info(PresentationInfo::default());
        let proxy = create_reader_proxy(4);

        assert!(!proxy.is_change_in_gapped_coherent_set(&change));
    }

    #[test]
    fn end_marker_directly_following_horizon_is_gapped() {
        // Members 5..12 were suppressed and advanced the horizon to 12; the marker at 13
        // directly follows it and closes the GAPped run.
        let marker = create_end_marker(13);
        let proxy = create_reader_proxy(12);

        assert!(proxy.is_change_in_gapped_coherent_set(&marker));
    }

    #[test]
    fn end_marker_not_following_horizon_is_not_gapped() {
        // Relevant DATA was sent after the GAPped run, so the marker no longer directly
        // follows the horizon and must be delivered.
        let marker = create_end_marker(20);
        let proxy = create_reader_proxy(13);

        assert!(!proxy.is_change_in_gapped_coherent_set(&marker));
    }

    #[test]
    fn extend_last_irrelevant_sn_never_moves_backwards() {
        let mut proxy = create_reader_proxy(4);

        proxy.extend_last_irrelevant_sn(SequenceNumber::from_i64(7));
        assert_eq!(proxy.last_irrelevant_sn().to_i64(), 7);

        proxy.extend_last_irrelevant_sn(SequenceNumber::from_i64(5));
        assert_eq!(proxy.last_irrelevant_sn().to_i64(), 7);
    }
}
