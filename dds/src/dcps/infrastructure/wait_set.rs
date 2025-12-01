//! WaitSet for waiting on multiple conditions.
//!
//! # Example
//!
//! ```no_run
//! # use int2dds::dcps::domain::DomainParticipantFactory;
//! # use int2dds::infrastructure::wait_set::WaitSet;
//! # use int2dds::infrastructure::status::StatusMask;
//! # use int2dds::core::time::Duration;
//! # use int2dds::topic::type_support::DdsType;
//! # use int2dds::subscription::sample_info::SampleStateKind;
//! # use int2dds::subscription::sample_info::ViewStateKind;
//! # use int2dds::subscription::sample_info::InstanceStateKind;
//! # use int2dds::core::types::LENGTH_UNLIMITED;
//! # #[derive(Clone, DdsType)]
//! # struct MyData { id: u32 }
//! # let factory = DomainParticipantFactory::get_instance();
//! # let participant = factory.create_participant(0, Default::default(), None, Default::default())?;
//! # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", Default::default(), None, Default::default())?;
//! # let subscriber = participant.create_subscriber(Default::default(), None, Default::default())?;
//! # let reader = subscriber.create_datareader::<MyData>(&topic, Default::default(), None, Default::default())?;
//! let mut wait_set = WaitSet::new();
//!
//! // Attach a condition for data availability
//! let mut condition = reader.get_statuscondition()?.clone();
//! condition.set_enabled_statuses(StatusMask::DATA_AVAILABLE)?;
//! wait_set.attach_condition(condition)?;
//!
//! // Wait indefinitely for data
//! loop {
//!     let triggered = wait_set.wait(Duration::infinite())?;
//!     println!("Condition triggered, {} conditions active", triggered.len());
//!
//!     // Read the available data
//!     let samples = reader.take(
//!         LENGTH_UNLIMITED,
//!         &[SampleStateKind::ANY_SAMPLE_STATE],
//!         &[ViewStateKind::ANY_VIEW_STATE],
//!         &[InstanceStateKind::ANY_INSTANCE_STATE]
//!     )?;
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
            if self.trigger_flag.load(Ordering::Acquire) {
                debug!("[WaitSet-{}] trigger_flag detected, checking conditions", self.instance_id);

                // Reset trigger_flag
                self.trigger_flag.store(false, Ordering::Release);

                // Check conditions
                let triggered_conditions = self.check_triggered_conditions()?;
                if !triggered_conditions.is_empty() {
                    info!(
                        "[WaitSet-{}] Conditions triggered during wait: {} conditions",
                        self.instance_id,
                        triggered_conditions.len()
                    );
                    return Ok(triggered_conditions);
                }
            }

            let conditions = self
                .conditions
                .lock()
                .map_err(|e| DdsError::Error(format!("Failed to lock conditions: {}", e)))?;

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

            // Final check if timeout exists
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
        let waiting_count = self.waiting_count.clone();
        let trigger_flag = self.trigger_flag.clone();
        let instance_id = self.instance_id;

        Arc::new(move || {
            let waiting_threads = waiting_count.load(Ordering::Acquire);
            if waiting_threads > 0 {
                debug!("[WaitSet-{}] Notifying {} waiting threads", instance_id, waiting_threads);
                condvar.notify_all();
            }
            trigger_flag.store(true, Ordering::Release);
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
            qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
            status::StatusMask,
            wait_set::WaitSet,
        },
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::qos::TopicQos,
        DdsType,
    };
    use speedy::{Readable, Writable};

    #[derive(DdsType, Readable, Writable)]
    #[dds_type(crate_path = "crate")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[test]
    fn test_waitset_matching() {
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();
        let domain_id = 13;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
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
        wait_set.wait(Duration::infinite()).unwrap();
    }

    #[test]
    fn test_waitset_timeout() {
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();
        let domain_id = 31;
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
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();

        let domain_id = 23;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
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
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();

        let domain_id = 23;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
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
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();

        let domain_id = 33;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
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
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();

        let domain_id = 34;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub1 = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let participant_sub2 = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub1 = participant_sub1
            .create_topic::<HelloWorldType>(
                "reuse_topic",
                "HelloWorldReuse",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic_sub2 = participant_sub2
            .create_topic::<HelloWorldType>(
                "reuse_topic",
                "HelloWorldReuse",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "reuse_topic",
                "HelloWorldReuse",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber1 = participant_sub1
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let subscriber2 = participant_sub2
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let reader1 = subscriber1
            .create_datareader::<HelloWorldType>(
                &topic_sub1,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let reader2 = subscriber2
            .create_datareader::<HelloWorldType>(
                &topic_sub2,
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
                &topic_pub,
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
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();

        let domain_id = 35;
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
        let _ = env_logger::builder()
            .filter_module("int2dds::dds::infrastructure::wait_set", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Error)
            .try_init();

        let domain_id = 36;
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

    #[test]
    fn test_waitset_with_readcondition() {
        let _ = env_logger::builder()
            .filter_module("int2dds::dds", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Info)
            .try_init();

        let domain_id = 57;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "read_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "read_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap(); // trigger O

        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Create ReadCondition: NOT_READ samples only
        let read_condition = reader
            .create_readcondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();

        // Connect ReadCondition to WaitSet
        wait_set.attach_condition(read_condition.clone()).unwrap();

        writer
            .write(
                &HelloWorldType { index: 0, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 1, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();

        // Wait for data arrival
        let result = wait_set.wait(Duration::from_seconds(10));
        assert!(result.is_ok());

        if result.is_ok() {
            let active_conditions = result.unwrap();

            // Check activated conditions
            for _condition in active_conditions {
                let samples = reader.read_w_condition(10, read_condition.clone()).unwrap();
                assert!(!samples.is_empty());

                for sample in samples.iter() {
                    if sample.sample_info().valid_data {
                        log::info!("Received: {:?}", sample.data());
                    }
                }
            }
        }

        assert_eq!(read_condition.get_trigger_value(), Ok(false));
    }

    #[test]
    fn test_waitset_with_querycondition() {
        let _ = env_logger::builder()
            .filter_module("int2dds::dds", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Info)
            .try_init();

        let domain_id = 67;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "query_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "query_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap(); // trigger O

        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Create QueryCondition: samples where id > 100 only
        let query_condition = reader
            .create_querycondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                "index > %0",            // query expression
                vec!["100".to_string()], // query parameters
            )
            .unwrap();

        // Connect QueryCondition to WaitSet
        wait_set.attach_condition(query_condition.clone()).unwrap();

        writer
            .write(
                &HelloWorldType { index: 0, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 100, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 200, message: "hello world!".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();

        // Wait for data arrival that satisfies condition
        let result = wait_set.wait(Duration::from_seconds(5));
        assert!(result.is_ok());

        if result.is_ok() {
            let active_conditions = result.unwrap();

            // Check activated conditions
            for _condition in active_conditions {
                let samples = reader.read_w_condition(10, query_condition.clone()).unwrap();
                assert_eq!(samples.len(), 1);

                for sample in samples.iter() {
                    if sample.sample_info().valid_data {
                        log::info!("Received: {:?}", sample.data());
                    }
                }
            }
        }

        assert_eq!(query_condition.get_trigger_value(), Ok(false));
    }

    #[test]
    fn test_waitset_with_querycondition_order_by() {
        let _ = env_logger::builder()
            .filter_module("int2dds::dds", log::LevelFilter::Debug)
            .filter_level(log::LevelFilter::Info)
            .try_init();

        let domain_id = 77;
        let factory = DomainParticipantFactory::get_instance();
        let participant_sub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let participant_pub = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_sub = participant_sub
            .create_topic::<HelloWorldType>(
                "order_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_pub = participant_pub
            .create_topic::<HelloWorldType>(
                "order_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant_sub
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic_sub,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant_pub
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorldType>(
                &topic_pub,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Wait for matching
        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Create QueryCondition for ORDER BY test: index > 0 ORDER BY message, index
        let query_condition = reader
            .create_querycondition(
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ALIVE_INSTANCE_STATE],
                "index > %0 ORDER BY message, index",
                vec!["0".to_string()], // index > 0 (all samples)
            )
            .unwrap();

        // Connect QueryCondition to WaitSet
        wait_set.attach_condition(query_condition.clone()).unwrap();

        // Send various data in order (for checking sort results)
        writer
            .write(
                &HelloWorldType { index: 300, message: "charlie".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 100, message: "alpha".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(&HelloWorldType { index: 200, message: "beta".to_string() }, InstanceHandle::NIL)
            .unwrap();
        writer
            .write(
                &HelloWorldType { index: 150, message: "alpha".to_string() },
                InstanceHandle::NIL,
            )
            .unwrap();
        writer
            .write(&HelloWorldType { index: 50, message: "beta".to_string() }, InstanceHandle::NIL)
            .unwrap();

        // Wait for data arrival
        let result = wait_set.wait(Duration::from_seconds(10));
        assert!(result.is_ok());
        std::thread::sleep(std::time::Duration::from_secs(10));

        if result.is_ok() {
            let active_conditions = result.unwrap();

            for _condition in active_conditions {
                let samples = reader.read_w_condition(10, query_condition.clone()).unwrap();
                assert_eq!(samples.len(), 5);

                log::info!("=== ORDER BY Test Results ===");
                log::info!("Expected order: ORDER BY message, index");
                log::info!("Should be: alpha(100), alpha(150), beta(50), beta(200), charlie(300)");

                // Check ORDER BY message, index results
                let expected_order = vec![
                    (100, "alpha"),   // message: alpha, index: 100
                    (150, "alpha"),   // message: alpha, index: 150
                    (50, "beta"),     // message: beta, index: 50
                    (200, "beta"),    // message: beta, index: 200
                    (300, "charlie"), // message: charlie, index: 300
                ];

                for (i, sample) in samples.iter().enumerate() {
                    if sample.sample_info().valid_data {
                        let data = sample.data().unwrap();
                        let (expected_index, expected_message) = expected_order[i];

                        log::info!(
                            "Sample {}: index={}, message='{}' (expected: index={}, message='{}')",
                            i + 1,
                            data.index,
                            data.message,
                            expected_index,
                            expected_message
                        );

                        // Verify ORDER BY sorting is correct
                        assert_eq!(data.index, expected_index, "Index mismatch at position {}", i);
                        assert_eq!(
                            data.message, expected_message,
                            "Message mismatch at position {}",
                            i
                        );
                    }
                }
            }
        }

        assert_eq!(query_condition.get_trigger_value(), Ok(false));
        log::info!("=== ORDER BY Test Completed Successfully ===");
    }
}
