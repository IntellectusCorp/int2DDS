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
            cache_change::CacheChange,
            cache_change_pool::{CacheChangePool, MAX_POOL_CAP},
            history_cache::HistoryCache,
        },
    },
    subscription::data_reader_history::ReaderChangeId,
};

// A remote writer's open coherent set: identified by its first member's sequence
// number, members held back until the set end is observed.
#[derive(Debug)]
struct PendingCoherentSet {
    set_start_sn: SequenceNumber,
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
        min_change.map(|change| change.sequence_number())
    }

    fn get_seq_num_max(&self) -> Option<SequenceNumber> {
        let max_change = self.changes.iter().max_by_key(|change| change.sequence_number());
        max_change.map(|change| change.sequence_number())
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

    // Number of members buffered for a remote writer's currently open coherent set.
    #[cfg(test)]
    pub(crate) fn pending_coherent_len(&self, writer_guid: Guid) -> usize {
        self.coherent_pending.get(&writer_guid).map_or(0, |set| set.changes.len())
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
        a_change: CacheChange,
        apply_filter: bool,
    ) -> RtpsResult<Vec<Arc<CacheChange>>> {
        let changes = self.prepare_changes_to_commit(a_change, apply_filter);

        self.commit_changes_to_datareader_cache(changes)
    }

    // Decides what one arrival lets through right now: the sample itself, the members of a
    // coherent set it closes, or nothing while its set is still open. Stores nothing.
    pub(crate) fn prepare_changes_to_commit(
        &mut self,
        a_change: CacheChange,
        apply_filter: bool,
    ) -> Vec<(CacheChange, bool)> {
        let coherent_set = a_change.presentation_info().coherent_set;
        let writer_guid = a_change.writer_guid();

        // End marker: a payload-less Data closes the writer's open set and is never stored,
        // regardless of this reader's presentation QoS. Accepts both an explicit
        // PID_COHERENT_SET=UNKNOWN and a payload-less Data carrying no coherent set id.
        if a_change.is_coherent_end_marker() {
            let members = self.close_and_take_coherent_set(writer_guid, a_change.sequence_number());
            return members.into_iter().map(|c| (c, false)).collect();
        }

        // A coherent reader holds set members back until their set closes; any other
        // arrival from a writer with an open set ends that set implicitly.
        if self.is_coherent_access() {
            match coherent_set {
                Some(set_id) => {
                    // A member of a different set closes the previous set first.
                    let members = if self
                        .coherent_pending
                        .get(&writer_guid)
                        .is_some_and(|pending| pending.set_start_sn != set_id)
                    {
                        // This member sits one past the previous set's (possibly lost) end marker.
                        self.close_and_take_coherent_set(
                            writer_guid,
                            a_change.sequence_number().previous(),
                        )
                    } else {
                        Vec::new()
                    };

                    // Hold this member back until its own set closes.
                    self.buffer_coherent_member(writer_guid, set_id, a_change);

                    return members.into_iter().map(|c| (c, false)).collect();
                }
                None => {
                    // A non-coherent sample closes this writer's open set (if any); commit that
                    // set and this sample together so a take() sees all of it or none.
                    let members = if self.coherent_pending.contains_key(&writer_guid) {
                        // This sample sits one past the open set's (possibly lost) end marker.
                        self.close_and_take_coherent_set(
                            writer_guid,
                            a_change.sequence_number().previous(),
                        )
                    } else {
                        Vec::new()
                    };

                    let mut batch: Vec<(CacheChange, bool)> =
                        members.into_iter().map(|c| (c, false)).collect();
                    batch.push((a_change, apply_filter));
                    return batch;
                }
            }
        }

        vec![(a_change, apply_filter)]
    }

    // Insert a batch of changes into the DCPS and RTPS histories under a single DataReader
    // cache lock, so a concurrent take() never observes a partially-committed coherent set.
    // Each entry carries its own apply_filter; returns the changes actually stored.
    pub(crate) fn commit_changes_to_datareader_cache(
        &mut self,
        changes: Vec<(CacheChange, bool)>,
    ) -> RtpsResult<Vec<Arc<CacheChange>>> {
        let mut available = Vec::new();
        if changes.is_empty() {
            return Ok(available);
        }

        if let Some(datareader_cache_weak) = &self.datareader_cache {
            if let Some(datareader_cache_arc) = datareader_cache_weak.upgrade() {
                let mut datareader_cache = datareader_cache_arc.lock().map_err(|_| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        "Failed to acquire DataReader cache lock",
                    )
                })?;

                // A coherent set that cannot be stored in full is dropped whole, never partially:
                // a member lost to History/ResourceLimits would make the set incomplete.
                // len > 1 limits this to real sets; a lone sample keeps the normal evict/reject path.
                if datareader_cache.is_coherent_access() && changes.len() > 1 {
                    let mut len_per_instance: HashMap<InstanceHandle, usize> = HashMap::new();
                    for (change, _) in &changes {
                        *len_per_instance.entry(change.instance_handle()).or_insert(0) += 1;
                    }
                    let fits = datareader_cache
                        .ensure_capacity_dry(&len_per_instance)
                        .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?;
                    if !fits {
                        return Ok(available);
                    }
                }

                for (mut change, apply_filter) in changes {
                    // Mutate the change before making it immutable
                    datareader_cache
                        .add_info_to_cache_change(&mut change)
                        .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?;

                    // Wrap in Arc once — no deep copy
                    let shared = Arc::new(change);

                    // Insert into DataReaderHistoryCache (Arc::clone only)
                    let (removed, filtered) = datareader_cache
                        .add_change_with_cleanup(Arc::clone(&shared), apply_filter)
                        .map_err(|e| RtpsError::new(RtpsErrorCode::DdsError, e.to_string()))?;

                    // Held by TIME_BASED_FILTER: not stored in either history, no notification.
                    if filtered {
                        continue;
                    }

                    // Insert into RTPS ReaderHistoryCache (Arc::clone only)
                    self.changes.push(Arc::clone(&shared));

                    if let Some(removed_change) = removed {
                        self.remove_change(removed_change)?;
                    }

                    available.push(shared);
                }

                return Ok(available);
            }
        }

        // No DataReader cache connected (builtin endpoint without DDS entity or expired weak ref)
        if self.is_builtin() {
            for (change, _apply_filter) in changes {
                // Built-in endpoint: Resource limits & History QoS not applied,
                // therefore arbitrarily limit size.
                if self.changes.len() >= BUILTIN_ENDPOINT_HISTORYCACHE_CAPACITY {
                    let removed = self.changes.remove(0);
                    self.pool.try_release(removed);
                }

                let shared = Arc::new(change);
                self.changes.push(Arc::clone(&shared));
                available.push(shared);
            }

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
            .or_insert_with(|| PendingCoherentSet { set_start_sn: set_id, changes: Vec::new() });
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

    // Close a writer's open coherent set and return its members ready for commit (coherent
    // id cleared), or an empty Vec when the set is incomplete or none is open. The caller
    // stores the returned members under one lock so the whole set is committed atomically.
    fn close_and_take_coherent_set(
        &mut self,
        writer_guid: Guid,
        end_sn: SequenceNumber,
    ) -> Vec<CacheChange> {
        // Removing closes the set; its members will not be buffered again.
        let Some(pending) = self.coherent_pending.remove(&writer_guid) else {
            return Vec::new();
        };

        // A complete set holds exactly one member per seq in [set_start_sn, end_sn), in order.
        // Any missing or misplaced seq (front, middle, or tail loss) makes it incomplete.
        let start = pending.set_start_sn.to_i64();
        for (idx, sn) in (start..end_sn.to_i64()).enumerate() {
            if pending.changes.get(idx).map(|m| m.sequence_number().to_i64()) != Some(sn) {
                debug!("Discarding incomplete coherent set {} from writer {} since sequence number {} is missing", start, writer_guid, sn);
                return Vec::new();
            }
        }

        // Clear each member's coherent id so re-insertion is not taken as a set boundary.
        pending
            .changes
            .into_iter()
            .map(|mut member| {
                let mut info = member.presentation_info().clone();
                info.coherent_set = None;
                member.set_presentation_info(info);
                member
            })
            .collect()
    }

    // True when the attached DCPS reader requests coherent access (INSTANCE or TOPIC scope).
    fn is_coherent_access(&self) -> bool {
        let Some(cache) = self.datareader_cache.as_ref().and_then(|weak| weak.upgrade()) else {
            return false;
        };
        cache.lock().map(|guard| guard.is_coherent_access()).unwrap_or(false)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::{entity_kind::EntityKind, types::ChangeKind};

    fn writer_guid() -> Guid {
        Guid::new([1; 12], EntityId::new([0, 0, 2], EntityKind::USER_DEFINED_WRITER_WITH_KEY))
    }

    // A coherent member with the given seq; non-empty payload so it is not read as an end marker.
    fn coherent_member(seq: i64) -> CacheChange {
        CacheChange::new(
            ChangeKind::Alive,
            writer_guid(),
            InstanceHandle::default(),
            SequenceNumber::from_i64(seq),
            vec![0u8; 4],
            None,
        )
    }

    fn reader_cache() -> ReaderHistoryCache {
        let owner = EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY);
        ReaderHistoryCache::new(owner, None)
    }

    // The judgement and the storing are separate steps, so a caller may hold the change back
    // between them.
    #[test]
    fn prepare_hands_back_a_plain_sample_without_storing_it() {
        let mut cache = reader_cache();

        let prepared = cache.prepare_changes_to_commit(coherent_member(1), true);

        assert_eq!(prepared.len(), 1);
        assert!(prepared[0].1);
        assert!(cache.changes.is_empty());
    }

    #[test]
    fn implicit_close_discards_set_with_lost_tail() {
        let mut cache = reader_cache();
        let guid = writer_guid();
        let set_id = SequenceNumber::from_i64(10);

        // Set A members 10, 11 buffered; member 12 and end marker 13 are lost.
        cache.buffer_coherent_member(guid, set_id, coherent_member(10));
        cache.buffer_coherent_member(guid, set_id, coherent_member(11));

        // Set B's first member at seq 14 implicitly closes A: end_sn = 14 - 1 = 13.
        let committed =
            cache.close_and_take_coherent_set(guid, SequenceNumber::from_i64(14).previous());

        assert!(committed.is_empty(), "set with a lost tail must be discarded on implicit close");
    }

    #[test]
    fn implicit_close_commits_complete_set_when_only_marker_lost() {
        let mut cache = reader_cache();
        let guid = writer_guid();
        let set_id = SequenceNumber::from_i64(10);

        // Set A members 10, 11, 12 all present; only end marker 13 is lost.
        for seq in [10, 11, 12] {
            cache.buffer_coherent_member(guid, set_id, coherent_member(seq));
        }

        // Set B's first member at seq 14 implicitly closes A: end_sn = 13, last(12).next() == 13.
        let committed =
            cache.close_and_take_coherent_set(guid, SequenceNumber::from_i64(14).previous());

        let seqs: Vec<i64> = committed.iter().map(|c| c.sequence_number().to_i64()).collect();
        assert_eq!(
            seqs,
            vec![10, 11, 12],
            "complete set must commit even if only the marker was lost"
        );
    }
}
