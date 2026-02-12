use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::Arc;

use crate::{
    common::instance_handle::InstanceHandle,
    rtps::{
        builtin::builtin_endpoints::BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY,
        common::{
            entity_id::EntityId, guid::Guid, rtps_error_code::RtpsResult, sequence::SequenceNumber,
        },
        entities::history::{cache_change::CacheChange, history_cache::HistoryCache},
        task::sending_handler::{MessageType, SendingHandler},
    },
};

#[derive(Debug)]
pub struct WriterHistoryCache {
    participant_guid: Guid,
    owner_id: EntityId,
    changes: BTreeMap<SequenceNumber, Arc<CacheChange>>,
}

impl HistoryCache for WriterHistoryCache {
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        self.changes.remove(&a_change.sequence_number());

        if let Some(handler) =
            SendingHandler::get_instance_by_participant_guid(self.participant_guid)
        {
            handler.push_message_and_wake(MessageType::OnUserCacheChangeRemoval(
                true,
                a_change.sequence_number(),
                a_change.writer_guid().entity_id(),
            ));
        }

        Ok(())
    }

    fn is_builtin(&self) -> bool {
        self.owner_id.entity_kind().is_built_in()
    }

    fn get_changes(&self) -> Vec<Arc<CacheChange>> {
        self.changes.values().cloned().collect()
    }

    fn get_change_from_instance_handle(
        &self,
        instance_handle: InstanceHandle,
    ) -> Vec<Arc<CacheChange>> {
        self.changes
            .values()
            .filter(|change| change.instance_handle() == instance_handle)
            .cloned()
            .collect()
    }

    fn get_seq_num_min(&self) -> Option<SequenceNumber> {
        self.changes.keys().next().copied()
    }

    fn get_seq_num_max(&self) -> Option<SequenceNumber> {
        self.changes.keys().next_back().copied()
    }
}

impl WriterHistoryCache {
    pub(crate) fn new(participant_guid: Guid, owner_id: EntityId) -> Self {
        Self { participant_guid, owner_id, changes: BTreeMap::new() }
    }

    pub(crate) fn get_change(&self, seq_num: SequenceNumber) -> Option<Arc<CacheChange>> {
        self.changes.get(&seq_num).cloned()
    }

    pub(crate) fn add_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        let sn = a_change.sequence_number();

        if !self.is_builtin() {
            self.changes.insert(sn, a_change.clone());

            // Let RTPS writer know that there is a CacheChange that has not been sent
            if let Some(handler) =
                SendingHandler::get_instance_by_participant_guid(self.participant_guid)
            {
                let writer_entity_id = a_change.writer_guid().entity_id();
                handler.push_message_and_wake(MessageType::UserUnsentChanges(writer_entity_id));
            }
        } else {
            // Built-in endpoint not connected to DDS entity, Resource limits & History QoS not applied
            // Therefore arbitrarily limit size
            if self.changes.len() >= BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY {
                self.changes.pop_first();
            }

            self.changes.insert(sn, a_change.clone());
        }

        Ok(())
    }

    pub(crate) fn _is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// Returns the next change with a sequence number strictly greater than `sn`.
    /// Uses BTreeMap range query for O(log N) lookup.
    pub(crate) fn next_change_after(&self, sn: SequenceNumber) -> Option<Arc<CacheChange>> {
        self.changes
            .range((Bound::Excluded(sn), Bound::Unbounded))
            .next()
            .map(|(_, change)| change.clone())
    }
}
