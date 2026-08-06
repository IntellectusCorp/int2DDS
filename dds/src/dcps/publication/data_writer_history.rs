//! DataWriter history cache implementation.
//!
//! This module implements the history cache for `DataWriter`, managing the storage and
//! lifecycle of written samples according to History and ResourceLimits QoS policies.
//!
//! The writer history cache handles:
//! - Per-instance sample storage
//! - KEEP_LAST / KEEP_ALL history policies
//! - Resource limit enforcement (max_samples, max_instances, max_samples_per_instance)
//! - Sample lifecycle for reliability protocol
//! - Blocking behavior for reliability max_blocking_time

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, Weak,
    },
    thread,
    time::Instant,
};

use ::log::debug;

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
            DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
            ReliabilityQosPolicy, ReliabilityQosPolicyKind, ResourceLimitsQosPolicy,
        },
    },
    publication::data_writer::DataWriter,
    rtps::{
        common::{guid::Guid, sequence::SequenceNumber, time::RtpsTime},
        entities::{
            history::{
                cache_change::CacheChange, cache_change_pool::CacheChangePool,
                history_cache::HistoryCache as rtps_history_cache,
            },
            writer::{StatefulWriter, Writer},
        },
    },
    utils::timer::timer_id::TimerId,
};

static WRITER_HISTORY_PROFILE_COUNT: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_LIFESPAN_US: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_ENSURE_US: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_RELEASE_US: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_PUSH_US: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_INSTANCE_US: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_RTPS_US: AtomicU64 = AtomicU64::new(0);
static WRITER_HISTORY_PROFILE_TOTAL_US: AtomicU64 = AtomicU64::new(0);

fn writer_history_profile_enabled() -> bool {
    std::env::var_os("RMW_INT2DDS_PROFILE").is_some()
}

fn elapsed_us(start: Instant, end: Instant) -> u64 {
    end.duration_since(start).as_micros() as u64
}

fn record_writer_history_profile(
    lifespan_us: u64,
    ensure_us: u64,
    release_us: u64,
    push_us: u64,
    instance_us: u64,
    rtps_us: u64,
    total_us: u64,
) {
    let n = WRITER_HISTORY_PROFILE_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    WRITER_HISTORY_PROFILE_LIFESPAN_US.fetch_add(lifespan_us, Ordering::Relaxed);
    WRITER_HISTORY_PROFILE_ENSURE_US.fetch_add(ensure_us, Ordering::Relaxed);
    WRITER_HISTORY_PROFILE_RELEASE_US.fetch_add(release_us, Ordering::Relaxed);
    WRITER_HISTORY_PROFILE_PUSH_US.fetch_add(push_us, Ordering::Relaxed);
    WRITER_HISTORY_PROFILE_INSTANCE_US.fetch_add(instance_us, Ordering::Relaxed);
    WRITER_HISTORY_PROFILE_RTPS_US.fetch_add(rtps_us, Ordering::Relaxed);
    WRITER_HISTORY_PROFILE_TOTAL_US.fetch_add(total_us, Ordering::Relaxed);

    if n % 300 == 0 {
        let divisor = n as f64;
        eprintln!(
            "INT2DDS_WRITER_HISTORY_PROFILE count={} total_avg_us={:.3} lifespan_avg_us={:.3} ensure_avg_us={:.3} release_avg_us={:.3} push_avg_us={:.3} instance_avg_us={:.3} rtps_avg_us={:.3}",
            n,
            WRITER_HISTORY_PROFILE_TOTAL_US.load(Ordering::Relaxed) as f64 / divisor,
            WRITER_HISTORY_PROFILE_LIFESPAN_US.load(Ordering::Relaxed) as f64 / divisor,
            WRITER_HISTORY_PROFILE_ENSURE_US.load(Ordering::Relaxed) as f64 / divisor,
            WRITER_HISTORY_PROFILE_RELEASE_US.load(Ordering::Relaxed) as f64 / divisor,
            WRITER_HISTORY_PROFILE_PUSH_US.load(Ordering::Relaxed) as f64 / divisor,
            WRITER_HISTORY_PROFILE_INSTANCE_US.load(Ordering::Relaxed) as f64 / divisor,
            WRITER_HISTORY_PROFILE_RTPS_US.load(Ordering::Relaxed) as f64 / divisor,
        );
    }
}

#[derive(Debug)]
pub(crate) struct DataWriterHistoryCache<Foo> {
    data_writer: Weak<DataWriter<Foo>>,
    rtps_writer: Option<Weak<dyn Writer + Send + Sync>>,
    max_samples: i32,
    max_instances: i32,
    max_samples_per_instance: i32,
    changes: Vec<Arc<CacheChange>>,
    instance_map: Arc<Mutex<HashMap<InstanceHandle, Vec<Weak<CacheChange>>>>>, // NoKey Writer shall not use this
    is_keep_all: bool,
    is_reliable: bool,
    // Sent samples are not retained: changes go straight to the RTPS transmit queue
    // and this cache stores nothing
    purge_sent_changes: bool,
    max_blocking_time: Duration,
    has_key: bool,
    lifespan_timers: Arc<Mutex<HashMap<Guid, TimerId>>>, // writer_guid -> timer_id
    pool: CacheChangePool,
}

