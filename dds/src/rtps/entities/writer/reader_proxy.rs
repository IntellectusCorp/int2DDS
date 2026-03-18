#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    rtps::{
        builtin::data::content_filtered_topic::FilterSignature,
        common::{entity_id::EntityId, guid::Guid, locator::Locator, sequence::SequenceNumber},
        entities::history::{history_cache::HistoryCache, writer_history::WriterHistoryCache},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ReaderProxy {
    remote_reader_guid: Guid,
    remote_group_entity_id: EntityId,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    requested_changes: Vec<SequenceNumber>, // List of sequence numbers requested again by Reader through ACKNACK or NACK message
    highest_sent_change_sn: SequenceNumber, // Sequence number of the last CacheChange recorded as sent to Reader
    max_acked_sn: SequenceNumber,
    expects_inline_qos: bool, // false
    is_active: bool,
    last_acknack_count: Option<u32>,
    last_nackfrag_count: Option<u32>,
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
            max_acked_sn,
            expects_inline_qos,
            is_active,
            last_acknack_count: None,
            last_nackfrag_count: None,
            content_filter_signatures: None,
            subscription_builtin_topic_data,
            last_irrelevant_sn,
            is_first_hb_sent: false,
        }
    }

    pub(crate) fn subscription_builtin_topic_data(&self) -> SubscriptionBuiltinTopicData {
        self.subscription_builtin_topic_data.clone()
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

    pub(crate) fn requested_changes(&self) -> Vec<SequenceNumber> {
        self.requested_changes.clone()
    }

    pub(crate) fn requested_changes_set(&mut self, req_seq_num_set: Vec<SequenceNumber>) {
        self.requested_changes = Vec::new();
        for seq_num in req_seq_num_set {
            self.requested_changes.push(seq_num);
        }
    }

    pub(crate) fn remove_cached_sn_on_cache_change_removal(&mut self, deleted_sn: SequenceNumber) {
        self.requested_changes.retain(|sn| *sn != deleted_sn);
    }

    pub(crate) fn empty_requested_changes(&mut self) {
        self.requested_changes = Vec::new();
    }

    pub(crate) fn unsent_changes(&self, history_cache: &WriterHistoryCache) -> bool {
        self.next_unsent_change(history_cache) != SequenceNumber::UNKNOWN
    }

    pub(crate) fn unacked_changes(&self, history_cache: &WriterHistoryCache) -> bool {
        history_cache.get_seq_num_max().map_or(false, |max_sn| max_sn > self.max_acked_sn)
    }

    pub(crate) fn unicast_locator_list(&self) -> Vec<Locator> {
        self.unicast_locator_list.clone()
    }

    pub(crate) fn unicast_locator_list_mut(&mut self) -> &mut Vec<Locator> {
        &mut self.unicast_locator_list
    }

    pub(crate) fn multicast_locator_list(&self) -> Vec<Locator> {
        self.multicast_locator_list.clone()
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

    pub(crate) fn last_nackfrag_count(&self) -> Option<u32> {
        self.last_nackfrag_count
    }

    pub(crate) fn set_last_nackfrag_count(&mut self, last_nackfrag_count: u32) {
        self.last_nackfrag_count = Some(last_nackfrag_count);
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
