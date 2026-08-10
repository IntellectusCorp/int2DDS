#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, warn};
use mio::{Events, Poll, Token, Waker};

use crate::rtps::common::entity_id::EntityId;
use crate::utils::timer::timer_handler::{TimerCallback, TimerMessage, TimerMessageQueue};
use crate::utils::timer::timer_id::TimerId;

const WAKER_TOKEN: Token = Token(0);

#[derive(Clone)]
pub(crate) struct Timer {
    id: TimerId,
    duration: Duration,
    next_trigger: Instant,
    start_time: Instant,
    execution_count: u64,
    repeating: bool,
    paused: bool,
    callback: TimerCallback,
}

impl Timer {
    pub(crate) fn new(
        id: TimerId,
        duration: Duration,
        repeating: bool,
        callback: TimerCallback,
    ) -> Self {
        let now = Instant::now();
        Self {
            id,
            duration,
            next_trigger: now + duration,
            start_time: now,
            execution_count: 0,
            repeating,
            paused: false,
            callback,
        }
    }

    pub(crate) fn is_ready(&self, now: Instant) -> bool {
        !self.paused && now >= self.next_trigger
    }

    pub(crate) fn trigger(&mut self) {
        // Execute the callback function
        (self.callback)();

        self.execution_count += 1;

        if self.repeating {
            // Use absolute scheduling to prevent drift
            // Calculate next trigger based on start time and execution count
            self.next_trigger =
                self.start_time + self.duration.mul_f64((self.execution_count + 1) as f64);

            // If we're behind schedule, schedule for immediate execution
            let now = Instant::now();
            if self.next_trigger <= now {
                self.next_trigger = now + Duration::from_nanos(1);
            }
        }
    }

    pub(crate) fn pause(&mut self) {
        self.paused = true;
    }

    pub(crate) fn resume(&mut self) {
        if self.paused {
            self.paused = false;
            // Reset timing to maintain schedule integrity
            let now = Instant::now();
            self.start_time = now;
            self.execution_count = 0;
            self.next_trigger = now + self.duration;
        }
    }

    pub(crate) fn modify_duration(&mut self, new_duration: Duration) {
        self.duration = new_duration;
        if !self.paused {
            // Reset timing with new duration
            let now = Instant::now();
            self.start_time = now;
            self.execution_count = 0;
            self.next_trigger = now + new_duration;
        }
    }

    pub(crate) fn time_until_trigger(&self, now: Instant) -> Option<Duration> {
        if self.paused {
            return None;
        }

        if now >= self.next_trigger {
            Some(Duration::ZERO)
        } else {
            Some(self.next_trigger - now)
        }
    }
}

pub struct TimerTask {
    poll: Poll,
    waker: Arc<Waker>,
    timers: HashMap<TimerId, Timer>,
    // Expiry-ordered index over the unpaused timers; every entry mirrors that
    // timer's current next_trigger and is updated eagerly on each state change.
    timers_sorted_by_expiry: BTreeSet<(Instant, TimerId)>,
    running: bool,
}

impl TimerTask {
    pub(crate) fn new() -> Self {
        let poll = Poll::new().expect("Failed to create poll");
        let waker =
            Arc::new(Waker::new(poll.registry(), WAKER_TOKEN).expect("Failed to create waker"));

        Self {
            poll,
            waker,
            timers: HashMap::new(),
            timers_sorted_by_expiry: BTreeSet::new(),
            running: false,
        }
    }

    pub(crate) fn waker(&self) -> Arc<Waker> {
        self.waker.clone()
    }

