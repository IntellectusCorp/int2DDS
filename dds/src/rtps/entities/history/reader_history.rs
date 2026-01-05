use std::{
    collections::HashSet,
    sync::{Arc, Mutex, Weak},
};

use crate::{
    common::instance_handle::InstanceHandle,
    infrastructure::history_cache::HistoryCache as dcps_history_cache,
    rtps::{
        builtin::builtin_endpoints::BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY,
        common::{
            entity_id::EntityId,
            guid::Guid,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
        },
        entities::history::{cache_change::CacheChange, history_cache::HistoryCache},
    },
    subscription::data_reader_history::ReaderChangeId,
};

#[derive(Debug)]
#[allow(clippy::type_complexity)]
pub struct ReaderHistoryCache {
    owner_id: EntityId,
    changes: Vec<Arc<CacheChange>>,
    datareader_cache: Option<
        Weak<
            Mutex<
                dyn dcps_history_cache<CacheChangeInputType = Arc<Mutex<CacheChange>>>
                    + Send
                    + Sync,
            >,
        >,
    >, // Builtin endpoints do not have associated DDS entity at the moment, optional for now.
}

impl HistoryCache for ReaderHistoryCache {
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        self.changes.retain(|change| {
            (change.writer_guid() != a_change.writer_guid())
                || (change.sequence_number() != a_change.sequence_number())
        });
        Ok(())
    }

    fn is_builtin(&self) -> bool {
        self.owner_id.entity_kind().is_built_in()
    }

    fn get_changes(&self) -> Vec<Arc<CacheChange>> {
        self.changes.clone()
    }

    fn get_change_from_instance_handle(
        &self,
        instance_handle: InstanceHandle,
    ) -> Vec<Arc<CacheChange>> {
        self.changes
            .iter()
            .filter(|change| change.instance_handle() == instance_handle)
            .cloned()
            .collect()
    }
}

#[allow(dead_code)]
#[allow(clippy::type_complexity)]
impl ReaderHistoryCache {
    pub(crate) fn new(
        owner_id: EntityId,
        datareader_cache: Option<
            Arc<
                Mutex<
                    dyn dcps_history_cache<CacheChangeInputType = Arc<Mutex<CacheChange>>>
                        + Send
                        + Sync,
                >,
            >,
        >,
    ) -> Self {
        Self {
            owner_id,
            changes: Vec::new(),
            datareader_cache: datareader_cache.map(|arc| Arc::downgrade(&arc)),
        }
    }

    pub(crate) fn get_change(
        &self,
        seq_num: SequenceNumber,
        writer_guid: Guid,
    ) -> Option<Arc<CacheChange>> {
        let change = self.changes.iter().find(|change| {
            change.sequence_number() == seq_num && change.writer_guid() == writer_guid
        });
        change.cloned()
    }

    pub(crate) fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<
            Mutex<
                dyn dcps_history_cache<CacheChangeInputType = Arc<Mutex<CacheChange>>>
                    + Send
                    + Sync,
            >,
        >,
    ) {
        self.datareader_cache = Some(datareader_cache);
    }

    /// Add CacheChange to ReaderHistoryCache.
    /// This takes mutex-wrapped CacheChange as input to set instance handle before making it immutable on the DataReader's side.
    pub(crate) fn add_change(&mut self, a_change: CacheChange) -> RtpsResult<Arc<CacheChange>> {
        if let Some(datareader_cache_weak) = &self.datareader_cache {
            if let Some(datareader_cache_arc) = datareader_cache_weak.upgrade() {
                // This can be modified by DataReader so wrap with mutex first.
                let mutex_wrapped = Arc::new(Mutex::new(a_change));

                let removed_cache: Option<Arc<CacheChange>> = match datareader_cache_arc.lock() {
                    Ok(mut datareader_cache) => {
                        // Add change to DataReaderHistoryCache
                        // DDS DataReaderHistoryCache may evict old changes according to History & Resource Limits QoS
                        datareader_cache
                            .add_change_with_cleanup(mutex_wrapped.clone())
                            .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?
                    }
                    Err(_) => {
                        return Err(RtpsError::new(
                            RtpsErrorCode::LockError,
                            "Failed to acquire DataReader cache lock",
                        ));
                    }
                };

                // If DataReaderHistoryCache evicted a CacheChange, remove it from RTPS ReaderHistoryCache as well
                if let Some(removed_change) = removed_cache {
                    self.remove_change(removed_change)?;
                }

                let change_guard = mutex_wrapped.lock().map_err(|_| {
                    RtpsError::new(RtpsErrorCode::LockError, "Failed to lock CacheChange")
                })?;
                let immutable_change = Arc::new(change_guard.clone());

                self.changes.push(immutable_change.clone());
                return Ok(immutable_change);
            }
        }

        // No DataReader cache connected (builtin endpoint without DDS entity or weak reference expired)
        if self.is_builtin() {
            // Built-in endpoint: Resource limits & History QoS not applied
            // Therefore arbitrarily limit size
            if self.changes.len() >= BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY {
                self.changes.remove(0);
            }

            let immutable_change = Arc::new(a_change);
            self.changes.push(immutable_change.clone());
            Ok(immutable_change)
        } else {
            // Non-builtin endpoint must have DataReader cache
            Err(RtpsError::new(
                RtpsErrorCode::DataReaderCacheNotSet,
                "DataReader cache is not set for ReaderHistoryCache",
            ))
        }
    }

    /// Remove changes by the given change IDs.
    pub(crate) fn remove_change_by_id_set(
        &mut self,
        change_id_set: HashSet<ReaderChangeId>,
    ) -> RtpsResult<()> {
        self.changes.retain(|change| {
            !change_id_set.contains(&(change.writer_guid(), change.sequence_number()))
        });
        Ok(())
    }
}
