#![allow(dead_code)]
#![allow(unused_variables)]

use crate::{
    common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    rtps::{
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            locator::Locator,
            sequence::SequenceNumber,
        },
        entities::history::writer_history::WriterHistoryCache,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ReaderLocator {
    locator: Locator,
    highest_sent_change_sn: SequenceNumber, // Sequence number of the last CacheChange recorded as sent to Reader
    requested_changes: Vec<SequenceNumber>, // List of sequence numbers requested again by Reader through ACKNACK or NACK message
    expects_inline_qos: bool,
    guid_prefix: GuidPrefix,
    remote_entity_id: EntityId,
    is_active: bool, // Additional fields
    subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
}

use std::hash::{Hash, Hasher};

impl PartialEq for ReaderLocator {
    fn eq(&self, other: &Self) -> bool {
        self.guid_prefix == other.guid_prefix
            && self.remote_entity_id == other.remote_entity_id
            && self.locator == other.locator
    }
}

impl Eq for ReaderLocator {}

impl Hash for ReaderLocator {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.guid_prefix.hash(state);
        self.remote_entity_id.hash(state);
    }
}

impl ReaderLocator {
    pub(crate) fn new(
        locator: Locator,
        highest_sent_change_sn: Option<SequenceNumber>,
        expects_inline_qos: bool,
        guid_prefix: GuidPrefix,
        remote_entity_id: EntityId,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> Self {
        Self {
            locator,
            highest_sent_change_sn: highest_sent_change_sn.unwrap_or(SequenceNumber::UNKNOWN),
            requested_changes: Vec::new(),
            expects_inline_qos,
            guid_prefix,
            remote_entity_id,
            is_active: false,
            subscription_builtin_topic_data,
        }
    }

    pub(crate) fn remote_reader_guid(&self) -> Guid {
        Guid::new(self.guid_prefix(), self.remote_entity_id())
    }

    pub(crate) fn guid_prefix(&self) -> GuidPrefix {
        self.guid_prefix
    }

    pub(crate) fn locator(&self) -> Locator {
        self.locator.clone()
    }

    pub(crate) fn highest_sent_change_sn(&self) -> SequenceNumber {
        self.highest_sent_change_sn
    }

    pub(crate) fn set_highest_sent_change_sn(&mut self, highest_sent_change_sn: SequenceNumber) {
        self.highest_sent_change_sn = highest_sent_change_sn;
    }

    pub(crate) fn next_requested_change(&self) -> SequenceNumber {
        self.requested_changes.iter().min().cloned().unwrap_or(SequenceNumber::INIT)
    }

    pub(crate) fn next_unsent_change(&self, history_cache: &WriterHistoryCache) -> SequenceNumber {
        history_cache
            .next_change_after(self.highest_sent_change_sn)
            .map(|change| change.sequence_number())
            .unwrap_or(SequenceNumber::UNKNOWN)
    }

    pub(crate) fn requested_changes(&self) -> &[SequenceNumber] {
        &self.requested_changes
    }

    pub(crate) fn requested_changes_set(&mut self, req_seq_num_set: Vec<SequenceNumber>) {
        for seq_num in req_seq_num_set {
            self.requested_changes.push(seq_num);
        }
    }

    pub(crate) fn unsent_changes(&self, history_cache: &WriterHistoryCache) -> bool {
        self.next_unsent_change(history_cache) != SequenceNumber::UNKNOWN
    }

    pub(crate) fn remote_entity_id(&self) -> EntityId {
        self.remote_entity_id
    }

    pub(crate) fn subscription_builtin_topic_data(&self) -> SubscriptionBuiltinTopicData {
        self.subscription_builtin_topic_data.clone()
    }

    pub(crate) fn set_subscription_builtin_topic_data(
        &mut self,
        data: SubscriptionBuiltinTopicData,
    ) {
        self.subscription_builtin_topic_data = data;
    }
}