impl<Foo: 'static + Clone> HistoryCache for DataWriterHistoryCache<Foo> {
    // Returns a reference to the list of CacheChanges.
    fn get_changes(&self) -> Vec<Arc<CacheChange>> {
        self.changes.clone()
    }

    fn insert_change_sorted(&mut self, change: Arc<CacheChange>) {
        let change_ts =
            change.source_timestamp().or(change.reception_timestamp()).unwrap_or(RtpsTime::ZERO);
        let pos = self
            .changes
            .binary_search_by_key(&change_ts, |c| {
                c.source_timestamp().or(c.reception_timestamp()).unwrap_or(RtpsTime::ZERO)
            })
            .unwrap_or_else(|pos| pos);
        self.changes.insert(pos, change);
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
        Ok(self.changes.len())
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

    // Returns the map of lifespan timers keyed by writer GUID.
    fn get_lifespan_timers(&self) -> Arc<Mutex<HashMap<Guid, TimerId>>> {
        self.lifespan_timers.clone()
    }

    // Adds the given CacheChange to the history vector and map.
    // Always returns Ok(None) — evicted changes are released directly to the pool
    // rather than returned to the caller.
    fn add_change_with_cleanup(
        &mut self,
        a_change: Arc<CacheChange>,
        _apply_filter: bool, // writer side never applies TIME_BASED_FILTER
    ) -> DdsResult<(Option<Arc<CacheChange>>, bool)> {
        // If purge_sent_changes, deliver to the RTPS cache,
        // then drop from history and return the buffer to the pool.
        if self.purge_sent_changes {
            let seq = a_change.sequence_number().to_i64();
            // RTPS add_change delivers synchronously to all matched reader locators
            self.add_change_to_rtps_writer_cache(a_change.clone())?;
            // Delivery is done: drop it from the RTPS cache and reclaim the buffer
            self.remove_change_from_rtps_writer_cache(a_change.clone())?;
            self.pool.try_release(a_change);
            debug!(
                "[history-strict][best-effort] purge seq={:?} dds_len={} rtps_len={}",
                seq,
                self.changes.len(),
                self.rtps_cache_len()
            );
            return Ok((None, false));
        }

        let profile = writer_history_profile_enabled();
        let total_t0 = Instant::now();
        let lifespan_t0 = Instant::now();
        // Set lifespan timer if lifespan qos is configured
        let lifespan_duration = self.data_writer.upgrade().and_then(|data_writer| {
            data_writer.get_qos_arc().ok().and_then(|qos| {
                if !qos.lifespan.duration.is_infinite() {
                    Some(qos.lifespan.duration)
                } else {
                    None
                }
            })
        });

        // Create timer only if it doesn't exist
        if let Some(duration) = lifespan_duration {
            let writer_guid = a_change.writer_guid();
            let timers = self.get_lifespan_timers();
            let timers_guard = timers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            let timer_exists = timers_guard.contains_key(&writer_guid);
            drop(timers_guard);

            if !timer_exists {
                if let Err(e) = self.register_lifespan_timer(
                    writer_guid,
                    duration,
                    TimerId::LifespanWriter { writer_guid },
                ) {
                    debug!("Failed to ensure lifespan timer: {:?}", e);
                }
            }
        }
        // end lifespan
        let lifespan_us = if profile { elapsed_us(lifespan_t0, Instant::now()) } else { 0 };

        let ensure_t0 = Instant::now();
        let removed = self.ensure_capacity(a_change.instance_handle())?;
        let ensure_us = if profile { elapsed_us(ensure_t0, Instant::now()) } else { 0 };

        // Release evicted change back to pool for buffer reuse.
        // Consume the Arc by value so Arc::try_unwrap succeeds (refcount == 1).
        let release_t0 = Instant::now();
        if let Some(evicted) = removed {
            self.pool.try_release(evicted);
        }
        let release_us = if profile { elapsed_us(release_t0, Instant::now()) } else { 0 };

        let push_t0 = Instant::now();
        if lifespan_duration.is_some() {
            self.insert_change_sorted(a_change.clone());
        } else {
            self.changes.push(a_change.clone());
        }
        let push_us = if profile { elapsed_us(push_t0, Instant::now()) } else { 0 };

        let instance_t0 = Instant::now();
        if self.has_key {
            self.add_change_to_instance_map(a_change.clone())?;
        }
        let instance_us = if profile { elapsed_us(instance_t0, Instant::now()) } else { 0 };

        let rtps_t0 = Instant::now();
        self.add_change_to_rtps_writer_cache(a_change)?;
        let rtps_us = if profile { elapsed_us(rtps_t0, Instant::now()) } else { 0 };

        debug!("add_change_with_cleanup completed");

        if profile {
            record_writer_history_profile(
                lifespan_us,
                ensure_us,
                release_us,
                push_us,
                instance_us,
                rtps_us,
                elapsed_us(total_t0, Instant::now()),
            );
        }

        // Writer-side callers never use the removed value
        // unlike DataReaderHistoryCache, which needs it to sync the RTPS history
        Ok((None, false))
    }

    // Removes the given CacheChange from the history vector and map.
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        self.changes.retain(|c| !Arc::ptr_eq(c, &a_change));
        self.remove_change_from_instance_map(&a_change)?;
        self.remove_change_from_rtps_writer_cache(a_change)?;
        Ok(())
    }

    // Ensures capacity before adding a new CacheChange to the history.
    // Returns Ok(None) if space is already available, or Ok(Some(CacheChange)) if space was secured after removing an existing CacheChange.
    // Returns Err(DdsError::OutOfResources) if space cannot be secured, in which case the Sample is rejected.
    fn ensure_capacity(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        // The order of condition checks below must not be changed.

        if self.has_key && self.is_max_instances_exceeded(instance_handle)? {
            // User needs to unregister instance.
            return Err(DdsError::OutOfResources);
        }

        if self.is_max_samples_per_instance_exceeded(instance_handle)? {
            let removed = self.try_remove_oldest_change_of_instance(instance_handle)?;
            return Ok(Some(removed));
        }

        if self.is_max_samples_exceeded()? {
            let removed = self.try_remove_oldest_change_of_all()?;
            return Ok(Some(removed));
        }

        Ok(None)
    }

    // Removes the oldest change from all instances.
    fn try_remove_oldest_change_of_all(&mut self) -> DdsResult<Arc<CacheChange>> {
        if self.is_reliable {
            self.remove_oldest_change_of_all_reliable()
        } else {
            self.remove_oldest_change_of_all()
        }
    }

    // Removes the oldest change from the given instance.
    fn try_remove_oldest_change_of_instance(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Arc<CacheChange>> {
        if self.is_reliable && self.has_key {
            self.remove_oldest_change_of_instance_reliable(instance_handle)
        } else {
            self.remove_oldest_change_of_all()
        }
    }

    // Registers a periodic timer that removes expired samples based on Lifespan QoS.
    fn register_lifespan_timer(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id: TimerId,
    ) -> DdsResult<()> {
        let data_writer_weak = if let Some(data_writer) = self.data_writer.upgrade() {
            Arc::downgrade(&data_writer)
        } else {
            return Err(DdsError::Error("DataWriter has been dropped".to_string()));
        };

        let lifespan_duration_clone = lifespan_duration;
        let writer_guid_clone = writer_guid;

        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(data_writer) = data_writer_weak.upgrade() {
                if let Ok(cache_arc) = data_writer.get_datawriter_cache() {
                    if let Ok(mut cache_guard) = cache_arc.lock() {
                        if let Err(e) = cache_guard.remove_lifespan_expired_changes(
                            writer_guid_clone,
                            lifespan_duration_clone,
                        ) {
                            debug!("Failed to check lifespan samples: {:?}", e);
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
}

impl<Foo: 'static + Clone> DataWriterHistoryCache<Foo> {
    pub(crate) fn new(
        data_writer: Weak<DataWriter<Foo>>,
        reliability_qos: ReliabilityQosPolicy,
        durability_qos: DurabilityQosPolicy,
        history_qos: HistoryQosPolicy,
        resource_limits_qos: ResourceLimitsQosPolicy,
        has_key: bool,
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
            // For KEEP_LAST, depth determines the maximum number of data items to maintain per instance, and previous values are discarded.
            HistoryQosPolicyKind::KeepLast(depth) => depth,
            // For KEEP_ALL, available resources are limited by RESOURCE_LIMITS QoS.
            HistoryQosPolicyKind::KeepAll => cap(resource_limits_qos.max_samples_per_instance),
        };

        let max_instances = if has_key { cap(resource_limits_qos.max_instances) } else { 1 };
        let max_samples = cap(resource_limits_qos.max_samples);

        let max_samples = i32::min(
            max_samples,
            max_instances.saturating_mul(max_samples_per_instance), // Prevent overflow
        );

        debug!(
            "Creating data writer's history cache with max_samples_per_instance: {:?}",
            max_samples_per_instance
        );
        debug!("Creating data writer's history cache with max_instances: {:?}", max_instances);
        debug!("Creating data writer's history cache with max_samples: {:?}", max_samples);

        Self {
            data_writer,
            rtps_writer: None,
            max_samples,
            max_instances,
            max_samples_per_instance,
            changes: Vec::new(),
            instance_map: Arc::new(Mutex::new(HashMap::new())),
            is_keep_all: history_qos.kind == HistoryQosPolicyKind::KeepAll,
            is_reliable: reliability_qos.kind == ReliabilityQosPolicyKind::Reliable,
            purge_sent_changes: reliability_qos.kind == ReliabilityQosPolicyKind::BestEffort
                && durability_qos.kind == DurabilityQosPolicyKind::Volatile
                && history_qos.kind == HistoryQosPolicyKind::KeepAll
                && !history_qos.strict,
            max_blocking_time: reliability_qos.max_blocking_time,
            has_key,
            lifespan_timers: Arc::new(Mutex::new(HashMap::new())),
            pool: {
                let pool_size = match history_qos.kind {
                    HistoryQosPolicyKind::KeepLast(depth) => {
                        if has_key {
                            // Instance count unknown at creation — pre-allocate for 1 instance
                            depth as usize
                        } else {
                            depth as usize
                        }
                    }
                    HistoryQosPolicyKind::KeepAll => 32,
                };
                CacheChangePool::with_capacity(pool_size)
            },
        }
    }

    /// Acquire a CacheChange from the pool (capacity preserved from previous use).
    pub(crate) fn acquire_change(&mut self) -> CacheChange {
        self.pool.acquire()
    }

    // current number of pooled (free) changes
    #[cfg(test)]
    pub(crate) fn pool_len(&self) -> usize {
        self.pool.len()
    }

    // current number of stored changes
    pub(crate) fn changes_len(&self) -> usize {
        self.changes.len()
    }

    // Must be called immediately after DataWriterHistoryCache creation.
    pub(crate) fn set_datawriter(&mut self, data_writer: Weak<DataWriter<Foo>>) {
        self.data_writer = data_writer;
    }

    // Sets the RTPS writer reference.
    pub(crate) fn set_rtpswriter(&mut self, rtps_writer: Option<Weak<dyn Writer + Send + Sync>>) {
        self.rtps_writer = rtps_writer;
    }

    pub(crate) fn register_instance(&mut self, instance_handle: InstanceHandle) -> DdsResult<()> {
        if self.is_max_instances_exceeded(instance_handle)? {
            return Err(DdsError::OutOfResources);
        }

        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        instance_map.entry(instance_handle).or_insert_with(Vec::new);

        Ok(())
    }

    // Removes the oldest change from all instances in reliable mode.
    fn remove_oldest_change_of_all_reliable(&mut self) -> DdsResult<Arc<CacheChange>> {
        // Get removable changes based on instance conditions
        let removable_changes = self.get_removable_changes()?;

        // If all instances have 1 or fewer samples, return immediately as waiting won't create space or result in a removable state
        if self.has_key && removable_changes.is_empty() {
            return Err(DdsError::OutOfResources);
        }

        // For KEEP_ALL, block; if resources are not freed after blocking, return OUT_OF_RESOURCE
        if self.is_keep_all {
            self.remove_first_acked_change_or_block(&removable_changes)
        }
        // For KEEP_LAST, remove the oldest change and invoke callback only if is still unacknowledged
        else {
            let removed = self.remove_oldest_change_of_all()?;
            self.trigger_status_if_unacked(&removed)?;
            Ok(removed)
        }
    }

    // Removes the oldest change from the given instance in reliable mode.
    fn remove_oldest_change_of_instance_reliable(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Arc<CacheChange>> {
        // For KEEP_ALL, block; if resources are not freed after blocking, return OUT_OF_RESOURCE
        if self.is_keep_all {
            let changes = self.get_changes_of_instance(instance_handle)?;
            self.remove_first_acked_change_or_block(&changes)
        }
        // For KEEP_LAST, remove the oldest change and invoke callback only if is still unacknowledged
        else {
            let to_remove = self.get_oldest_change_of_instance(instance_handle)?;
            if let Some(change) = to_remove {
                self.trigger_status_if_unacked(&change)?;
                self.remove_change(change.clone())?;
                Ok(change)
            } else {
                Err(DdsError::Error("No changes found in instance to remove".to_string()))
            }
        }
    }

    // Removes the first acknowledged change, or blocks for max_blocking_time if none exists.
    fn remove_first_acked_change_or_block(
        &mut self,
        changes: &Vec<Arc<CacheChange>>,
    ) -> DdsResult<Arc<CacheChange>> {
        let start_time = std::time::Instant::now();

        loop {
            let oldest_acked_change = self.get_first_acked_change_from_vec(changes)?;

            match oldest_acked_change {
                // Remove the first acknowledged change and exit
                Some(change) => {
                    self.remove_change(change.clone())?;
                    return Ok(change);
                }

                // If no acknowledged change exists, repeat checking for max_blocking_time duration
                None => {
                    let elapsed = start_time.elapsed();
                    let elapsed_duration =
                        Duration::new(elapsed.as_secs() as i32, elapsed.subsec_nanos());

                    if elapsed_duration >= self.max_blocking_time {
                        return Err(DdsError::OutOfResources);
                    }

                    // Avoid busy waiting if max_blocking_time is significant.
                    if self.max_blocking_time > Duration::from_nanos(1_000_000) {
                        thread::sleep(std::time::Duration::from_micros(100));
                    }
                }
            }
        }
    }

    // Removes every change with a sequence number at or below the given one.
    pub(crate) fn remove_changes_acked_up_to(
        &mut self,
        max_acked: SequenceNumber,
    ) -> DdsResult<()> {
        let before = self.changes.len();
        let acked: Vec<_> =
            self.changes.iter().filter(|c| c.sequence_number() <= max_acked).cloned().collect();
        let removing = acked.len();
        for change in acked {
            self.remove_change(change.clone())?;
            self.pool.try_release(change);
        }
        debug!(
            "[history-strict] ack up to {} before={} removing={} after={} rtps_len={}",
            max_acked.to_i64(),
            before,
            removing,
            self.changes.len(),
            self.rtps_cache_len()
        );
        Ok(())
    }

    // Removes and returns the oldest change from all instances.
    fn remove_oldest_change_of_all(&mut self) -> DdsResult<Arc<CacheChange>> {
        // The oldest change is always at index 0. Read it directly instead of
        // snapshotting the whole change list.
        let oldest_change = self.changes.first().cloned();
        match oldest_change {
            Some(change) => {
                self.remove_change(change.clone())?;
                Ok(change)
            }
            None => Err(DdsError::Error("No changes found to remove".to_string())),
        }
    }

    // Returns the oldest acknowledged change from the given change vector.
    #[allow(clippy::ptr_arg)]
    fn get_first_acked_change_from_vec(
        &self,
        changes: &Vec<Arc<CacheChange>>,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        let rtps_writer = self.get_upgraded_rtps_writer()?;
        let stateful_writer = rtps_writer
            .as_any()
            .downcast_ref::<StatefulWriter>()
            .ok_or_else(|| DdsError::Error("Failed to downcast to StatefulWriter".to_string()))?;

        for change in changes.iter() {
            if stateful_writer.is_change_acked_by_all(change.sequence_number()) {
                return Ok(Some(change.clone()));
            }
        }
        Ok(None)
    }

    // Triggers unacked_sample_removed state if the given change is in unacked state.
    fn trigger_status_if_unacked(&self, change: &Arc<CacheChange>) -> DdsResult<()> {
        if self.get_first_acked_change_from_vec(&vec![change.clone()])?.is_none() {
            // TODO: trigger unacked_sample_removed status
            debug!(
                "Unacked sample removed for sequence number {}",
                change.sequence_number().to_i64()
            );
        }
        Ok(())
    }

    // Returns the change with the smallest sequence number from the given instance.
    fn get_oldest_change_of_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        // For no-key writer, instance map is not used.
        if instance_handle == InstanceHandle::NIL {
            return Err(DdsError::BadParameter);
        }

        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        if let Some(changes) = instance_map.get(&instance_handle) {
            let min_change = changes
                .iter()
                .filter_map(|weak_change| weak_change.upgrade())
                .min_by_key(|c| c.sequence_number());
            Ok(min_change)
        } else {
            Err(DdsError::Error("instance_map does not contain the instance_handle".to_string()))
        }
    }

    // Returns all changes for the given instance.
    fn get_changes_of_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Vec<Arc<CacheChange>>> {
        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        if let Some(weak_changes) = instance_map.get(&instance_handle) {
            let strong_changes: Vec<Arc<CacheChange>> =
                weak_changes.iter().filter_map(|weak_change| weak_change.upgrade()).collect();
            Ok(strong_changes)
        } else {
            Err(DdsError::Error("instance_map does not contain the instance_handle".to_string()))
        }
    }

    // Upgrades the weak reference to rtps_writer to a strong reference.
    fn get_upgraded_rtps_writer(&self) -> DdsResult<Arc<dyn Writer + Send + Sync>> {
        self.rtps_writer
            .as_ref()
            .ok_or_else(|| DdsError::Error("Failed to upgrade rtps writer weak".to_string()))?
            .upgrade()
            .ok_or_else(|| DdsError::Error("Failed to upgrade rtps writer weak".to_string()))
    }

    // Checks the following conditions and returns changes that can be removed.
    // 2.2.2.4.2.11 Service is allowed to discard samples of some other instance as long as at least one sample remains for such an instance.
    fn get_removable_changes(&self) -> DdsResult<Vec<Arc<CacheChange>>> {
        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let mut removable_changes = Vec::new();
        for (_, changes) in instance_map.iter() {
            if changes.len() > 1 {
                removable_changes
                    .extend(changes.iter().filter_map(|weak_change| weak_change.upgrade()));
            }
        }

        Ok(removable_changes)
    }

    // Adds CacheChange to the instance map.
    fn add_change_to_instance_map(&self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        if !self.has_key {
            return Ok(());
        }
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        instance_map
            .entry(a_change.instance_handle())
            .or_insert_with(Vec::new)
            .push(Arc::downgrade(&a_change));
        Ok(())
    }

    // Removes CacheChange from the instance map.
    fn remove_change_from_instance_map(&self, a_change: &Arc<CacheChange>) -> DdsResult<()> {
        if !self.has_key {
            return Ok(());
        }
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(changes) = instance_map.get_mut(&a_change.instance_handle()) {
            changes.retain(|weak_change| {
                if let Some(strong_change) = weak_change.upgrade() {
                    !Arc::ptr_eq(&strong_change, a_change)
                } else {
                    false
                }
            });
        }
        Ok(())
    }

    // Adds CacheChange to the corresponding RTPS writer cache.
    fn add_change_to_rtps_writer_cache(&self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        if let Some(rtps_writer) = self
            .rtps_writer
            .as_ref()
            .ok_or_else(|| DdsError::Error("Failed to upgrade rtps writer weak".to_string()))?
            .upgrade()
        {
            let rtps_writer_cache = rtps_writer.writer_cache();
            let cache_binding = rtps_writer_cache.lock();
            if let Ok(mut cache_guard) = cache_binding {
                let res = cache_guard.add_change(a_change, &*rtps_writer);
                if res.is_ok() {
                    Ok(())
                } else {
                    Err(DdsError::Error(res.err().unwrap().to_string()))
                }
            } else {
                Err(DdsError::Error("Failed to lock rtps writer cache mutex".to_string()))
            }
        } else {
            Err(DdsError::Error("Failed to upgrade rtps writer weak".to_string()))
        }
    }

    // Debug string for the RTPS transmit queue length, naming why a count is
    // unavailable. Non-blocking so an observation never deadlocks.
    fn rtps_cache_len(&self) -> String {
        let Some(weak) = self.rtps_writer.as_ref() else {
            return "no-writer".to_string();
        };
        let Some(writer) = weak.upgrade() else {
            return "writer-dropped".to_string();
        };
        match writer.writer_cache().try_lock() {
            Ok(guard) => guard.len().to_string(),
            Err(std::sync::TryLockError::WouldBlock) => "busy".to_string(),
            Err(std::sync::TryLockError::Poisoned(_)) => "poisoned".to_string(),
        }
    }

    // Removes CacheChange from the corresponding RTPS writer cache.
    fn remove_change_from_rtps_writer_cache(&self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        if let Some(rtps_writer) = self
            .rtps_writer
            .as_ref()
            .ok_or_else(|| DdsError::Error("Failed to upgrade rtps writer weak".to_string()))?
            .upgrade()
        {
            let rtps_writer_cache = rtps_writer.writer_cache();
            let cache_binding = rtps_writer_cache.lock();
            if let Ok(mut cache_guard) = cache_binding {
                let res = cache_guard.remove_change(a_change);
                if res.is_ok() {
                    Ok(())
                } else {
                    Err(DdsError::Error(res.err().unwrap().to_string()))
                }
            } else {
                Err(DdsError::Error("Failed to lock rtps writer cache mutex".to_string()))
            }
        } else {
            Err(DdsError::Error("Failed to upgrade rtps writer weak".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::StatusMask,
        publication::qos::{DataWriterQos, PublisherQos},
        rtps::{
            common::{
                entity_id::EntityId, entity_kind::EntityKind, guid::Guid, sequence::SequenceNumber,
                types::ChangeKind,
            },
            entities::writer::reader_proxy::ReaderProxy,
        },
        subscription::{
            data_reader::tests::TestData,
            qos::{DataReaderQos, SubscriberQos},
        },
        topic::qos::TopicQos,
    };

    // Helper function to create a test CacheChange.
    // Payload is non-empty so the change is not classified as a coherent-set end marker.
    fn create_change(seq: i64, handle: InstanceHandle) -> Arc<CacheChange> {
        Arc::new(CacheChange::new(
            ChangeKind::Alive,
            Guid::UNKNOWN,
            handle,
            SequenceNumber::from_i64(seq),
            vec![1],
            None,
        ))
    }

    fn create_datawriter(
        datawriter_qos: DataWriterQos,
    ) -> (crate::domain::domain_participant::DomainParticipant, DataWriter<TestData>) {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "TestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let writer = publisher
            .create_datawriter::<TestData>(&topic, datawriter_qos, None, StatusMask::default())
            .unwrap();

        (domain_participant, writer)
    }

    #[test]
    fn test_history_cache_add_change_success_max_samples_per_instance_exceeded_reliable() {
        let writer_qos: DataWriterQos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(100),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2,
                max_instances: 1,
                max_samples_per_instance: 2,
            },
            ..Default::default()
        };

        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 1 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let mut cache_guard = datawriter_cache.lock().unwrap();

        // Adding 2 changes should succeed
        assert!(cache_guard
            .add_change_with_cleanup(create_change(1, InstanceHandle::new([1; 16])), false)
            .is_ok());
        assert!(cache_guard
            .add_change_with_cleanup(create_change(2, InstanceHandle::new([1; 16])), false)
            .is_ok());

        let datawriter_clone = data_writer.clone();

        // Ack change1 after 50ms in a separate thread
        let handle = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(50));

            let rtps_writer = datawriter_clone.get_rtps_writer().unwrap();
            let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
            let reader_proxies = stateful_writer.reader_proxies();
            let mut reader_proxies_guard = reader_proxies.lock().unwrap();
            let reader_proxy = reader_proxies_guard.get_mut(0).unwrap();

            reader_proxy.acked_changes_set(SequenceNumber::from_i64(1));
        });

        let result = cache_guard
            .add_change_with_cleanup(create_change(3, InstanceHandle::new([1; 16])), false);

        handle.join().unwrap();

        // Successshould
        assert!(result.is_ok());
        assert!(cache_guard.changes.len() == 2);

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_add_change_fail_max_samples_per_instance_exceeded_reliable() {
        let writer_qos: DataWriterQos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2,
                max_instances: 1,
                max_samples_per_instance: 2,
            },
            ..Default::default()
        };

        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let data_reader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            ..Default::default()
        };

        let subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &data_reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            subscription_builtin_topic_data,
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let mut cache_guard = datawriter_cache.lock().unwrap();

        // Adding 2 changes should succeed
        assert!(cache_guard
            .add_change_with_cleanup(create_change(1, InstanceHandle::new([1; 16])), false)
            .is_ok());
        assert!(cache_guard
            .add_change_with_cleanup(create_change(2, InstanceHandle::new([1; 16])), false)
            .is_ok());

        // Returns error after blocking time since no ACK processing
        let result = cache_guard
            .add_change_with_cleanup(create_change(3, InstanceHandle::new([1; 16])), false);

        assert!(result.is_err());
        assert!(matches!(result, Err(DdsError::OutOfResources)));

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_add_change_fail_max_instances_exceeded_reliable_remaining_samples() {
        let writer_qos: DataWriterQos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2,
                max_instances: 2,
                max_samples_per_instance: 1,
            },
            ..Default::default()
        };

        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 1 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let mut cache_guard = datawriter_cache.lock().unwrap();

        let instance_handle_1 = InstanceHandle::new([1; 16]);
        let instance_handle_2 = InstanceHandle::new([2; 16]);
        let instance_handle_3 = InstanceHandle::new([3; 16]);

        // Adding 2 changes should succeed
        assert!(cache_guard
            .add_change_with_cleanup(create_change(1, instance_handle_1), false)
            .is_ok());
        assert!(cache_guard
            .add_change_with_cleanup(create_change(2, instance_handle_2), false)
            .is_ok());

        // Returns error immediately without blocking since max_instances(2) is exceeded
        let result =
            cache_guard.add_change_with_cleanup(create_change(3, instance_handle_3), false);

        assert!(result.is_err());
        assert!(matches!(result, Err(DdsError::OutOfResources)));

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_add_change_fail_max_samples_exceeded_reliable_remaining_samples() {
        let writer_qos: DataWriterQos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2, // Scenario with minimum max_samples, passes both max_instances and max_samples_per_instance condition checks
                max_instances: 4,
                max_samples_per_instance: 2,
            },
            ..Default::default()
        };

        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 1 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let mut cache_guard = datawriter_cache.lock().unwrap();

        let instance_handle_1 = InstanceHandle::new([1; 16]);
        let instance_handle_2 = InstanceHandle::new([2; 16]);
        let instance_handle_3 = InstanceHandle::new([3; 16]);

        assert!(cache_guard
            .add_change_with_cleanup(create_change(1, instance_handle_1), false)
            .is_ok());
        assert!(cache_guard
            .add_change_with_cleanup(create_change(2, instance_handle_2), false)
            .is_ok());

        // Returns error immediately without blocking since all instances except instance_handle_3 have only 1 sample remaining
        let result =
            cache_guard.add_change_with_cleanup(create_change(3, instance_handle_3), false);

        assert!(result.is_err());
        assert!(matches!(result, Err(DdsError::OutOfResources)));

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_add_change_fail_max_samples_exceeded_reliable_unacked() {
        let writer_qos: DataWriterQos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2, // Scenario with minimum max_samples, passes both max_instances and max_samples_per_instance condition checks
                max_instances: 4,
                max_samples_per_instance: 2,
            },
            ..Default::default()
        };

        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let data_reader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            ..Default::default()
        };

        let subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &data_reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            subscription_builtin_topic_data,
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let mut cache_guard = datawriter_cache.lock().unwrap();

        let instance_handle_1 = InstanceHandle::new([1; 16]);
        let instance_handle_2 = InstanceHandle::new([2; 16]);

        // Add 2 changes to instance_handle_1, reaching max_samples
        assert!(cache_guard
            .add_change_with_cleanup(create_change(1, instance_handle_1), false)
            .is_ok());
        assert!(cache_guard
            .add_change_with_cleanup(create_change(2, instance_handle_1), false)
            .is_ok());

        // To add to instance_handle_2, one sample from instance_handle_1 should be ACKED, but returns error after blocking time since no ACK processing
        let result =
            cache_guard.add_change_with_cleanup(create_change(3, instance_handle_2), false);

        assert!(result.is_err());
        assert!(matches!(result, Err(DdsError::OutOfResources)));

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_add_change_success_max_samples_exceeded_reliable() {
        let writer_qos: DataWriterQos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 2, // Scenario with minimum max_samples, passes both max_instances and max_samples_per_instance condition checks
                max_instances: 4,
                max_samples_per_instance: 2,
            },
            ..Default::default()
        };

        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 }, // Empty acked list initially
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let mut cache_guard = datawriter_cache.lock().unwrap();

        let instance_handle_1 = InstanceHandle::new([1; 16]);
        let instance_handle_2 = InstanceHandle::new([2; 16]);

        // Add 2 changes to instance_handle_1, reaching max_samples
        assert!(cache_guard
            .add_change_with_cleanup(create_change(1, instance_handle_1), false)
            .is_ok());
        assert!(cache_guard
            .add_change_with_cleanup(create_change(2, instance_handle_1), false)
            .is_ok());

        // To add to instance_handle_2, one sample from instance_handle_1 should be ACKED
        let datawriter_clone = data_writer.clone();

        // Ack change1 after 50ms in a separate thread
        let handle = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(50));

            let rtps_writer = datawriter_clone.get_rtps_writer().unwrap();
            let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
            let reader_proxies = stateful_writer.reader_proxies();
            let mut reader_proxies_guard = reader_proxies.lock().unwrap();
            let reader_proxy = reader_proxies_guard.get_mut(0).unwrap();

            reader_proxy.acked_changes_set(SequenceNumber::from_i64(1));
        });

        // Should succeed after deleting since change1 was acked
        let result =
            cache_guard.add_change_with_cleanup(create_change(3, instance_handle_2), false);
        handle.join().unwrap();

        assert!(result.is_ok());
        assert!(cache_guard.changes.len() == 2);

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_get_first_acked_change_from_vec() {
        let writer_qos: DataWriterQos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };
        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        stateful_writer.matched_reader_add(ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber::from_i64(2),
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));

        // This reader didn't ack 2
        stateful_writer.matched_reader_add(ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 1 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));

        // This reader acked 1, 2
        stateful_writer.matched_reader_add(ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber::from_i64(2),
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        ));

        let change1 = create_change(1, InstanceHandle::new([1; 16]));
        let change2 = create_change(2, InstanceHandle::new([1; 16]));
        let change3 = create_change(3, InstanceHandle::new([1; 16]));

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let cache_guard = datawriter_cache.lock().unwrap();
        let first_acked_change =
            cache_guard.get_first_acked_change_from_vec(&vec![change1, change2, change3]);
        assert!(first_acked_change.is_ok());
        let first_acked_change = first_acked_change.unwrap();
        assert!(first_acked_change.is_some());
        assert_eq!(first_acked_change.unwrap().sequence_number(), SequenceNumber::from_i64(1));

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_remove_first_acked_change_or_block_success() {
        let writer_qos: DataWriterQos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };
        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 1 },
            false,
            true,
            SubscriptionBuiltinTopicData::default(),
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let change1 = create_change(1, InstanceHandle::new([1; 16]));
        let change2 = create_change(2, InstanceHandle::new([1; 16]));
        let changes = vec![change1.clone(), change2.clone()];

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let datawriter_clone = data_writer.clone();

        // Ack change1 after 50ms in a separate thread
        let handle = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(50));

            let rtps_writer = datawriter_clone.get_rtps_writer().unwrap();
            let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
            let reader_proxies = stateful_writer.reader_proxies();
            let mut reader_proxies_guard = reader_proxies.lock().unwrap();
            let reader_proxy = reader_proxies_guard.get_mut(0).unwrap();

            reader_proxy.acked_changes_set(SequenceNumber::from_i64(1));
        });

        let mut cache_guard = datawriter_cache.lock().unwrap();
        let result = cache_guard.remove_first_acked_change_or_block(&changes);

        handle.join().unwrap();

        // Successshould
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), change1);

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_remove_first_acked_change_or_block_timeout() {
        let writer_qos: DataWriterQos = DataWriterQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
            },
            ..Default::default()
        };
        let (participant, data_writer) = create_datawriter(writer_qos);
        let rtps_writer = data_writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let data_reader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_millis(1000),
            },
            ..Default::default()
        };

        let subscription_builtin_topic_data = SubscriptionBuiltinTopicData::new(
            &data_reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber { high: 0, low: 0 },
            SequenceNumber { high: 0, low: 0 },
            false,
            true,
            subscription_builtin_topic_data,
            SequenceNumber::new(0, 0),
        );

        // Reader that has not acked anything initially
        stateful_writer.matched_reader_add(reader_proxy);

        let change1 = create_change(1, InstanceHandle::new([1; 16]));
        let change2 = create_change(2, InstanceHandle::new([1; 16]));
        let changes = vec![change1.clone(), change2.clone()];

        let datawriter_cache = data_writer.get_datawriter_cache().unwrap();
        let datawriter_clone = data_writer.clone();

        // Ack change1 after 110ms in a separate thread
        let handle = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(110));

            let rtps_writer = datawriter_clone.get_rtps_writer().unwrap();
            let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
            let reader_proxies = stateful_writer.reader_proxies();
            let mut reader_proxies_guard = reader_proxies.lock().unwrap();
            let reader_proxy = reader_proxies_guard.get_mut(0).unwrap();

            reader_proxy.acked_changes_set(SequenceNumber::from_i64(1));
        });

        let mut cache_guard = datawriter_cache.lock().unwrap();
        let result = cache_guard.remove_first_acked_change_or_block(&changes);

        handle.join().unwrap();

        // OutOfResources error should occur
        assert!(result.is_err());
        assert!(matches!(result, Err(DdsError::OutOfResources)));

        drop(cache_guard);
        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_history_cache_get_removable_changes_returns_only_instances_with_multiple_changes() {
        // Arrange: key_a has 2, key_b has 1, key_c has 0
        let key_a = InstanceHandle::new([1; 16]);
        let key_b = InstanceHandle::new([2; 16]);
        let key_c = InstanceHandle::new([3; 16]);

        let change_a1 = create_change(1, key_a);
        let change_a2 = create_change(2, key_a);
        let change_b1 = create_change(3, key_b);

        let mut instance_map = HashMap::new();
        instance_map.insert(key_a, vec![Arc::downgrade(&change_a1), Arc::downgrade(&change_a2)]);
        instance_map.insert(key_b, vec![Arc::downgrade(&change_b1)]);
        instance_map.insert(key_c, vec![]);

        let history_cache = DataWriterHistoryCache::<()> {
            data_writer: Weak::<DataWriter<()>>::new(),
            rtps_writer: Some(Weak::<StatefulWriter>::new()),
            max_samples: 100,
            max_instances: 10,
            max_samples_per_instance: 10,
            changes: vec![],
            instance_map: Arc::new(Mutex::new(instance_map)),
            is_keep_all: false,
            is_reliable: false,
            purge_sent_changes: false,
            max_blocking_time: Duration::from_millis(100),
            has_key: true,
            lifespan_timers: Arc::new(Mutex::new(HashMap::new())),
            pool: CacheChangePool::new(),
        };

        // Act
        let result = history_cache.get_removable_changes();

        // Assert: Should return only 2 changes from key_a
        assert!(result.is_ok());
        let removable_changes = result.unwrap();
        assert_eq!(removable_changes.len(), 2);

        for change in &removable_changes {
            assert_eq!(change.instance_handle(), key_a);
        }
    }

    #[test]
    fn test_history_cache_get_removable_changes_returns_empty_when_all_have_one_or_less() {
        // Arrange: All instances have 1 or fewer changes
        let key_a = InstanceHandle::new([1; 16]);
        let key_b = InstanceHandle::new([2; 16]);

        let change_a1 = create_change(1, key_a);
        let change_b1 = create_change(2, key_b);

        let mut instance_map = HashMap::new();
        instance_map.insert(key_a, vec![Arc::downgrade(&change_a1)]);
        instance_map.insert(key_b, vec![Arc::downgrade(&change_b1)]);

        let history_cache = DataWriterHistoryCache::<()> {
            data_writer: Weak::<DataWriter<()>>::new(),
            rtps_writer: Some(Weak::<StatefulWriter>::new()),
            max_samples: 100,
            max_instances: 10,
            max_samples_per_instance: 10,
            changes: vec![],
            instance_map: Arc::new(Mutex::new(instance_map)),
            is_keep_all: false,
            is_reliable: false,
            purge_sent_changes: false,
            max_blocking_time: Duration::from_millis(100),
            has_key: true,
            lifespan_timers: Arc::new(Mutex::new(HashMap::new())),
            pool: CacheChangePool::new(),
        };

        // Act
        let result = history_cache.get_removable_changes();

        // Assert: There should be no removable changes
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    fn keepall_writer_qos(
        reliability: ReliabilityQosPolicyKind,
        durability: DurabilityQosPolicyKind,
        strict: bool,
    ) -> DataWriterQos {
        DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict },
            reliability: ReliabilityQosPolicy {
                kind: reliability,
                max_blocking_time: Duration::from_millis(100),
            },
            durability: DurabilityQosPolicy { kind: durability },
            resource_limits: ResourceLimitsQosPolicy {
                max_samples: 100,
                max_instances: 10,
                max_samples_per_instance: 100,
            },
            ..Default::default()
        }
    }

    fn add_reliable_reader(stateful_writer: &StatefulWriter, octet: u8) -> Guid {
        let guid = Guid::new(
            [0; 12],
            EntityId::new([0, 0, octet], EntityKind::USER_DEFINED_READER_WITH_KEY),
        );
        let mut reader_qos = DataReaderQos::default();
        reader_qos.reliability.kind = ReliabilityQosPolicyKind::Reliable;
        let sub_data = SubscriptionBuiltinTopicData::new(
            &reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        let reader_proxy = ReaderProxy::new(
            guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            sub_data,
            SequenceNumber::new(0, 0),
        );
        stateful_writer.matched_reader_add(reader_proxy);
        guid
    }

    fn add_best_effort_reader(stateful_writer: &StatefulWriter, octet: u8) -> Guid {
        let guid = Guid::new(
            [0; 12],
            EntityId::new([0, 0, octet], EntityKind::USER_DEFINED_READER_WITH_KEY),
        );
        let mut reader_qos = DataReaderQos::default();
        reader_qos.reliability.kind = ReliabilityQosPolicyKind::BestEffort;
        let sub_data = SubscriptionBuiltinTopicData::new(
            &reader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );
        let reader_proxy = ReaderProxy::new(
            guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            sub_data,
            SequenceNumber::new(0, 0),
        );
        stateful_writer.matched_reader_add(reader_proxy);
        guid
    }

    fn set_reader_acked(stateful_writer: &StatefulWriter, reader_guid: Guid, seq: i64) {
        let proxies = stateful_writer.reader_proxies();
        let mut guard = proxies.lock().unwrap();
        let proxy = guard.iter_mut().find(|p| p.remote_reader_guid() == reader_guid).unwrap();
        proxy.acked_changes_set(SequenceNumber::from_i64(seq));
    }

    fn add_changes(writer: &DataWriter<TestData>, count: i64) {
        let handle = InstanceHandle::new([1; 16]);
        let cache = writer.get_datawriter_cache().unwrap();
        let mut guard = cache.lock().unwrap();
        for seq in 1..=count {
            guard.add_change_with_cleanup(create_change(seq, handle), false).unwrap();
        }
    }

    fn changes_len(writer: &DataWriter<TestData>) -> usize {
        writer.get_datawriter_cache().unwrap().lock().unwrap().changes.len()
    }

    // DDS cache and RTPS transmit queue must always hold the same change set
    fn rtps_changes_len(writer: &DataWriter<TestData>) -> usize {
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let cache = rtps_writer.writer_cache();
        let guard = cache.lock().unwrap();
        guard.len()
    }

    #[test]
    fn test_volatile_keepall_nonstrict_removes_acked_changes() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader_guid = add_reliable_reader(stateful_writer, 1);

        add_changes(&writer, 3);
        assert_eq!(changes_len(&writer), 3);
        assert_eq!(rtps_changes_len(&writer), 3);

        set_reader_acked(stateful_writer, reader_guid, 2);
        stateful_writer.process_acked_changes();
        assert_eq!(changes_len(&writer), 1, "only the unacked change (seq 3) should remain");
        assert_eq!(rtps_changes_len(&writer), 1, "RTPS queue must drop the same acked changes");

        // A second notification with no further acks must not remove anything else
        stateful_writer.process_acked_changes();
        assert_eq!(changes_len(&writer), 1);
        assert_eq!(rtps_changes_len(&writer), 1);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_volatile_keepall_nonstrict_min_acked_across_reliable_readers() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader_a = add_reliable_reader(stateful_writer, 1);
        let reader_b = add_reliable_reader(stateful_writer, 2);

        add_changes(&writer, 3);
        set_reader_acked(stateful_writer, reader_a, 3);
        set_reader_acked(stateful_writer, reader_b, 1);
        stateful_writer.process_acked_changes();

        assert_eq!(changes_len(&writer), 2, "min acked is seq 1, so seq 2 and 3 remain");
        assert_eq!(rtps_changes_len(&writer), 2, "RTPS queue must match the DDS cache");

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_transient_keepall_nonstrict_keeps_acked_changes() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::TransientLocal,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader_guid = add_reliable_reader(stateful_writer, 1);

        add_changes(&writer, 3);
        set_reader_acked(stateful_writer, reader_guid, 3);
        stateful_writer.process_acked_changes();

        assert_eq!(changes_len(&writer), 3, "transient-local must retain acked samples");
        assert_eq!(rtps_changes_len(&writer), 3);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_strict_keepall_keeps_acked_changes() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            true,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader_guid = add_reliable_reader(stateful_writer, 1);

        add_changes(&writer, 3);
        set_reader_acked(stateful_writer, reader_guid, 3);
        stateful_writer.process_acked_changes();

        assert_eq!(changes_len(&writer), 3, "strict keep-all must retain acked samples");
        assert_eq!(rtps_changes_len(&writer), 3);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_best_effort_volatile_nonstrict_keepall_purges_on_write() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::BestEffort,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));

        add_changes(&writer, 3);

        assert_eq!(
            changes_len(&writer),
            0,
            "best-effort volatile keep-all must not retain samples"
        );
        assert_eq!(
            rtps_changes_len(&writer),
            0,
            "RTPS queue must be drained after synchronous send"
        );

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_best_effort_volatile_strict_keepall_keeps_samples() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::BestEffort,
            DurabilityQosPolicyKind::Volatile,
            true,
        ));

        add_changes(&writer, 3);

        assert_eq!(changes_len(&writer), 3, "strict best-effort keep-all must retain samples");
        assert_eq!(rtps_changes_len(&writer), 3);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_unmatch_lagging_reliable_reader_advances_floor() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader_fast = add_reliable_reader(stateful_writer, 1);
        let reader_slow = add_reliable_reader(stateful_writer, 2);

        add_changes(&writer, 3);
        set_reader_acked(stateful_writer, reader_fast, 3);
        set_reader_acked(stateful_writer, reader_slow, 1);
        stateful_writer.process_acked_changes();
        assert_eq!(changes_len(&writer), 2, "floor is the slow reader's ack (seq 1)");

        // The lagging reader leaves; the floor advances to the remaining reader (seq 3).
        stateful_writer.remove_matched_reader_and_update_status(reader_slow).unwrap();
        assert_eq!(changes_len(&writer), 0, "unmatch advances floor over remaining readers");
        assert_eq!(rtps_changes_len(&writer), 0);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_unmatch_all_reliable_readers_purges_to_highest_sent() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader = add_reliable_reader(stateful_writer, 1);

        add_changes(&writer, 3);
        stateful_writer.process_acked_changes();
        assert_eq!(changes_len(&writer), 3, "nothing acked yet, all retained");

        // No reliable reader remains; everything transmitted becomes removable.
        stateful_writer.remove_matched_reader_and_update_status(reader).unwrap();
        assert_eq!(changes_len(&writer), 0, "no reliable reader left -> purge to highest-sent");
        assert_eq!(rtps_changes_len(&writer), 0);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_best_effort_reader_only_reliable_writer_purges_after_send() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        add_best_effort_reader(stateful_writer, 1);

        add_changes(&writer, 3);
        // Best-effort reader sends no ACKNACK; the post-send trigger purges what was sent.
        stateful_writer.process_acked_changes();
        assert_eq!(changes_len(&writer), 0, "best-effort-only reliable writer purges after send");
        assert_eq!(rtps_changes_len(&writer), 0);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }

    #[test]
    fn test_mixed_readers_best_effort_does_not_hold_floor() {
        let (participant, writer) = create_datawriter(keepall_writer_qos(
            ReliabilityQosPolicyKind::Reliable,
            DurabilityQosPolicyKind::Volatile,
            false,
        ));
        let rtps_writer = writer.get_rtps_writer().unwrap();
        let stateful_writer = rtps_writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reliable = add_reliable_reader(stateful_writer, 1);
        add_best_effort_reader(stateful_writer, 2);

        add_changes(&writer, 3);
        set_reader_acked(stateful_writer, reliable, 3);
        stateful_writer.process_acked_changes();
        assert_eq!(changes_len(&writer), 0, "best-effort reader must not pin the floor");
        assert_eq!(rtps_changes_len(&writer), 0);

        participant.delete_contained_entities().unwrap();
        DomainParticipantFactory::get_instance().delete_participant(participant).unwrap();
    }
}
