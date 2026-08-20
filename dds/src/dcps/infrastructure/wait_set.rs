//! WaitSet for waiting on multiple conditions.
//!
//! # Example
//!
//! ```no_run
//! # use int2dds::domain::domain_participant_factory::DomainParticipantFactory;
//! # use int2dds::infrastructure::wait_set::WaitSet;
//! # use int2dds::domain::qos::DomainParticipantQos;
//! # use int2dds::topic::qos::TopicQos;
//! # use int2dds::subscription::qos::{SubscriberQos, DataReaderQos};
//! # use int2dds::infrastructure::status::StatusMask;
//! # use int2dds::core::time::Duration;
//! # use int2dds::topic::type_support::DdsType;
//! # use int2dds::subscription::sample_info::SampleStateKind;
//! # use int2dds::subscription::sample_info::ViewStateKind;
//! # use int2dds::subscription::sample_info::InstanceStateKind;
//! # use int2dds::core::types::LENGTH_UNLIMITED;
//! # #[derive(DdsType)]
//! # struct MyData { id: u32 }
//! # let factory = DomainParticipantFactory::get_instance();
//! # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
//! # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", TopicQos::default(), None, StatusMask::default()).unwrap();
//! # let subscriber = participant.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
//! # let reader = subscriber.create_datareader::<MyData>(&topic, DataReaderQos::default(), None, StatusMask::default()).unwrap();
//! let mut wait_set = WaitSet::new();
//!
//! // Attach a condition for data availability
//! let mut condition = reader.get_statuscondition().unwrap().clone();
//! condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();
//! wait_set.attach_condition(condition).unwrap();
//!
//! // Wait indefinitely for data
//! loop {
//!     let triggered = wait_set.wait(Duration::infinite()).unwrap();
//!     println!("Condition triggered, {} conditions active", triggered.len());
//!
//!     // Read the available data
//!     let samples = reader.take(
//!         LENGTH_UNLIMITED,
//!         &[SampleStateKind::ANY_SAMPLE_STATE],
//!         &[ViewStateKind::ANY_VIEW_STATE],
//!         &[InstanceStateKind::ANY_INSTANCE_STATE]
//!     ).unwrap();
//!
//!     for sample in samples {
//!         println!("Received data: {:?}", sample.data());
//!     }
//! }
//! ```

use log::{debug, info};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Instant;

use crate::{
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
    },
    infrastructure::condition::Condition,
    rtps::common::time::RtpsDuration,
};

// Global counter for tracking WaitSet instances
static WAITSET_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// State guarded by [`WaitSet::notify_lock`], the mutex the condvar is paired with.
///
/// `notify_lock` is a **leaf**: no other lock may be acquired while it is held. That is what keeps
/// the notifier deadlock-free. Notifications arrive on the RTPS receive thread, which already
/// holds the reader's `matched_writers` and `status_callback` locks and the condition's
/// `waitset_callback` lock, while `attach_condition`/`detach_condition` take `conditions` before
/// `waitset_callback`. Pairing the condvar with `conditions` instead would invert those two.
#[derive(Default)]
struct NotifyState {
    /// Bumped once per notification. A waiter compares it against the value it snapshotted before
    /// its last scan; a difference means something may have become triggered and it must look
    /// again. Unlike a consume-once flag it is not stolen by whichever waiter wakes first, so
    /// every waiter observes every notification.
    generation: u64,
    /// Threads currently blocked inside `condvar.wait*`. Zero means a wake would reach nobody, so
    /// it is skipped -- this is what collapses a burst of notifications into a single wake.
    parked: usize,
}

pub struct WaitSet {
    // Attached conditions keyed by Condition::identity(), so attach and detach
    // need no scan over the attached set.
    conditions: Arc<Mutex<HashMap<usize, Arc<dyn Condition + Send + Sync>>>>,
    condvar: Arc<Condvar>,
    waiting_count: Arc<AtomicUsize>,
    notify_lock: Arc<Mutex<NotifyState>>,
    // Wake-up callback planted into every attached condition. Captures only
    // per-WaitSet state, so one instance is shared by all attaches.
    notify: Arc<dyn Fn() + Send + Sync>,
    instance_id: usize,
}

