//! Timer management system for scheduled callbacks.
//!
//! This module provides a timer system for scheduling periodic and one-shot callbacks
//! in the RTPS layer, used for heartbeats, liveliness assertions, discovery announcements,
//! and other time-based operations.

#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use log::{debug, error};
use mio::Waker;

use crate::rtps::common::guid::{self, Guid, GuidPrefix};
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::utils::timer::timer_task::TimerTask;

pub type TimerCallback = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub(crate) enum TimerMessage {
    AddTimer(String, Duration, bool, TimerCallback), // timer_id, duration, repeating, callback
    RemoveTimer(String),                             // timer_id
    PauseTimer(String),                              // timer_id
    ResumeTimer(String),                             // timer_id
    ModifyTimer(String, Duration),                   // timer_id, new_duration
    Terminate,
}

pub(crate) type TimerMessageQueue = Arc<Mutex<Vec<TimerMessage>>>;

pub(crate) static INSTANCE: OnceLock<Mutex<HashMap<GuidPrefix, Arc<Mutex<TimerHandler>>>>> =
    OnceLock::new();

pub(crate) struct TimerHandler {
    guid_prefix: GuidPrefix,
    timer_task: Option<Arc<Mutex<TimerTask>>>,
    timer_thread_join_handle: Option<thread::JoinHandle<()>>,
    message_queue: Arc<Mutex<Vec<TimerMessage>>>,
    waker: Option<Arc<Waker>>,
}

impl TimerHandler {
    fn new(guid_prefix: GuidPrefix) -> Self {
        Self {
            guid_prefix,
            timer_task: None,
            timer_thread_join_handle: None,
            message_queue: Arc::new(Mutex::new(Vec::new())),
            waker: None,
        }
    }

    pub(crate) fn get_instance(guid_prefix: GuidPrefix) -> Arc<Mutex<TimerHandler>> {
        let map_mutex = INSTANCE.get_or_init(|| Mutex::new(HashMap::new()));

        if let Ok(map_guard) = map_mutex.lock() {
            if let Some(handler) = map_guard.get(&guid_prefix) {
                return handler.clone();
            }
        } else {
            panic!("Failed to acquire timer handler map lock");
        }

        let mut new_handler = TimerHandler::new(guid_prefix);
        new_handler.spawn_event_loop();
        let handler_arc = Arc::new(Mutex::new(new_handler));

        let mut map_guard = INSTANCE
            .get()
            .expect("Timer handler map should be initialized")
            .lock()
            .expect("Failed to acquire timer handler map lock");
        let entry = map_guard.entry(guid_prefix).or_insert_with(|| handler_arc.clone());
        entry.clone()
    }

    pub(crate) fn get_instance_by_participant_guid(
        participant_guid: Guid,
    ) -> Option<Arc<Mutex<TimerHandler>>> {
        let map_mutex = INSTANCE.get()?;
        match map_mutex.lock() {
            Ok(map_guard) => map_guard.get(&participant_guid.prefix()).cloned(),
            Err(_) => None,
        }
    }

    fn spawn_event_loop(&mut self) {
        if self.timer_task.is_none() {
            let timer_task = TimerTask::new();
            self.waker = Some(timer_task.waker());
            self.timer_task = Some(Arc::new(Mutex::new(timer_task)));
        }
        if self.timer_thread_join_handle.is_none() {
            // let participant_guid =
            //     self.participant.upgrade().expect("Participant already dropped").guid();
            match self.timer_task.clone() {
                Some(timer_task) => {
                    let timer_task_clone = timer_task.clone();
                    let message_queue_clone = self.message_queue.clone();
                    let guid_prefix = self.guid_prefix.clone();
                    let handle = thread::Builder::new()
                        .name("timer task thread".to_string())
                        .spawn(move || {
                            // Register thread name for monitoring
                            {
                                use crate::rtps::task::thread_monitor::ThreadMonitor;
                                ThreadMonitor::register_current_thread_name_with_guid_prefix(
                                    "timer task thread",
                                    guid_prefix,
                                );
                            }

                            let _ =
                                timer_task_clone.lock().unwrap().event_loop(message_queue_clone);
                            // Cleanup thread from registry before exit
                            {
                                use crate::rtps::task::thread_monitor::ThreadMonitor;
                                ThreadMonitor::remove_map_guard();
                            }
                            debug!("timer task thread finished");
                        })
                        .expect("Failed to create timer task thread");
                    self.timer_thread_join_handle = Some(handle);
                }
                None => {
                    panic!("timer task is not set");
                }
            }
        }
    }