    pub(crate) fn event_loop(
        &mut self,
        message_queue: TimerMessageQueue,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.running = true;
        let mut events = Events::with_capacity(128);

        while self.running {
            let now = Instant::now();
            let timeout = self.calculate_next_timeout(now); // Return None to wait indefinitely if no timers are present

            match self.poll.poll(&mut events, timeout) {
                Ok(()) => {
                    for event in events.iter() {
                        match event.token() {
                            WAKER_TOKEN => {
                                self.process_messages(&message_queue);
                            }
                            _ => {
                                warn!("Unexpected token: {:?}", event.token());
                            }
                        }
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!("Poll interrupted in timer task, continuing: {}", e);
                    self.process_messages(&message_queue);
                }
                Err(e) => {
                    error!("Poll error in timer task: {}", e);
                    break;
                }
            }

            let now = Instant::now();
            self.process_expired_timers(now);
        }

        Ok(())
    }

    fn calculate_next_timeout(&self, now: Instant) -> Option<Duration> {
        // Return None to make event loop wait indefinitely if nothing is scheduled
        self.timers_sorted_by_expiry
            .first()
            .map(|(expiry, _)| expiry.saturating_duration_since(now))
    }

    fn process_messages(&mut self, message_queue: &TimerMessageQueue) {
        let messages = {
            match message_queue.lock() {
                Ok(mut queue) => {
                    let messages = queue.drain(..).collect::<Vec<_>>();
                    messages
                }
                Err(e) => {
                    error!("Failed to acquire timer message queue lock: {}", e);
                    return;
                }
            }
        };

        for message in messages {
            match message {
                TimerMessage::AddTimer(timer_id, duration, repeating, callback) => {
                    self.add_timer(timer_id, duration, repeating, callback);
                }
                TimerMessage::RemoveTimer(timer_id) => {
                    self.remove_timer(&timer_id);
                }
                TimerMessage::RemoveTimersByEntity(entity_id) => {
                    self.remove_timers_by_entity(&entity_id);
                }
                TimerMessage::PauseTimer(timer_id) => {
                    self.pause_timer(&timer_id);
                }
                TimerMessage::ResumeTimer(timer_id) => {
                    self.resume_timer(&timer_id);
                }
                TimerMessage::ModifyTimer(timer_id, new_duration) => {
                    self.modify_timer(&timer_id, new_duration);
                }
                TimerMessage::Terminate => {
                    self.running = false;
                    break;
                }
            }
        }
    }

    fn process_expired_timers(&mut self, now: Instant) {
        while let Some(&(expiry, timer_id)) = self.timers_sorted_by_expiry.first() {
            if expiry > now {
                break;
            }
            self.timers_sorted_by_expiry.pop_first();

            let Some(timer) = self.timers.get_mut(&timer_id) else { continue };
            timer.trigger();

            if timer.repeating {
                self.timers_sorted_by_expiry.insert((timer.next_trigger, timer_id));
            } else {
                self.timers.remove(&timer_id);
            }
        }
    }

    fn add_timer(
        &mut self,
        timer_id: TimerId,
        duration: Duration,
        repeating: bool,
        callback: TimerCallback,
    ) {
        if self.timers.contains_key(&timer_id) {
            debug!("Timer '{}' already exists, skipping add", timer_id);
            return;
        }

        let timer = Timer::new(timer_id, duration, repeating, callback);
        self.timers_sorted_by_expiry.insert((timer.next_trigger, timer_id));
        self.timers.insert(timer_id, timer);
        debug!("Added timer '{}' with duration {:?}, repeating: {}", timer_id, duration, repeating);
    }

    fn remove_timer(&mut self, timer_id: &TimerId) {
        if let Some(timer) = self.timers.remove(timer_id) {
            self.timers_sorted_by_expiry.remove(&(timer.next_trigger, *timer_id));
            debug!("Removed timer '{}'", timer_id);
        } else {
            debug!("Attempted to remove non-existent timer '{}'", timer_id);
        }
    }

    fn remove_timers_by_entity(&mut self, entity_id: &EntityId) {
        let Self { timers, timers_sorted_by_expiry, .. } = self;

        let before = timers.len();
        timers.retain(|id, timer| {
            if id.belongs_to_entity(entity_id) {
                timers_sorted_by_expiry.remove(&(timer.next_trigger, *id));
                false
            } else {
                true
            }
        });

        let removed = before - timers.len();
        if removed > 0 {
            debug!("Removed {} timer(s) for entity {}", removed, entity_id);
        }
    }

    fn pause_timer(&mut self, timer_id: &TimerId) {
        if let Some(timer) = self.timers.get_mut(timer_id) {
            self.timers_sorted_by_expiry.remove(&(timer.next_trigger, *timer_id));
            timer.pause();
            debug!("Paused timer '{}'", timer_id);
        } else {
            debug!("Attempted to pause non-existent timer '{}'", timer_id);
        }
    }

    fn resume_timer(&mut self, timer_id: &TimerId) {
        if let Some(timer) = self.timers.get_mut(timer_id) {
            let was_paused = timer.paused;
            timer.resume();

            if was_paused {
                self.timers_sorted_by_expiry.insert((timer.next_trigger, *timer_id));
            }
            debug!("Resumed timer '{}'", timer_id);
        } else {
            debug!("Attempted to resume non-existent timer '{}'", timer_id);
        }
    }

    fn modify_timer(&mut self, timer_id: &TimerId, new_duration: Duration) {
        if let Some(timer) = self.timers.get_mut(timer_id) {
            self.timers_sorted_by_expiry.remove(&(timer.next_trigger, *timer_id));
            timer.modify_duration(new_duration);

            if !timer.paused {
                self.timers_sorted_by_expiry.insert((timer.next_trigger, *timer_id));
            }
            debug!("Modified timer '{}' duration to {:?}", timer_id, new_duration);
        } else {
            debug!("Attempted to modify non-existent timer '{}'", timer_id);
        }
    }

    pub(crate) fn get_timer_info(&self, timer_id: &TimerId) -> Option<(Duration, bool, bool)> {
        self.timers.get(timer_id).map(|timer| (timer.duration, timer.repeating, timer.paused))
    }

    pub(crate) fn list_timers(&self) -> Vec<TimerId> {
        self.timers.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::entity_id::EntityId;
    use crate::rtps::common::entity_kind::EntityKind;
    use crate::rtps::common::guid::Guid;
    use crate::rtps::common::sequence::SequenceNumber;
    use std::sync::Arc;

    fn noop_callback() -> TimerCallback {
        Arc::new(|| {})
    }

    #[test]
    fn test_remove_timers_by_entity() {
        let mut task = TimerTask::new();

        let writer_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let reader_a = EntityId { entity_key: [0x00, 0x00, 0x07], entity_kind: EntityKind(0x07) };
        let reader_b = EntityId { entity_key: [0x00, 0x00, 0x08], entity_kind: EntityKind(0x07) };
        let remote_guid = Guid::new(
            [0x01; 12],
            EntityId { entity_key: [0x00, 0x00, 0x01], entity_kind: EntityKind(0x07) },
        );

        // Register 10 timers: 4 writer + 3 reader_a + 3 reader_b
        let ids = [
            TimerId::PeriodicHeartbeat { entity_id: writer_id },
            TimerId::PeriodicHeartbeatDelay { entity_id: writer_id },
            TimerId::NackResponse { writer_entity_id: writer_id, remote_reader_guid: remote_guid },
            TimerId::PreemptiveHeartbeat { entity_id: writer_id, remote_reader_guid: remote_guid },
            TimerId::Acknack { reader_entity_id: reader_a, remote_writer_guid: remote_guid },
            TimerId::NackFrag {
                reader_entity_id: reader_a,
                remote_writer_guid: remote_guid,
                sequence_number: SequenceNumber::new(0, 1),
            },
            TimerId::PreemptiveAcknack { entity_id: reader_a, remote_writer_guid: remote_guid },
            TimerId::Acknack { reader_entity_id: reader_b, remote_writer_guid: remote_guid },
            TimerId::NackFrag {
                reader_entity_id: reader_b,
                remote_writer_guid: remote_guid,
                sequence_number: SequenceNumber::new(0, 1),
            },
            TimerId::PreemptiveAcknack { entity_id: reader_b, remote_writer_guid: remote_guid },
        ];

        for id in &ids {
            task.add_timer(*id, Duration::from_secs(60), false, noop_callback());
        }
        assert_eq!(task.list_timers().len(), 10);

        // Remove writer timers (4)
        task.remove_timers_by_entity(&writer_id);
        assert_eq!(task.list_timers().len(), 6);

        // Remove reader_a timers (3)
        task.remove_timers_by_entity(&reader_a);
        assert_eq!(task.list_timers().len(), 3);

        // Remove reader_b timers (3)
        task.remove_timers_by_entity(&reader_b);
        assert_eq!(task.list_timers().len(), 0);
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    fn counting_callback(counter: &Arc<AtomicUsize>) -> TimerCallback {
        let counter = counter.clone();
        Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
    }

    fn entity(key: u8) -> EntityId {
        EntityId { entity_key: [0x00, 0x00, key], entity_kind: EntityKind(0x02) }
    }

    fn heartbeat_id(key: u8) -> TimerId {
        TimerId::PeriodicHeartbeat { entity_id: entity(key) }
    }

    // The expiry index must hold exactly the unpaused timers, each under its
    // current next_trigger.
    fn assert_expiry_index_consistent(task: &TimerTask) {
        let unpaused_count = task.timers.values().filter(|timer| !timer.paused).count();
        assert_eq!(task.timers_sorted_by_expiry.len(), unpaused_count);

        for (expiry, timer_id) in &task.timers_sorted_by_expiry {
            let timer = task.timers.get(timer_id).expect("scheduled timer must exist");
            assert_eq!(*expiry, timer.next_trigger);
            assert!(!timer.paused);
        }
    }

    #[test]
    fn fires_in_expiry_order_regardless_of_insertion_order() {
        let mut task = TimerTask::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        for (key, millis) in [(1u8, 30u64), (2, 10), (3, 20)] {
            let order = order.clone();
            task.add_timer(
                heartbeat_id(key),
                Duration::from_millis(millis),
                false,
                Arc::new(move || order.lock().unwrap().push(millis)),
            );
        }

        task.process_expired_timers(Instant::now() + Duration::from_millis(50));

        assert_eq!(*order.lock().unwrap(), vec![10, 20, 30]);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn equal_expiries_each_fire_exactly_once() {
        let mut task = TimerTask::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let expiry = Instant::now() + Duration::from_millis(10);

        for key in [1u8, 2, 3] {
            let timer_id = heartbeat_id(key);
            let timer = Timer {
                id: timer_id,
                duration: Duration::from_millis(10),
                next_trigger: expiry,
                start_time: Instant::now(),
                execution_count: 0,
                repeating: false,
                paused: false,
                callback: counting_callback(&counter),
            };
            task.timers_sorted_by_expiry.insert((timer.next_trigger, timer_id));
            task.timers.insert(timer_id, timer);
        }

        task.process_expired_timers(expiry + Duration::from_millis(1));

        assert_eq!(counter.load(Ordering::SeqCst), 3);
        assert!(task.timers.is_empty());
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn next_timeout_tracks_the_earliest_expiry() {
        let mut task = TimerTask::new();
        let now = Instant::now();

        task.add_timer(heartbeat_id(1), Duration::from_millis(100), false, noop_callback());
        let long_only = task.calculate_next_timeout(now).unwrap();
        assert!(long_only > Duration::from_millis(50));

        task.add_timer(heartbeat_id(2), Duration::from_millis(10), false, noop_callback());
        let with_short = task.calculate_next_timeout(now).unwrap();
        assert!(with_short < Duration::from_millis(20));

        task.remove_timer(&heartbeat_id(2));
        let after_remove = task.calculate_next_timeout(now).unwrap();
        assert!(after_remove > Duration::from_millis(50));
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn modify_reschedules_and_old_expiry_does_not_fire() {
        let mut task = TimerTask::new();
        let counter = Arc::new(AtomicUsize::new(0));

        task.add_timer(
            heartbeat_id(1),
            Duration::from_millis(10),
            false,
            counting_callback(&counter),
        );
        task.modify_timer(&heartbeat_id(1), Duration::from_secs(3600));

        task.process_expired_timers(Instant::now() + Duration::from_millis(100));
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        task.modify_timer(&heartbeat_id(1), Duration::from_millis(5));
        task.process_expired_timers(Instant::now() + Duration::from_millis(100));
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn removed_timer_does_not_fire_and_readd_uses_new_expiry() {
        let mut task = TimerTask::new();
        let counter = Arc::new(AtomicUsize::new(0));

        task.add_timer(
            heartbeat_id(1),
            Duration::from_millis(10),
            false,
            counting_callback(&counter),
        );
        task.remove_timer(&heartbeat_id(1));
        task.add_timer(
            heartbeat_id(1),
            Duration::from_secs(3600),
            false,
            counting_callback(&counter),
        );

        task.process_expired_timers(Instant::now() + Duration::from_millis(100));

        assert_eq!(counter.load(Ordering::SeqCst), 0);
        assert_eq!(task.timers_sorted_by_expiry.len(), 1);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn paused_timer_skips_and_resume_rearms() {
        let mut task = TimerTask::new();
        let counter = Arc::new(AtomicUsize::new(0));

        task.add_timer(
            heartbeat_id(1),
            Duration::from_millis(10),
            true,
            counting_callback(&counter),
        );
        task.pause_timer(&heartbeat_id(1));
        assert!(task.calculate_next_timeout(Instant::now()).is_none());

        task.process_expired_timers(Instant::now() + Duration::from_millis(100));
        assert_eq!(counter.load(Ordering::SeqCst), 0);

        task.resume_timer(&heartbeat_id(1));
        task.process_expired_timers(Instant::now() + Duration::from_millis(100));
        assert!(counter.load(Ordering::SeqCst) >= 1);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn repeating_rearms_and_oneshot_disappears() {
        let mut task = TimerTask::new();
        let repeating_count = Arc::new(AtomicUsize::new(0));
        let oneshot_count = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();

        task.add_timer(
            heartbeat_id(1),
            Duration::from_millis(10),
            true,
            counting_callback(&repeating_count),
        );
        task.add_timer(
            heartbeat_id(2),
            Duration::from_millis(10),
            false,
            counting_callback(&oneshot_count),
        );

        task.process_expired_timers(now + Duration::from_millis(15));

        assert_eq!(oneshot_count.load(Ordering::SeqCst), 1);
        assert!(repeating_count.load(Ordering::SeqCst) >= 1);
        assert_eq!(task.timers.len(), 1);
        assert_eq!(task.timers_sorted_by_expiry.len(), 1);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn behind_schedule_repeating_catches_up_and_makes_progress() {
        let mut task = TimerTask::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let far_future = Instant::now() + Duration::from_millis(45);

        task.add_timer(
            heartbeat_id(1),
            Duration::from_millis(10),
            true,
            counting_callback(&counter),
        );
        task.process_expired_timers(far_future);

        assert!(counter.load(Ordering::SeqCst) >= 4);
        let (next_expiry, _) =
            task.timers_sorted_by_expiry.first().expect("repeating timer stays scheduled");
        assert!(*next_expiry > far_future);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn entity_removal_prunes_the_expiry_index() {
        let mut task = TimerTask::new();

        task.add_timer(heartbeat_id(1), Duration::from_millis(10), true, noop_callback());
        task.add_timer(heartbeat_id(2), Duration::from_millis(10), true, noop_callback());

        task.remove_timers_by_entity(&entity(1));

        assert_eq!(task.timers.len(), 1);
        assert_eq!(task.timers_sorted_by_expiry.len(), 1);
        assert_expiry_index_consistent(&task);
    }

    #[test]
    fn empty_task_waits_indefinitely() {
        let task = TimerTask::new();
        assert!(task.calculate_next_timeout(Instant::now()).is_none());
    }
}
