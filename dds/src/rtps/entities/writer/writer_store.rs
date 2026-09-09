use std::ops::Deref;
use std::sync::Arc;

use dashmap::DashMap;

use crate::rtps::common::entity_id::EntityId;

use super::Writer;

// A writer leased for callback-producing work (discovery, liveliness). Holding it keeps the
// writer's in-flight-callback count raised, and dropping it lowers the count. Deletion drains
// that count to zero, so no callback outlives the delete call. The count is raised while the
// store's shard lock is held (see `get_writer_callback_lease`), serializing it against `remove`.
pub(crate) struct WriterCallbackLease {
    writer: Arc<dyn Writer + Send + Sync>,
}

impl WriterCallbackLease {
    fn new(writer: Arc<dyn Writer + Send + Sync>) -> Self {
        writer.enter_callback();
        Self { writer }
    }
}

impl Deref for WriterCallbackLease {
    type Target = Arc<dyn Writer + Send + Sync>;

    fn deref(&self) -> &Self::Target {
        &self.writer
    }
}

impl Drop for WriterCallbackLease {
    fn drop(&mut self) {
        self.writer.exit_callback();
    }
}

// Dual-indexed writer storage for O(1) lookup by both EntityId and topic name.
// Reads far outnumber writes, so we maintain two maps to avoid DashMap iteration overhead
pub(crate) struct WriterStore {
    by_id: DashMap<EntityId, Arc<dyn Writer + Send + Sync>>,
    by_topic: DashMap<String, Vec<Arc<dyn Writer + Send + Sync>>>, // SEDP matching requires topic-based lookup
}

impl WriterStore {
    pub(crate) fn new() -> Self {
        Self { by_id: DashMap::new(), by_topic: DashMap::new() }
    }

    // Add a writer to the store, indexed by both EntityId and topic name
    pub(crate) fn add(&self, topic: &str, writer: Arc<dyn Writer + Send + Sync>) {
        let entity_id = writer.guid().entity_id();
        self.by_id.insert(entity_id, writer.clone());
        self.by_topic.entry(topic.to_string()).or_default().push(writer);
    }

    // Remove a writer from the store by EntityId and topic name, returning it if found
    pub(crate) fn remove(
        &self,
        topic: &str,
        entity_id: EntityId,
    ) -> Option<Arc<dyn Writer + Send + Sync>> {
        if let Some((_, writer)) = self.by_id.remove(&entity_id) {
            if let Some(mut writers) = self.by_topic.get_mut(topic) {
                writers.retain(|w| w.guid().entity_id() != entity_id);
            }
            Some(writer)
        } else {
            None
        }
    }

    // Get a writer by its EntityId
    pub(crate) fn get(&self, entity_id: EntityId) -> Option<Arc<dyn Writer + Send + Sync>> {
        self.by_id.get(&entity_id).map(|w| w.clone())
    }

    // Lease a writer for callback-producing work, raising its in-flight count while the shard
    // lock is held. `remove` takes the same shard's write lock, so either this raises the count
    // before removal (deletion then drains it) or removal wins and this returns None.
    pub(crate) fn get_writer_callback_lease(
        &self,
        entity_id: EntityId,
    ) -> Option<WriterCallbackLease> {
        self.by_id.get(&entity_id).map(|w| WriterCallbackLease::new(w.value().clone()))
    }

    // Get all writers associated with a given topic name
    pub(crate) fn get_by_topic(&self, topic: &str) -> Vec<Arc<dyn Writer + Send + Sync>> {
        self.by_topic.get(topic).map(|writers| writers.clone()).unwrap_or_default()
    }

    // Iterate over all writers in the store
    pub(crate) fn iter_all(&self) -> Vec<Arc<dyn Writer + Send + Sync>> {
        self.by_id.iter().map(|w| w.value().clone()).collect()
    }

    // Snapshot of every writer's EntityId in the store
    pub(crate) fn all_entity_ids(&self) -> Vec<EntityId> {
        self.by_id.iter().map(|w| *w.key()).collect()
    }
}
