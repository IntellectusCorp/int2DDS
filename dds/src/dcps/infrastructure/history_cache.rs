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
        common::{guid::Guid, time::RtpsTime},
        entities::{history::cache_change::CacheChange, participant::Participant},
    },
    utils::timer::timer_handler::TimerHandler,
};

pub(crate) trait HistoryCache {
    type CacheChangeInputType;

    fn get_changes(&self) -> &Vec<Arc<CacheChange>>;
    fn get_changes_mut(&mut self) -> &mut Vec<Arc<CacheChange>>;
    fn get_instance_map(
        &self,
    ) -> Arc<Mutex<HashMap<InstanceHandle, Vec<std::sync::Weak<CacheChange>>>>>;
    fn get_rtps_participant(&self) -> DdsResult<Arc<Participant>>;
    fn add_change_with_cleanup(
        &mut self,
        a_change: Self::CacheChangeInputType,
    ) -> DdsResult<Option<Arc<CacheChange>>>; // Returns removed CacheChange while ensuring capacity
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
    fn get_max_samples(&self) -> i32;
    fn get_max_instances(&self) -> i32;
    fn get_max_samples_per_instance(&self) -> i32;

    fn is_max_instances_exceeded(&self, instance_handle: InstanceHandle) -> DdsResult<bool> {
        if instance_handle.is_nil() {
            return Ok(false);
        }

        let instance_map = self.get_instance_map();
        let instance_map_guard = instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let exceeded = !instance_map_guard.contains_key(&instance_handle)
            && instance_map_guard.len() as i32 >= self.get_max_instances();

        Ok(exceeded)
    }

    fn is_max_samples_exceeded(&self) -> bool {
        self.get_changes().len() as i32 >= self.get_max_samples()
    }

    fn is_max_samples_per_instance_exceeded(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<bool> {
        if instance_handle.is_nil() {
            // MAX_SAMPLES already considered DEPTH when initialized for NO_KEY && KEEP_LAST
            return Ok(self.is_max_samples_exceeded());
        }

        let instance_map = self.get_instance_map();
        let instance_map_guard = instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let exceeded = instance_map_guard.contains_key(&instance_handle)
            && instance_map_guard[&instance_handle].len() as i32
                >= self.get_max_samples_per_instance();

        Ok(exceeded)
    }

    // Used when lifespan qos is enabled
    fn insert_change_sorted(&mut self, change: Arc<CacheChange>) {
        let change_ts =
            change.source_timestamp().or(change.reception_timestamp()).unwrap_or(RtpsTime::ZERO);

        let changes = self.get_changes_mut();
        let pos = changes
            .binary_search_by_key(&change_ts, |c| {
                c.source_timestamp().or(c.reception_timestamp()).unwrap_or(RtpsTime::ZERO)
            })
            .unwrap_or_else(|pos| pos);
        changes.insert(pos, change);
    }

    fn lifespan_timers(&self) -> Arc<Mutex<HashMap<Guid, String>>>;

    fn get_timer_handler(&self) -> DdsResult<Arc<Mutex<TimerHandler>>> {
        let rtps_participant = self.get_rtps_participant()?;
        Ok(TimerHandler::get_instance(rtps_participant))
    }

    fn lifespan_timer_with_callback(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id_prefix: &str,
        callback: Arc<dyn Fn() + Send + Sync>,
    ) -> DdsResult<()> {
        let lifespan_timers = self.lifespan_timers();
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

        let timer_id = format!("{}_{:?}", timer_id_prefix, writer_guid);
        let timer_handler = self.get_timer_handler()?;

        {
            let handler = timer_handler
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock timer handler: {}", e)))?;

            handler.add_timer(timer_id.clone(), std_duration, true, move || {
                callback();
            });
        }

        timers_guard.insert(writer_guid, timer_id);
        Ok(())
    }

    fn lifespan_timer(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id_prefix: &str,
    ) -> DdsResult<()>;

    fn update_lifespan_timer_interval(
        &self,
        writer_guid: Guid,
        interval_duration: std::time::Duration,
    ) -> DdsResult<()> {
        let timer_handler = self.get_timer_handler()?;
        let lifespan_timers = self.lifespan_timers();
        let timer_id = {
            let timers_guard = lifespan_timers
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock lifespan_timers: {}", e)))?;
            timers_guard.get(&writer_guid).cloned()
        };

        if let Some(timer_id) = timer_id {
            let handler = timer_handler
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock timer handler: {}", e)))?;
            handler.modify_timer(timer_id, interval_duration);
        }
        Ok(())
    }

    fn lifespan_expired(
        &mut self,
        writer_guid: Guid,
        lifespan_duration: Duration,
    ) -> DdsResult<bool> {
        use log::debug;

        let current_rtps_time = RtpsTime::now();
        let changes = self.get_changes_mut();

        let mut expired_changes = Vec::new();
        let mut first_non_expired: Option<(Arc<CacheChange>, RtpsTime)> = None;

        for change in changes.iter() {
            if change.writer_guid() != writer_guid {
                continue;
            }

            let Some(source_ts) = change.source_timestamp() else {
                // If no source_timestamp, mark for removal
                expired_changes.push(change.clone());
                continue;
            };

            let expiry_rtps_time = source_ts.add_nanos(lifespan_duration.as_nanos().max(0) as u64);

            if current_rtps_time >= expiry_rtps_time {
                // If expired, mark for removal
                expired_changes.push(change.clone());
            } else {
                // When encountering first non-expired change, all subsequent changes are non-expired, so stop
                first_non_expired = Some((change.clone(), expiry_rtps_time));
                break;
            }
        }

        // Remove all expired changes at once
        for expired_change in expired_changes {
            if let Err(e) = self.remove_change(expired_change.clone()) {
                debug!("[HistoryCache] Failed to remove expired change: {:?}", e);
            } else {
                debug!(
                    "[HistoryCache] Removed expired change seq_num: {:?}, writer_guid: {:?}",
                    expired_change.sequence_number().to_i64(),
                    writer_guid
                );
            }
        }

        // If there are non-expired changes, update timer interval
        if let Some((_, expiry_rtps_time)) = first_non_expired {
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
}
