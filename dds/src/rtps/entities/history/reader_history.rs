use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, Weak},
};

use log::debug;

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

// A remote writer's open coherent set: identified by its first member's sequence
// number, members held back until the set end is observed.
#[derive(Debug)]
struct PendingCoherentSet {
    set_id: SequenceNumber,
    changes: Vec<CacheChange>,
}

#[derive(Debug)]
#[allow(clippy::type_complexity)]
pub struct ReaderHistoryCache {
    owner_id: EntityId,
    changes: Vec<Arc<CacheChange>>,
    // Builtin endpoints do not have associated DDS entity at the moment, optional for now.
    // Use dyn trait object to erase the type parameter.
    datareader_cache: Option<Weak<Mutex<dyn dcps_history_cache + Send + Sync>>>,
    pool: CacheChangePool,
    // Open coherent sets per remote writer, held back until each set completes.
    coherent_pending: HashMap<Guid, PendingCoherentSet>,
    // Max buffered members per open set (reader max_samples); oversized sets are discarded.
    coherent_pending_cap: usize,
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
            coherent_pending: HashMap::new(),
            coherent_pending_cap: usize::MAX,
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
                self.coherent_pending_cap = if depth > 0 { depth } else { usize::MAX };
            }
        }
    }

    /// Add CacheChange to ReaderHistoryCache.
    /// Mutates the change via DataReaderHistoryCache (instance handle, reception timestamp),
    /// then wraps in Arc once and shares between RTPS and DCPS histories. Zero deep copies.
    /// Returns every change made available by this call (the stored change and/or coherent
    /// members committed by a set close); a held or buffered sample yields no changes.
    pub(crate) fn add_change(
        &mut self,
        mut a_change: CacheChange,
        apply_filter: bool,
    ) -> RtpsResult<Vec<Arc<CacheChange>>> {
        let coherent_set = a_change.presentation_info().coherent_set;
        let writer_guid = a_change.writer_guid();

        // End marker: protocol metadata, not data. It closes the writer's open set and is
        // never stored, regardless of this reader's presentation QoS.
        if coherent_set == Some(SequenceNumber::UNKNOWN) {
            return self.close_coherent_set(writer_guid, Some(a_change.sequence_number()));
        }

        // Members committed by an implicit set close below ride along in the result.
        let mut available = Vec::new();

        // A TOPIC+coherent reader holds set members back until their set closes; any other
        // arrival from a writer with an open set ends that set implicitly.
        if self.topic_coherent_access() {
            match coherent_set {
                Some(set_id) => {
                    // A member of a different set closes the previous set first.
                    let committed = if self
                        .coherent_pending
                        .get(&writer_guid)
                        .is_some_and(|pending| pending.set_id != set_id)
                    {
                        self.close_coherent_set(writer_guid, None)?
                    } else {
                        Vec::new()
                    };

                    // Buffer the member until its set closes
                    self.buffer_coherent_member(writer_guid, set_id, a_change);
                    return Ok(committed);
                }
                None => {
                    // If no coherent set, any open set from this writer is implicitly closed
                    if self.coherent_pending.contains_key(&writer_guid) {
                        available = self.close_coherent_set(writer_guid, None)?;
                    }
                }
            }
        }

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
                let (removed, filtered) = datareader_cache
                    .add_change_with_cleanup(Arc::clone(&shared), apply_filter)
                    .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?;

                // Held by TIME_BASED_FILTER: not stored in either history, no notification.
                if filtered {
                    return Ok(available);
                }

                // Insert into RTPS ReaderHistoryCache (Arc::clone only)
                self.changes.push(Arc::clone(&shared));

                if let Some(removed_change) = removed {
                    self.remove_change(removed_change)?;
                }

                available.push(shared);
                return Ok(available);
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
            available.push(shared);
            Ok(available)
        } else {
            // Non-builtin endpoint must have DataReader cache
            Err(RtpsError::new(
                RtpsErrorCode::DataReaderCacheNotSet,
                "DataReader cache is not set for ReaderHistoryCache",
            ))
        }
    }

    // Buffer a coherent member until its set closes; an oversized set is discarded whole.
    fn buffer_coherent_member(
        &mut self,
        writer_guid: Guid,
        set_id: SequenceNumber,
        change: CacheChange,
    ) {
        let cap = self.coherent_pending_cap;
        let pending = self
            .coherent_pending
            .entry(writer_guid)
            .or_insert_with(|| PendingCoherentSet { set_id, changes: Vec::new() });
        pending.changes.push(change);
        if pending.changes.len() > cap {
            debug!(
                "Discarding coherent set {} from writer {}: exceeds max_samples {}",
                set_id.to_i64(),
                writer_guid,
                cap
            );
            self.coherent_pending.remove(&writer_guid);
        }
    }

    // Close a writer's open coherent set: commit members through the normal add path when
    // the set is contiguous from its start (and reaches the marker, when one is given),
    // else discard them all. Returns the stored members for notification.
    fn close_coherent_set(
        &mut self,
        writer_guid: Guid,
        marker_seq: Option<SequenceNumber>,
    ) -> RtpsResult<Vec<Arc<CacheChange>>> {
        // Removing closes the set; members re-added below cannot be buffered again.
        let Some(pending) = self.coherent_pending.remove(&writer_guid) else {
            return Ok(Vec::new());
        };

        // The set id is the first member's seq; a different head means the front was lost.
        let starts_at_set_id =
            pending.changes.first().is_some_and(|c| c.sequence_number() == pending.set_id);

        // Adjacent members must be exactly +1 apart: no holes in the middle.
        let contiguous = pending
            .changes
            .windows(2)
            .all(|pair| pair[1].sequence_number() == pair[0].sequence_number().next());

        // The marker consumes the seq right after the last member; a gap means tail loss.
        // An implicit end (no marker) cannot check the tail and passes.
        let ends_at_marker = marker_seq.is_none_or(|m| {
            pending.changes.last().is_some_and(|c| c.sequence_number().next() == m)
        });

        // Incomplete: behave as if none of the set was received.
        if !(starts_at_set_id && contiguous && ends_at_marker) {
            debug!(
                "Discarding incomplete coherent set {} from writer {} \
                 (starts_at_set_id={}, contiguous={}, ends_at_marker={})",
                pending.set_id.to_i64(),
                writer_guid,
                starts_at_set_id,
                contiguous,
                ends_at_marker
            );
            return Ok(Vec::new());
        }

        // Every stored member is returned so each one gets its own notification.
        let mut committed = Vec::new();

        for mut member in pending.changes {
            // Clear the set marker so this add is not consumed as a boundary again;
            // members also bypass TIME_BASED_FILTER so the set is stored atomically.
            let mut info = member.presentation_info().clone();
            info.coherent_set = None;
            member.set_presentation_info(info);
            committed.extend(self.add_change(member, false)?);
        }

        Ok(committed)
    }

    // True when the attached DCPS reader requests TOPIC-scope coherent access.
    fn topic_coherent_access(&self) -> bool {
        let Some(cache) = self.datareader_cache.as_ref().and_then(|weak| weak.upgrade()) else {
            return false;
        };
        cache.lock().map(|guard| guard.topic_coherent_access()).unwrap_or(false)
    }

    // Drop the writer's open coherent set (connectivity change: incomplete sets are discarded).
    pub(crate) fn discard_coherent_pending(&mut self, writer_guid: Guid) {
        self.coherent_pending.remove(&writer_guid);
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