    pub(crate) fn wake_event_loop(&self) {
        if let Some(waker) = self.waker.clone() {
            if let Err(e) = waker.wake() {
                error!("Failed to wake timer event loop: {}", e);
            }
        }
    }

    pub(crate) fn add_timer<F>(
        &self,
        timer_id: String,
        duration: Duration,
        repeating: bool,
        callback: F,
    ) where
        F: Fn() + Send + Sync + 'static,
    {
        let callback_arc = Arc::new(callback);
        self.push_message_and_wake(TimerMessage::AddTimer(
            timer_id,
            duration,
            repeating,
            callback_arc,
        ));
    }

    pub(crate) fn remove_timer(&self, timer_id: String) {
        self.push_message_and_wake(TimerMessage::RemoveTimer(timer_id));
    }

    pub(crate) fn pause_timer(&self, timer_id: String) {
        self.push_message_and_wake(TimerMessage::PauseTimer(timer_id));
    }

    pub(crate) fn resume_timer(&self, timer_id: String) {
        self.push_message_and_wake(TimerMessage::ResumeTimer(timer_id));
    }

    pub(crate) fn modify_timer(&self, timer_id: String, new_duration: Duration) {
        self.push_message_and_wake(TimerMessage::ModifyTimer(timer_id, new_duration));
    }

    pub(crate) fn terminate(&self) {
        self.push_message_and_wake(TimerMessage::Terminate);
    }

    fn push_message_and_wake(&self, message: TimerMessage) {
        match self.message_queue.lock() {
            Ok(mut queue_guard) => {
                queue_guard.push(message);
            }
            Err(e) => {
                error!("Failed to acquire timer message queue lock: {}", e);
                return;
            }
        }
        self.wake_event_loop();
    }

    pub(crate) fn get_timer_task(&self) -> Option<Arc<Mutex<TimerTask>>> {
        self.timer_task.clone()
    }

    pub(crate) fn join_timer_thread(&mut self) -> RtpsResult<()> {
        if let Some(handle) = self.timer_thread_join_handle.take() {
            handle.join().map_err(|_| RtpsError::new(RtpsErrorCode::ThreadJoinError, None))?;
        }
        // Clear timer_task to release Poll and Waker file descriptors
        self.timer_task = None;
        // Clear waker reference
        self.waker = None;
        Ok(())
    }

