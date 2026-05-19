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
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

use log::{debug, trace, warn};

use crate::{
    core::time::{Duration, Time},
    rtps::common::guid::Guid,
};

struct TrackerInfo {
    last_update: Time,
    lease_duration: Duration,
    is_alive: bool, // Track alive/lost state to prevent duplicate LOST events
}

impl TrackerInfo {
    fn update_timestamp(&mut self, timestamp: Time) {
        self.last_update = timestamp;
        self.is_alive = true; // Restore to alive when updated
    }
    fn last_update(&self) -> Time {
        self.last_update
    }
    fn lease_duration(&self) -> Duration {
        self.lease_duration
    }
}

type ShutdownSignal = Arc<(Mutex<bool>, Condvar)>;

pub(crate) struct LivelinessMonitor {
    writer_trackers: Arc<Mutex<HashMap<Guid, TrackerInfo>>>,
    monitor_task: Option<JoinHandle<()>>,
    shutdown_waker: ShutdownSignal,
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
        let shutdown_waker: ShutdownSignal = Arc::new((Mutex::new(false), Condvar::new()));

        let monitor_task = Some(Self::spawn_monitor(
            Arc::clone(&writer_trackers),
            Arc::clone(&shutdown_waker),
            Arc::clone(&callback),
        ));

        Self { writer_trackers, monitor_task, shutdown_waker, _callback: callback }
    }

    // Track Writer
    pub(crate) fn track_writer(&self, guid: &Guid, lease_duration: Duration) {
        debug!("[LivelinessMonitor] Starting to track Entity: {:?}", guid);
        if let Ok(mut trackers) = self.writer_trackers.lock() {
            let now = Time::now();
            let was_new = trackers
                .insert(*guid, TrackerInfo { last_update: now, lease_duration, is_alive: true })
                .is_none();
            if was_new {
                debug!(
                    "[LivelinessMonitor] New Entity {:?} added to tracking at {:?}. Total tracked: {}",
                    guid, now, trackers.len()
                );
            } else {
                debug!(
                    "[LivelinessMonitor] Entity {:?} re-tracked (updated) at {:?}. Total tracked: {}",
                    guid, now, trackers.len()
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
                let now = Time::now();
                let old_time = tracker_info.last_update();
                tracker_info.update_timestamp(now);
                debug!(
                    "[LivelinessMonitor] Guid {:?} rescheduled - old_time: {:?}, new_time: {:?}",
                    guid, old_time, now
                );
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
        {
            let (lock, cvar) = &*self.shutdown_waker;
            if let Ok(mut stopped) = lock.lock() {
                *stopped = true;
                cvar.notify_all();
            }
        }
        if let Some(task) = self.monitor_task.take() {
            let _ = task.join();
        }
    }

    fn spawn_monitor(
        writer_trackers: Arc<Mutex<HashMap<Guid, TrackerInfo>>>,
        shutdown_waker: ShutdownSignal,
        callback: Arc<dyn Fn(Guid) -> bool + Send + Sync>,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("liveliness_monitor".to_string())
            .spawn(move || {
            let (lock, cvar) = &*shutdown_waker;
            loop {
                let sleep_duration = {
                    if let Ok(writer_trackers) = writer_trackers.lock() {
                        if writer_trackers.is_empty() {
                            Duration::from_millis(100)
                        } else {
                            let now = Time::now();
                            writer_trackers
                                .values()
                                .map(|info| {
                                    let elapsed = now - info.last_update;
                                    if elapsed >= info.lease_duration {
                                        Duration::from_millis(0)
                                    } else {
                                        info.lease_duration - elapsed
                                    }
                                })
                                .min()
                                .unwrap_or(Duration::from_millis(100))
                                .min(Duration::from_millis(100)) // Maximum 100ms
                                .max(Duration::from_millis(1)) // Minimum 1ms
                        }
                    } else {
                        warn!("[LivelinessMontitor Thread] Failed to acquire lock for writer_trackers");
                        Duration::from_millis(10)
                    }
                };
                let sleep_std: std::time::Duration = sleep_duration.try_into().unwrap();
                let stopped = match lock.lock() {
                    Ok(g) => g,
                    Err(_) => break,
                };
                let (stopped, _) = cvar
                    .wait_timeout_while(stopped, sleep_std, |stop| !*stop)
                    .expect("liveliness monitor cvar wait failed");
                if *stopped {
                    break;
                }
                drop(stopped);

                // Check liveliness - collect expired guids first (without holding lock during callback)
                let expired_guids: Vec<(Guid, Duration, Duration)> = {
                    if let Ok(mut writer_trackers) = writer_trackers.lock() {
                        let now = Time::now();
                        let mut expired = Vec::new();
                        for (guid, tracker_info) in writer_trackers.iter_mut() {
                            let elapsed = now - tracker_info.last_update();
                            let lease_duration = tracker_info.lease_duration();
                            if elapsed > lease_duration && tracker_info.is_alive {
                                tracker_info.is_alive = false; // Mark as lost before releasing lock
                                expired.push((*guid, elapsed, lease_duration));
                            } else if elapsed <= lease_duration {
                                trace!(
                                    "[LivelinessMontitor Thread] Entity Guid {:?} OK - elapsed: {:?}, remaining: {:?}",
                                    guid, elapsed, lease_duration - elapsed
                                );
                            }
                        }
                        expired
                    } else {
                        warn!("[LivelinessMontitor Thread] Failed to acquire lock for liveliness check");
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

            debug!("[LivelinessMontitor Thread] Shutting down - shutdown signal received");
        }).expect("Failed to create liveliness monitor thread")
    }
}
