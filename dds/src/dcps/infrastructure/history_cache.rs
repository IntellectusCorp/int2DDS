//! History cache management for DataWriters and DataReaders.
//!
//! This module provides the infrastructure for managing historical data samples according
//! to the History QoS policy. The history cache stores samples for each instance, enforcing
//! depth limits and managing sample lifecycle.
//!
//! History caches are used internally by DataWriters and DataReaders to implement
//! KEEP_LAST and KEEP_ALL history policies, as well as Durability QoS for late-joining
//! subscribers.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use crate::{
    common::instance_handle::InstanceHandle,
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
    },
    rtps::{
        common::{
            guid::{Guid, GuidPrefix},
            time::RtpsTime,
        },
        entities::history::cache_change::CacheChange,
    },
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
};

pub(crate) trait HistoryCache {
    fn get_changes(&self) -> Vec<Arc<CacheChange>>;
    fn get_max_samples(&self) -> i32;
    fn get_max_instances(&self) -> i32;
    fn get_max_samples_per_instance(&self) -> i32;
    // Storage-agnostic counts backing the resource-limit checks below.
    fn sample_count(&self) -> usize;
    fn instance_count(&self) -> usize;
    fn contains_instance(&self, instance_handle: InstanceHandle) -> bool;
    fn sample_count_of_instance(&self, instance_handle: InstanceHandle) -> usize;
    fn get_lifespan_timers(&self) -> Arc<Mutex<HashMap<Guid, TimerId>>>;
    fn get_timer_handler(&self, guid_prefix: GuidPrefix) -> DdsResult<Arc<Mutex<TimerHandler>>> {
        Ok(TimerHandler::get_instance(guid_prefix))
    }
    // Add a change under History/ResourceLimits. Returns (evicted, filtered); filtered is true
    // when apply_filter held the sample via TIME_BASED_FILTER (nothing stored).
    fn add_change_with_cleanup(
        &mut self,
        a_change: Arc<CacheChange>,
        apply_filter: bool,
    ) -> DdsResult<(Option<Arc<CacheChange>>, bool)>;

    /// Mutate a CacheChange before making it immutable (set instance handle, reception timestamp, etc.)
    /// Default: no-op. Only DataReaderHistoryCache overrides this.
    fn add_info_to_cache_change(&mut self, _change: &mut CacheChange) -> DdsResult<()> {
        Ok(())
    }

