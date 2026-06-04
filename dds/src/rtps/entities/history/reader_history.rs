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
        entities::history::{
            cache_change::CacheChange, cache_change_pool::CacheChangePool,
            history_cache::HistoryCache,
        },
    },
    subscription::data_reader_history::ReaderChangeId,
};

// Upper bound on pooled idle changes when history is effectively unbounded
// (KEEP_ALL with unlimited resource limits reports max_samples as i32::MAX).
const MAX_POOL_CAP: usize = 1024;

#[derive(Debug)]
#[allow(clippy::type_complexity)]
pub struct ReaderHistoryCache {
    owner_id: EntityId,
    changes: Vec<Arc<CacheChange>>,
    // Builtin endpoints do not have associated DDS entity at the moment, optional for now.
    // Use dyn trait object to erase the type parameter.
    datareader_cache: Option<Weak<Mutex<dyn dcps_history_cache + Send + Sync>>>,
    pool: CacheChangePool,
}

impl HistoryCache for ReaderHistoryCache {
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> RtpsResult<()> {
        // Locate by identity, then remove(pos) so ownership of the Arc comes back
        // to us (retain would drop silently and lose the pool-return chance).
        let removed = self
            .changes
            .iter()
            .position(|c| {
                c.writer_guid() == a_change.writer_guid()
                    && c.sequence_number() == a_change.sequence_number()
            })
            .map(|pos| self.changes.remove(pos));
        // Release caller's Arc first so the vec's Arc can try_unwrap on its own.
        self.pool.try_release(a_change);
        if let Some(change) = removed {
            self.pool.try_release(change);
        }
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

    fn get_seq_num_min(&self) -> Option<SequenceNumber> {
        let min_change = self.changes.iter().min_by_key(|change| change.sequence_number());
        match min_change {
            Some(change) => Some(change.sequence_number()),
            None => None,
        }
    }

    fn get_seq_num_max(&self) -> Option<SequenceNumber> {
        let max_change = self.changes.iter().max_by_key(|change| change.sequence_number());
        match max_change {
            Some(change) => Some(change.sequence_number()),
            None => None,
        }
    }
}

#[allow(dead_code)]
#[allow(clippy::type_complexity)]
impl ReaderHistoryCache {
    pub(crate) fn new(
        owner_id: EntityId,
        datareader_cache: Option<Arc<Mutex<dyn dcps_history_cache + Send + Sync>>>,
    ) -> Self {
        Self {
            owner_id,
            changes: Vec::new(),
            datareader_cache: datareader_cache.map(|arc| Arc::downgrade(&arc)),
            // Pool starts empty; fills up as evictions return changes through try_release.
            // Steady-state size converges to history depth.
            pool: CacheChangePool::with_capacity(0),
        }
    }

    pub(crate) fn acquire_change(&mut self) -> CacheChange {
        self.pool.acquire()
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
        datareader_cache: Weak<Mutex<dyn dcps_history_cache + Send + Sync>>,
    ) {
        self.datareader_cache = Some(datareader_cache);
        // Size the pool to the history's steady-state working set so released
        // changes are retained; with cap 0 the pool drops everything.
        if let Some(cache) = self.datareader_cache.as_ref().and_then(|w| w.upgrade()) {
            if let Ok(guard) = cache.lock() {
                let depth = guard.get_max_samples().max(0) as usize;
                self.pool.set_cap(depth.min(MAX_POOL_CAP));
            }
        }
    }

    /// Add CacheChange to ReaderHistoryCache.
    /// Mutates the change via DataReaderHistoryCache (instance handle, reception timestamp),
    /// then wraps in Arc once and shares between RTPS and DCPS histories. Zero deep copies.
    pub(crate) fn add_change(&mut self, mut a_change: CacheChange) -> RtpsResult<Arc<CacheChange>> {
        if let Some(datareader_cache_weak) = &self.datareader_cache {
            if let Some(datareader_cache_arc) = datareader_cache_weak.upgrade() {
                let mut datareader_cache = datareader_cache_arc.lock().map_err(|_| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        "Failed to acquire DataReader cache lock",
                    )
                })?;

                // Mutate the change before making it immutable
                datareader_cache
                    .add_info_to_cache_change(&mut a_change)
                    .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?;

                // Wrap in Arc once — no deep copy
                let shared = Arc::new(a_change);

                // Insert into DataReaderHistoryCache (Arc::clone only)
                let removed = datareader_cache
                    .add_change_with_cleanup(Arc::clone(&shared))
                    .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?;

                // Insert into RTPS ReaderHistoryCache (Arc::clone only)
                self.changes.push(Arc::clone(&shared));

                if let Some(removed_change) = removed {
                    self.remove_change(removed_change)?;
                }

                return Ok(shared);
            }
        }

        // No DataReader cache connected (builtin endpoint without DDS entity or weak reference expired)
        if self.is_builtin() {
            // Built-in endpoint: Resource limits & History QoS not applied
            // Therefore arbitrarily limit size
            if self.changes.len() >= BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY {
                let removed = self.changes.remove(0);
                self.pool.try_release(removed);
            }

            let shared = Arc::new(a_change);
            self.changes.push(Arc::clone(&shared));
            Ok(shared)
        } else {
            // Non-builtin endpoint must have DataReader cache
            Err(RtpsError::new(
                RtpsErrorCode::DataReaderCacheNotSet,
                "DataReader cache is not set for ReaderHistoryCache",
            ))
        }
    }

    // Remove changes by the given change IDs.
    pub(crate) fn remove_change_by_id_set(
        &mut self,
        change_id_set: HashSet<ReaderChangeId>,
    ) -> RtpsResult<()> {
        let mut i = 0;
        while i < self.changes.len() {
            let id = (self.changes[i].writer_guid(), self.changes[i].sequence_number());
            if change_id_set.contains(&id) {
                let removed = self.changes.remove(i);
                self.pool.try_release(removed);
            } else {
                i += 1;
            }
        }
        Ok(())
    }
}
