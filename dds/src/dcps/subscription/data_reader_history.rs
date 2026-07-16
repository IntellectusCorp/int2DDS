//! DataReader history cache implementation.
//!
//! This module implements the history cache for `DataReader`, managing the storage and
//! retrieval of received samples according to History, ResourceLimits, and Ownership QoS policies.
//!
//! The reader history cache handles:
//! - Per-instance sample storage with view/instance state tracking
//! - KEEP_LAST / KEEP_ALL history policies
//! - Resource limit enforcement (max_samples, max_instances, max_samples_per_instance)
//! - Sample state management (READ/NOT_READ, NEW/NOT_NEW, ALIVE/DISPOSED/NO_WRITERS)
//! - Ownership strength arbitration for exclusive ownership
//! - TimeBasedFilter enforcement

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, Mutex, Weak},
};

use dashmap::DashMap;
use log::{debug, error};

use crate::{
    common::instance_handle::InstanceHandle,
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
        types::LENGTH_UNLIMITED,
    },
    infrastructure::{
        history_cache::HistoryCache,
        qos_policy::{
            DestinationOrderQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
            OwnershipQosPolicyKind, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
            ResourceLimitsQosPolicy,
        },
        status::{SampleRejectedStatus, SampleRejectedStatusKind, StatusInfo, StatusKind},
    },
    rtps::{
        common::{guid::Guid, sequence::SequenceNumber, time::RtpsTime, types::ChangeKind},
        entities::history::cache_change::CacheChange,
    },
    subscription::{
        data_reader::DataReader,
        sample_info::InstanceStateKind,
        time_based_filter::{FilterOutcome, TimeBasedFilter},
    },
    topic::type_support::DdsType,
    utils::timer::timer_id::TimerId,
};

pub(crate) type ReaderChangeId = (Guid, SequenceNumber);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OwnershipInfo {
    ownership_strength: i32,
    owner_guid: Guid,
}

impl PartialOrd for OwnershipInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OwnershipInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .ownership_strength
            .cmp(&self.ownership_strength) // ownership_strength in desc
            .then_with(|| self.owner_guid.cmp(&other.owner_guid)) // owner_guid in asc
    }
}

// #[derive(Debug)]
pub(crate) struct DataReaderHistoryCache<Foo> {
    data_reader: Weak<DataReader<Foo>>,
    max_samples: i32,
    max_instances: i32,
    max_samples_per_instance: i32,
    // Instance-organized primary store; BTreeMap key order gives deterministic instance blocks,
    // each bucket kept in DESTINATION_ORDER. NoKey reader uses the NIL bucket.
    instance_map: Arc<Mutex<BTreeMap<InstanceHandle, Vec<Arc<CacheChange>>>>>,
    destination_order_kind: DestinationOrderQosPolicyKind,
    can_auto_remove: bool, // auto remove oldest changes when full
    ownership_kind: OwnershipQosPolicyKind,
    owner_candidates: Arc<DashMap<InstanceHandle, BTreeSet<OwnershipInfo>>>, // Track valid writers per instance (includes writers that missed deadline or unregistered, not just strictly alive ones by Liveliness QoS)
    lifespan_timers: Arc<Mutex<HashMap<Guid, TimerId>>>, // writer_guid -> timer_id
    #[allow(clippy::type_complexity)]
    status_callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
    // Reader-side TIME_BASED_FILTER state. Always present; min_separation is read live per
    // sample so a runtime QoS change takes effect immediately.
    time_based_filter: TimeBasedFilter,
    // Receive-side ContentFilteredTopic hook (type-erased so the trait impl can call it).
    content_filter: Option<Arc<dyn Fn(&CacheChange) -> bool + Send + Sync>>,
}

// DESTINATION_ORDER comparison key: reception or source timestamp, then sequence number.
fn dest_order_key(
    change: &CacheChange,
    kind: DestinationOrderQosPolicyKind,
) -> (Option<RtpsTime>, SequenceNumber) {
    let ts = if kind == DestinationOrderQosPolicyKind::ByReceptionTimestamp {
        change.reception_timestamp()
    } else {
        change.source_timestamp()
    };
    (ts, change.sequence_number())
}