    pub(crate) fn remove_map_guard(guid_prefix: &GuidPrefix) {
        if let Some(map_mutex) = INSTANCE.get() {
            if let Ok(mut map_guard) = map_mutex.lock() {
                map_guard.remove(guid_prefix);
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Arc;
    use std::thread;
    use std::time::{Duration, Instant};

    use crate::rtps::common::guid::Guid;
    use crate::rtps::common::types::{DomainId, ParticipantId};
    use crate::rtps::entities::entity::Entity;
    use crate::rtps::entities::participant::Participant;
    use crate::utils::timer::timer_handler::TimerHandler;

    // Helper to create mock participant for tests
    fn create_mock_participant(
        domain_id: DomainId,
        participant_id: ParticipantId,
    ) -> Arc<Participant> {
        let _guid_prefix = Guid::generate_unique_guid_prefix();

        // Create actual Participant - this may need adjustment to match your codebase's Participant creation method
        // For now, create a simple structure
        Arc::new(Participant::new(domain_id, participant_id, "127.0.0.1".to_string()))
    }

    #[test]
    fn test_timer_basic_functionality() {
        let participant = create_mock_participant(0, 1);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let executed_clone = executed.clone();

        // Add timer
        {
            let handler = timer_handler.lock().unwrap();
            handler.add_timer(
                "test_timer".to_string(),
                Duration::from_millis(100),
                false, // one-shot
                move || {
                    executed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                },
            );
        }

        // Wait for timer to execute
        thread::sleep(Duration::from_millis(200));

        // Verify timer was executed
        assert!(executed.load(std::sync::atomic::Ordering::SeqCst));

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_timer_execution_order() {
        let participant = create_mock_participant(0, 2);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));

        {
            let handler = timer_handler.lock().unwrap();

            // 10 second timer (actually 300ms for test)
            let order_clone = execution_order.clone();
            handler.add_timer(
                "timer_10".to_string(),
                Duration::from_millis(300),
                false,
                move || {
                    order_clone.lock().unwrap().push(10);
                },
            );

            // 5 second timer (actually 200ms for test)
            let order_clone = execution_order.clone();
            handler.add_timer(
                "timer_5".to_string(),
                Duration::from_millis(200),
                false,
                move || {
                    order_clone.lock().unwrap().push(5);
                },
            );

            // 2 second timer (actually 100ms for test)
            let order_clone = execution_order.clone();
            handler.add_timer(
                "timer_2".to_string(),
                Duration::from_millis(100),
                false,
                move || {
                    order_clone.lock().unwrap().push(2);
                },
            );
        }

        // Wait for all timers to execute
        thread::sleep(Duration::from_millis(500));

        // Verify execution order: should be 2, 5, 10
        let order = execution_order.lock().unwrap();
        assert_eq!(*order, vec![2, 5, 10]);

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_timer_removal() {
        let participant = create_mock_participant(0, 3);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let executed_clone = executed.clone();

        {
            let handler = timer_handler.lock().unwrap();

            // Add timer
            handler.add_timer(
                "remove_me".to_string(),
                Duration::from_millis(200),
                false,
                move || {
                    executed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                },
            );

            // Remove it immediately
            handler.remove_timer("remove_me".to_string());
        }

        // Wait longer than the timer would have executed
        thread::sleep(Duration::from_millis(300));

        // Verify timer was not executed
        assert!(!executed.load(std::sync::atomic::Ordering::SeqCst));

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_repeating_timer() {
        let participant = create_mock_participant(0, 4);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let count_clone = count.clone();

        {
            let handler = timer_handler.lock().unwrap();
            handler.add_timer(
                "repeating_timer".to_string(),
                Duration::from_millis(50),
                true, // repeating
                move || {
                    count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                },
            );
        }

        // Wait 250ms (expect approximately 5 executions)
        thread::sleep(Duration::from_millis(250));

        let final_count = count.load(std::sync::atomic::Ordering::SeqCst);

        // Verify executed at least 3 times, at most 8 times (accounting for timing variations)
        println!("Repeating timer count: {}", final_count);
        assert!(final_count >= 3 && final_count <= 8);

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.remove_timer("repeating_timer".to_string());
            handler.terminate();
        }
    }

    #[test]
    fn test_timer_pause_resume() {
        let participant = create_mock_participant(0, 5);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let executed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let executed_clone = executed.clone();

        {
            let handler = timer_handler.lock().unwrap();
            handler.add_timer(
                "pause_test".to_string(),
                Duration::from_millis(100),
                false,
                move || {
                    executed_clone.store(true, std::sync::atomic::Ordering::SeqCst);
                },
            );

            // Pause immediately
            handler.pause_timer("pause_test".to_string());
        }

        // Wait longer than original execution time
        thread::sleep(Duration::from_millis(200));

        // Verify not yet executed
        assert!(!executed.load(std::sync::atomic::Ordering::SeqCst));

        // Resume
        {
            let handler = timer_handler.lock().unwrap();
            handler.resume_timer("pause_test".to_string());
        }

        // Wait for it to execute after resume
        thread::sleep(Duration::from_millis(150));

        // Verify now executed
        assert!(executed.load(std::sync::atomic::Ordering::SeqCst));

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_multiple_participants() {
        let participant1 = create_mock_participant(0, 10);
        let participant2 = create_mock_participant(0, 20);

        let handler1 = TimerHandler::get_instance(participant1.guid().prefix());
        let handler2 = TimerHandler::get_instance(participant2.guid().prefix());

        let count1 = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let count2 = Arc::new(std::sync::atomic::AtomicU32::new(0));

        let count1_clone = count1.clone();
        let count2_clone = count2.clone();

        // Add timers to each participant
        {
            let h1 = handler1.lock().unwrap();
            h1.add_timer("p1_timer".to_string(), Duration::from_millis(50), true, move || {
                count1_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        }

        {
            let h2 = handler2.lock().unwrap();
            h2.add_timer("p2_timer".to_string(), Duration::from_millis(75), true, move || {
                count2_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        }

        // Wait 300ms
        thread::sleep(Duration::from_millis(300));

        let final_count1 = count1.load(std::sync::atomic::Ordering::SeqCst);
        let final_count2 = count2.load(std::sync::atomic::Ordering::SeqCst);

        // Participant 1 should execute more often with faster period
        println!("Multi-participant counts: p1={}, p2={}", final_count1, final_count2);
        assert!(final_count1 > final_count2);
        assert!(final_count1 >= 3); // At least 3 times (accounting for timing)
        assert!(final_count2 >= 2); // At least 2 times (accounting for timing)

        // Cleanup
        {
            let h1 = handler1.lock().unwrap();
            h1.remove_timer("p1_timer".to_string());
            h1.terminate();
        }

        {
            let h2 = handler2.lock().unwrap();
            h2.remove_timer("p2_timer".to_string());
            h2.terminate();
        }
    }

    #[test]
    fn test_timer_modify_duration() {
        let participant = create_mock_participant(0, 6);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let start_time = Instant::now();
        let execution_time = Arc::new(std::sync::Mutex::new(None));
        let execution_time_clone = execution_time.clone();

        {
            let handler = timer_handler.lock().unwrap();
            handler.add_timer(
                "modify_test".to_string(),
                Duration::from_millis(200), // Originally 200ms
                false,
                move || {
                    let mut time = execution_time_clone.lock().unwrap();
                    *time = Some(start_time.elapsed());
                },
            );

            // Immediately modify to 100ms
            handler.modify_timer("modify_test".to_string(), Duration::from_millis(100));
        }

        // Wait 150ms (longer than modified 100ms, shorter than original 200ms)
        thread::sleep(Duration::from_millis(150));

        let execution_time = execution_time.lock().unwrap();
        assert!(execution_time.is_some());

        let elapsed = execution_time.unwrap();
        // Verify executed near modified time (100ms) with margin
        println!("Timer modify test elapsed: {:?}", elapsed);
        assert!(elapsed >= Duration::from_millis(80) && elapsed <= Duration::from_millis(150));

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_example_basic_timers() {
        let participant = create_mock_participant(0, 100);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        {
            let handler = timer_handler.lock().unwrap();
            let count_clone = execution_count.clone();

            // Originally 2-second timer, reduced to 100ms for test
            handler.add_timer(
                "oneshot_timer".to_string(),
                Duration::from_millis(100),
                false,
                move || {
                    count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    println!("One-shot timer executed!");
                },
            );

            let count_clone2 = execution_count.clone();
            // Originally 5-second repeating timer, reduced to 50ms for test
            handler.add_timer(
                "heartbeat_timer".to_string(),
                Duration::from_millis(50),
                true,
                move || {
                    count_clone2.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    println!("Heartbeat timer executed!");
                },
            );
        }

        // Wait 300ms (heartbeat executes multiple times, oneshot executes once)
        thread::sleep(Duration::from_millis(300));

        let final_count = execution_count.load(std::sync::atomic::Ordering::SeqCst);
        println!("Total execution count: {}", final_count);
        assert!(final_count >= 4 && final_count <= 10);

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.remove_timer("heartbeat_timer".to_string());
            handler.terminate();
        }
    }

    #[test]
    fn test_example_multiple_participants() {
        let participants = vec![
            create_mock_participant(0, 200),
            create_mock_participant(0, 201),
            create_mock_participant(0, 202),
        ];

        let mut timer_handlers = Vec::new();
        let execution_counts = Arc::new(std::sync::Mutex::new(vec![0u32; participants.len()]));

        // Setup timer for each participant
        for (i, participant) in participants.iter().enumerate() {
            let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

            if let Ok(handler) = timer_handler.lock() {
                let timer_id = format!("participant_{}_timer", i);
                let interval = Duration::from_millis(100 * (i as u64 + 1));
                let counts_clone = execution_counts.clone();

                handler.add_timer(timer_id.clone(), interval, true, move || {
                    let mut counts = counts_clone.lock().unwrap();
                    counts[i] += 1;
                    let _now =
                        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
                    let iso_time =
                        chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::now())
                            .format("%Y-%m-%dT%H:%M:%S%.3fZ");
                    println!("Participant {} timer executed at {}!", i, iso_time);
                });
            }

            timer_handlers.push(timer_handler);
        }

        // Wait 500ms
        thread::sleep(Duration::from_millis(500));

        let final_counts = execution_counts.lock().unwrap();
        println!("Final counts: {:?}", *final_counts);
        assert!(final_counts[0] > final_counts[1]);
        assert!(final_counts[1] > final_counts[2]);
        assert!(final_counts[0] >= 3);

        // Cleanup
        for (i, handler) in timer_handlers.iter().enumerate() {
            if let Ok(h) = handler.lock() {
                h.remove_timer(format!("participant_{}_timer", i));
                h.terminate();
            }
        }
    }

    #[test]
    fn test_example_dds_scenario_timers() {
        let participant = create_mock_participant(0, 300);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_counts = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

        {
            let handler = timer_handler.lock().unwrap();

            // SPDP timer (originally 30 seconds -> 150ms for test)
            let counts_clone = execution_counts.clone();
            handler.add_timer(
                "spdp_announcement".to_string(),
                Duration::from_millis(150),
                true,
                move || {
                    let mut counts = counts_clone.lock().unwrap();
                    *counts.entry("spdp".to_string()).or_insert(0) += 1;
                    println!("SPDP announcement timer executed");
                },
            );

            // Heartbeat timer (originally 200ms -> 30ms for test)
            let counts_clone = execution_counts.clone();
            handler.add_timer(
                "heartbeat_sender".to_string(),
                Duration::from_millis(30),
                true,
                move || {
                    let mut counts = counts_clone.lock().unwrap();
                    *counts.entry("heartbeat".to_string()).or_insert(0) += 1;
                    println!("Heartbeat timer executed");
                },
            );

            // Statistics collection timer (originally 5 seconds -> 50ms, one-shot)
            let counts_clone = execution_counts.clone();
            handler.add_timer(
                "stats_collection".to_string(),
                Duration::from_millis(50),
                false,
                move || {
                    let mut counts = counts_clone.lock().unwrap();
                    *counts.entry("stats".to_string()).or_insert(0) += 1;
                    println!("Statistics collection timer executed");
                },
            );
        }

        // Wait 200ms
        thread::sleep(Duration::from_millis(200));

        let final_counts = execution_counts.lock().unwrap();
        println!("DDS scenario counts: {:?}", *final_counts);
        assert_eq!(*final_counts.get("stats").unwrap_or(&0), 1);
        assert!(*final_counts.get("spdp").unwrap_or(&0) >= 1);
        assert!(*final_counts.get("heartbeat").unwrap_or(&0) >= 4);

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.remove_timer("spdp_announcement".to_string());
            handler.remove_timer("heartbeat_sender".to_string());
            handler.terminate();
        }
    }

    #[test]
    fn test_timer_order_from_timer_order_test() {
        // Timer execution order test from timer_order_test.rs
        let participant = create_mock_participant(0, 400);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let start_time = Instant::now();

        {
            let handler = timer_handler.lock().unwrap();

            // 50ms timer (originally 10 seconds)
            let order_clone = execution_order.clone();
            let start = start_time;
            handler.add_timer(
                "timer_50ms".to_string(),
                Duration::from_millis(50),
                false,
                move || {
                    let elapsed = start.elapsed();
                    order_clone.lock().unwrap().push((50, elapsed.as_millis()));
                },
            );

            // 30ms timer (originally 5 seconds)
            let order_clone = execution_order.clone();
            let start = start_time;
            handler.add_timer(
                "timer_30ms".to_string(),
                Duration::from_millis(30),
                false,
                move || {
                    let elapsed = start.elapsed();
                    order_clone.lock().unwrap().push((30, elapsed.as_millis()));
                },
            );

            // 10ms timer (originally 2 seconds)
            let order_clone = execution_order.clone();
            let start = start_time;
            handler.add_timer(
                "timer_10ms".to_string(),
                Duration::from_millis(10),
                false,
                move || {
                    let elapsed = start.elapsed();
                    order_clone.lock().unwrap().push((10, elapsed.as_millis()));
                },
            );
        }

        // Wait for all timers to execute
        thread::sleep(Duration::from_millis(100));

        let order = execution_order.lock().unwrap();

        // Verify execution order: should be 10ms -> 30ms -> 50ms
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].0, 10); // First should be 10ms timer
        assert_eq!(order[1].0, 30); // Second should be 30ms timer
        assert_eq!(order[2].0, 50); // Third should be 50ms timer

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_timer_deletion_before_execution_from_timer_order_test() {
        // Timer deletion before execution test from timer_order_test.rs
        let participant = create_mock_participant(0, 500);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_count = Arc::new(std::sync::atomic::AtomicU32::new(0));

        {
            let handler = timer_handler.lock().unwrap();

            // Setup repeating timer
            let count_clone = execution_count.clone();
            handler.add_timer(
                "keep_timer".to_string(),
                Duration::from_millis(100),
                true,
                move || {
                    count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                },
            );

            let count_clone = execution_count.clone();
            handler.add_timer(
                "delete_timer".to_string(),
                Duration::from_millis(500), // This timer will be deleted before execution
                true,
                move || {
                    count_clone.fetch_add(100, std::sync::atomic::Ordering::SeqCst);
                    // Add large value
                },
            );
        }

        // Delete delete_timer after 200ms (well before it executes at 500ms)
        thread::sleep(Duration::from_millis(200));

        {
            let handler = timer_handler.lock().unwrap();
            handler.remove_timer("delete_timer".to_string());
        }

        // Wait an additional 400ms
        thread::sleep(Duration::from_millis(400));

        let final_count = execution_count.load(std::sync::atomic::Ordering::SeqCst);

        // Verify keep_timer executed but delete_timer did not
        // Total time: 600ms, keep_timer at 100ms = ~6 executions
        println!("Timer deletion test count: {}", final_count);
        assert!(final_count >= 3 && final_count <= 15); // With timing margin
        assert!(final_count < 100); // Verify delete_timer was not executed

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.remove_timer("keep_timer".to_string());
            handler.terminate();
        }
    }

    #[test]
    fn test_dynamic_timer_management_from_timer_order_test() {
        // Dynamic timer management test from timer_order_test.rs
        let participant = create_mock_participant(0, 600);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_results = Arc::new(std::sync::Mutex::new(Vec::new()));

        {
            let handler = timer_handler.lock().unwrap();

            // Add timers with large intervals for CI stability
            // Timers: 300ms, 600ms, 900ms, 1200ms, 1500ms
            for i in 1..=5 {
                let timer_id = format!("fast_timer_{}", i);
                let delay_ms = i * 300; // 300ms, 600ms, 900ms, 1200ms, 1500ms
                let results_clone = execution_results.clone();

                handler.add_timer(
                    timer_id.clone(),
                    Duration::from_millis(delay_ms as u64),
                    false,
                    move || {
                        results_clone.lock().unwrap().push(i);
                    },
                );
            }
        }

        // Delete timers 4 and 5 after 1050ms (after 300, 600, 900ms execute, before 1200, 1500ms)
        thread::sleep(Duration::from_millis(1050));

        {
            let handler = timer_handler.lock().unwrap();
            handler.remove_timer("fast_timer_4".to_string()); // Delete 1200ms timer
            handler.remove_timer("fast_timer_5".to_string()); // Delete 1500ms timer
        }

        // Wait an additional 600ms
        thread::sleep(Duration::from_millis(600));

        let results = execution_results.lock().unwrap();

        // Verify timers 1, 2, 3 executed and 4, 5 did not
        assert!(results.contains(&1)); // 300ms
        assert!(results.contains(&2)); // 600ms
        assert!(results.contains(&3)); // 900ms
        assert!(!results.contains(&4)); // 1200ms (deleted before execution)
        assert!(!results.contains(&5)); // 1500ms (deleted before execution)

        assert_eq!(results.len(), 3);

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }

    #[test]
    fn test_timer_order_with_mixed_operations_from_timer_order_test() {
        // Mixed operations test from timer_order_test.rs (add/delete/modify combined)
        let participant = create_mock_participant(0, 700);
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());

        let execution_log = Arc::new(std::sync::Mutex::new(Vec::new()));

        {
            let handler = timer_handler.lock().unwrap();

            // Setup initial timers
            let log_clone = execution_log.clone();
            handler.add_timer(
                "timer_a".to_string(),
                Duration::from_millis(100),
                false,
                move || {
                    log_clone.lock().unwrap().push("A".to_string());
                },
            );

            let log_clone = execution_log.clone();
            handler.add_timer(
                "timer_b".to_string(),
                Duration::from_millis(200),
                false,
                move || {
                    log_clone.lock().unwrap().push("B".to_string());
                },
            );

            // Modify timer_b to 50ms (will be faster than A)
            handler.modify_timer("timer_b".to_string(), Duration::from_millis(50));

            let log_clone = execution_log.clone();
            handler.add_timer(
                "timer_c".to_string(),
                Duration::from_millis(150),
                false,
                move || {
                    log_clone.lock().unwrap().push("C".to_string());
                },
            );
        }

        // Wait for all timers to execute
        thread::sleep(Duration::from_millis(250));

        let log = execution_log.lock().unwrap();

        // Expected order: B(50ms) -> A(100ms) -> C(150ms)
        assert_eq!(log.len(), 3);
        assert_eq!(log[0], "B");
        assert_eq!(log[1], "A");
        assert_eq!(log[2], "C");

        // Cleanup
        {
            let handler = timer_handler.lock().unwrap();
            handler.terminate();
        }
    }
}
