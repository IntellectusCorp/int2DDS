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
}