impl Default for WaitSet {
    fn default() -> Self {
        Self::new()
    }
}

impl WaitSet {
    /// Creates a new `WaitSet` for waiting on multiple conditions.
    ///
    /// A WaitSet allows an application to wait until one or more attached conditions become
    /// true. This is the primary mechanism for event-driven DDS applications. Instead of polling,
    /// you can wait for conditions such as data availability, status changes, or other events.
    ///
    /// # Returns
    ///
    /// Returns a new `WaitSet` instance with no conditions attached.
    pub fn new() -> Self {
        let instance_id = WAITSET_COUNTER.fetch_add(1, Ordering::Relaxed);
        debug!("[WaitSet-{}] Creating new WaitSet instance", instance_id);

        let condvar = Arc::new(Condvar::new());
        let notify_lock = Arc::new(Mutex::new(NotifyState::default()));
        let notify = Self::make_notify(condvar.clone(), notify_lock.clone(), instance_id);

        Self {
            conditions: Arc::new(Mutex::new(HashMap::new())),
            condvar,
            waiting_count: Arc::new(AtomicUsize::new(0)),
            notify_lock,
            notify,
            instance_id,
        }
    }

    fn lock_notify_state(&self) -> DdsResult<MutexGuard<'_, NotifyState>> {
        self.notify_lock
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock notify state: {}", e)))
    }

    fn notify_generation(&self) -> DdsResult<u64> {
        Ok(self.lock_notify_state()?.generation)
    }

    fn has_no_conditions(&self) -> DdsResult<bool> {
        Ok(self
            .conditions
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?
            .is_empty())
    }

    /// Attaches a condition to this WaitSet.
    ///
    /// Once attached, the WaitSet will wake up when this condition becomes true (triggered).
    /// Multiple conditions can be attached to a single WaitSet, and the WaitSet will wake up
    /// when any of them triggers.
    ///
    /// Attaching the same condition multiple times has no effect - it will only be attached once.
    ///
    /// # Arguments
    ///
    /// * `condition` - The condition to attach. This can be a `StatusCondition`, `GuardCondition`, `ReadCondition`,
    ///   or `QueryCondition`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or a `DdsError` if attachment fails.
    ///
    /// # Errors
    ///
    /// Returns an error if internal synchronization fails.
    pub fn attach_condition<T: Into<Arc<dyn Condition + Send + Sync>>>(
        &self,
        condition: T,
    ) -> DdsResult<()> {
        let new_condition = condition.into();

        let mut conditions = self
            .conditions
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

        // Duplicate condition handles are detected by object identity, which is also the map key.
        let new_identity = new_condition.identity() as usize;
        if conditions.contains_key(&new_identity) {
            debug!("[WaitSet-{}] Condition already attached, skipping", self.instance_id);
            return Ok(());
        }

        // Check trigger value and notify only when waiting threads exist
        let waiting_threads = self.waiting_count.load(Ordering::Acquire);
        let should_notify =
            waiting_threads > 0 && new_condition.get_trigger_value().unwrap_or(false);

        new_condition.set_waitset_callback(Some(self.notify.clone()));
        conditions.insert(new_identity, new_condition);
        let condition_count = conditions.len();
        debug!(
            "[WaitSet-{}] Condition attached successfully. Total conditions: {}",
            self.instance_id, condition_count
        );
        drop(conditions); // Release mutex before notification

        if should_notify {
            debug!(
                "[WaitSet-{}] Notifying waiting threads - new condition is triggered",
                self.instance_id
            );
            // Must go through the same path as a status notification. Poking the condvar directly
            // leaves the generation unchanged, which a waiter cannot tell apart from a spurious
            // wake-up, so it re-parks without ever looking at the newly attached condition.
            (self.notify)();
        }

        Ok(())
    }

    pub fn detach_condition<T: Into<Arc<dyn Condition + Send + Sync>>>(
        &self,
        condition: T,
    ) -> DdsResult<()> {
        let remove_condition = condition.into();
        let mut conditions = self
            .conditions
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

        match conditions.remove(&(remove_condition.identity() as usize)) {
            Some(attached) => {
                attached.set_waitset_callback(None);
                Ok(())
            }
            None => Err(DdsError::PreconditionNotMet),
        }
    }

    /// Waits for one or more attached conditions to become triggered.
    ///
    /// This operation blocks the calling thread until at least one attached condition becomes
    /// true (triggered), or until the specified timeout expires. This is the primary method for
    /// implementing event-driven DDS applications.
    ///
    /// If one or more conditions are already triggered when this operation is called, it returns
    /// immediately with those conditions.
    ///
    /// # Arguments
    ///
    /// * `timeout` - Maximum duration to wait. Use `Duration::infinite()` to wait indefinitely,
    ///   or specify a finite duration in seconds and nanoseconds.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<Arc<dyn Condition>>)` containing all triggered conditions, or a `DdsError`
    /// if the timeout expires or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `Timeout` - The specified timeout expired before any condition became triggered
    /// * Internal synchronization fails
    pub fn wait(&self, timeout: Duration) -> DdsResult<Vec<Arc<dyn Condition + Send + Sync>>> {
        let timeout_type = if timeout.is_infinite() { "INFINITE" } else { "FINITE" };
        debug!(
            "[WaitSet-{}] Starting wait operation with {} timeout",
            self.instance_id, timeout_type
        );

        // Increment waiting thread count
        self.waiting_count.fetch_add(1, Ordering::AcqRel);

        // Automatically release waiting state flag
        struct WaitGuard<'a> {
            waiting_count: &'a AtomicUsize,
            instance_id: usize,
        }
        impl<'a> Drop for WaitGuard<'a> {
            fn drop(&mut self) {
                self.waiting_count.fetch_sub(1, Ordering::AcqRel);
                debug!("[WaitSet-{}] Wait operation cleanup completed", self.instance_id);
            }
        }
        let _guard =
            WaitGuard { waiting_count: &self.waiting_count, instance_id: self.instance_id };

        // Snapshot before the first scan, so a notification racing that scan shows up as a
        // generation change rather than being missed.
        let mut snapshot = self.notify_generation()?;

        // Check immediately triggered conditions
        let triggered_conditions = self.check_triggered_conditions()?;
        if !triggered_conditions.is_empty() {
            info!(
                "[WaitSet-{}] Immediate trigger detected: {} conditions triggered",
                self.instance_id,
                triggered_conditions.len()
            );
            return Ok(triggered_conditions);
        }

        // An empty set has nothing to wait for; a deleted condition drains its way here.
        if self.has_no_conditions()? {
            debug!("[WaitSet-{}] No conditions attached, returning empty", self.instance_id);
            return Ok(Vec::new());
        }

        // Set timeout
        let deadline = if timeout.is_infinite() {
            None
        } else {
            let rtps_duration: RtpsDuration = timeout.into();
            let std_timeout = rtps_duration.to_std_duration();
            Some(Instant::now() + std_timeout)
        };

        // condvar-based waiting loop
        loop {
            let mut state = self.lock_notify_state()?;

            if state.generation == snapshot {
                // Nothing has been notified since our last scan. Park. The decision to park and
                // the notifier's generation bump are both made under `notify_lock`, so a
                // notification cannot slip into the gap between the two and be lost.
                let timed_out = match deadline {
                    None => {
                        state.parked += 1;
                        let mut guard = self.condvar.wait(state).map_err(|e| {
                            DdsError::Error(format!("Condition variable wait failed: {}", e))
                        })?;
                        guard.parked -= 1;
                        state = guard;
                        false
                    }
                    Some(deadline_time) => {
                        let now = Instant::now();
                        if now >= deadline_time {
                            debug!("[WaitSet-{}] Wait timeout reached", self.instance_id);
                            return Err(DdsError::Timeout);
                        }
                        let remaining_time = deadline_time - now;
                        state.parked += 1;
                        let (mut guard, wait_result) =
                            self.condvar.wait_timeout(state, remaining_time).map_err(|e| {
                                DdsError::Error(format!("Condition variable wait failed: {}", e))
                            })?;
                        guard.parked -= 1;
                        state = guard;
                        wait_result.timed_out()
                    }
                };

                if state.generation == snapshot {
                    // A genuine spurious wake-up: nothing was notified, so nothing can have
                    // become triggered that the last scan did not already see. Re-park rather
                    // than pay for another walk over every attached condition.
                    drop(state);
                    if timed_out {
                        debug!("[WaitSet-{}] Wait timeout reached", self.instance_id);
                        return Err(DdsError::Timeout);
                    }
                    continue;
                }
            }

            // The generation moved, either while we were parked or in the window before we could
            // park. Either way, rescan.
            snapshot = state.generation;
            drop(state); // never hold notify_lock while taking `conditions`

            debug!("[WaitSet-{}] Notification observed, checking conditions", self.instance_id);
            let triggered_conditions = self.check_triggered_conditions()?;
            if !triggered_conditions.is_empty() {
                debug!(
                    "[WaitSet-{}] Conditions triggered during wait: {} conditions",
                    self.instance_id,
                    triggered_conditions.len()
                );
                return Ok(triggered_conditions);
            }

            // A deleted condition drains the set. With nothing left to wait for, return empty
            // instead of re-parking until timeout.
            if self.has_no_conditions()? {
                debug!("[WaitSet-{}] All conditions gone, returning empty", self.instance_id);
                return Ok(Vec::new());
            }

            // A wake-up can be stale if the condition was cleared before we checked it.
            // Fall through to the normal wait/timeout path.
            if let Some(deadline_time) = deadline {
                if Instant::now() >= deadline_time {
                    debug!("[WaitSet-{}] Wait timeout reached", self.instance_id);
                    return Err(DdsError::Timeout);
                }
            }
        }
    }
    pub fn get_conditions(&self) -> DdsResult<Vec<Arc<dyn Condition + Send + Sync>>> {
        let conditions = self
            .conditions
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

        Ok(conditions.values().cloned().collect())
    }

    /// Function to check and return triggered conditions
    fn check_triggered_conditions(&self) -> DdsResult<Vec<Arc<dyn Condition + Send + Sync>>> {
        let mut conditions = self
            .conditions
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

        if conditions.is_empty() {
            return Ok(Vec::new());
        }

        let mut triggered = Vec::new();
        // Conditions whose entity was deleted: never reported as triggered, just dropped from the
        // set below. Draining the set is what lets a parked waiter return.
        let mut dead_keys = Vec::new();

        for (key, condition) in conditions.iter() {
            if condition.is_dead() {
                dead_keys.push(*key);
                continue;
            }
            match condition.get_trigger_value() {
                Ok(true) => triggered.push(Arc::clone(condition)),
                Ok(false) => {}
                Err(e) => {
                    debug!("[WaitSet-{}] Condition check failed: {:?}", self.instance_id, e);
                }
            }
        }

        if !dead_keys.is_empty() {
            debug!(
                "[WaitSet-{}] dropping {} condition(s) whose entity was deleted",
                self.instance_id,
                dead_keys.len()
            );
        }
        for key in dead_keys {
            if let Some(condition) = conditions.remove(&key) {
                condition.set_waitset_callback(None);
            }
        }

        Ok(triggered)
    }

    fn make_notify(
        condvar: Arc<Condvar>,
        notify_lock: Arc<Mutex<NotifyState>>,
        instance_id: usize,
    ) -> Arc<dyn Fn() + Send + Sync> {
        Arc::new(move || {
            // Runs on the RTPS receive thread, which is already holding the reader's
            // `matched_writers` and `status_callback` locks and the condition's
            // `waitset_callback` lock. Takes exactly one leaf lock, holds it for two field
            // updates, and releases it before the wake.
            let should_wake = match notify_lock.lock() {
                Ok(mut state) => {
                    // Bumped unconditionally: a waiter that has taken its snapshot but not yet
                    // parked is not counted in `parked`, and it must still observe this.
                    state.generation = state.generation.wrapping_add(1);
                    state.parked > 0
                }
                Err(e) => {
                    log::error!("[WaitSet-{}] Failed to lock notify state: {}", instance_id, e);
                    // Poisoned: wake unconditionally rather than strand a waiter.
                    true
                }
            };

            if should_wake {
                debug!("[WaitSet-{}] Notifying waiting threads", instance_id);
                condvar.notify_all();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        common::instance_handle::InstanceHandle,
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            condition::Condition,
            guard_condition::GuardCondition,
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
                ReliabilityQosPolicyKind,
            },
            status::StatusMask,
            status_condition::StatusCondition,
            wait_set::WaitSet,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::qos::{DataReaderQos, SubscriberQos},
        test_utils::unique_domain_id,
        topic::qos::TopicQos,
        DdsType,
    };
    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    /// Two distinct conditions that happen to be in the same state are
    /// indistinguishable through `Debug`, so identity must come from the
    /// condition object itself. Detaching one must leave the other attached.
    #[test]
    fn detach_removes_the_requested_condition_not_a_look_alike() {
        let first = StatusCondition::<DataReaderQos>::new(None);
        let second = StatusCondition::<DataReaderQos>::new(None);

        let wait_set = WaitSet::new();
        wait_set.attach_condition(first.clone()).unwrap();
        wait_set.attach_condition(second.clone()).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 2);

        wait_set.detach_condition(second.clone()).unwrap();

        let remaining = wait_set.get_conditions().unwrap();
        assert_eq!(remaining.len(), 1);
        let survivor = remaining[0]
            .as_any()
            .downcast_ref::<StatusCondition<DataReaderQos>>()
            .expect("survivor should still be a StatusCondition");
        assert!(
            Arc::ptr_eq(&survivor.enabled_statuses, &first.enabled_statuses),
            "detaching `second` removed `first` instead"
        );
    }

    /// The same condition handed in twice must attach once. Every
    /// `Into<Arc<dyn Condition>>` allocates a fresh Arc, so pointer equality on
    /// the trait object cannot be what decides this.
    #[test]
    fn attaching_the_same_condition_twice_attaches_it_once() {
        let condition = StatusCondition::<DataReaderQos>::new(None);

        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();

        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);
    }

    #[test]
    fn test_waitset_matching() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // std::thread::sleep(std::time::Duration::from_secs(2));
        let condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition).unwrap();
        let res = wait_set.wait(Duration::from_seconds(10));

        assert!(res.is_ok());

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_timeout() {
        let domain_id = unique_domain_id();
        let participant_qos = DomainParticipantQos::default();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(domain_id, participant_qos, None, StatusMask::default())
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "hello_world_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let reader_qos = DataReaderQos::default();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
            .unwrap();

        // std::thread::sleep(std::time::Duration::from_secs(2));
        let condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition).unwrap();
        let result = wait_set.wait(Duration::from_seconds(5));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::Timeout);

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_multiple_status_conditions_different_entities_1() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader_qos = DataReaderQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let reader = subscriber
            .create_datareader::<HelloWorldType>(&topic, reader_qos, None, StatusMask::default())
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldType>(&topic, writer_qos, None, StatusMask::default())
            .unwrap();

        // Set StatusConditions with different status masks (using only actually implemented statuses)
        let condition1 = reader.get_statuscondition().unwrap().clone();
        condition1.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap(); // trigger X

        let condition2 = writer.get_statuscondition().unwrap().clone();
        condition2.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap(); // trigger O

        // Add multiple StatusConditions to WaitSet
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition1).unwrap();
        wait_set.attach_condition(condition2).unwrap();

        // Check if conditions were properly added
        let conditions = wait_set.get_conditions().unwrap();
        assert_eq!(conditions.len(), 2);

        let result = wait_set.wait(Duration::from_seconds(5));
        assert!(result.is_ok());

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_multiple_status_conditions_different_entities_2() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Set StatusConditions with different status masks (using only actually implemented statuses)
        let condition1 = reader.get_statuscondition().unwrap().clone();
        condition1.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap(); // trigger O

        let condition2 = writer.get_statuscondition().unwrap().clone();
        condition2.set_enabled_statuses(StatusMask::OFFERED_INCOMPATIBLE_QOS).unwrap(); // trigger X

        // Add multiple StatusConditions to WaitSet
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition1).unwrap();
        wait_set.attach_condition(condition2).unwrap();

        // Check if conditions were properly added
        let conditions = wait_set.get_conditions().unwrap();
        assert_eq!(conditions.len(), 2);

        std::thread::sleep(std::time::Duration::from_secs(5));

        writer
            .write(
                &HelloWorldType { index: 0, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();

        let result = wait_set.wait(Duration::from_seconds(10));
        assert!(result.is_ok());

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_status_condition_multiple_masks() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Set mask combining multiple statuses (using only actually implemented statuses)
        let condition = reader.get_statuscondition().unwrap().clone();
        let combined_mask = StatusMask::SUBSCRIPTION_MATCHED
            | StatusMask::REQUESTED_INCOMPATIBLE_QOS
            | StatusMask::DATA_AVAILABLE;
        condition.set_enabled_statuses(combined_mask).unwrap();

        // Check if set mask is correct
        let enabled_statuses = condition.get_enabled_statuses().unwrap();
        assert!(enabled_statuses.contains(StatusMask::SUBSCRIPTION_MATCHED));
        assert!(enabled_statuses.contains(StatusMask::REQUESTED_INCOMPATIBLE_QOS));
        assert!(enabled_statuses.contains(StatusMask::DATA_AVAILABLE));

        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition).unwrap();

        let result = wait_set.wait(Duration::from_seconds(20));
        assert!(result.is_ok());

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_reuse_functionality() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();

        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "reuse_topic",
                "HelloWorldReuse",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber1 = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let subscriber2 = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let reader1 = subscriber1
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let reader2 = subscriber2
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos {
                    reliability: ReliabilityQosPolicy {
                        kind: ReliabilityQosPolicyKind::Reliable,
                        max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
                    },
                    ..Default::default()
                },
                None,
                StatusMask::default(),
            )
            .unwrap();

        let _writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic,
                DataWriterQos {
                    reliability: ReliabilityQosPolicy {
                        kind: ReliabilityQosPolicyKind::BestEffort,
                        max_blocking_time: Duration { sec: 0, nanosec: 100_000_000 },
                    },
                    ..Default::default()
                },
                None,
                StatusMask::default(),
            )
            .unwrap();

        let wait_set = WaitSet::new();

        // First use: reader1's condition
        let condition1 = reader1.get_statuscondition().unwrap().clone();
        condition1.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition1.clone()).unwrap();

        // First wait call
        let result1 = wait_set.wait(Duration::from_seconds(20));
        assert!(result1.is_ok());

        // condition detach
        wait_set.detach_condition(condition1).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 0);

        // Second use: reuse with reader2's condition (using actually implemented status)
        let condition2 = reader2.get_statuscondition().unwrap().clone();
        condition2.set_enabled_statuses(StatusMask::REQUESTED_INCOMPATIBLE_QOS).unwrap();
        wait_set.attach_condition(condition2.clone()).unwrap();

        // Second wait call
        let result2 = wait_set.wait(Duration::from_seconds(20));
        assert!(result2.is_ok());
        // Check final state
        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_duplicate_condition_attach() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "duplicate_topic",
                "HelloWorldDup",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();

        let wait_set = WaitSet::new();

        // Attempt to attach same condition multiple times
        wait_set.attach_condition(condition.clone()).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);

        // Duplicate attach succeeds but is not actually added
        wait_set.attach_condition(condition.clone()).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);

        wait_set.attach_condition(condition.clone()).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_waitset_detach_nonexistent_condition() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorldType>(
                "detach_topic",
                "HelloWorldDetach",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();

        let wait_set = WaitSet::new();

        // Attempt to detach condition that was not added
        let result = wait_set.detach_condition(condition.clone());
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::PreconditionNotMet);

        // Detach after normal addition
        wait_set.attach_condition(condition.clone()).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);

        wait_set.detach_condition(condition.clone()).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 0);

        // Attempt to detach already detached condition again
        let result = wait_set.detach_condition(condition);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::PreconditionNotMet);

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    /// Helper: a fresh guard condition already attached to `wait_set`.
    fn attach_guard(wait_set: &WaitSet) -> Arc<GuardCondition> {
        let guard = Arc::new(GuardCondition::new());
        let condition: Arc<dyn Condition + Send + Sync> = guard.clone();
        wait_set.attach_condition(condition).unwrap();
        guard
    }

    /// `attach_condition` notifies when the condition being attached is already triggered. That
    /// notification used to poke the condvar without touching the notify state, which a waiter
    /// cannot tell apart from a spurious wake-up, so it re-blocked without ever looking at the
    /// new condition. With an infinite timeout the waiter never woke at all.
    #[test]
    fn attaching_a_triggered_condition_wakes_a_parked_waiter() {
        let wait_set = Arc::new(WaitSet::new());
        attach_guard(&wait_set); // attached, never triggered

        let waiter = {
            let wait_set = wait_set.clone();
            std::thread::spawn(move || wait_set.wait(Duration::from_seconds(30)))
        };

        // Far beyond the park latency: the waiter is provably blocked on the condvar.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let late = Arc::new(GuardCondition::new());
        late.set_trigger_value(true).unwrap();
        let condition: Arc<dyn Condition + Send + Sync> = late.clone();

        let start = std::time::Instant::now();
        wait_set.attach_condition(condition).unwrap();

        let triggered = waiter.join().unwrap().expect("attach must wake the parked waiter");
        assert_eq!(triggered.len(), 1);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "waiter re-blocked instead of rescanning after attach; took {:?}",
            start.elapsed()
        );
    }

    /// The old trigger flag was consume-once: `swap(false)` meant the first waiter to wake took
    /// the notification and any other waiter on the same WaitSet re-blocked and timed out. The
    /// generation counter is compared, not consumed, so every waiter observes every notification.
    #[test]
    fn every_waiter_observes_the_same_trigger() {
        let wait_set = Arc::new(WaitSet::new());
        let guard = attach_guard(&wait_set);

        let waiters: Vec<_> = (0..2)
            .map(|_| {
                let wait_set = wait_set.clone();
                std::thread::spawn(move || wait_set.wait(Duration::from_seconds(10)))
            })
            .collect();

        std::thread::sleep(std::time::Duration::from_millis(300));
        guard.set_trigger_value(true).unwrap();

        for waiter in waiters {
            let triggered = waiter.join().unwrap().expect("every waiter must be woken");
            assert_eq!(triggered.len(), 1);
        }
    }

    /// A notification landing between the waiter's scan and its park used to be lost: the notifier
    /// took no lock the waiter parked on, so `notify_all` reached nobody and the waiter slept until
    /// its timeout -- forever, with an infinite one. The park decision and the generation bump now
    /// happen under the same lock, so the window is closed.
    ///
    /// This races the window deliberately and repeatedly, with 400 attached conditions to stretch
    /// the scan the trigger has to land inside. It is a cheap guard, not a proof: the pre-fix code
    /// also passes it on Windows, where `Condvar` wakes spuriously often enough to reach the old
    /// post-wake recovery path, so the bug shows up there as a stall rather than a hang. The
    /// deterministic evidence for the notify state being correct is in the two tests above.
    #[test]
    fn a_trigger_racing_the_park_is_not_lost() {
        const ROUNDS: usize = 200;
        const FILLER: usize = 400;

        for round in 0..ROUNDS {
            let wait_set = Arc::new(WaitSet::new());
            for _ in 0..FILLER {
                attach_guard(&wait_set);
            }
            let guard = attach_guard(&wait_set);

            let waiter = {
                let wait_set = wait_set.clone();
                std::thread::spawn(move || wait_set.wait(Duration::from_millis(2000)))
            };

            guard.set_trigger_value(true).unwrap();

            let start = std::time::Instant::now();
            let triggered = waiter
                .join()
                .unwrap()
                .unwrap_or_else(|e| panic!("round {round}: wake-up lost ({e:?})"));
            assert_eq!(triggered.len(), 1);
            assert!(
                start.elapsed() < std::time::Duration::from_secs(1),
                "round {}: waiter stalled for {:?}",
                round,
                start.elapsed()
            );
        }
    }

    /// Production shape: 400 conditions on one WaitSet, all becoming triggered in the same burst.
    /// Only the first notification finds a parked thread, so the rest are collapsed; the waiter
    /// still has to come back with everything that is triggered by the time it rescans, and a
    /// follow-up wait must report the full set.
    #[test]
    fn four_hundred_conditions_are_all_reported() {
        const N: usize = 400;

        let wait_set = Arc::new(WaitSet::new());
        let guards: Vec<_> = (0..N).map(|_| attach_guard(&wait_set)).collect();

        let waiter = {
            let wait_set = wait_set.clone();
            std::thread::spawn(move || wait_set.wait(Duration::from_seconds(30)))
        };
        std::thread::sleep(std::time::Duration::from_millis(300));

        let start = std::time::Instant::now();
        for guard in &guards {
            guard.set_trigger_value(true).unwrap();
        }

        let triggered = waiter.join().unwrap().expect("the burst must wake the waiter");
        assert!(!triggered.is_empty());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "burst took {:?}",
            start.elapsed()
        );

        // Everything is latched, so the next wait returns the full set from its immediate check.
        assert_eq!(wait_set.wait(Duration::from_seconds(5)).unwrap().len(), N);
    }

    /// Deleting the entity a condition belongs to must wake a thread parked in `wait` on it,
    /// rather than leaving it hung. The enabled status never fires, so without the fix the waiter
    /// would block until the timeout.
    #[test]
    fn deleting_a_reader_wakes_a_waiter_on_its_status_condition() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = participant
            .create_topic::<HelloWorldType>(
                "wake_on_delete",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let condition = reader.get_statuscondition().unwrap().clone();
        // A status that will not fire, so the condition stays untriggered.
        condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap();

        let wait_set = Arc::new(WaitSet::new());
        wait_set.attach_condition(condition).unwrap();

        let waiter = {
            let wait_set = wait_set.clone();
            std::thread::spawn(move || wait_set.wait(Duration::from_seconds(10)))
        };

        // Far beyond the park latency: the waiter is provably blocked on the condvar.
        std::thread::sleep(std::time::Duration::from_millis(300));

        let start = std::time::Instant::now();
        subscriber.delete_datareader(reader).unwrap();
        let result = waiter.join().unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "deleting the reader did not wake the waiter; it blocked for {elapsed:?}"
        );
        let conditions = result.expect("wait should wake and return rather than time out");
        assert!(conditions.is_empty(), "the deleted condition must not be reported as triggered");

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
}
