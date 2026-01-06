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

    pub(crate) fn is_ready(&self) -> bool {
        !self.paused && Instant::now() >= self.next_trigger
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

    pub(crate) fn time_until_trigger(&self) -> Option<Duration> {
        if self.paused {
            return None;
        }

        let now = Instant::now();
        if now >= self.next_trigger {
            Some(Duration::from_millis(0))
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
            let timeout = self.calculate_next_timeout();

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

                    self.process_expired_timers();
                }
                Err(e) => {
                    error!("Poll error in timer task: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    fn calculate_next_timeout(&self) -> Option<Duration> {
        let min_timeout = self.timers.values().filter_map(|timer| timer.time_until_trigger()).min();

        match min_timeout {
            Some(timeout) => {
                // Cap timeout to improve precision, minimum 1ms for responsiveness
                Some(timeout.min(Duration::from_millis(100)).max(Duration::from_millis(1)))
            }
            None => Some(Duration::from_millis(100)), // Reduced default timeout from 1000ms to 100ms
        }
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

    fn process_expired_timers(&mut self) {
        let expired_timer_ids: Vec<String> = self
            .timers
            .values()
            .filter(|timer| timer.is_ready())
            .map(|timer| timer.id.clone())
            .collect();

        for timer_id in expired_timer_ids {
            if let Some(timer) = self.timers.get_mut(&timer_id) {
                timer.trigger();

                if !timer.repeating {
                    self.timers.remove(&timer_id);
                }
            }
        }
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
