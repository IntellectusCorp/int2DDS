//! Liveliness monitoring infrastructure for QoS liveliness compliance monitoring.
//!
//! This module implements a background monitoring system that tracks liveliness of
//! remote entities (participants or writers). When an entity fails to signal its
//! presence within the lease duration, the monitor triggers a callback to handle
//! the liveliness lost event.
//!
//! The monitor runs in a background thread, periodically checking whether tracked
//! entities have exceeded their lease duration since their last update.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration as StdDuration, Instant},
};

use log::{debug, trace, warn};

use crate::{
    core::time::Duration,
    rtps::common::guid::Guid,
};

struct TrackerInfo {
    last_update: Instant,
    lease_duration: StdDuration,
    is_alive: bool, // Track alive/lost state to prevent duplicate LOST events
}

pub(crate) struct LivelinessMonitor {
    writer_trackers: Arc<Mutex<HashMap<Guid, TrackerInfo>>>,
    monitor_task: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    _callback: Arc<dyn Fn(Guid) -> bool + Send + Sync>,
}

impl Drop for LivelinessMonitor {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl LivelinessMonitor {
    pub(crate) fn new(callback: Arc<dyn Fn(Guid) -> bool + Send + Sync>) -> Self {
        let writer_trackers = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let monitor_task = Some(Self::spawn_monitor(
            Arc::clone(&writer_trackers),
            Arc::clone(&shutdown),
            Arc::clone(&callback),
        ));

        Self { writer_trackers, monitor_task, shutdown, _callback: callback }
    }

    pub(crate) fn track_writer(&self, guid: &Guid, lease_duration: Duration) {
        debug!("[LivelinessMonitor] Starting to track Entity: {:?}", guid);
        if let Ok(mut trackers) = self.writer_trackers.lock() {
            let now = Instant::now();
            let std_lease: StdDuration = lease_duration
                .try_into()
                .unwrap_or(StdDuration::from_secs(i32::MAX as u64));
            let was_new = trackers
                .insert(*guid, TrackerInfo { last_update: now, lease_duration: std_lease, is_alive: true })
                .is_none();
            if was_new {
                debug!(
                    "[LivelinessMonitor] New Entity {:?} added to tracking. Total tracked: {}",
                    guid, trackers.len()
                );
            } else {
                debug!(
                    "[LivelinessMonitor] Entity {:?} re-tracked (updated). Total tracked: {}",
                    guid, trackers.len()
                );
            }
        } else {
            warn!("[LivelinessMonitor] Failed to acquire lock for tracking Entity: {:?}", guid);
        }
    }

    pub(crate) fn update_writer(&self, guid: &Guid) {
        trace!("[LivelinessMonitor] Rescheduling Guid: {:?}", guid);
        if let Ok(mut trackers) = self.writer_trackers.lock() {
            if let Some(tracker_info) = trackers.get_mut(guid) {
                tracker_info.last_update = Instant::now();
                tracker_info.is_alive = true;
                debug!("[LivelinessMonitor] Guid {:?} rescheduled", guid);
            } else {
                warn!("[LivelinessMonitor] Attempted to reschedule untracked Guid: {:?}", guid);
            }
        } else {
            warn!("[LivelinessMonitor] Failed to acquire lock for rescheduling Guid: {:?}", guid);
        }
    }

    pub(crate) fn cancel_writer(&self, guid: Guid) {
        debug!("[LivelinessMonitor] Canceling tracking for Guid: {:?}", guid);
        if let Ok(mut trackers) = self.writer_trackers.lock() {
            if trackers.remove(&guid).is_some() {
                debug!(
                    "[LivelinessMonitor] Guid {:?} removed from tracking. Remaining tracked: {}",
                    guid,
                    trackers.len()
                );
            } else {
                warn!("[LivelinessMonitor] Attempted to cancel untracked Guid: {:?}", guid);
            }
        } else {
            warn!("[LivelinessMonitor] Failed to acquire lock for canceling Guid: {:?}", guid);
        }
    }

    pub(crate) fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(task) = self.monitor_task.take() {
            let _ = task.join();
        }
    }

    fn spawn_monitor(
        writer_trackers: Arc<Mutex<HashMap<Guid, TrackerInfo>>>,
        shutdown: Arc<AtomicBool>,
        callback: Arc<dyn Fn(Guid) -> bool + Send + Sync>,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("liveliness_monitor".to_string())
            .spawn(move || {
            while !shutdown.load(Ordering::Relaxed) {
                let sleep_duration = {
                    if let Ok(writer_trackers) = writer_trackers.lock() {
                        if writer_trackers.is_empty() {
                            StdDuration::from_millis(100)
                        } else {
                            let now = Instant::now();
                            writer_trackers
                                .values()
                                .map(|info| {
                                    let elapsed = now.duration_since(info.last_update);
                                    if elapsed >= info.lease_duration {
                                        StdDuration::from_millis(0)
                                    } else {
                                        info.lease_duration - elapsed
                                    }
                                })
                                .min()
                                .unwrap_or(StdDuration::from_millis(100))
                                .min(StdDuration::from_millis(100)) // Maximum 100ms
                                .max(StdDuration::from_millis(1)) // Minimum 1ms
                        }
                    } else {
                        warn!("[LivelinessMonitor Thread] Failed to acquire lock for writer_trackers");
                        StdDuration::from_millis(10)
                    }
                };
                thread::sleep(sleep_duration);

                // Check liveliness - collect expired guids first (without holding lock during callback)
                let expired_guids: Vec<(Guid, StdDuration, StdDuration)> = {
                    if let Ok(mut writer_trackers) = writer_trackers.lock() {
                        let now = Instant::now();
                        let mut expired = Vec::new();
                        for (guid, tracker_info) in writer_trackers.iter_mut() {
                            let elapsed = now.duration_since(tracker_info.last_update);
                            let lease_duration = tracker_info.lease_duration;
                            if elapsed > lease_duration && tracker_info.is_alive {
                                tracker_info.is_alive = false; // Mark as lost before releasing lock
                                expired.push((*guid, elapsed, lease_duration));
                            } else if elapsed <= lease_duration {
                                trace!(
                                    "[LivelinessMonitor Thread] Entity Guid {:?} OK - elapsed: {:?}, remaining: {:?}",
                                    guid, elapsed, lease_duration - elapsed
                                );
                            }
                        }
                        expired
                    } else {
                        warn!("[LivelinessMonitor Thread] Failed to acquire lock for liveliness check");
                        Vec::new()
                    }
                };

                // Execute callbacks without holding the lock to prevent deadlock
                let mut to_remove = Vec::new();
                for (guid, elapsed, lease_duration) in expired_guids {
                    warn!(
                        "[LivelinessMonitor] Entity {:?} LOST (elapsed: {:?} > lease: {:?})",
                        guid, elapsed, lease_duration
                    );
                    let result = callback.as_ref()(guid);
                    if result {
                        to_remove.push(guid);
                    }
                }

                // Remove trackers that should be removed
                if !to_remove.is_empty() {
                    if let Ok(mut writer_trackers) = writer_trackers.lock() {
                        for guid in to_remove {
                            writer_trackers.remove(&guid);
                            debug!("[LivelinessMonitor] Removed tracker for {:?}", guid);
                        }
                    }
                }
            }

            debug!("[LivelinessMonitor Thread] Shutting down - shutdown signal received");
        }).expect("Failed to create liveliness monitor thread")
    }
}
