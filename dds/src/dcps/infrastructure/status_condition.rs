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
    sync::{Arc, Mutex, Weak},
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

#[derive(Clone)]
pub struct StatusCondition<Q> {
    entity: Option<Weak<dyn EntityInternal<Qos = Q> + Send + Sync>>,
    pub(crate) enabled_statuses: Arc<Mutex<StatusMask>>, // mask (list of enabled statuses)
    pub(crate) status_changes: Arc<Mutex<StatusMask>>,
    waitset_callback: Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>,
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
            .field("enabled_statuses", &self.enabled_statuses.lock().unwrap())
            .field("status_changes", &self.status_changes.lock().unwrap())
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
            enabled_statuses: Arc::new(Mutex::new(StatusMask::default())),
            status_changes: Arc::new(Mutex::new(StatusMask::empty())),
            waitset_callback: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_enabled_statuses(&mut self, mask: StatusMask) -> DdsResult<()> {
        {
            let mut enabled_statuses = match self.enabled_statuses.lock() {
                Ok(guard) => guard,
                Err(e) => return Err(DdsError::Error(e.to_string())),
            };

            *enabled_statuses = mask;
        }

        Ok(())
    }

    pub fn get_enabled_statuses(&self) -> DdsResult<StatusMask> {
        let enabled_statuses =
            self.enabled_statuses.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(*enabled_statuses)
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
        match self.status_changes.lock() {
            Ok(status_changes) => {
                debug!("get_status_changes called: {:?}", *status_changes);
                Ok(*status_changes)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
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
        match self.status_changes.lock() {
            Ok(mut status_changes) => {
                status_changes.remove(*status); //.retain(|x| x != status);
                debug!("removed status_changes: {:?}", status_changes);
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn add_communication_status(&self, status: &StatusKind) -> DdsResult<()> {
        match self.status_changes.lock() {
            Ok(mut status_changes) => {
                status_changes.insert(*status);
                debug!("status_changes: {:?}", status_changes);

                // Check if it's a status of interest in enabled_statuses
                let should_trigger = match self.enabled_statuses.lock() {
                    Ok(enabled_statuses) => enabled_statuses.contains(*status),
                    Err(e) => return Err(DdsError::Error(e.to_string())),
                };

                // Call callback only when a status of interest is added
                if should_trigger {
                    if let Ok(callback) = self.waitset_callback.lock() {
                        if let Some(callback) = callback.as_ref() {
                            callback(); // Always true (state is activated)
                        }
                    }
                }

                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {

    use std::time::Duration;

    use crate::{
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::{StatusKind, StatusMask},
        subscription::qos::{DataReaderQos, SubscriberQos},
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
    // test with 31_hello_world_best_effort_publisher
    fn test_status_condition() {
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

        std::thread::sleep(Duration::from_secs(5));

        let mut condition = reader.get_statuscondition().unwrap();
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
    }
}
