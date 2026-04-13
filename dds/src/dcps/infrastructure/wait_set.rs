//! WaitSet for waiting on multiple conditions.
//!
//! # Example
//!
//! ```no_run
//! # use int2dds::domain::domain_participant_factory::DomainParticipantFactory;
//! # use int2dds::infrastructure::wait_set::WaitSet;
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
//! # let participant = factory.create_participant(0, Default::default(), None, Default::default()).unwrap();
//! # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", Default::default(), None, Default::default()).unwrap();
//! # let subscriber = participant.create_subscriber(Default::default(), None, Default::default()).unwrap();
//! # let reader = subscriber.create_datareader::<MyData>(&topic, Default::default(), None, Default::default()).unwrap();
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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
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

pub struct WaitSet {
    conditions: Arc<Mutex<Vec<Arc<dyn Condition + Send + Sync>>>>,
    condvar: Arc<Condvar>,
    waiting_count: Arc<AtomicUsize>,
    trigger_flag: Arc<AtomicBool>,
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

        Self {
            conditions: Arc::new(Mutex::new(Vec::new())),
            condvar: Arc::new(Condvar::new()),
            waiting_count: Arc::new(AtomicUsize::new(0)),
            trigger_flag: Arc::new(AtomicBool::new(false)),
            instance_id,
        }
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

        // Check for duplicate conditions
        for existing_condition in conditions.iter() {
            if Arc::ptr_eq(existing_condition, &new_condition) || {
                std::ptr::eq(
                    existing_condition.as_ref() as *const dyn Condition as *const (),
                    new_condition.as_ref() as *const dyn Condition as *const (),
                ) || format!("{:?}", existing_condition) == format!("{:?}", new_condition)
            } {
                debug!("[WaitSet-{}] Condition already attached, skipping", self.instance_id);
                return Ok(());
            }
        }

        // Check trigger value and notify only when waiting threads exist
        let waiting_threads = self.waiting_count.load(Ordering::Acquire);
        let should_notify =
            waiting_threads > 0 && new_condition.get_trigger_value().unwrap_or(false);

        let notify_fn = self.get_notify();

        new_condition.set_waitset_callback(Some(Arc::new(move || {
            notify_fn();
        })));
        conditions.push(new_condition);
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
            self.condvar.notify_one();
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

        // Try Arc::ptr_eq first, then use StatusCondition's PartialEq if failed
        let pos = conditions.iter().position(|c| {
            Arc::ptr_eq(c, &remove_condition) || {
                // Compare with StatusCondition PartialEq
                std::ptr::eq(
                    c.as_ref() as *const dyn Condition as *const (),
                    remove_condition.as_ref() as *const dyn Condition as *const (),
                ) || format!("{:?}", c) == format!("{:?}", remove_condition)
            }
        });

