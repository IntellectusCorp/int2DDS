#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, error, warn};
use mio::{Events, Poll, Token, Waker};

use crate::utils::timer::timer_handler::{TimerCallback, TimerMessage, TimerMessageQueue};

const WAKER_TOKEN: Token = Token(0);

#[derive(Clone)]
pub(crate) struct Timer {
    id: String,
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
        id: String,
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
    timers: HashMap<String, Timer>,
    running: bool,
}

impl TimerTask {
    pub(crate) fn new() -> Self {
        let poll = Poll::new().expect("Failed to create poll");
        let waker =
            Arc::new(Waker::new(poll.registry(), WAKER_TOKEN).expect("Failed to create waker"));

        Self { poll, waker, timers: HashMap::new(), running: false }
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
        // Return None to make event loop wait indefinitely if no timers are present
        if self.timers.is_empty() {
            return None;
        }

        self.timers.values().filter_map(|timer| timer.time_until_trigger(now)).min()
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
                TimerMessage::RemoveTimersWithPrefix(prefix) => {
                    self.remove_timers_with_prefix(&prefix);
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
        self.timers.retain(|_, timer| {
            if timer.is_ready(now) {
                timer.trigger();
                timer.repeating
            } else {
                true
            }
        });
    }

    fn add_timer(
        &mut self,
        timer_id: String,
        duration: Duration,
        repeating: bool,
        callback: TimerCallback,
    ) {
        if self.timers.contains_key(&timer_id) {
            debug!("Timer '{}' already exists, skipping add", timer_id);
            return;
        }

        let timer = Timer::new(timer_id.clone(), duration, repeating, callback);
        self.timers.insert(timer_id.clone(), timer);
        debug!("Added timer '{}' with duration {:?}, repeating: {}", timer_id, duration, repeating);
    }

    fn remove_timer(&mut self, timer_id: &str) {
        if self.timers.remove(timer_id).is_some() {
            debug!("Removed timer '{}'", timer_id);
        } else {
            debug!("Attempted to remove non-existent timer '{}'", timer_id);
        }
    }

    fn remove_timers_with_prefix(&mut self, prefix: &str) {
        let before = self.timers.len();
        self.timers.retain(|id, _| !id.starts_with(prefix));
        let removed = before - self.timers.len();
        if removed > 0 {
            debug!("Removed {} timer(s) with prefix '{}'", removed, prefix);
        }
    }

    fn pause_timer(&mut self, timer_id: &str) {
        if let Some(timer) = self.timers.get_mut(timer_id) {
            timer.pause();
            debug!("Paused timer '{}'", timer_id);
        } else {
            debug!("Attempted to pause non-existent timer '{}'", timer_id);
        }
    }

    fn resume_timer(&mut self, timer_id: &str) {
        if let Some(timer) = self.timers.get_mut(timer_id) {
            timer.resume();
            debug!("Resumed timer '{}'", timer_id);
        } else {
            debug!("Attempted to resume non-existent timer '{}'", timer_id);
        }
    }

    fn modify_timer(&mut self, timer_id: &str, new_duration: Duration) {
        if let Some(timer) = self.timers.get_mut(timer_id) {
            timer.modify_duration(new_duration);
            debug!("Modified timer '{}' duration to {:?}", timer_id, new_duration);
        } else {
            debug!("Attempted to modify non-existent timer '{}'", timer_id);
        }
    }

    pub(crate) fn get_timer_info(&self, timer_id: &str) -> Option<(Duration, bool, bool)> {
        self.timers.get(timer_id).map(|timer| (timer.duration, timer.repeating, timer.paused))
    }

    pub(crate) fn list_timers(&self) -> Vec<String> {
        self.timers.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rtps::common::entity_id::EntityId;
    use crate::rtps::common::entity_kind::EntityKind;
    use crate::rtps::common::guid::Guid;
    use crate::rtps::common::sequence::SequenceNumber;
    use crate::utils::timer::timer_id::TimerId;
    use std::sync::Arc;

    fn noop_callback() -> TimerCallback {
        Arc::new(|| {})
    }

    #[test]
    fn test_remove_timers_with_prefix() {
        let mut task = TimerTask::new();

        let writer_id = EntityId { entity_key: [0x00, 0x00, 0x03], entity_kind: EntityKind(0x02) };
        let reader_a = EntityId { entity_key: [0x00, 0x00, 0x07], entity_kind: EntityKind(0x07) };
        let reader_b = EntityId { entity_key: [0x00, 0x00, 0x08], entity_kind: EntityKind(0x07) };
        let remote_guid = Guid::new(
            [0x01; 12],
            EntityId { entity_key: [0x00, 0x00, 0x01], entity_kind: EntityKind(0x07) },
        );

        // Register 7 timers: 3 writer + 2 reader_a + 2 reader_b
        let ids = [
            TimerId::PeriodicHeartbeat { entity_id: writer_id }.to_string(),
            TimerId::PeriodicHeartbeatDelay { entity_id: writer_id }.to_string(),
            TimerId::NackResponse { writer_entity_id: writer_id, remote_reader_guid: remote_guid }
                .to_string(),
            TimerId::Acknack { reader_entity_id: reader_a, remote_writer_guid: remote_guid }
                .to_string(),
            TimerId::NackFrag {
                reader_entity_id: reader_a,
                remote_writer_guid: remote_guid,
                sequence_number: SequenceNumber::new(0, 1),
            }
            .to_string(),
            TimerId::Acknack { reader_entity_id: reader_b, remote_writer_guid: remote_guid }
                .to_string(),
            TimerId::NackFrag {
                reader_entity_id: reader_b,
                remote_writer_guid: remote_guid,
                sequence_number: SequenceNumber::new(0, 1),
            }
            .to_string(),
        ];

        for id in &ids {
            task.add_timer(id.clone(), Duration::from_secs(60), false, noop_callback());
        }
        assert_eq!(task.list_timers().len(), 7);

        // Remove writer timers (3)
        task.remove_timers_with_prefix(&TimerId::entity_prefix(writer_id));
        assert_eq!(task.list_timers().len(), 4);

        // Remove reader_a timers (2)
        task.remove_timers_with_prefix(&TimerId::entity_prefix(reader_a));
        assert_eq!(task.list_timers().len(), 2);

        // Remove reader_b timers (2)
        task.remove_timers_with_prefix(&TimerId::entity_prefix(reader_b));
        assert_eq!(task.list_timers().len(), 0);
    }
}
