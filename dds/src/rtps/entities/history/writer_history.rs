use std::collections::BTreeMap;
use std::ops::Bound;
use std::sync::{Arc, Weak};

use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode};
use crate::{
    common::instance_handle::InstanceHandle,
    rtps::{
        builtin::builtin_endpoints::BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY,
        common::{entity_id::EntityId, rtps_error_code::RtpsResult, sequence::SequenceNumber},
        entities::entity::Entity,
        entities::history::{cache_change::CacheChange, history_cache::HistoryCache},
        entities::participant::Participant,
        entities::writer::Writer,
        task::sending_handler::{MessageType, SendingHandler},
    },
};

#[derive(Debug)]
pub struct WriterHistoryCache {
    participant: Weak<Participant>,
    owner_id: EntityId,
    changes: BTreeMap<SequenceNumber, Arc<CacheChange>>,
    highest_sn: SequenceNumber, // Highest sequence number ever added to this cache
}

impl HistoryCache for WriterHistoryCache {
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        self.changes.remove(&a_change.sequence_number());

        if let Some(participant) = self.participant.upgrade() {
            if let Some(handler) =
                SendingHandler::get_instance_by_participant_guid(participant.guid())
            {
                handler.push_message_and_wake(MessageType::OnUserCacheChangeRemoval(
                    true,
                    a_change.sequence_number(),
                    a_change.writer_guid().entity_id(),
                ));
            }
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
    pub(crate) fn new(participant: Weak<Participant>, owner_id: EntityId) -> Self {
        Self {
            participant,
            owner_id,
            changes: BTreeMap::new(),
            highest_sn: SequenceNumber::new(0, 0),
        }
    }

    pub(crate) fn get_change(&self, seq_num: SequenceNumber) -> Option<Arc<CacheChange>> {
        self.changes.get(&seq_num).cloned()
    }

    pub(crate) fn add_change(
        &mut self,
        a_change: Arc<CacheChange>,
        writer: &(dyn Writer + Send + Sync),
    ) -> RtpsResult<()> {
        if self.is_builtin() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidEntityKind,
                "add_change must not be called on a built-in writer history",
            ));
        }

        let sn = a_change.sequence_number();
        self.changes.insert(sn, a_change.clone());

        if sn > self.highest_sn {
            self.highest_sn = sn;
        }

        if let Some(participant) = self.participant.upgrade() {
            let (_, _, user_logic_arc) = participant.get_logics();
            if let Some(user_logic) = user_logic_arc.as_ref() {
                user_logic.send_unsent_changes(writer, self)?
            }
        }

        Ok(())
    }

    pub(crate) fn add_change_builtin(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        if !self.is_builtin() {
            return Err(RtpsError::new(
                RtpsErrorCode::InvalidEntityKind,
                "add_change_builtin must not be called on a non-built-in writer history",
            ));
        }

        let sn = a_change.sequence_number();

        // Built-in endpoint not connected to DDS entity, Resource limits & History QoS not applied
        // Therefore arbitrarily limit size
        if self.changes.len() >= BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY {
            self.changes.pop_first();
        }

        self.changes.insert(sn, a_change.clone());

        if sn > self.highest_sn {
            self.highest_sn = sn;
        }

        Ok(())
    }

    pub(crate) fn highest_sn(&self) -> SequenceNumber {
        self.highest_sn
    }

    pub(crate) fn is_empty(&self) -> bool {
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