        if let Some(pos) = pos {
            conditions[pos].set_waitset_callback(None);
            conditions.remove(pos);
            Ok(())
        } else {
            Err(DdsError::PreconditionNotMet)
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

        // Check immediately triggered conditions
        let triggered_conditions = self.check_triggered_conditions()?;
        if !triggered_conditions.is_empty() {
            info!(
                "[WaitSet-{}] Immediate trigger detected: {} conditions triggered",
                self.instance_id,
                triggered_conditions.len()
            );
            self.trigger_flag.store(false, Ordering::Release);
            return Ok(triggered_conditions);
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
            if self.trigger_flag.swap(false, Ordering::AcqRel) {
                debug!("[WaitSet-{}] trigger_flag detected, checking conditions", self.instance_id);

                // Check conditions
                let triggered_conditions = self.check_triggered_conditions()?;
                if !triggered_conditions.is_empty() {
                    debug!(
                        "[WaitSet-{}] Conditions triggered during wait: {} conditions",
                        self.instance_id,
                        triggered_conditions.len()
                    );
                    return Ok(triggered_conditions);
                }
                // A wake-up can be stale if the condition was cleared before we checked it.
                // After consuming the flag, fall through to the normal wait/timeout path.
            }

            let conditions = self
                .conditions
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

            // trigger_flag is false, wait for notification
            match deadline {
                None => {
                    let _conditions = self.condvar.wait(conditions).map_err(|e| {
                        DdsError::Error(format!("Condition variable wait failed: {}", e))
                    })?;
                }
                Some(deadline_time) => {
                    let now = Instant::now();
                    if now >= deadline_time {
                        debug!("[WaitSet-{}] Wait timeout reached", self.instance_id);
                        return Err(DdsError::Timeout);
                    }
                    let remaining_time = deadline_time - now;
                    let (_conditions, _wait_result) =
                        self.condvar.wait_timeout(conditions, remaining_time).map_err(|e| {
                            DdsError::Error(format!("Condition variable wait failed: {}", e))
                        })?;
                }
            };

            // After waking, check trigger_flag first before checking timeout
            // This handles the race condition where notification was sent
            // between trigger_flag check and condvar.wait() entry
            if self.trigger_flag.load(Ordering::Acquire) {
                continue;
            }

            // Only return timeout if trigger_flag is still false
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

        Ok(conditions.clone())
    }

    /// Function to check and return triggered conditions
    fn check_triggered_conditions(&self) -> DdsResult<Vec<Arc<dyn Condition + Send + Sync>>> {
        let conditions = self
            .conditions
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

        if conditions.is_empty() {
            return Ok(Vec::new());
        }

        let mut triggered = Vec::new();
        for (idx, condition) in conditions.iter().enumerate() {
            match condition.get_trigger_value() {
                Ok(true) => {
                    debug!("[WaitSet-{}] Condition #{} TRIGGERED", self.instance_id, idx + 1);
                    triggered.push(Arc::clone(condition));
                }
                Ok(false) => continue,
                Err(e) => {
                    debug!(
                        "[WaitSet-{}] Condition #{} check failed: {:?}",
                        self.instance_id,
                        idx + 1,
                        e
                    );
                    continue;
                }
            }
        }

        if !triggered.is_empty() {
            debug!(
                "[WaitSet-{}] Total triggered conditions: {}/{}",
                self.instance_id,
                triggered.len(),
                conditions.len()
            );
        }

        Ok(triggered)
    }

    pub(crate) fn get_notify(&self) -> Arc<dyn Fn() + Send + Sync> {
        let condvar = self.condvar.clone();
        let trigger_flag = self.trigger_flag.clone();
        let instance_id = self.instance_id;

        Arc::new(move || {
            trigger_flag.store(true, Ordering::Release);
            debug!("[WaitSet-{}] Notifying waiting threads", instance_id);
            condvar.notify_all();
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        common::instance_handle::InstanceHandle,
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            qos_policy::{
                HistoryQosPolicy, HistoryQosPolicyKind, ReliabilityQosPolicy,
                ReliabilityQosPolicyKind,
            },
            status::StatusMask,
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
        let mut condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition).unwrap();
        let res = wait_set.wait(Duration::from_seconds(10));

        assert!(res.is_ok());
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
        let mut condition = reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition).unwrap();
        let result = wait_set.wait(Duration::from_seconds(5));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::Timeout);
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
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
        let mut condition1 = reader.get_statuscondition().unwrap().clone();
        condition1.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap(); // trigger X

        let mut condition2 = writer.get_statuscondition().unwrap().clone();
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
        let mut condition1 = reader.get_statuscondition().unwrap().clone();
        condition1.set_enabled_statuses(StatusMask::DATA_AVAILABLE).unwrap(); // trigger O

        let mut condition2 = writer.get_statuscondition().unwrap().clone();
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
        let mut condition = reader.get_statuscondition().unwrap().clone();
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
        let mut condition1 = reader1.get_statuscondition().unwrap().clone();
        condition1.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition1.clone()).unwrap();

        // First wait call
        let result1 = wait_set.wait(Duration::from_seconds(20));
        assert!(result1.is_ok());

        // condition detach
        wait_set.detach_condition(condition1).unwrap();
        assert_eq!(wait_set.get_conditions().unwrap().len(), 0);

        // Second use: reuse with reader2's condition (using actually implemented status)
        let mut condition2 = reader2.get_statuscondition().unwrap().clone();
        condition2.set_enabled_statuses(StatusMask::REQUESTED_INCOMPATIBLE_QOS).unwrap();
        wait_set.attach_condition(condition2.clone()).unwrap();

        // Second wait call
        let result2 = wait_set.wait(Duration::from_seconds(20));
        assert!(result2.is_ok());
        // Check final state
        assert_eq!(wait_set.get_conditions().unwrap().len(), 1);
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

        let mut condition = reader.get_statuscondition().unwrap().clone();
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

        let mut condition = reader.get_statuscondition().unwrap().clone();
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
    }
}
