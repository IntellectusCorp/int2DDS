//! Deadline monitoring infrastructure for QoS deadline compliance monitoring.
//!
//! This module implements the background monitoring system that tracks deadline QoS policy
//! violations. It monitors both offered deadlines (DataWriter) and requested deadlines
//! (DataReader), triggering status updates and listener callbacks when deadlines are missed.
//!
//! The deadline monitor runs in a background thread, periodically checking registered
//! instances to ensure data is written or received within the specified deadline period.

use log::{debug, trace, warn};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

use crate::{
    common::instance_handle::InstanceHandle,
    core::time::{Duration, Time},
    infrastructure::status::{
        OfferedDeadlineMissedStatus, RequestedDeadlineMissedStatus, StatusInfo, StatusKind,
    },
};

pub(crate) struct DeadlineMonitor {
    _period: Duration,
    trackers: Arc<Mutex<HashMap<InstanceHandle, Time>>>,
    monitor_task: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    _is_writer: bool,
    _on_deadline_missed: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
}

impl Drop for DeadlineMonitor {
    fn drop(&mut self) {
        debug!("[DeadlineMonitor] Dropping monitor instance");
        self.shutdown();
        if let Some(handle) = self.monitor_task.take() {
            debug!("[DeadlineMonitor] Waiting for monitor thread to finish");
            match handle.join() {
                Ok(_) => debug!("[DeadlineMonitor] Monitor thread finished successfully"),
                Err(e) => warn!("[DeadlineMonitor] Monitor thread panicked: {:?}", e),
            }
        }
        debug!("[DeadlineMonitor] Drop completed");
    }
}

impl DeadlineMonitor {
    pub(crate) fn new(
        period: Duration,
        callback: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
        is_writer: bool,
    ) -> Self {
        debug!(
            "[DeadlineMonitor] Creating new monitor - period: {:?}, is_writer: {}",
            period, is_writer
        );

        let shutdown = Arc::new(AtomicBool::new(false));
        let trackers = Arc::new(Mutex::new(HashMap::new()));

        // Start thread
        let monitor_task = Some(Self::spawn_monitor(
            period,
            Arc::clone(&trackers),
            Arc::clone(&shutdown),
            Arc::clone(&callback),
            is_writer,
        ));

        debug!("[DeadlineMonitor] Monitor thread spawned successfully");

        Self {
            _period: period,
            trackers,
            monitor_task,
            shutdown,
            _is_writer: is_writer,
            _on_deadline_missed: callback,
        }
    }

    pub(crate) fn reschedule_instance(&self, handle: &InstanceHandle) {
        trace!("[DeadlineMonitor] Rescheduling instance: {:?}", handle);
        if let Ok(mut trackers) = self.trackers.lock() {
            if let Some(last_update) = trackers.get_mut(handle) {
                let now = Time::now();
                let old_time = *last_update;
                *last_update = now;
                debug!(
                    "[DeadlineMonitor] Instance {:?} rescheduled - old_time: {:?}, new_time: {:?}",
                    handle, old_time, now
                );
            } else {
                warn!("[DeadlineMonitor] Attempted to reschedule untracked instance: {:?}", handle);
            }
        } else {
            warn!(
                "[DeadlineMonitor] Failed to acquire lock for rescheduling instance: {:?}",
                handle
            );
        }
    }

    pub(crate) fn track_instance(&self, handle: &InstanceHandle) {
        debug!("[DeadlineMonitor] Starting to track instance: {:?}", handle);
        if let Ok(mut trackers) = self.trackers.lock() {
            let now = Time::now();
            let was_new = trackers.insert(*handle, now).is_none();
            if was_new {
                debug!(
                    "[DeadlineMonitor] New instance {:?} added to tracking at {:?}. Total tracked: {}",
                    handle, now, trackers.len()
                );
            } else {
                debug!(
                    "[DeadlineMonitor] Instance {:?} re-tracked (updated) at {:?}. Total tracked: {}",
                    handle, now, trackers.len()
                );
            }
        } else {
            warn!("[DeadlineMonitor] Failed to acquire lock for tracking instance: {:?}", handle);
        }
    }

    pub(crate) fn cancel_instance(&self, handle: &InstanceHandle) {
        debug!("[DeadlineMonitor] Canceling tracking for instance: {:?}", handle);
        if let Ok(mut trackers) = self.trackers.lock() {
            if trackers.remove(handle).is_some() {
                debug!(
                    "[DeadlineMonitor] Instance {:?} removed from tracking. Remaining tracked: {}",
                    handle,
                    trackers.len()
                );
            } else {
                warn!("[DeadlineMonitor] Attempted to cancel untracked instance: {:?}", handle);
            }
        } else {
            warn!("[DeadlineMonitor] Failed to acquire lock for canceling instance: {:?}", handle);
        }
    }