    // True when the owning reader requests coherent access
    // Default: false. Only DataReaderHistoryCache overrides this.
    fn is_coherent_access(&self) -> bool {
        false
    }

    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> DdsResult<()>;
    fn ensure_capacity(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Option<Arc<CacheChange>>>;
    fn try_remove_oldest_change_of_all(&mut self) -> DdsResult<Arc<CacheChange>>;
    fn try_remove_oldest_change_of_instance(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Arc<CacheChange>>;

    fn is_max_instances_exceeded(&self, instance_handle: InstanceHandle) -> DdsResult<bool> {
        Ok(!self.contains_instance(instance_handle)
            && self.instance_count() as i32 >= self.get_max_instances())
    }

    fn is_max_samples_exceeded(&self) -> bool {
        self.sample_count() as i32 >= self.get_max_samples()
    }

    fn is_max_samples_per_instance_exceeded(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<bool> {
        if instance_handle.is_nil() {
            // MAX_SAMPLES already considered DEPTH when initialized for NO_KEY && KEEP_LAST
            return Ok(self.is_max_samples_exceeded());
        }

        Ok(self.contains_instance(instance_handle)
            && self.sample_count_of_instance(instance_handle) as i32
                >= self.get_max_samples_per_instance())
    }

    // Dry run of ensure_capacity for a batch given its per-instance sample counts. Non-mutating;
    // default true for caches that never add changes as an atomic batch.
    fn ensure_capacity_dry(&self, _len_per_instance: &HashMap<InstanceHandle, usize>) -> bool {
        true
    }

    // Insert keeping source/reception-timestamp order. Used when lifespan qos is enabled.
    fn insert_change_sorted(&mut self, change: Arc<CacheChange>);

    fn register_lifespan_timer(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id: TimerId,
    ) -> DdsResult<()>;

    fn register_lifespan_timer_with_callback(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id: TimerId,
        callback: Arc<dyn Fn() + Send + Sync>,
    ) -> DdsResult<()> {
        let lifespan_timers = self.get_lifespan_timers();
        let mut timers_guard = lifespan_timers
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock lifespan_timers: {}", e)))?;

        // Check if timer already exists
        if timers_guard.contains_key(&writer_guid) {
            return Ok(());
        }

        // Create Timer
        let std_duration = std::time::Duration::try_from(lifespan_duration)
            .map_err(|e| DdsError::Error(format!("Failed to convert Duration: {:?}", e)))?;

        let timer_handler = self.get_timer_handler(writer_guid.prefix())?;

        {
            let handler = timer_handler
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock timer handler: {}", e)))?;

            handler.add_timer(timer_id, std_duration, true, move || {
                callback();
            });
        }

        timers_guard.insert(writer_guid, timer_id);
        Ok(())
    }

    fn update_lifespan_timer_interval(
        &self,
        writer_guid: Guid,
        interval_duration: std::time::Duration,
    ) -> DdsResult<()> {
        let timer_handler = self.get_timer_handler(writer_guid.prefix())?;
        let lifespan_timers = self.get_lifespan_timers();
        let timer_id = {
            let timers_guard = lifespan_timers
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock lifespan_timers: {}", e)))?;
            timers_guard.get(&writer_guid).copied()
        };

        if let Some(timer_id) = timer_id {
            let handler = timer_handler
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock timer handler: {}", e)))?;
            handler.modify_timer(timer_id, interval_duration);
        }
        Ok(())
    }

    // Collect a writer's lifespan-expired changes plus the earliest expiry among the survivors
    // (for the next timer interval). Default assumes get_changes() is source-timestamp ordered
    // and early-breaks; stores that are not source-ordered override this.
    fn collect_lifespan_expired(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        now: RtpsTime,
    ) -> (Vec<Arc<CacheChange>>, Option<RtpsTime>) {
        let mut expired = Vec::new();
        let mut earliest_survivor = None;
        for change in self.get_changes().iter() {
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
                earliest_survivor = Some(expiry);
                break;
            }
        }
        (expired, earliest_survivor)
    }

    fn remove_lifespan_expired_changes(
        &mut self,
        writer_guid: Guid,
        lifespan_duration: Duration,
    ) -> DdsResult<bool> {
        use log::debug;

        let current_rtps_time = RtpsTime::now();
        let (expired_changes, earliest_survivor) =
            self.collect_lifespan_expired(writer_guid, lifespan_duration, current_rtps_time);

        // Remove all expired changes at once
        for expired_change in expired_changes {
            if let Err(e) = self.remove_change(expired_change.clone()) {
                debug!("[HistoryCache] Failed to remove expired change: {:?}", e);
            } else {
                debug!(
                    "[HistoryCache] Removed expired change seq_num: {}, writer_guid: {}",
                    expired_change.sequence_number().to_i64(),
                    writer_guid
                );
            }
        }

        // If there are non-expired changes, update timer interval
        if let Some(expiry_rtps_time) = earliest_survivor {
            let interval_nanos =
                expiry_rtps_time.to_nanos().saturating_sub(current_rtps_time.to_nanos());
            let interval_duration = std::time::Duration::from_nanos(interval_nanos);
            self.update_lifespan_timer_interval(writer_guid, interval_duration)?;
            Ok(true)
        } else {
            // If all changes are expired or none exist, reset timer to original lifespan duration
            let std_duration = std::time::Duration::try_from(lifespan_duration)
                .map_err(|e| DdsError::Error(format!("Failed to convert Duration: {:?}", e)))?;
            self.update_lifespan_timer_interval(writer_guid, std_duration)?;
            Ok(false)
        }
    }

    // Drops samples whose `source_timestamp + lifespan` has elapsed; called from
    // read/take so expiration is enforced independently of the cleanup timer.
    fn purge_expired_on_read(&mut self) -> DdsResult<()> {
        let now = RtpsTime::now();
        let mut to_remove = Vec::new();

        for change in self.get_changes().iter() {
            let Some(lifespan) = change.lifespan_duration() else { continue };
            if lifespan.is_infinite() {
                continue;
            }
            let Some(source_ts) = change.source_timestamp() else { continue };
            let expiry = source_ts.add_nanos(lifespan.as_nanos().max(0) as u64);
            if now >= expiry {
                to_remove.push(change.clone());
            }
        }

        for change in to_remove {
            let _ = self.remove_change(change);
        }
        Ok(())
    }
}
