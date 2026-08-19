//! StatusCondition - Condition triggered by entity status changes.
//!
//! A `StatusCondition` is a condition object that is triggered when the status of its
//! associated entity changes. Each DDS entity has an associated status condition that
//! can be retrieved via `get_statuscondition()`.
//!
//! StatusConditions can be attached to a `WaitSet` and configured to monitor specific
//! status changes using `set_enabled_statuses()`. When one of the enabled statuses
//! changes, the condition becomes triggered and wakes any waiting threads.
//!
//! This provides an alternative to listeners for monitoring entity status changes,
//! particularly useful when integrating DDS events into an event loop or reactor pattern.

use std::{
    any::Any,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, Weak,
    },
};

use log::debug;

use crate::{
    core::error::{DdsError, DdsResult},
    infrastructure::{
        condition::{impl_dds_condition, impl_dds_condition_impl, ConditionInternal},
        entity::EntityInternal,
        status::StatusMask,
    },
};

use super::{condition::Condition, entity::Entity, status::StatusKind};

/// `StatusMask` is `bitflags` over `u32`, and `bits()` / `from_bits_retain()` are exact inverses,
/// so a mask round-trips through an `AtomicU32` unchanged. `from_bits_truncate` is deliberately
/// not used: it would drop bits the previous `Mutex<StatusMask>` stored verbatim.
#[inline]
fn load_mask(cell: &AtomicU32) -> StatusMask {
    StatusMask::from_bits_retain(cell.load(Ordering::Acquire))
}

#[derive(Clone)]
pub struct StatusCondition<Q> {
    entity: Option<Weak<dyn EntityInternal<Qos = Q> + Send + Sync>>,
    /// Raw bits of the enabled-status mask (the list of statuses this condition monitors).
    ///
    /// Not behind a `Mutex` because `WaitSet::check_triggered_conditions` reads it once per
    /// attached condition on every wake-up. A mutex makes each of those reads a lock-prefixed
    /// read-modify-write, so at 400 conditions the scanning thread writes to 400 cache lines it
    /// only wanted to read, evicting them from every core that touches the same conditions. As an
    /// atomic it is genuinely read-only in steady state and the lines stay shared.
    pub(crate) enabled_statuses: Arc<AtomicU32>,
    /// Raw bits of the changed-status mask. Same reasoning, plus this one is also written by every
    /// reader under a participant, twice per received sample, on the participant's shared
    /// condition.
    pub(crate) status_changes: Arc<AtomicU32>,
    #[allow(clippy::type_complexity)]
    waitset_callback: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
    // Set true when the owning entity is deleted, so a WaitSet reports this condition as
    // triggered and drops it instead of leaving a waiter hung.
    is_dead: Arc<AtomicBool>,
}

impl<Q> PartialEq for StatusCondition<Q> {
    fn eq(&self, other: &Self) -> bool {
        // If Arc pointers are the same, consider as same StatusCondition
        Arc::ptr_eq(&self.enabled_statuses, &other.enabled_statuses)
            && Arc::ptr_eq(&self.status_changes, &other.status_changes)
    }
}

impl<Q> Debug for StatusCondition<Q> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusCondition")
            .field("entity", &self.entity.as_ref().map(|_| "Weak<EntityInternal>"))
            .field("enabled_statuses", &load_mask(&self.enabled_statuses))
            .field("status_changes", &load_mask(&self.status_changes))
            .field(
                "waitset_callback",
                &self.waitset_callback.lock().unwrap().as_ref().map(|_| "Arc<dyn Fn(bool)>"),
            )
            .finish()
    }
}

impl_dds_condition!(StatusCondition<Q>, Q: Debug + 'static);
impl<Q: Debug + 'static> Condition for StatusCondition<Q> {
    fn get_trigger_value(&self) -> DdsResult<bool> {
        let enabled_statuses = self.get_enabled_statuses()?;
        debug!("enabled_statuses: {:?}", enabled_statuses);

        let status_changes = self.get_status_changes()?;
        debug!("status_changes in get_trigger_value: {:?}", status_changes);

        let result = !(enabled_statuses & status_changes).is_empty();
        debug!("enabled_statuses & status_changes: {:?}", enabled_statuses & status_changes);
        debug!("result calculation: {:?}", result);

        Ok(result)
    }
}