    fn spawn_monitor(
        period: Duration,
        trackers: Arc<Mutex<HashMap<InstanceHandle, Time>>>,
        shutdown: Arc<AtomicBool>,
        callback: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
        is_writer: bool,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("deadline_monitor".to_string())
            .spawn(move || {
            debug!(
                "[DeadlineMonitor Thread] Started - period: {:?}, mode: {}",
                period,
                if is_writer { "Writer" } else { "Reader" }
            );

            while !shutdown.load(Ordering::Relaxed) {
                let sleep_duration = {
                    if let Ok(trackers) = trackers.lock() {
                        if trackers.is_empty() {
                            trace!(
                                "[DeadlineMonitor Thread] No instances to track, sleeping 100ms"
                            );
                            Duration::from_millis(100)
                        } else {
                            let now = Time::now();
                            trace!(
                                "[DeadlineMonitor Thread] Checking {} instances at {:?}",
                                trackers.len(),
                                now
                            );

                            let min_time_until_deadline = trackers
                                .values()
                                .map(|last_update| {
                                    let elapsed = now - *last_update;
                                    trace!(
                                        "[DeadlineMonitor Thread] Instance last_update: {:?}, elapsed: {:?}, period: {:?}",
                                        last_update, elapsed, period
                                    );
                                    if elapsed >= period {
                                        Duration::from_millis(0)
                                    } else {
                                        period - elapsed
                                    }
                                })
                                .min()
                                .unwrap_or(Duration::from_millis(100));

                            let final_duration = min_time_until_deadline
                                .min(Duration::from_millis(100))
                                .max(Duration::from_millis(1));

                            trace!(
                                "[DeadlineMonitor Thread] Next check in: {:?} (min until deadline: {:?})",
                                final_duration, min_time_until_deadline
                            );

                            final_duration
                        }
                    } else {
                        warn!(
                            "[DeadlineMonitor Thread] Failed to acquire lock for sleep calculation"
                        );
                        Duration::from_millis(10)
                    }
                };

                thread::sleep(sleep_duration.try_into().unwrap());

                // Check deadline
                if let Ok(mut trackers_guard) = trackers.lock() {
                    let now = Time::now();
                    let num_tracked = trackers_guard.len();

                    if num_tracked > 0 {
                        trace!(
                            "[DeadlineMonitor Thread] Performing deadline check for {} instances",
                            num_tracked
                        );
                    }

                    for (handle, last_update) in trackers_guard.iter_mut() {
                        let elapsed = now - *last_update;

                        if elapsed > period {
                            warn!(
                                "[DeadlineMonitor Thread] DEADLINE MISSED! Instance: {:?}, elapsed: {:?}, period: {:?}, last_update: {:?}, now: {:?}",
                                handle, elapsed, period, *last_update, now
                            );

                            let (status, info): (StatusKind, Arc<dyn StatusInfo>) = if is_writer {
                                (
                                    StatusKind::OFFERED_DEADLINE_MISSED,
                                    Arc::new(OfferedDeadlineMissedStatus {
                                        total_count: 0,
                                        total_count_change: 0,
                                        last_instance_handle: *handle,
                                    }),
                                )
                            } else {
                                (
                                    StatusKind::REQUESTED_DEADLINE_MISSED,
                                    Arc::new(RequestedDeadlineMissedStatus {
                                        total_count: 0,
                                        total_count_change: 0,
                                        last_instance_handle: *handle,
                                    }),
                                )
                            };

                            debug!(
                                "[DeadlineMonitor Thread] Invoking callback for {} status, instance: {:?}",
                                if is_writer { "OFFERED_DEADLINE_MISSED" } else { "REQUESTED_DEADLINE_MISSED" },
                                handle
                            );

                            callback.as_ref()(status, Some(info));

                            // Update time for next check
                            *last_update = now;

                            debug!(
                                "[DeadlineMonitor Thread] Instance {:?} last_update reset to {:?} for next deadline check",
                                handle, now
                            );
                        } else {
                            trace!(
                                "[DeadlineMonitor Thread] Instance {:?} OK - elapsed: {:?}, remaining: {:?}",
                                handle, elapsed, period - elapsed
                            );
                        }
                    }
                } else {
                    warn!("[DeadlineMonitor Thread] Failed to acquire lock for deadline check");
                }
            }

            debug!("[DeadlineMonitor Thread] Shutting down - shutdown signal received");
        }).expect("Failed to create deadline monitor thread")
    }

