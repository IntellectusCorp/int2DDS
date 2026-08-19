use std::ops::Deref;
use std::sync::Arc;

use dashmap::DashMap;

use crate::rtps::common::entity_id::EntityId;

use super::Reader;

// A reader leased for callback-producing work (delivery, discovery, liveliness). Holding it
// keeps the reader's in-flight-callback count raised, and dropping it lowers the count. Deletion
// drains that count to zero, so no callback outlives the delete call. The count is raised while
// the store's shard lock is held (see `get_reader_callback_lease`), serializing it against `remove`.
pub(crate) struct ReaderCallbackLease {
    reader: Arc<dyn Reader + Send + Sync>,
}

impl ReaderCallbackLease {
    fn new(reader: Arc<dyn Reader + Send + Sync>) -> Self {
        reader.enter_callback();
        Self { reader }
    }
}

impl Deref for ReaderCallbackLease {
    type Target = Arc<dyn Reader + Send + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.reader
    }
}

impl Drop for ReaderCallbackLease {
    fn drop(&mut self) {
        self.reader.exit_callback();
    }
}

// Dual-indexed reader storage for O(1) lookup by both EntityId and topic name.
// Reads far outnumber writes, so we maintain two maps to avoid DashMap iteration overhead
pub(crate) struct ReaderStore {
    by_id: DashMap<EntityId, Arc<dyn Reader + Send + Sync>>,
    by_topic: DashMap<String, Vec<Arc<dyn Reader + Send + Sync>>>, // SEDP matching requires topic-based lookup
}

impl ReaderStore {
    pub(crate) fn new() -> Self {
        Self { by_id: DashMap::new(), by_topic: DashMap::new() }
    }

    // Add a reader to the store, indexed by both EntityId and topic name
    pub(crate) fn add(&self, topic: &str, reader: Arc<dyn Reader + Send + Sync>) {
        let entity_id = reader.guid().entity_id();
        self.by_id.insert(entity_id, reader.clone());
        self.by_topic.entry(topic.to_string()).or_default().push(reader);
    }

    // Remove a reader from the store by EntityId and topic name, returning it if found
    pub(crate) fn remove(
        &self,
        topic: &str,
        entity_id: EntityId,
    ) -> Option<Arc<dyn Reader + Send + Sync>> {
        if let Some((_, reader)) = self.by_id.remove(&entity_id) {
            if let Some(mut readers) = self.by_topic.get_mut(topic) {
                readers.retain(|r| r.guid().entity_id() != entity_id);
            }
            Some(reader)
        } else {
            None
        }
    }

    // Get a reader by its EntityId
    pub(crate) fn get_reader(&self, entity_id: EntityId) -> Option<Arc<dyn Reader + Send + Sync>> {
        self.by_id.get(&entity_id).map(|r| r.clone())
    }

    // Lease a reader for callback-producing work, raising its in-flight count while the shard lock
    // is held. `remove` takes the same shard's write lock, so either this raises the count before
    // removal (deletion then drains it) or removal wins and this returns None (no callback runs).
    pub(crate) fn get_reader_callback_lease(
        &self,
        entity_id: EntityId,
    ) -> Option<ReaderCallbackLease> {
        self.by_id.get(&entity_id).map(|r| ReaderCallbackLease::new(r.value().clone()))
    }

    // Get all readers associated with a given topic name
    pub(crate) fn get_by_topic(&self, topic: &str) -> Vec<Arc<dyn Reader + Send + Sync>> {
        self.by_topic.get(topic).map(|readers| readers.clone()).unwrap_or_default()
    }

    // Iterate over all readers in the store
    pub(crate) fn iter_all(&self) -> Vec<Arc<dyn Reader + Send + Sync>> {
        self.by_id.iter().map(|r| r.value().clone()).collect()
    }

    // Iterate over all readers as callback leases, each raised under the shard lock (see
    // `get_reader_callback_lease`). Holding a lease keeps its reader out of a completing delete.
    pub(crate) fn iter_all_callback_leases(&self) -> Vec<ReaderCallbackLease> {
        self.by_id.iter().map(|r| ReaderCallbackLease::new(r.value().clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        infrastructure::qos_policy::ReliabilityQosPolicyKind,
        rtps::{
            common::{entity_id::EntityId, entity_kind::EntityKind, guid::Guid, types::TopicKind},
            entities::reader::StatefulReader,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    };

    fn stateful_reader(entity_id: EntityId) -> Arc<dyn Reader + Send + Sync> {
        let guid = Guid::new([7; 12], entity_id);
        let subscription_data = SubscriptionBuiltinTopicData::new(
            &DataReaderQos::default(),
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        Arc::new(StatefulReader::new(
            guid,
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            entity_id,
            false,
            None,
            None,
            subscription_data,
            guid,
        ))
    }

    #[test]
    fn a_callback_lease_raises_and_lowers_the_in_flight_count() {
        let entity_id = EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_READER_WITH_KEY);
        let store = ReaderStore::new();
        store.add("topic", stateful_reader(entity_id));

        let reader = store.get_reader(entity_id).expect("reader present");
        assert_eq!(reader.in_flight_callbacks(), 0);

        let first = store.get_reader_callback_lease(entity_id).expect("reader present");
        assert_eq!(reader.in_flight_callbacks(), 1);

        let second = store.get_reader_callback_lease(entity_id).expect("reader present");
        assert_eq!(reader.in_flight_callbacks(), 2, "concurrent leases must stack");

        drop(second);
        assert_eq!(reader.in_flight_callbacks(), 1);

        drop(first);
        assert_eq!(reader.in_flight_callbacks(), 0);
    }

    #[test]
    fn a_callback_lease_for_an_unknown_reader_is_none() {
        let store = ReaderStore::new();
        store.add(
            "topic",
            stateful_reader(EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_READER_WITH_KEY)),
        );

        let missing = EntityId::new([9, 9, 9], EntityKind::USER_DEFINED_READER_WITH_KEY);
        assert!(store.get_reader_callback_lease(missing).is_none());
    }
}