impl<Q: Debug> From<StatusCondition<Q>> for Arc<dyn Condition + Send + Sync>
where
    StatusCondition<Q>: Condition + Send + Sync + 'static,
{
    fn from(condition: StatusCondition<Q>) -> Self {
        Arc::new(condition) as Arc<dyn Condition + Send + Sync>
    }
}

impl<Q: Debug> StatusCondition<Q> {
    pub(crate) fn new(entity: Option<Weak<dyn EntityInternal<Qos = Q> + Send + Sync>>) -> Self {
        Self {
            entity,
            // Routed through `StatusMask::default()` rather than a literal so `status.rs` stays
            // the single source of truth: the default is every status, not none.
            enabled_statuses: Arc::new(AtomicU32::new(StatusMask::default().bits())),
            status_changes: Arc::new(AtomicU32::new(StatusMask::empty().bits())),
            waitset_callback: Arc::new(Mutex::new(None)),
            is_dead: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_enabled_statuses(&self, mask: StatusMask) -> DdsResult<()> {
        self.enabled_statuses.store(mask.bits(), Ordering::Release);

        // The trigger value is `enabled_statuses & status_changes`, so widening the mask can make
        // an already-attached condition triggered without any further status change. Nothing else
        // will notice: `add_communication_status` only fires when a status arrives, and a thread
        // already blocked in `WaitSet::wait` has passed its immediate check. Re-evaluate here and
        // wake it, or the wake-up is lost until the next unrelated status change.
        //
        // Reads the two masks separately, matching the lock ordering `get_trigger_value` and
        // `add_communication_status` use, and holds neither while invoking the callback.
        let enabled_statuses = self.get_enabled_statuses()?;
        let status_changes = self.get_status_changes()?;

        if !(enabled_statuses & status_changes).is_empty() {
            debug!("set_enabled_statuses made the condition triggered, notifying");
            if let Ok(callback) = self.waitset_callback.lock() {
                if let Some(callback) = callback.as_ref() {
                    callback();
                }
            }
        }

        Ok(())
    }

    pub fn get_enabled_statuses(&self) -> DdsResult<StatusMask> {
        Ok(load_mask(&self.enabled_statuses))
    }

    pub fn get_entity(&self) -> DdsResult<Box<dyn Entity<Qos = Q> + Send + Sync>> {
        if let Some(weak_ref) = self.entity.as_ref() {
            if let Some(entity_arc) = weak_ref.upgrade() {
                return Ok(entity_arc.clone_box());
            }
        }

        // If entity is None or reference is expired
        Err(DdsError::Error("Entity reference is invalid or expired".to_string()))
    }

    pub(crate) fn get_status_changes(&self) -> DdsResult<StatusMask> {
        let status_changes = load_mask(&self.status_changes);
        debug!("get_status_changes called: {:?}", status_changes);
        Ok(status_changes)
    }

    pub(crate) fn set_communication_status(
        &self,
        status: &StatusKind,
        trigger_value: bool,
    ) -> DdsResult<()> {
        debug!("status {:?} - {}", status, trigger_value);
        if trigger_value {
            self.add_communication_status(status)
        } else {
            self.remove_communication_status(status)
        }
    }

    fn remove_communication_status(&self, status: &StatusKind) -> DdsResult<()> {
        // `StatusMask::remove` is `bits & !other.bits()` on the raw integer, not `self & !other`:
        // the `!` operator truncates to known bits first, so it is a different function.
        let previous = self.status_changes.fetch_and(!status.bits(), Ordering::AcqRel);
        debug!(
            "removed status_changes: {:?}",
            StatusMask::from_bits_retain(previous & !status.bits())
        );
        Ok(())
    }

    fn add_communication_status(&self, status: &StatusKind) -> DdsResult<()> {
        // fetch_or, never load-then-store: every DataReader forwards both DATA_ON_READERS and
        // DATA_AVAILABLE to the participant's shared condition from unsynchronised threads, so a
        // non-atomic update would drop one bit and one wake-up with it.
        let previous = self.status_changes.fetch_or(status.bits(), Ordering::AcqRel);
        debug!("status_changes: {:?}", StatusMask::from_bits_retain(previous | status.bits()));

        // `contains` is all-of: fire only when every bit of `status` is enabled. Deliberately not
        // the any-of test `get_trigger_value` uses.
        let should_trigger = load_mask(&self.enabled_statuses).contains(*status);

        // Call callback only when a status of interest is added.
        if should_trigger {
            if let Ok(callback) = self.waitset_callback.lock() {
                if let Some(callback) = callback.as_ref() {
                    callback(); // Always true (state is activated)
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use crate::{
        core::time::Duration as DdsDuration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::{
            status::{StatusKind, StatusMask},
            status_condition::StatusCondition,
            wait_set::WaitSet,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        test_utils::unique_domain_id,
        topic::qos::TopicQos,
        DdsType,
    };

    /// Widening the enabled mask can make an attached condition triggered with no further status
    /// change. A thread already blocked in `WaitSet::wait` has passed its immediate check, so
    /// unless `set_enabled_statuses` notifies, that wake-up is lost until an unrelated status
    /// arrives -- and with `Duration::infinite()` it is lost for good.
    #[test]
    fn widening_the_enabled_mask_wakes_a_blocked_waitset() {
        // The status is already latched, but not enabled, so the condition is not triggered yet.
        let condition = StatusCondition::<DataReaderQos>::new(None);
        condition.set_enabled_statuses(StatusMask::empty()).unwrap();
        condition.set_communication_status(&StatusKind::DATA_AVAILABLE, true).unwrap();
        assert!(!condition.get_trigger_value().unwrap());

        let wait_set = std::sync::Arc::new(WaitSet::new());
        wait_set.attach_condition(condition.clone()).unwrap();

        let waiter = {
            let wait_set = wait_set.clone();
            std::thread::spawn(move || wait_set.wait(DdsDuration::from_seconds(30)))
        };

        // Far beyond the park latency, so the notification lands while the waiter is genuinely
        // blocked on the condvar rather than in the window before it parks.
        std::thread::sleep(Duration::from_millis(300));

        let start = std::time::Instant::now();
        condition.set_enabled_statuses(StatusKind::DATA_AVAILABLE).unwrap();

        let triggered = waiter.join().unwrap().expect("widening the mask must wake the waiter");
        assert_eq!(triggered.len(), 1);
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "waiter was not woken by the mask change; took {:?}",
            start.elapsed()
        );
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[test]
    // test with 31_hello_world_best_effort_publisher
    fn test_status_condition() {
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

        std::thread::sleep(Duration::from_secs(5));

        let condition = reader.get_statuscondition().unwrap();
        condition.set_enabled_statuses(StatusKind::SUBSCRIPTION_MATCHED).unwrap();
        println!("condition: {:?}", condition);

        println!("{:?}", condition.get_status_changes().unwrap());
        let reader_result = reader.status_condition.lock().unwrap().get_trigger_value().unwrap();
        let result = condition.get_trigger_value().unwrap();
        println!("condition: {:?}", condition);
        println!("reader result: {:?}", reader_result);
        println!("result: {:?}", result);
        reader.get_subscription_matched_status().unwrap();
        println!("{:?}", condition.get_status_changes().unwrap());
        let reader_result = reader.status_condition.lock().unwrap().get_trigger_value().unwrap();
        let result = condition.get_trigger_value().unwrap();
        println!("condition: {:?}", condition);
        println!("reader result: {:?}", reader_result);
        println!("result: {:?}", result);

        subscriber.delete_datareader(reader).unwrap();

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    /// A fresh StatusCondition monitors every status: `StatusMask::default()` is
    /// `StatusKind::all()` (0x7FFF), not `empty()`. Backing the mask with a zero-initialised
    /// integer would silently disable every WaitSet notification, and nothing else in the suite
    /// would notice -- every other test sets an explicit mask before it waits on anything.
    #[test]
    fn enabled_statuses_default_to_every_status() {
        let condition = StatusCondition::<DataReaderQos>::new(None);
        assert_eq!(condition.get_enabled_statuses().unwrap(), StatusMask::ALL);
        assert_eq!(condition.get_enabled_statuses().unwrap().bits(), 0x7FFF);
    }

    /// The observable consequence of that default, which the value assertion alone does not pin.
    #[test]
    fn an_unconfigured_condition_triggers_on_any_status() {
        let condition = StatusCondition::<DataReaderQos>::new(None);
        assert!(!condition.get_trigger_value().unwrap());

        condition.set_communication_status(&StatusKind::SUBSCRIPTION_MATCHED, true).unwrap();
        assert!(condition.get_trigger_value().unwrap());
    }

    /// Clearing one status must leave the others latched. `StatusMask::remove` is
    /// `bits & !other.bits()` on the raw integer, which is not the same as `self & !other`: the
    /// `!` operator truncates to known bits first.
    #[test]
    fn removing_one_status_leaves_the_others() {
        let condition = StatusCondition::<DataReaderQos>::new(None);
        condition.set_communication_status(&StatusKind::DATA_AVAILABLE, true).unwrap();
        condition.set_communication_status(&StatusKind::SUBSCRIPTION_MATCHED, true).unwrap();
        condition.set_communication_status(&StatusKind::DATA_AVAILABLE, false).unwrap();

        let changes = condition.get_status_changes().unwrap();
        assert!(!changes.contains(StatusKind::DATA_AVAILABLE));
        assert!(changes.contains(StatusKind::SUBSCRIPTION_MATCHED));
    }

    /// Every DataReader forwards DATA_ON_READERS and DATA_AVAILABLE to the participant's single
    /// StatusCondition, so concurrent additions from unsynchronised threads are the normal case at
    /// 400 readers, not an edge case. A read-modify-write that is not atomic drops one of them,
    /// and with it a wake-up.
    #[test]
    fn concurrent_status_additions_are_not_lost() {
        const ITERATIONS: usize = 2_000;

        let condition = std::sync::Arc::new(StatusCondition::<DataReaderQos>::new(None));
        let kinds = [
            StatusKind::DATA_ON_READERS,
            StatusKind::DATA_AVAILABLE,
            StatusKind::SUBSCRIPTION_MATCHED,
            StatusKind::LIVELINESS_CHANGED,
        ];

        let handles: Vec<_> = kinds
            .iter()
            .map(|kind| {
                let condition = condition.clone();
                let kind = *kind;
                std::thread::spawn(move || {
                    for _ in 0..ITERATIONS {
                        condition.set_communication_status(&kind, true).unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        let changes = condition.get_status_changes().unwrap();
        for kind in kinds {
            assert!(changes.contains(kind), "lost {:?}; got {:?}", kind, changes);
        }
    }

    /// Masks must keep rendering as status names. Storing them as raw integers would degrade every
    /// log line in this module.
    #[test]
    fn debug_renders_masks_as_status_names() {
        let condition = StatusCondition::<DataReaderQos>::new(None);
        condition.set_enabled_statuses(StatusKind::DATA_AVAILABLE).unwrap();
        condition.set_communication_status(&StatusKind::SUBSCRIPTION_MATCHED, true).unwrap();

        let rendered = format!("{:?}", condition);
        assert!(rendered.contains("DATA_AVAILABLE"), "got: {rendered}");
        assert!(rendered.contains("SUBSCRIPTION_MATCHED"), "got: {rendered}");
    }
}