    pub(crate) fn shutdown(&self) {
        debug!("[DeadlineMonitor] Shutdown requested");
        self.shutdown.store(true, Ordering::Relaxed);
        debug!("[DeadlineMonitor] Shutdown flag set");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::thread;

    // Test helper: callback counter
    fn create_callback_counter(
    ) -> (Arc<AtomicUsize>, Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>)
    {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = Arc::clone(&counter);

        let callback = Arc::new(move |_status: StatusKind, _info: Option<Arc<dyn StatusInfo>>| {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        (counter, callback)
    }

    // Test helper: create instance handle
    fn create_test_handle(id: u8) -> InstanceHandle {
        let mut value = [0u8; 16];
        value[0] = id;
        InstanceHandle::new(value)
    }

    #[test]
    fn test_deadline_monitor_creation_and_shutdown() {
        let period = Duration::from_millis(100);
        let (counter, callback) = create_callback_counter();

        let monitor = DeadlineMonitor::new(period, callback, true);

        // Callback should not be called immediately after creation
        thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Explicit termination
        monitor.shutdown();
        thread::sleep(std::time::Duration::from_millis(50));
    }

    #[test]
    fn test_track_and_cancel_instance() {
        let period = Duration::from_millis(200);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle = create_test_handle(1);

        // Start instance tracking
        monitor.track_instance(&handle);

        // Wait short time (before deadline)
        thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Cancel tracking
        monitor.cancel_instance(&handle);

        // Callback should not be called even after deadline passed
        thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(counter.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_deadline_miss_detection() {
        let period = Duration::from_millis(100);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle = create_test_handle(2);

        // Start instance tracking
        monitor.track_instance(&handle);

        // Wait for deadline to pass
        thread::sleep(std::time::Duration::from_millis(150));

        // Callback should have been called at least once
        assert!(counter.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn test_multiple_deadline_misses() {
        let period = Duration::from_millis(100);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, false); // Reader mode

        let handle = create_test_handle(3);
        monitor.track_instance(&handle);

        // Wait to miss deadline multiple times
        thread::sleep(std::time::Duration::from_millis(350));

        // Callback should have been called multiple times
        let count = counter.load(Ordering::SeqCst);
        assert!(count >= 2, "Expected at least 2 deadline misses, got {}", count);
    }

    #[test]
    fn test_multiple_instances() {
        let period = Duration::from_millis(100);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle1 = create_test_handle(4);
        let handle2 = create_test_handle(5);
        let handle3 = create_test_handle(6);

        // Track multiple instances
        monitor.track_instance(&handle1);
        monitor.track_instance(&handle2);
        monitor.track_instance(&handle3);

        // Wait for deadline to pass
        thread::sleep(std::time::Duration::from_millis(150));

        // All 3 instances should detect deadline miss
        let count = counter.load(Ordering::SeqCst);
        assert!(
            count >= 3,
            "Expected at least 3 deadline misses (one per instance), got {}",
            count
        );
    }

    #[test]
    fn test_thread_safety_concurrent_track() {
        let period = Duration::from_millis(500);
        let (_counter, callback) = create_callback_counter();
        let monitor = Arc::new(DeadlineMonitor::new(period, callback, true));

        let mut handles = vec![];

        // Call track_instance simultaneously from multiple threads
        for i in 0..10 {
            let monitor_clone = Arc::clone(&monitor);
            let handle = thread::spawn(move || {
                let instance = create_test_handle(100 + i);
                monitor_clone.track_instance(&instance);
            });
            handles.push(handle);
        }

        // Wait for all threads to finish
        for handle in handles {
            handle.join().unwrap();
        }

        // Completes normally if there was no data race
        assert!(true);
    }

    #[test]
    fn test_thread_safety_concurrent_cancel() {
        let period = Duration::from_millis(500);
        let (_counter, callback) = create_callback_counter();
        let monitor = Arc::new(DeadlineMonitor::new(period, callback, true));

        // Start tracking instances first
        for i in 0..10 {
            let instance = create_test_handle(200 + i);
            monitor.track_instance(&instance);
        }

        let mut handles = vec![];

        // Call cancel_instance simultaneously from multiple threads
        for i in 0..10 {
            let monitor_clone = Arc::clone(&monitor);
            let handle = thread::spawn(move || {
                let instance = create_test_handle(200 + i);
                monitor_clone.cancel_instance(&instance);
            });
            handles.push(handle);
        }

        // Wait for all threads to finish
        for handle in handles {
            handle.join().unwrap();
        }

        // Completes normally if there was no data race
        assert!(true);
    }

    #[test]
    fn test_thread_safety_mixed_operations() {
        let period = Duration::from_millis(300);
        let (_counter, callback) = create_callback_counter();
        let monitor = Arc::new(DeadlineMonitor::new(period, callback, true));

        let mut handles = vec![];

        // Call track and cancel mixed from multiple threads
        for i in 0..20 {
            let monitor_clone = Arc::clone(&monitor);
            let handle = thread::spawn(move || {
                let instance = create_test_handle((i % 10) as u8);
                if i % 2 == 0 {
                    monitor_clone.track_instance(&instance);
                } else {
                    monitor_clone.cancel_instance(&instance);
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to finish
        for handle in handles {
            handle.join().unwrap();
        }

        // Wait short time then terminate
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(true);
    }

    #[test]
    fn test_drop_cleanup() {
        let period = Duration::from_millis(100);
        let (_counter, callback) = create_callback_counter();

        {
            let monitor = DeadlineMonitor::new(period, callback, true);
            let handle = create_test_handle(7);
            monitor.track_instance(&handle);

            thread::sleep(std::time::Duration::from_millis(50));
            // monitor is dropped here
        }

        // Check if thread terminated normally after Drop (success if no panic)
        thread::sleep(std::time::Duration::from_millis(100));
        assert!(true);
    }

    #[test]
    fn test_writer_mode_status() {
        let period = Duration::from_millis(100);
        let status_kind = Arc::new(Mutex::new(None));
        let status_kind_clone = Arc::clone(&status_kind);

        let callback = Arc::new(move |kind: StatusKind, _info: Option<Arc<dyn StatusInfo>>| {
            *status_kind_clone.lock().unwrap() = Some(kind);
        });

        let monitor = DeadlineMonitor::new(period, callback, true); // Writer mode
        let handle = create_test_handle(8);
        monitor.track_instance(&handle);

        thread::sleep(std::time::Duration::from_millis(150));

        // In Writer mode, OFFERED_DEADLINE_MISSED should occur
        let kind = status_kind.lock().unwrap();
        assert_eq!(*kind, Some(StatusKind::OFFERED_DEADLINE_MISSED));
    }

    #[test]
    fn test_reader_mode_status() {
        let period = Duration::from_millis(100);
        let status_kind = Arc::new(Mutex::new(None));
        let status_kind_clone = Arc::clone(&status_kind);

        let callback = Arc::new(move |kind: StatusKind, _info: Option<Arc<dyn StatusInfo>>| {
            *status_kind_clone.lock().unwrap() = Some(kind);
        });

        let monitor = DeadlineMonitor::new(period, callback, false); // Reader mode
        let handle = create_test_handle(9);
        monitor.track_instance(&handle);

        thread::sleep(std::time::Duration::from_millis(150));

        // In Reader mode, REQUESTED_DEADLINE_MISSED should occur
        let kind = status_kind.lock().unwrap();
        assert_eq!(*kind, Some(StatusKind::REQUESTED_DEADLINE_MISSED));
    }

    #[test]
    fn test_no_deadline_miss_before_period() {
        let period = Duration::from_millis(200);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle = create_test_handle(10);
        monitor.track_instance(&handle);

        // Callback should not be called before deadline
        thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Callback should be called after deadline
        thread::sleep(std::time::Duration::from_millis(150));
        assert!(counter.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn test_shutdown_stops_monitoring() {
        let period = Duration::from_millis(100);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle = create_test_handle(11);
        monitor.track_instance(&handle);

        // Terminate immediately
        monitor.shutdown();
        thread::sleep(std::time::Duration::from_millis(50));

        // Callback should not be called after termination even if deadline passed
        let count_before = counter.load(Ordering::SeqCst);
        thread::sleep(std::time::Duration::from_millis(200));
        let count_after = counter.load(Ordering::SeqCst);

        assert_eq!(count_before, count_after, "Callback should not be called after shutdown");
    }

    #[test]
    fn test_very_short_period() {
        let period = Duration::from_millis(10);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle = create_test_handle(12);
        monitor.track_instance(&handle);

        // Should work normally even with short period
        thread::sleep(std::time::Duration::from_millis(50));
        assert!(counter.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn test_same_instance_retrack() {
        let period = Duration::from_millis(100);
        let (counter, callback) = create_callback_counter();
        let monitor = DeadlineMonitor::new(period, callback, true);

        let handle = create_test_handle(13);

        // Track same instance multiple times (overwrite)
        monitor.track_instance(&handle);
        thread::sleep(std::time::Duration::from_millis(30));
        monitor.track_instance(&handle); // Update time
        thread::sleep(std::time::Duration::from_millis(30));
        monitor.track_instance(&handle); // Update time

        // Should have no deadline miss in short time after last track
        thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        // Deadline miss occurs after sufficient time passed since last track
        thread::sleep(std::time::Duration::from_millis(80));
        assert!(counter.load(Ordering::SeqCst) >= 1);
    }
}