impl<Foo: 'static + Clone + Debug> HistoryCache for DataReaderHistoryCache<Foo> {
    // Returns a reference to the list of CacheChanges.
    // Walk instance buckets in handle order, flattening each bucket's ordered samples.
    fn get_changes(&self) -> Vec<Arc<CacheChange>> {
        self.instance_map
            .lock()
            .map(|m| m.values().flatten().cloned().collect())
            .unwrap_or_default()
    }

    // Insert into the change's instance bucket in DESTINATION_ORDER.
    fn insert_change_sorted(&mut self, change: Arc<CacheChange>) {
        let kind = self.destination_order_kind;
        if let Ok(mut map) = self.instance_map.lock() {
            let bucket = map.entry(change.instance_handle()).or_default();
            if kind == DestinationOrderQosPolicyKind::ByReceptionTimestamp {
                // reception_timestamp is stamped just before insertion (add_info_to_cache_change,
                // under this lock), so arrival order equals reception order — append at the tail.
                bucket.push(change);
            } else {
                // BY_SOURCE_TIMESTAMP: source timestamp may arrive out of order, so position by key.
                let key = dest_order_key(&change, kind);
                let pos = bucket.partition_point(|c| dest_order_key(c, kind) <= key);
                bucket.insert(pos, change);
            }
        }
    }

    // Returns the maximum number of samples allowed.
    fn get_max_samples(&self) -> i32 {
        self.max_samples
    }

    // Returns the maximum number of instances allowed.
    fn get_max_instances(&self) -> i32 {
        self.max_instances
    }

    // Returns the maximum number of samples per instance allowed.
    fn get_max_samples_per_instance(&self) -> i32 {
        self.max_samples_per_instance
    }

    fn sample_count(&self) -> DdsResult<usize> {
        let map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(map.values().map(|b| b.len()).sum())
    }

    fn instance_count(&self) -> DdsResult<usize> {
        let map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(map.len())
    }

    fn contains_instance(&self, instance_handle: InstanceHandle) -> DdsResult<bool> {
        let map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(map.contains_key(&instance_handle))
    }

    fn sample_count_of_instance(&self, instance_handle: InstanceHandle) -> DdsResult<usize> {
        let map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(map.get(&instance_handle).map_or(0, |v| v.len()))
    }

    // Buckets are DESTINATION_ORDER sorted, not globally source-timestamp ordered. Each BY_SOURCE
    // bucket is source-sorted, so early-break within it; BY_RECEPTION buckets are arrival order, so
    // full-scan. Earliest survivor is tracked across buckets for the next timer.
    fn collect_lifespan_expired(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        now: RtpsTime,
    ) -> (Vec<Arc<CacheChange>>, Option<RtpsTime>) {
        let source_ordered =
            self.destination_order_kind == DestinationOrderQosPolicyKind::BySourceTimestamp;
        let mut expired = Vec::new();
        let mut earliest_survivor: Option<RtpsTime> = None;
        if let Ok(map) = self.instance_map.lock() {
            for bucket in map.values() {
                for change in bucket.iter() {
                    if change.writer_guid() != writer_guid {
                        continue;
                    }
                    let Some(source_ts) = change.source_timestamp() else {
                        expired.push(change.clone());
                        continue;
                    };
                    let expiry = source_ts.add_nanos(lifespan_duration.as_nanos().max(0) as u64);
                    if now >= expiry {
                        expired.push(change.clone());
                    } else {
                        earliest_survivor = Some(match earliest_survivor {
                            Some(e) if e <= expiry => e,
                            _ => expiry,
                        });
                        if source_ordered {
                            break;
                        }
                    }
                }
            }
        }
        (expired, earliest_survivor)
    }

    // Returns the map of lifespan timers keyed by writer GUID.
    fn get_lifespan_timers(&self) -> Arc<Mutex<HashMap<Guid, TimerId>>> {
        self.lifespan_timers.clone()
    }

    // Registers a periodic timer that removes expired samples based on Lifespan QoS.
    fn register_lifespan_timer(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id: TimerId,
    ) -> DdsResult<()> {
        use log::debug;

        let data_reader_weak = if let Some(data_reader) = self.data_reader.upgrade() {
            Arc::downgrade(&data_reader)
        } else {
            return Err(DdsError::Error("DataReader has been dropped".to_string()));
        };

        let lifespan_duration_clone = lifespan_duration;
        let writer_guid_clone = writer_guid;

        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(data_reader) = data_reader_weak.upgrade() {
                if let Ok(cache_arc) = data_reader.get_datareader_cache() {
                    if let Ok(mut cache_guard) = cache_arc.lock() {
                        if let Err(e) = cache_guard.remove_lifespan_expired_changes(
                            writer_guid_clone,
                            lifespan_duration_clone,
                        ) {
                            debug!(
                                "[DataReaderHistoryCache] Failed to check lifespan samples: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        });

        self.register_lifespan_timer_with_callback(
            writer_guid,
            lifespan_duration,
            timer_id,
            callback,
        )
    }

    // Add a change under History/ResourceLimits. With apply_filter, an ALIVE owner sample inside
    // the separation window is held (returns (None, true)) and delivered later by a timer.
    fn add_change_with_cleanup(
        &mut self,
        immutable_change: Arc<CacheChange>,
        apply_filter: bool,
    ) -> DdsResult<(Option<Arc<CacheChange>>, bool)> {
        let writer_guid = immutable_change.writer_guid();

        // ContentFilteredTopic: drop non-matching samples at receipt so they never enter the history.
        if let Some(filter) = &self.content_filter {
            if !filter(&immutable_change) {
                return Ok((None, false));
            }
        }

        // Ensure ownership candidate is registered before any ownership check.
        self.add_to_owner_candidate_if_new(
            immutable_change.instance_handle(),
            immutable_change.ownership_strength().unwrap_or(0),
            immutable_change.writer_guid(),
        )?;

        // TIME_BASED_FILTER (receive path only).
        if apply_filter {
            let min_separation = self.reader_min_separation();
            if !min_separation.is_zero() {
                if immutable_change.kind() == ChangeKind::Alive
                    && self.is_writer_owner_of_instance(
                        writer_guid,
                        immutable_change.instance_handle(),
                    )?
                {
                    let nanos = min_separation.as_nanos().max(0) as u64;
                    match self.time_based_filter.on_alive_sample(
                        &immutable_change,
                        nanos,
                        RtpsTime::now(),
                    ) {
                        FilterOutcome::Held { deliver_after } => {
                            if let Some(delay) = deliver_after {
                                self.schedule_tbf_timer(immutable_change.instance_handle(), delay);
                            }
                            return Ok((None, true));
                        }
                        FilterOutcome::Deliver => {}
                    }
                }
                // Dispose/unregister bypass the filter; drop any held sample to keep order.
                else if immutable_change.kind() != ChangeKind::Alive {
                    self.time_based_filter.discard_pending(immutable_change.instance_handle());
                }
            }
        }

        // Reject changes from a non-owner writer and advance the instance state.
        self.update_instance_state(&immutable_change)?;

        // Check lifespan qos
        let lifespan_duration =
            immutable_change.lifespan_duration().and_then(|lifespan_duration| {
                if !lifespan_duration.is_infinite() {
                    Some(lifespan_duration)
                } else {
                    None
                }
            });

        // Create timer only if it does not exist
        if let Some(duration) = lifespan_duration {
            let timers = self.get_lifespan_timers();
            let timers_guard = timers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            let timer_exists = timers_guard.contains_key(&writer_guid);
            drop(timers_guard);

            if !timer_exists {
                if let Err(e) = self.register_lifespan_timer(
                    writer_guid,
                    duration,
                    TimerId::LifespanReader { writer_guid },
                ) {
                    debug!("[DataReaderHistoryCache] Failed to ensure lifespan timer: {:?}", e);
                }
            }
        }
        // end lifespan

        let removed_change = self.ensure_capacity(immutable_change.instance_handle())?;

        self.insert_change_sorted(immutable_change.clone());

        Ok((removed_change, false))
    }

    fn add_info_to_cache_change(&mut self, change: &mut CacheChange) -> DdsResult<()> {
        let data_reader = self
            .data_reader
            .upgrade()
            .ok_or(DdsError::Error("DataReader has been dropped".to_string()))?;

        let instance_handle = data_reader.fallback_instance_handle(change)?;
        change.set_instance_handle(instance_handle);
        change.set_reception_timestamp(RtpsTime::now());

        Ok(())
    }

    // True when the owning reader's PRESENTATION requests coherent access (INSTANCE or TOPIC scope).
    fn is_coherent_access(&self) -> bool {
        self.data_reader.upgrade().is_some_and(|reader| reader.is_subscriber_coherent())
    }

    // Removes the given CacheChange from its instance bucket.
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        let mut map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(bucket) = map.get_mut(&a_change.instance_handle()) {
            bucket.retain(|c| {
                !(c.sequence_number() == a_change.sequence_number()
                    && c.writer_guid() == a_change.writer_guid())
            });
        }
        Ok(())
    }

    // Ensures capacity before adding a new CacheChange to the history.
    // Returns Ok(None) if space is already available, or Ok(Some(CacheChange)) if space was secured after removing an existing CacheChange.
    // Returns Err(DdsError::OutOfResources) if space cannot be secured, in which case the Sample is rejected.
    fn ensure_capacity(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        // NO_KEY
        if instance_handle.is_nil() && self.is_max_samples_per_instance_exceeded(instance_handle)? {
            if !self.can_auto_remove {
                self.on_sample_rejected(SampleRejectedStatus {
                    total_count: 0,
                    total_count_change: 1,
                    last_reason: SampleRejectedStatusKind::RejectedBySamplesLimit,
                    last_instance_handle: instance_handle,
                });
                return Err(DdsError::OutOfResources);
            }
            let removed = self.try_remove_oldest_change_of_all()?;
            return Ok(Some(removed));
        }

        // WITH_KEY
        if !instance_handle.is_nil() {
            if self.is_max_samples_per_instance_exceeded(instance_handle)? {
                if !self.can_auto_remove {
                    self.on_sample_rejected(SampleRejectedStatus {
                        total_count: 0,
                        total_count_change: 1,
                        last_reason: SampleRejectedStatusKind::RejectedBySamplesPerInstanceLimit,
                        last_instance_handle: instance_handle,
                    });
                    return Err(DdsError::OutOfResources);
                }
                let removed = self.try_remove_oldest_change_of_instance(instance_handle)?;
                return Ok(Some(removed));
            } else if self.is_max_instances_exceeded(instance_handle)?
                && !self.remove_unused_instance()?
            {
                self.on_sample_rejected(SampleRejectedStatus {
                    total_count: 0,
                    total_count_change: 1,
                    last_reason: SampleRejectedStatusKind::RejectedByInstancesLimit,
                    last_instance_handle: instance_handle,
                });
                return Err(DdsError::OutOfResources);
            } else if self.is_max_samples_exceeded()? {
                if !self.can_auto_remove {
                    self.on_sample_rejected(SampleRejectedStatus {
                        total_count: 0,
                        total_count_change: 1,
                        last_reason: SampleRejectedStatusKind::RejectedBySamplesLimit,
                        last_instance_handle: instance_handle,
                    });
                    return Err(DdsError::OutOfResources);
                }

                let removed = self.try_remove_oldest_change_of_all()?;
                return Ok(Some(removed));
            }
        }

        Ok(None)
    }

    // Dry run of ensure_capacity for a batch given its per-instance sample counts: whether all
    // fit without any being dropped. Non-mutating, so the caller can reject the batch as a unit.
    fn ensure_capacity_dry(
        &self,
        len_per_instance: &HashMap<InstanceHandle, usize>,
    ) -> DdsResult<bool> {
        let map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        // Per instance: with auto-remove, eviction frees older samples so only the batch's own
        // members compete for depth; otherwise the batch must fit alongside what is already stored.
        for (handle, len_of_cache_changes) in len_per_instance {
            if handle.is_nil() {
                continue; // NO_KEY instances are bounded by max_samples below.
            }

            let existing_len =
                if self.can_auto_remove { 0 } else { map.get(handle).map_or(0, |v| v.len()) };

            if existing_len + len_of_cache_changes > self.max_samples_per_instance as usize {
                return Ok(false);
            }
        }

        // New instances the batch introduces must fit alongside the existing ones.
        let new_instance_count =
            len_per_instance.keys().filter(|h| !h.is_nil() && !map.contains_key(*h)).count();

        if map.len() + new_instance_count > self.max_instances as usize {
            return Ok(false);
        }

        // Total samples: with auto-remove only the batch competes; otherwise it adds to the total.
        let batch_total_len: usize = len_per_instance.values().sum();

        let existing_total: usize =
            if self.can_auto_remove { 0 } else { map.values().map(|v| v.len()).sum() };

        Ok(existing_total + batch_total_len <= self.max_samples as usize)
    }

    // Removes the oldest change across all instances (DESTINATION_ORDER).
    fn try_remove_oldest_change_of_all(&mut self) -> DdsResult<Arc<CacheChange>> {
        let kind = self.destination_order_kind;
        let oldest = {
            let map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            // Each bucket is DESTINATION_ORDER sorted, so its front is that instance's oldest;
            // the global oldest is the minimum of the fronts.
            map.values()
                .filter_map(|bucket| bucket.first())
                .min_by_key(|c| dest_order_key(c, kind))
                .map(Arc::clone)
        };

        match oldest {
            Some(change) => {
                self.remove_change(change.clone())?;
                Ok(change)
            }
            None => Err(DdsError::Error(
                "There is no change to remove, this should never happen".to_string(),
            )),
        }
    }

    // Removes the oldest change from the specified instance (DESTINATION_ORDER).
    fn try_remove_oldest_change_of_instance(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Arc<CacheChange>> {
        let oldest = {
            let instance_map =
                self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            // Bucket is DESTINATION_ORDER sorted; its front is the instance's oldest sample.
            instance_map.get(&instance_handle).and_then(|bucket| bucket.first()).map(Arc::clone)
        };

        match oldest {
            Some(change) => {
                self.remove_change(Arc::clone(&change))?;
                Ok(change)
            }
            None => Err(DdsError::Error(
                "There is no change to remove in this instance, this should never happen"
                    .to_string(),
            )),
        }
    }
}

impl<Foo: DdsType> DataReaderHistoryCache<Foo> {
    // Install the receive-side ContentFilteredTopic hook. Requires set_datareader first.
    pub(crate) fn set_content_filter(&mut self) {
        let data_reader = self.data_reader.clone();
        self.content_filter = Some(Arc::new(move |change: &CacheChange| {
            data_reader
                .upgrade()
                .map(|dr| dr.passes_content_filter(change).unwrap_or(true))
                .unwrap_or(true)
        }));
    }
}

impl<Foo: 'static + Clone + Debug> DataReaderHistoryCache<Foo> {
    pub(crate) fn new(
        data_reader: Weak<DataReader<Foo>>,
        reliability_qos: ReliabilityQosPolicy,
        history_qos: HistoryQosPolicy,
        resource_limits_qos: ResourceLimitsQosPolicy,
        has_key: bool,
        ownership_kind: OwnershipQosPolicyKind,
        destination_order_kind: DestinationOrderQosPolicyKind,
    ) -> Self {
        #[inline]
        fn cap(v: i32) -> i32 {
            if v == LENGTH_UNLIMITED {
                i32::MAX
            } else {
                v
            }
        }

        // 2.2.3.18 HISTORY
        let max_samples_per_instance = match history_qos.kind {
            // For KEEP_LAST, depth determines the maximum number of data samples to keep per instance, discarding previous values.
            HistoryQosPolicyKind::KeepLast(depth) => depth,
            // For KEEP_ALL, available resources are limited by the RESOURCE_LIMITS QoS.
            HistoryQosPolicyKind::KeepAll => cap(resource_limits_qos.max_samples_per_instance),
        };

        let max_instances = if has_key { cap(resource_limits_qos.max_instances) } else { 1 };
        let max_samples = cap(resource_limits_qos.max_samples);

        let max_samples = i32::min(
            max_samples,
            max_instances.saturating_mul(max_samples_per_instance), // Prevent overflow
        );

        debug!(
            "Creating data reader's history cache with max_samples_per_instance: {:?}",
            max_samples_per_instance
        );
        debug!("Creating data reader's history cache with max_instances: {:?}", max_instances);
        debug!("Creating data reader's history cache with max_samples: {:?}", max_samples);

        Self {
            data_reader,
            max_samples,
            max_instances,
            max_samples_per_instance,
            instance_map: Arc::new(Mutex::new(BTreeMap::new())),
            destination_order_kind,
            can_auto_remove: history_qos.kind != HistoryQosPolicyKind::KeepAll
                || reliability_qos.kind == ReliabilityQosPolicyKind::BestEffort, // 2.2.3.18 - 3. If keep_all && resource limits reached, then the behavior will depend on the RELIABILITY QoS.
            owner_candidates: Arc::new(DashMap::new()),
            ownership_kind,
            lifespan_timers: Arc::new(Mutex::new(HashMap::new())),
            status_callback: Arc::new(Mutex::new(None)),
            time_based_filter: TimeBasedFilter::new(),
            content_filter: None,
        }
    }

    // TOPIC ordered_access: merge the per-instance buckets (each already DESTINATION_ORDER
    // sorted) into one globally DESTINATION_ORDER ordered list via k-way merge of sorted runs.
    pub(crate) fn get_changes_for_topic_scoped_ordered_access(&self) -> Vec<Arc<CacheChange>> {
        let kind = self.destination_order_kind;
        self.instance_map
            .lock()
            .map(|map| {
                // K-way merge of sorted runs: each instance bucket is already DESTINATION_ORDER sorted.
                let buckets: Vec<&Vec<Arc<CacheChange>>> = map.values().collect();

                // Initialize cursors for each bucket.
                let mut cursor = vec![0usize; buckets.len()];

                // Prepare the output vector with the total capacity.
                let total: usize = buckets.iter().map(|b| b.len()).sum();
                let mut out = Vec::with_capacity(total);

                // Perform the k-way merge by repeatedly selecting the next smallest element from the buckets.
                for _ in 0..total {
                    // Pick the bucket whose next sample has the smallest DESTINATION_ORDER key.
                    let mut best: Option<usize> = None;
                    for (i, bucket) in buckets.iter().enumerate() {
                        if cursor[i] >= bucket.len() {
                            continue;
                        }
                        // Compare the current sample in this bucket with the best candidate found so far.
                        let is_smaller = best.is_none_or(|bi| {
                            dest_order_key(&bucket[cursor[i]], kind)
                                < dest_order_key(&buckets[bi][cursor[bi]], kind)
                        });
                        if is_smaller {
                            best = Some(i);
                        }
                    }
                    let Some(bi) = best else {
                        error!(
                            "All buckets exhausted during k-way merge, this should never happen"
                        );
                        break;
                    };
                    out.push(buckets[bi][cursor[bi]].clone());
                    cursor[bi] += 1;
                }
                out
            })
            .unwrap_or_default()
    }

    // Must be called immediately after DataReaderHistoryCache creation.
    pub(crate) fn set_datareader(&mut self, data_reader: Weak<DataReader<Foo>>) {
        self.data_reader = data_reader;
    }

    // Returns the owner Guid of the instance corresponding to the instance handle.
    fn get_owner_of_instance(&self, instance_handle: InstanceHandle) -> Option<Guid> {
        self.owner_candidates.get(&instance_handle)?.first().map(|owner_info| owner_info.owner_guid)
    }

    // Updates the instance state based on the CacheChange kind.
    // Returns true if this change actually changed the instance state.
    fn update_instance_state(&self, cache_change: &CacheChange) -> DdsResult<bool> {
        // if cache_change.instance_handle().is_nil() {
        //     return Ok(());
        // }

        let data_reader = self
            .data_reader
            .upgrade()
            .ok_or(DdsError::Error("DataReader has been dropped".to_string()))?;

        let change_kind = cache_change.kind();
        let mut state_changed = false;

        match change_kind {
            ChangeKind::Alive
            | ChangeKind::AliveFiltered
            | ChangeKind::NotAliveDisposed
            | ChangeKind::NotAliveDisposedUnregistered => {
                if !self.is_writer_owner_of_instance(
                    cache_change.writer_guid(),
                    cache_change.instance_handle(),
                )? {
                    debug!(
                        "Rejecting change, writer {} is not owner of instance: {}",
                        cache_change.writer_guid(),
                        cache_change.instance_handle()
                    );
                    return Err(DdsError::IllegalOperation);
                }

                let new_state = match change_kind {
                    ChangeKind::Alive | ChangeKind::AliveFiltered => {
                        InstanceStateKind::ALIVE_INSTANCE_STATE
                    }
                    _ => InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE,
                };

                state_changed |= data_reader.update_instance_state(
                    cache_change.instance_handle(),
                    new_state,
                    Some(cache_change),
                )?;
            }
            _ => {}
        }

        if matches!(
            change_kind,
            ChangeKind::NotAliveUnregistered | ChangeKind::NotAliveDisposedUnregistered
        ) {
            state_changed |= self.revoke_writer_ownership(
                cache_change.writer_guid(),
                Some(cache_change.instance_handle()),
                true,
                false,
            )?;
        }

        Ok(state_changed)
    }

    // Removes the writer from owner candidates.
    // instance_handle None removes it from every instance; Some only from that instance.
    // Returns true if any instance's state actually changed to NOT_ALIVE_NO_WRITERS.
    pub(crate) fn revoke_writer_ownership(
        &self,
        remote_writer_guid: Guid,
        instance_handle: Option<InstanceHandle>,
        update_state: bool, // should update instance state to NOT_ALIVE_NO_WRITERS if no candidates left
        synthesize_notification: bool, // should data reader create an invalid-data sample
    ) -> DdsResult<bool> {
        debug!("Removing writer {} from owner candidates", remote_writer_guid);
        let mut any_changed = false;
        match instance_handle {
            Some(target) => {
                if let Some(mut entry) = self.owner_candidates.get_mut(&target) {
                    any_changed |= self.revoke_writer_ownership_for_instance(
                        target,
                        entry.value_mut(),
                        remote_writer_guid,
                        update_state,
                        synthesize_notification,
                    )?;
                }
            }
            None => {
                for mut entry in self.owner_candidates.iter_mut() {
                    let key = *entry.key();
                    any_changed |= self.revoke_writer_ownership_for_instance(
                        key,
                        entry.value_mut(),
                        remote_writer_guid,
                        update_state,
                        synthesize_notification,
                    )?;
                }
            }
        }

        Ok(any_changed)
    }

    // Removes the writer from a single instance's candidate set, updating its state to
    // NOT_ALIVE_NO_WRITERS when none remain. Returns true if the state actually changed.
    fn revoke_writer_ownership_for_instance(
        &self,
        instance_handle: InstanceHandle,
        writers: &mut BTreeSet<OwnershipInfo>,
        remote_writer_guid: Guid,
        update_state: bool,
        synthesize_notification: bool,
    ) -> DdsResult<bool> {
        writers.retain(|owner_info| owner_info.owner_guid != remote_writer_guid);

        if !writers.is_empty() {
            debug!("Now the owner is {:?}", writers.first());
            return Ok(false);
        }

        debug!("No writers left for instance {}", instance_handle);
        if !update_state {
            return Ok(false);
        }

        let data_reader = self
            .data_reader
            .upgrade()
            .ok_or(DdsError::Error("DataReader has been dropped".to_string()))?;
        let state_changed = data_reader.update_instance_state(
            instance_handle,
            InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE,
            None,
        )?;
        // Skip the synthetic sample when the transition was rejected (e.g. the
        // instance is already DISPOSED): no state change, no notification.
        if synthesize_notification && state_changed {
            data_reader.mark_pending_notification(instance_handle)?;
        }
        Ok(state_changed)
    }

    // Revokes the current owner of the instance without updating instance state.
    pub(crate) fn revoke_current_owner_from_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        if self.ownership_kind == OwnershipQosPolicyKind::Shared {
            return Ok(()); // This does not apply for Shared ownership
        }

        let current_owner = self.get_owner_of_instance(instance_handle);
        if let Some(owner_guid) = current_owner {
            self.revoke_writer_ownership(owner_guid, Some(instance_handle), false, false)?;
        }

        Ok(())
    }

    // Checks if the Writer that sent the CacheChange is the owner of the instance.
    pub(crate) fn is_writer_owner_of_instance(
        &self,
        writer_guid: Guid,
        instance_handle: InstanceHandle,
    ) -> DdsResult<bool> {
        if self.ownership_kind == OwnershipQosPolicyKind::Shared {
            return Ok(true);
        }

        let owner = self.get_owner_of_instance(instance_handle);

        if let Some(owner_guid) = owner {
            Ok(owner_guid == writer_guid)
        } else {
            Ok(false)
        }
    }

    // Adds a new owner candidate to the owner candidate list for the specified instance.
    pub(crate) fn add_to_owner_candidate_if_new(
        &self,
        instance_handle: InstanceHandle,
        ownership_strength: i32,
        owner_guid: Guid,
    ) -> DdsResult<()> {
        let mut entry = self.owner_candidates.entry(instance_handle).or_default();
        if !entry.iter().any(|info| info.owner_guid == owner_guid) {
            debug!(
                "Adding new owner candidate {} with strength {} for instance {}",
                owner_guid, ownership_strength, instance_handle
            );
            entry.insert(OwnershipInfo { ownership_strength, owner_guid });
        }
        Ok(())
    }

    // Removes unused instances from the instance map.
    fn remove_unused_instance(&mut self) -> DdsResult<bool> {
        let mut key_to_remove: Option<InstanceHandle> = None;
        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for (instance_handle, changes) in instance_map.iter() {
            if changes.is_empty() && self.check_if_no_writers(*instance_handle)? {
                key_to_remove = Some(*instance_handle);
                break;
            }
        }

        drop(instance_map);

        if let Some(key) = key_to_remove {
            self.remove_all_instance_resources(key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // Drop all per-instance resources: the cache index entry, the TIME_BASED_FILTER state, and
    // the SampleInfo bookkeeping.
    pub(crate) fn remove_all_instance_resources(&self, instance_handle: InstanceHandle) {
        if let Ok(mut instance_map) = self.instance_map.lock() {
            instance_map.remove(&instance_handle);
        }
        self.time_based_filter.remove_instance(instance_handle);
        if let Some(data_reader) = self.data_reader.upgrade() {
            data_reader.remove_instance_info(instance_handle);
        }
    }

    // Read the live minimum_separation from the reader QoS (0 when unavailable).
    fn reader_min_separation(&self) -> Duration {
        self.data_reader
            .upgrade()
            .and_then(|data_reader| data_reader.get_qos().ok())
            .map(|qos| qos.time_based_filter.minimum_separation)
            .unwrap_or_else(|| Duration::new(0, 0))
    }

    // Schedule a one-shot timer to deliver the instance's held sample after the window elapses.
    fn schedule_tbf_timer(&self, instance_handle: InstanceHandle, delay: std::time::Duration) {
        let Some(data_reader) = self.data_reader.upgrade() else { return };
        let Ok(rtps_reader) = data_reader.get_rtps_reader() else { return };
        let reader_guid = rtps_reader.guid();
        let timer_id =
            TimerId::TimeBasedFilter { reader_entity_id: reader_guid.entity_id(), instance_handle };

        let Ok(timer_handler) = self.get_timer_handler(reader_guid.prefix()) else { return };
        let Ok(handler) = timer_handler.lock() else { return };

        let weak_data_reader = self.data_reader.clone();
        handler.remove_timer(timer_id);
        handler.add_timer(timer_id, delay, false, move || {
            deliver_held_sample(&weak_data_reader, instance_handle);
        });
    }

    // Checks if there is no writer writing to this instance.
    fn check_if_no_writers(&self, instance_handle: InstanceHandle) -> DdsResult<bool> {
        Ok(self.get_owner_of_instance(instance_handle).is_none())
    }

    // Removes all samples of the specified instance.
    pub(crate) fn remove_all_changes_of_instance(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(changes) = instance_map.get_mut(&instance_handle) {
            changes.clear();
        }

        Ok(())
    }

    // Retrieves CacheChange using the SequenceNumber and the Guid of the Writer that sent it.
    pub(crate) fn get_change(
        &self,
        seq_num: SequenceNumber,
        writer_guid: Guid,
    ) -> Option<Arc<CacheChange>> {
        let instance_map = self.instance_map.lock().ok()?;
        instance_map
            .values()
            .flatten()
            .find(|change| {
                change.sequence_number() == seq_num && change.writer_guid() == writer_guid
            })
            .cloned()
    }

    // Retrieves all change identifiers (writer GUID and sequence number) of the specified instance.
    pub(crate) fn get_change_id_set_of_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<HashSet<ReaderChangeId>> {
        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let changes = if let Some(changes) = instance_map.get(&instance_handle) {
            changes
                .iter()
                .map(|change| (change.writer_guid(), change.sequence_number()))
                .collect::<HashSet<ReaderChangeId>>()
        } else {
            HashSet::new()
        };

        Ok(changes)
    }

    // Sets the callback function for status updates.
    #[allow(clippy::type_complexity)]
    pub(crate) fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        match self.status_callback.lock() {
            Ok(mut callback) => {
                callback.replace(f);
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }

    // Invokes the status callback when a sample is rejected.
    fn on_sample_rejected(&self, info: SampleRejectedStatus) {
        match self.status_callback.lock() {
            Ok(callback) => {
                if let Some(callback) = callback.as_ref() {
                    callback(StatusKind::SAMPLE_REJECTED, Some(Arc::new(info)));
                }
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }
}

// Timer callback: re-deliver the instance's held sample (no filter) through the RTPS cache.
fn deliver_held_sample<Foo: 'static + Clone + Debug>(
    weak_data_reader: &Weak<DataReader<Foo>>,
    instance_handle: InstanceHandle,
) {
    let Some(data_reader) = weak_data_reader.upgrade() else { return };
    let Ok(rtps_reader) = data_reader.get_rtps_reader() else { return };
    let Ok(cache_arc) = data_reader.get_datareader_cache() else { return };

    let pending = match cache_arc.lock() {
        Ok(cache) => {
            cache.time_based_filter.take_pending_on_timer(instance_handle, RtpsTime::now())
        }
        Err(_) => return,
    };
    let Some(change) = pending else { return };

    match rtps_reader.reader_cache().lock() {
        // Re-deliver without the filter (already decided) so it stores and notifies.
        Ok(mut reader_cache) => match reader_cache.add_change((*change).clone(), false) {
            Ok(delivered) => {
                drop(reader_cache);
                for change in delivered {
                    rtps_reader.on_change(change);
                }
            }
            Err(e) => debug!("[TimeBasedFilter] Failed to deliver held sample: {:?}", e),
        },
        Err(e) => debug!("[TimeBasedFilter] Failed to lock reader cache: {:?}", e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::topic::type_support::DdsType;
    use crate::infrastructure::qos_policy::{
        OwnershipQosPolicy, PresentationQosAccessScopeKind, PresentationQosPolicy,
        ReliabilityQosPolicyKind,
    };
    use crate::rtps::entities::history::cache_change::PresentationInfo;
    use crate::{
        core::time::Duration,
        domain::{
            domain_participant::DomainParticipant,
            domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos,
        },
        infrastructure::status::StatusMask,
        rtps::common::{
            entity_id::EntityId, entity_kind::EntityKind, guid::Guid, sequence::SequenceNumber,
            types::ChangeKind,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    };

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct ShapeType {
        #[dds(key)]
        color: String,
        x: i32,
        y: i32,
        shapesize: i32,
        additional_payload_size: Vec<u8>,
    }

    // Helper function to create a test CacheChange
    fn create_change_with_key(seq: i64, handle: InstanceHandle) -> Arc<CacheChange> {
        Arc::new(CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY)),
            handle,
            SequenceNumber::from_i64(seq),
            vec![
                0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0, 20,
                0, 0, 0, 0, 0, 0, 0,
            ],
            Some(RtpsTime::now()),
        ))
    }

    // Keyed test CacheChange with an explicit source_timestamp (for DESTINATION_ORDER tests).
    fn create_change_with_key_src(
        seq: i64,
        handle: InstanceHandle,
        source: RtpsTime,
    ) -> CacheChange {
        CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY)),
            handle,
            SequenceNumber::from_i64(seq),
            vec![
                0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0, 20,
                0, 0, 0, 0, 0, 0, 0,
            ],
            Some(source),
        )
    }

    // Helper function to create a test CacheChange
    fn create_change_no_key(seq: i64, handle: InstanceHandle) -> Arc<CacheChange> {
        Arc::new(CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY)),
            handle,
            SequenceNumber::from_i64(seq),
            vec![
                0, 1, 0, 0, 92, 0, 0, 0, 12, 0, 0, 0, 72, 101, 108, 108, 111, 32, 119, 111, 114,
                108, 100, 0,
            ],
            Some(RtpsTime::now()),
        ))
    }

    fn create_with_key_datareader(
        data_reader_qos: DataReaderQos,
    ) -> (DomainParticipant, DataReader<ShapeType>) {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<ShapeType>(
                "TestTopic",
                "ShapeType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let reader = subscriber
            .create_datareader::<ShapeType>(&topic, data_reader_qos, None, StatusMask::default())
            .unwrap();

        (domain_participant, reader)
    }

    // Same as create_with_key_datareader but with an explicit SubscriberQos and its own
    // participant on a unique domain, returned for the test's teardown.
    fn create_with_key_datareader_in_subscriber(
        subscriber_qos: SubscriberQos,
        data_reader_qos: DataReaderQos,
    ) -> (DomainParticipant, DataReader<ShapeType>) {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(
                crate::test_utils::unique_domain_id(),
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = domain_participant
            .create_topic::<ShapeType>(
                "TestTopic",
                "ShapeType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(subscriber_qos, None, StatusMask::default())
            .unwrap();

        let reader = subscriber
            .create_datareader::<ShapeType>(&topic, data_reader_qos, None, StatusMask::default())
            .unwrap();

        (domain_participant, reader)
    }

    fn topic_coherent_subscriber_qos() -> SubscriberQos {
        SubscriberQos {
            presentation: PresentationQosPolicy {
                access_scope: PresentationQosAccessScopeKind::Topic,
                coherent_access: true,
                ordered_access: false,
            },
            ..Default::default()
        }
    }

    fn instance_coherent_subscriber_qos() -> SubscriberQos {
        SubscriberQos {
            presentation: PresentationQosPolicy {
                access_scope: PresentationQosAccessScopeKind::Instance,
                coherent_access: true,
                ordered_access: false,
            },
            ..Default::default()
        }
    }

    // Keyed coherent-set member: create_change_with_key_src plus PID_COHERENT_SET.
    fn create_coherent_member(seq: i64, handle: InstanceHandle, set_id: i64) -> CacheChange {
        let mut change = create_change_with_key_src(seq, handle, RtpsTime::now());
        change.set_presentation_info(PresentationInfo {
            coherent_set: Some(SequenceNumber::from_i64(set_id)),
            ..Default::default()
        });
        change
    }

    // Coherent set end marker: payload-less Data with PID_COHERENT_SET = UNKNOWN.
    fn create_coherent_end_marker_with_sn_unknown(seq: i64) -> CacheChange {
        let mut change = CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY)),
            InstanceHandle::default(),
            SequenceNumber::from_i64(seq),
            Vec::new(),
            Some(RtpsTime::now()),
        );
        change.set_presentation_info(PresentationInfo {
            coherent_set: Some(SequenceNumber::UNKNOWN),
            ..Default::default()
        });
        change
    }

    // End marker without a coherent set id: a payload-less Alive Data.
    fn create_coherent_end_marker_without_id(seq: i64) -> CacheChange {
        CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY)),
            InstanceHandle::default(),
            SequenceNumber::from_i64(seq),
            Vec::new(),
            Some(RtpsTime::now()),
        )
    }

    #[test]
    fn coherent_set_commits_all_members_on_marker_without_id() {
        // A payload-less Data carrying no coherent set id must close and commit the buffered
        // set just like the PID_COHERENT_SET=UNKNOWN marker, and must not itself be stored.
        let (participant, data_reader) = create_with_key_datareader_in_subscriber(
            topic_coherent_subscriber_qos(),
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            },
        );
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);

        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            for seq in 1..=3 {
                let available =
                    reader_cache.add_change(create_coherent_member(seq, handle, 1), false).unwrap();
                assert!(available.is_empty(), "member {} must be buffered, not stored", seq);
            }

            let committed =
                reader_cache.add_change(create_coherent_end_marker_without_id(4), false).unwrap();
            let seqs: Vec<i64> = committed.iter().map(|c| c.sequence_number().to_i64()).collect();
            assert_eq!(seqs, vec![1, 2, 3]);
        }

        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let stored: Vec<i64> = datareader_cache
            .lock()
            .unwrap()
            .get_changes()
            .iter()
            .map(|c| c.sequence_number().to_i64())
            .collect();
        assert_eq!(stored, vec![1, 2, 3], "end marker must not be stored");

        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn coherent_set_commits_all_members_on_sn_unknown_marker() {
        // Members are buffered (nothing available) until the end marker commits them all
        // at once; the marker itself is never stored.
        let (participant, data_reader) = create_with_key_datareader_in_subscriber(
            topic_coherent_subscriber_qos(),
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            },
        );
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);

        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            for seq in 1..=3 {
                let available =
                    reader_cache.add_change(create_coherent_member(seq, handle, 1), false).unwrap();
                assert!(available.is_empty(), "member {} must be buffered, not stored", seq);
            }

            let committed = reader_cache
                .add_change(create_coherent_end_marker_with_sn_unknown(4), false)
                .unwrap();
            let seqs: Vec<i64> = committed.iter().map(|c| c.sequence_number().to_i64()).collect();
            assert_eq!(seqs, vec![1, 2, 3]);
        }

        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let stored: Vec<i64> = datareader_cache
            .lock()
            .unwrap()
            .get_changes()
            .iter()
            .map(|c| c.sequence_number().to_i64())
            .collect();
        assert_eq!(stored, vec![1, 2, 3], "marker must not be stored");

        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn instance_scope_coherent_set_commits_all_members_on_sn_unknown_marker() {
        // INSTANCE scope buffers set members per writer just like TOPIC scope: nothing is
        // available until the end marker commits them all at once.
        let (participant, data_reader) = create_with_key_datareader_in_subscriber(
            instance_coherent_subscriber_qos(),
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            },
        );
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);

        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            for seq in 1..=3 {
                let available =
                    reader_cache.add_change(create_coherent_member(seq, handle, 1), false).unwrap();
                assert!(available.is_empty(), "member {} must be buffered, not stored", seq);
            }

            let committed = reader_cache
                .add_change(create_coherent_end_marker_with_sn_unknown(4), false)
                .unwrap();
            let seqs: Vec<i64> = committed.iter().map(|c| c.sequence_number().to_i64()).collect();
            assert_eq!(seqs, vec![1, 2, 3]);
        }

        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn coherent_set_with_middle_gap_is_discarded() {
        // Member 2 never arrives: the end marker finds the hole and drops the whole set.
        let (participant, data_reader) = create_with_key_datareader_in_subscriber(
            topic_coherent_subscriber_qos(),
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            },
        );
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);

        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            for seq in [1, 3] {
                reader_cache.add_change(create_coherent_member(seq, handle, 1), false).unwrap();
            }

            let committed = reader_cache
                .add_change(create_coherent_end_marker_with_sn_unknown(4), false)
                .unwrap();
            assert!(committed.is_empty(), "incomplete set must be discarded");
        }

        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        assert!(datareader_cache.lock().unwrap().get_changes().is_empty());

        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn coherent_set_with_lost_tail_is_discarded() {
        // Members are contiguous but the marker seq shows the last member was lost.
        let (participant, data_reader) = create_with_key_datareader_in_subscriber(
            topic_coherent_subscriber_qos(),
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            },
        );
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);

        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            for seq in [1, 2] {
                reader_cache.add_change(create_coherent_member(seq, handle, 1), false).unwrap();
            }

            // Marker at seq 4 implies member 3 existed but never arrived.
            let committed = reader_cache
                .add_change(create_coherent_end_marker_with_sn_unknown(4), false)
                .unwrap();
            assert!(committed.is_empty(), "set with lost tail must be discarded");
        }

        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        assert!(datareader_cache.lock().unwrap().get_changes().is_empty());

        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn non_coherent_reader_stores_members_and_drops_marker() {
        // Without TOPIC+coherent presentation, members flow through immediately and the
        // end marker is never stored.
        let (participant, data_reader) = create_with_key_datareader_in_subscriber(
            SubscriberQos::default(),
            DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            },
        );
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);

        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            let available =
                reader_cache.add_change(create_coherent_member(1, handle, 1), false).unwrap();
            assert_eq!(available.len(), 1, "member must be stored immediately");

            let available = reader_cache
                .add_change(create_coherent_end_marker_with_sn_unknown(2), false)
                .unwrap();
            assert!(available.is_empty(), "marker must be dropped");
        }

        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let stored: Vec<i64> = datareader_cache
            .lock()
            .unwrap()
            .get_changes()
            .iter()
            .map(|c| c.sequence_number().to_i64())
            .collect();
        assert_eq!(stored, vec![1]);

        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    fn create_no_key_datareader(
        data_reader_qos: DataReaderQos,
    ) -> (DomainParticipant, DataReader<HelloWorldType>) {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<HelloWorldType>(
                "TestTopic",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                data_reader_qos,
                None,
                StatusMask::default(),
            )
            .unwrap();

        (domain_participant, reader)
    }

    // Reclaiming an instance drops all three per-instance resources: the cache index entry,
    // the SampleInfo bookkeeping, and the TIME_BASED_FILTER state.
    #[test]
    fn remove_all_instance_resources_clears_index_sample_info_and_filter() {
        let mut qos = DataReaderQos::default();
        qos.time_based_filter.minimum_separation = Duration::from_millis(500);
        let (participant, reader) = create_with_key_datareader(qos);
        let handle = InstanceHandle::new([7; 16]);

        // Populate all three via the filtered add path: cache index, SampleInfo state, and
        // TIME_BASED_FILTER state.
        let cache_arc = reader.get_datareader_cache().unwrap();
        cache_arc
            .lock()
            .unwrap()
            .add_change_with_cleanup(create_change_with_key(1, handle), true)
            .unwrap();

        assert!(cache_arc.lock().unwrap().contains_instance(handle).unwrap());
        assert!(reader.get_instance_infos().unwrap().contains_key(&handle));
        assert!(cache_arc.lock().unwrap().time_based_filter.tracks_instance(handle));

        cache_arc.lock().unwrap().remove_all_instance_resources(handle);

        assert!(!cache_arc.lock().unwrap().contains_instance(handle).unwrap());
        assert!(!reader.get_instance_infos().unwrap().contains_key(&handle));
        assert!(!cache_arc.lock().unwrap().time_based_filter.tracks_instance(handle));

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn get_changes_returns_instance_blocks_not_insertion_order() {
        // Interleave two instances so insertion order (B1,A2,B3,A4) differs from
        // instance-block order (A2,A4,B1,B3). get_changes must walk instance buckets.
        let reader_qos = DataReaderQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(100),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 10,
                max_instances: 2,
                max_samples_per_instance: 10,
            },
            ..Default::default()
        };

        let (participant, data_reader) = create_with_key_datareader(reader_qos);
        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let mut datareader_cache = datareader_cache.lock().unwrap();

        let instance_a = InstanceHandle::new([1; 16]);
        let instance_b = InstanceHandle::new([2; 16]);

        datareader_cache
            .add_change_with_cleanup(create_change_with_key(1, instance_b), false)
            .unwrap();
        datareader_cache
            .add_change_with_cleanup(create_change_with_key(2, instance_a), false)
            .unwrap();
        datareader_cache
            .add_change_with_cleanup(create_change_with_key(3, instance_b), false)
            .unwrap();
        datareader_cache
            .add_change_with_cleanup(create_change_with_key(4, instance_a), false)
            .unwrap();

        let seqs: Vec<i64> =
            datareader_cache.get_changes().iter().map(|c| c.sequence_number().to_i64()).collect();
        // Instance A (handle [1;16]) block first, then instance B; within a block, arrival order.
        assert_eq!(seqs, vec![2, 4, 1, 3]);

        drop(datareader_cache);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn by_source_timestamp_orders_bucket_by_source_not_arrival() {
        // BY_SOURCE_TIMESTAMP: a sample with a smaller source_timestamp arriving later still
        // sorts ahead of an earlier-arriving sample with a larger source_timestamp.
        let mut reader_qos = DataReaderQos::default();
        reader_qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true };
        reader_qos.destination_order.kind = DestinationOrderQosPolicyKind::BySourceTimestamp;

        let (participant, data_reader) = create_with_key_datareader(reader_qos);
        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let mut datareader_cache = datareader_cache.lock().unwrap();

        let handle = InstanceHandle::new([1; 16]);
        // seq1 arrives first with a later source ts; seq2 arrives second with an earlier one.
        datareader_cache
            .add_change_with_cleanup(
                Arc::new(create_change_with_key_src(1, handle, RtpsTime::from_nanos(200))),
                false,
            )
            .unwrap();
        datareader_cache
            .add_change_with_cleanup(
                Arc::new(create_change_with_key_src(2, handle, RtpsTime::from_nanos(100))),
                false,
            )
            .unwrap();

        let seqs: Vec<i64> =
            datareader_cache.get_changes().iter().map(|c| c.sequence_number().to_i64()).collect();
        // Ordered by source timestamp ascending: seq2 (100) ahead of seq1 (200).
        assert_eq!(seqs, vec![2, 1]);

        drop(datareader_cache);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn add_change_stores_samples_in_reception_timestamp_order() {
        // reader_history::add_change stamps reception_timestamp (add_info_to_cache_change) right
        // before inserting, so the stored history ends up in reception order — the property the
        // BY_RECEPTION tail push relies on. Drives the real add_change path; if stamping is
        // removed or ordered after the insert, reception_timestamp is unset/out of order here.
        let (participant, data_reader) = create_with_key_datareader(DataReaderQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            ..Default::default()
        });
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let handle = InstanceHandle::new([1; 16]);
        {
            let reader_cache = rtps_reader.reader_cache();
            let mut reader_cache = reader_cache.lock().unwrap();
            for seq in 1..=3 {
                reader_cache
                    .add_change(create_change_with_key_src(seq, handle, RtpsTime::ZERO), false)
                    .unwrap();
            }
        }

        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let datareader_cache = datareader_cache.lock().unwrap();
        let stamps: Vec<RtpsTime> = datareader_cache
            .get_changes()
            .iter()
            .map(|c| {
                c.reception_timestamp().expect("reception_timestamp must be stamped at add_change")
            })
            .collect();
        let mut expected = stamps.clone();
        expected.sort();
        assert_eq!(stamps, expected, "stored order must follow reception_timestamp");

        drop(datareader_cache);
        drop(rtps_reader);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn topic_ordered_merges_instances_by_creation_order() {
        // Two instances A,B interleaved by ascending source timestamp: A0,B0,A1,B1.
        // get_changes() returns instance blocks; get_changes_for_topic_scoped_ordered_access()
        // k-way merges the buckets into one globally source-ordered list.
        let mut reader_qos = DataReaderQos::default();
        reader_qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true };
        reader_qos.destination_order.kind = DestinationOrderQosPolicyKind::BySourceTimestamp;

        let (participant, data_reader) = create_with_key_datareader(reader_qos);
        let datareader_cache = data_reader.get_datareader_cache().unwrap();
        let mut datareader_cache = datareader_cache.lock().unwrap();

        let a = InstanceHandle::new([1; 16]);
        let b = InstanceHandle::new([2; 16]);
        for (seq, handle, ts) in [(1, a, 10u64), (2, b, 20), (3, a, 30), (4, b, 40)] {
            datareader_cache
                .add_change_with_cleanup(
                    Arc::new(create_change_with_key_src(seq, handle, RtpsTime::from_nanos(ts))),
                    false,
                )
                .unwrap();
        }

        // INSTANCE scope: bucket A (seq 1,3) then bucket B (seq 2,4).
        let blocks: Vec<i64> =
            datareader_cache.get_changes().iter().map(|c| c.sequence_number().to_i64()).collect();
        assert_eq!(blocks, vec![1, 3, 2, 4]);

        // TOPIC+ordered: global source-timestamp order across buckets.
        let merged: Vec<i64> = datareader_cache
            .get_changes_for_topic_scoped_ordered_access()
            .iter()
            .map(|c| c.sequence_number().to_i64())
            .collect();
        assert_eq!(merged, vec![1, 2, 3, 4]);

        drop(datareader_cache);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    mod history_qos {
        use super::*;

        #[test]
        fn test_history_cache_creation() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy {
                    kind: HistoryQosPolicyKind::KeepLast(10),
                    strict: true,
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 50,
                    max_instances: 5,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 50);
            assert_eq!(datareader_cache.get_max_instances(), 5);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 10);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_keep_last() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(5), strict: true },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 20,
                    max_instances: 4,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 20);
            assert_eq!(datareader_cache.get_max_instances(), 4);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 5);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_keep_last_with_unlimited_resource_limits() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(5), strict: true },
                ..Default::default()
            };
            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 2147483647);
            assert_eq!(datareader_cache.get_max_instances(), 2147483647);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 5);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_keep_all() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 20,
                    max_instances: 4,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 20);
            assert_eq!(datareader_cache.get_max_instances(), 4);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 10);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }
    }

    mod resource_limits_qos {
        use super::*;

        #[test]
        fn test_history_cache_no_key_instance_limits() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 20,
                    max_instances: 4,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let (participant, data_reader) = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 10);
            assert_eq!(datareader_cache.get_max_instances(), 1);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 10);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_success_no_key_max_samples_exceeded() {
            // can_auto_remove = true
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(3), strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3,
                    max_instances: 1,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            }; // now max_samples is 3

            let (participant, data_reader) = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // Add 3 changes for instance 1
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(1, InstanceHandle::NIL), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(2, InstanceHandle::NIL), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(3, InstanceHandle::NIL), false)
                .is_ok());

            // Add 1 change for instance 2
            let result = datareader_cache
                .add_change_with_cleanup(create_change_no_key(4, InstanceHandle::NIL), false);
            assert!(result.unwrap().0.unwrap().sequence_number().to_i64() == 1);

            let changes = datareader_cache.get_changes();
            assert!(changes.len() == 3);
            assert!(changes.get(0).unwrap().sequence_number().to_i64() == 2);
            assert!(changes.get(1).unwrap().sequence_number().to_i64() == 3);
            assert!(changes.get(2).unwrap().sequence_number().to_i64() == 4);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_fail_no_key_max_samples_exceeded() {
            // KeepAll + Reliable = can_auto_remove = false
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3,
                    max_instances: 1,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            }; // now max_samples is 3

            let (participant, data_reader) = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // Add 3 changes for instance 1
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(1, InstanceHandle::NIL), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(2, InstanceHandle::NIL), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(3, InstanceHandle::NIL), false)
                .is_ok());

            // Add 1 change for instance 2
            let result = datareader_cache
                .add_change_with_cleanup(create_change_no_key(4, InstanceHandle::NIL), false);
            assert!(matches!(result, Err(DdsError::OutOfResources)));

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_success_with_key_max_samples_per_instance_exceeded() {
            // KeepLast + Reliable = can_auto_remove = true
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(2), strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 10,
                    max_instances: 10,
                    max_samples_per_instance: 2,
                },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            assert!(datareader_cache.can_auto_remove);

            let instance_a = InstanceHandle::new([1; 16]);

            // Add 2 to instance A (max_samples_per_instance reached)
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a), false)
                .is_ok());

            // 3rd addition should succeed with auto_remove
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_a), false)
                .is_ok());

            let changes = datareader_cache.get_changes();
            assert_eq!(changes.len(), 2);
            // The oldest change (seq=1) should be removed, leaving only seq=2,3
            assert_eq!(changes.get(0).unwrap().sequence_number().to_i64(), 2);
            assert_eq!(changes.get(1).unwrap().sequence_number().to_i64(), 3);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_fail_with_key_max_samples_per_instance_exceeded() {
            // KeepAll + Reliable = can_auto_remove = false
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 10,
                    max_instances: 10,
                    max_samples_per_instance: 2,
                },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);

            // Add 2 to instance A
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a), false)
                .is_ok());

            // 3rd addition should fail because auto_remove is disabled
            let result = datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_a), false);
            assert!(matches!(result, Err(DdsError::OutOfResources)));

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_success_with_key_max_instances_exceeded() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(3), strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 10,
                    max_instances: 2, // Maximum 2 instances
                    max_samples_per_instance: 5,
                },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);
            let instance_c = InstanceHandle::new([3; 16]);

            let writer_1 = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            // Add instance A, B (max_instances reached)
            let change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_a, // different instance
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            let change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_b, // different instance
                SequenceNumber::from_i64(2),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(change_a.clone()), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(change_b.clone()), false)
                .is_ok());

            let change_c = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_c, // different instance
                SequenceNumber::from_i64(3),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            // Currently adding instance C should result in an error
            let result =
                datareader_cache.add_change_with_cleanup(Arc::new(change_c.clone()), false);
            assert!(matches!(result, Err(DdsError::OutOfResources)));

            // Make instance A enter NOT_ALIVE_NO_WRITERS state
            let change_a_unregister = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_1.clone(),
                instance_a,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            // Writer A unregisters instance A
            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(change_a_unregister.clone()), false)
                .is_ok());

            // Data reader took all the samples from instance A
            assert!(datareader_cache.remove_change(Arc::new(change_a)).is_ok());
            assert!(datareader_cache.remove_change(Arc::new(change_a_unregister)).is_ok());

            // Now adding instance C should succeed via remove_unused_instance
            assert!(datareader_cache.add_change_with_cleanup(Arc::new(change_c), false).is_ok());

            let changes = datareader_cache.get_changes();
            assert_eq!(changes.len(), 2);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_success_with_key_max_samples_exceeded() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(3), strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3, // Maximum 3 samples total
                    max_instances: 2,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            // Add 3 samples total (max_samples reached)
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_b), false)
                .is_ok());

            // 4th addition should succeed via try_remove_oldest_change_of_all
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(4, instance_b), false)
                .is_ok());

            let changes = datareader_cache.get_changes();
            assert_eq!(changes.len(), 3);
            // The oldest change (seq=1) should be removed, leaving only seq=2,3,4
            assert_eq!(changes.get(0).unwrap().sequence_number().to_i64(), 2);
            assert_eq!(changes.get(1).unwrap().sequence_number().to_i64(), 3);
            assert_eq!(changes.get(2).unwrap().sequence_number().to_i64(), 4);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_add_change_fail_with_key_max_samples_exceeded() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3, // Maximum 3 samples total
                    max_instances: 2,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            // Add 3 samples total
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a), false)
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_b), false)
                .is_ok());

            // 4th addition should fail because auto_remove is disabled
            let result = datareader_cache
                .add_change_with_cleanup(create_change_with_key(4, instance_b), false);
            assert!(matches!(result, Err(DdsError::OutOfResources)));

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }
    }

    mod ownership_qos {
        use super::*;

        #[test]
        fn test_history_cache_owner_set_upon_add_change_with_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            // This writer will publish both instances
            let writer_1 = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_a, // different instance
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_b, // different instance
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change_a.set_ownership_strength(Some(0));
            change_b.set_ownership_strength(Some(0));

            // Add owner candidates
            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // Check ownership
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();

            assert!(is_owner_a);
            assert!(is_owner_b);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_owner_set_upon_add_change_no_key() {}

        #[test]
        fn test_history_cache_everyone_is_owner_when_shared() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Shared },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_handle = InstanceHandle::new([1; 16]);

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change_a.set_ownership_strength(Some(0));
            change_b.set_ownership_strength(Some(0));

            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // Check ownership
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();

            assert!(is_owner_a);
            assert!(is_owner_b);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_owner_change_when_stronger_candidate_added_with_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_handle = InstanceHandle::new([1; 16]);

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // A is no longer the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_owner_change_when_stronger_candidate_added_no_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let instance_handle = InstanceHandle::NIL;

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 0, 0, 0, 0, 52, 0, 0, 0, 72, 101, 108, 108, 111, 87, 111, 114, 108,
                    100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 0, 0, 0, 0, 52, 0, 0, 0, 72, 101, 108, 108, 111, 87, 111, 114, 108,
                    100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                ],
                Some(RtpsTime::now()),
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // A is no longer the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_weaker_candidate_fail_to_add_change_with_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let instance_handle = InstanceHandle::NIL;

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![1, 2, 3],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![1, 2, 3],
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A cannot add change again
            let mut change_a_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );
            change_a_2.set_ownership_strength(Some(20));

            let res = datareader_cache.add_change_with_cleanup(Arc::new(change_a_2), false);
            assert!(matches!(res, Err(DdsError::IllegalOperation)));

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_weaker_candidate_fail_to_add_change_no_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let instance_handle = InstanceHandle::NIL;

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                InstanceHandle::NIL,
                SequenceNumber::from_i64(1),
                vec![1, 2, 3],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                InstanceHandle::NIL,
                SequenceNumber::from_i64(1),
                vec![1, 2, 3],
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A cannot add change again
            let mut change_a_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );
            change_a_2.set_ownership_strength(Some(20));

            let res = datareader_cache.add_change_with_cleanup(Arc::new(change_a_2), false);
            assert!(matches!(res, Err(DdsError::IllegalOperation)));

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_disposed_when_owner() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(change.clone()), false).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Owner disposes the instance
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![],
                None,
            );

            change_disposed.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(change_disposed.clone()), false)
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_not_disposed_when_not_owner() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // B is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A cannot add change again
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![],
                None,
            );
            change_disposed.set_ownership_strength(Some(20));

            // Non owner cannot dispose the instance
            let res = datareader_cache.add_change_with_cleanup(Arc::new(change_disposed), false);
            assert!(matches!(res, Err(DdsError::IllegalOperation)));

            // Verify instance state is still ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_alive_again_when_owner_writes() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(change.clone()), false).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Owner disposes the instance
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );

            change_disposed.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(change_disposed.clone()), false)
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            let mut change_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(3),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change_2.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(change_2.clone()), false).unwrap();

            // Verify instance state is ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_still_not_alive_when_non_owner_writes() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(change.clone()), false).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Owner disposes the instance
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );

            change_disposed.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(change_disposed.clone()), false)
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            // This is the weaker writer
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change_2.set_ownership_strength(Some(10));
            let res = datareader_cache.add_change_with_cleanup(Arc::new(change_2.clone()), false);
            assert!(res.is_err());

            // Verify instance state is still NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_anyone_can_dispose_and_reclaim_when_shared() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Shared },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(change.clone()), false).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Disposes the instance with lesser strength
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );

            change_disposed.set_ownership_strength(Some(10));
            datareader_cache
                .add_change_with_cleanup(Arc::new(change_disposed.clone()), false)
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            // This is the weaker writer
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            change_2.set_ownership_strength(Some(5));
            let res = datareader_cache.add_change_with_cleanup(Arc::new(change_2.clone()), false);
            assert!(res.is_ok());

            // Verify instance state is ALIVE now
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_ownership_changes_when_owner_unregister() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // B is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer B now unregister
            let mut change_unregistered = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );
            change_unregistered.set_ownership_strength(Some(30));

            let res =
                datareader_cache.add_change_with_cleanup(Arc::new(change_unregistered), false);
            assert!(res.is_ok());

            // Now writer A is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            // Verify instance state is still ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_history_cache_no_writers_when_everyone_unregister() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ],
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_a.clone()), false).unwrap();

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache.add_change_with_cleanup(Arc::new(change_b.clone()), false).unwrap();

            // B is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A, B all unregister
            let mut change_unregistered_a = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );

            let mut change_unregistered_b = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );
            change_unregistered_a.set_ownership_strength(Some(20));
            change_unregistered_b.set_ownership_strength(Some(30));

            let res = datareader_cache
                .add_change_with_cleanup(Arc::new(change_unregistered_a.clone()), false);
            let res_b = datareader_cache
                .add_change_with_cleanup(Arc::new(change_unregistered_b.clone()), false);

            assert!(res.is_ok());
            assert!(res_b.is_ok());

            // Now take all samples
            datareader_cache.remove_change(Arc::new(change_a)).unwrap();
            datareader_cache.remove_change(Arc::new(change_b)).unwrap();
            datareader_cache.remove_change(Arc::new(change_unregistered_a)).unwrap();
            datareader_cache.remove_change(Arc::new(change_unregistered_b)).unwrap();

            // Verify instance state is NOT_ALIVE_NO_WRITERS
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_unregister_one_instance_keeps_other_instances_alive() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let (participant, data_reader) = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            let data = vec![
                0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0, 20,
                0, 0, 0, 0, 0, 0, 0,
            ];

            // Same writer owns both instances
            let mut alive_a = CacheChange::new(
                ChangeKind::Alive,
                writer.clone(),
                instance_a,
                SequenceNumber::from_i64(1),
                data.clone(),
                None,
            );
            alive_a.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(alive_a), false).unwrap();

            let mut alive_b = CacheChange::new(
                ChangeKind::Alive,
                writer.clone(),
                instance_b,
                SequenceNumber::from_i64(2),
                data.clone(),
                None,
            );
            alive_b.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(alive_b), false).unwrap();

            // Writer unregisters only instance A
            let mut unregister_a = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer.clone(),
                instance_a,
                SequenceNumber::from_i64(3),
                vec![],
                None,
            );
            unregister_a.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(unregister_a), false).unwrap();

            // A loses its only writer; B is untouched and stays ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            assert_eq!(
                instance_info.get(&instance_a).unwrap().instance_state,
                InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE
            );
            assert_eq!(
                instance_info.get(&instance_b).unwrap().instance_state,
                InstanceStateKind::ALIVE_INSTANCE_STATE
            );

            drop(datareader_cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }

        #[test]
        fn test_dispose_twice_keeps_both_samples_in_cache() {
            let (participant, data_reader) = create_with_key_datareader(DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
                ..Default::default()
            });
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut cache = datareader_cache.lock().unwrap();

            let writer = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let instance = InstanceHandle::new([1; 16]);
            let data = vec![
                0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0, 20,
                0, 0, 0, 0, 0, 0, 0,
            ];

            let alive = CacheChange::new(
                ChangeKind::Alive,
                writer.clone(),
                instance,
                SequenceNumber::from_i64(1),
                data,
                None,
            );
            cache.add_change_with_cleanup(Arc::new(alive), false).unwrap();

            // The remote disposes the same instance twice; the second moves no state.
            let dispose1 = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer.clone(),
                instance,
                SequenceNumber::from_i64(2),
                vec![],
                None,
            );
            cache.add_change_with_cleanup(Arc::new(dispose1), false).unwrap();

            let dispose2 = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer.clone(),
                instance,
                SequenceNumber::from_i64(3),
                vec![],
                None,
            );
            cache.add_change_with_cleanup(Arc::new(dispose2), false).unwrap();

            // KeepAll history: both dispose samples must be retained.
            let disposed_in_cache = cache
                .get_changes()
                .iter()
                .filter(|c| {
                    c.kind() == ChangeKind::NotAliveDisposed && c.instance_handle() == instance
                })
                .count();
            assert_eq!(disposed_in_cache, 2);

            drop(cache);
            participant.delete_contained_entities().unwrap();
            DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
        }
    }
}
