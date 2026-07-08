//! Topic - The fundamental unit of data distribution in DDS.
//!
//! A `Topic` associates a name with a data type and a set of QoS policies. It serves as the
//! link between publishers and subscribers, allowing DataWriters to publish samples and
//! DataReaders to receive them when they share the same topic name and compatible types.
//!
//! Topics are created by a `DomainParticipant` and can be shared by multiple DataWriters
//! and DataReaders within the same domain. The topic's QoS policies control aspects such
//! as reliability, durability, and resource limits for data distribution.
//!
//! # Key Features
//!
//! - **Type Safety**: Each topic is associated with a specific data type implementing `DdsType`
//! - **Name-based Discovery**: Publishers and subscribers find each other through matching topic names
//! - **QoS Policies**: Control data distribution behavior (reliability, durability, history, etc.)
//! - **Inconsistency Detection**: Monitors when multiple topics with the same name have incompatible types
//!
//! # Lifecycle
//!
//! Topics are created through `DomainParticipant::create_topic()` and must be deleted with
//! `DomainParticipant::delete_topic()` before the participant is deleted. A topic cannot be
//! deleted while DataReaders or DataWriters are still using it.

use arc_swap::ArcSwap;
use std::{
    any::Any,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
};

use super::{
    qos::TopicQos,
    topic_description::{TopicDescription, TopicDescriptionInternal},
    topic_listener::TopicListener,
};
use crate::{
    common::instance_handle::InstanceHandle,
    core::error::{DdsError, DdsResult},
    domain::domain_participant::DomainParticipant,
    infrastructure::{
        domain_entity::DomainEntity,
        entity::{
            impl_check_parent_enabled, impl_dds_entity, impl_dds_entity_impl, BaseEntity,
            EnableChild, Entity, EntityInternal, UpdateStatus,
        },
        qos_policy::Qos,
        status::{InconsistentTopicStatus, StatusInfo, StatusKind, StatusMask},
        status_condition::StatusCondition,
    },
    rtps::common::guid::Guid,
    topic::topic_description::{impl_topic_description, impl_topic_description_impl},
};

#[derive(Clone)]
pub struct Topic {
    // Indicates whether this entity is a built-in entity.
    //
    // Built-in entities are managed internally and have restricted operations:
    // - Cannot be deleted (delete_topic)
    // - Cannot modify QoS (set_qos)
    //
    // See also: DomainParticipant::get_builtin_subscriber()
    is_builtin: bool,
    guid: Guid,
    qos: Arc<ArcSwap<TopicQos>>,
    update_lock: Arc<Mutex<()>>,
    listener: Arc<RwLock<Option<Arc<dyn TopicListener>>>>,
    mask: Arc<RwLock<StatusMask>>,
    status_condition: Arc<Mutex<StatusCondition<TopicQos>>>,
    pub(crate) self_ref: Option<Arc<Topic>>,
    enabled: Arc<AtomicBool>,
    deleted: Arc<AtomicBool>,
    topic_name: String,
    type_name: String,
    participant: Option<Weak<DomainParticipant>>,
    inconsistent_topic_status: Arc<Mutex<InconsistentTopicStatus>>,
}

impl Debug for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Topic")
            .field("guid", &self.guid)
            .field("qos", &**self.qos.load())
            .field(
                "listener",
                &self.listener.read().unwrap().as_ref().map(|_| "Arc<dyn TopicListener>"),
            )
            .field("mask", &self.mask.read().unwrap())
            .field("status_condition", &self.status_condition.lock().unwrap())
            .field("self_ref", &self.self_ref.as_ref().map(|_| "Arc<Topic>"))
            .field("enabled", &self.enabled.load(std::sync::atomic::Ordering::Acquire))
            .field("deleted", &self.deleted.load(std::sync::atomic::Ordering::Acquire))
            .field("inconsistent_topic_status", &self.inconsistent_topic_status.lock().unwrap())
            .finish()
    }
}

impl PartialEq for Topic {
    fn eq(&self, other: &Self) -> bool {
        self.guid() == other.guid()
    }
}
impl Eq for Topic {}

impl Drop for Topic {
    fn drop(&mut self) {
        // Builtin entities are managed separately, skip orphan handling
        if self.is_builtin {
            return;
        }

        // Only handle drop for the last reference (not clones)
        if let Some(ref self_arc) = self.self_ref {
            if Arc::strong_count(self_arc) > 1 {
                return; // This is a clone, skip orphan handling
            }
        } else {
            return; // Never fully initialized
        }

        if !self.deleted.load(Ordering::SeqCst) {
            if let Ok(ref participant) = self.get_participant() {
                let topic_handle = InstanceHandle::from_guid(&self.guid);
                participant.handle_topic_drop(&topic_handle);
            }
        }
    }
}

impl_topic_description!(Topic);
impl_dds_entity!(Topic, TopicQos);
impl EnableChild for Topic {
    impl_check_parent_enabled!(get_participant);
}
impl DomainEntity for Topic {}
impl UpdateStatus for Topic {
    fn update_status(
        &self,
        status: StatusKind,
        info: Option<Arc<dyn StatusInfo>>,
    ) -> DdsResult<()> {
        if info.is_some() {
            return Err(DdsError::BadParameter);
        }
        match status {
            StatusKind::INCONSISTENT_TOPIC => {
                #[cfg(test)]
                log::info!("Topic's update_status called!");
                #[cfg(not(test))]
                log::debug!("Topic's update_status called!");
                self.handle_inconsistent_topic_status()
            }
            status => {
                log::error!("Unknown status received for Topic - StatusKind: {:?}", status);
                Err(DdsError::BadParameter)
            }
        }
    }
}

impl Topic {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        is_builtin: bool,
        topic_name: &str,
        type_name: &str,
        qos: TopicQos,
        listener: Option<Arc<dyn TopicListener>>,
        mask: StatusMask,
        handle: InstanceHandle,
        participant: &Arc<DomainParticipant>,
    ) -> Self {
        let mut topic = Self {
            is_builtin,
            guid: handle.to_guid(),
            qos: Arc::new(ArcSwap::from_pointee(qos)),
            update_lock: Arc::new(Mutex::new(())),
            listener: Arc::new(RwLock::new(listener)),
            mask: Arc::new(RwLock::new(mask)),
            status_condition: Arc::new(Mutex::new(StatusCondition::new(None))),
            self_ref: None,
            enabled: Arc::new(AtomicBool::new(false)),
            deleted: Arc::new(AtomicBool::new(false)),
            topic_name: topic_name.to_owned(),
            type_name: type_name.to_owned(),
            participant: Some(Arc::downgrade(participant)),
            inconsistent_topic_status: Arc::new(Mutex::new(InconsistentTopicStatus::default())),
        };
        let topic_arc = Arc::new(topic.clone());
        let weak_ref = Arc::downgrade(&topic_arc);
        {
            let mut status_condition = topic.status_condition.lock().unwrap();
            *status_condition = StatusCondition::new(Some(weak_ref));
        }
        topic.self_ref = Some(topic_arc); // Without Arc, the new() function completes and memory is freed. The StatusCondition's entity field returns None.
        topic
    }

    /// Returns whether this topic is a built-in entity.
    pub(crate) fn is_builtin(&self) -> bool {
        self.is_builtin
    }

    pub fn get_inconsistent_topic_status(&self) -> DdsResult<InconsistentTopicStatus> {
        self.is_deleted()?;

        // Topic, DomainParticipant StatusCondition reset
        self.set_communication_status(&StatusKind::INCONSISTENT_TOPIC, false)?;
        self.get_participant()?.set_communication_status(&StatusKind::INCONSISTENT_TOPIC, false)?;

        self.take_inconsistent_topic_status()
    }

    // For Entity
    pub fn set_listener(
        &mut self,
        listener: Option<Arc<dyn TopicListener>>,
        mask: StatusMask,
    ) -> DdsResult<()> {
        self.is_deleted()?;
        {
            match self.listener.write() {
                Ok(mut guard) => {
                    *guard = listener;
                }
                Err(e) => return Err(DdsError::Error(e.to_string())),
            }
        }
        {
            match self.mask.write() {
                Ok(mut guard) => {
                    *guard = mask;
                }
                Err(e) => return Err(DdsError::Error(e.to_string())),
            }
        }
        Ok(())
    }

    // For Entity
    pub fn get_listener(&self) -> DdsResult<Option<Arc<dyn TopicListener>>> {
        self.is_deleted()?;
        match self.listener.read() {
            Ok(guard) => Ok(guard.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub(crate) fn is_enabled(&self) -> DdsResult<()> {
        self.is_deleted()?;
        if self.enabled.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(DdsError::NotEnabled)
        }
    }

    pub(crate) fn guid(&self) -> Guid {
        self.guid
    }

    pub(crate) fn delete(&mut self) {
        self.self_ref = None;
        self.deleted.store(true, Ordering::SeqCst);
    }

    fn is_deleted(&self) -> DdsResult<()> {
        if self.deleted.load(Ordering::SeqCst) {
            Err(DdsError::AlreadyDeleted)
        } else {
            Ok(())
        }
    }

    fn take_inconsistent_topic_status(&self) -> DdsResult<InconsistentTopicStatus> {
        let mut status_guard =
            self.inconsistent_topic_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // 1. Reset total_count_change
        status_guard.total_count_change = 0;

        Ok(result)
    }

    fn handle_inconsistent_topic_status(&self) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .inconsistent_topic_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::INCONSISTENT_TOPIC) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_inconsistent_topic(self, &status);
                listener_called = true;
            }
            let participant = self.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_inconsistent_topic(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_inconsistent_topic_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status(&StatusKind::INCONSISTENT_TOPIC, true)?;
        self.get_participant()?.set_communication_status(&StatusKind::INCONSISTENT_TOPIC, true)?;

        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{
        mpsc::{sync_channel, SyncSender},
        Arc,
    };

    use int2dds_derive::DdsType;

    use crate::{
        core::{error::DdsError, time::Duration},
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::StatusMask,
        publication::qos::{DataWriterQos, PublisherQos},
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::{qos::TopicQos, topic_listener::TopicListener},
    };

    #[derive(DdsType)]
    pub struct HelloWorld {
        pub index: u32,
        pub message: String,
    }

    #[derive(DdsType)]
    pub struct HelloWorldWithKey {
        #[dds(key)]
        pub index: u32,
        pub message: String,
    }

    #[test]
    fn test_delete_flag() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<HelloWorld>(
                "hello_world",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap(); //

        assert!(topic.get_instance_handle().is_ok()); // Success if ok

        let found_topic =
            domain_participant.find_topic("hello_world", Duration::infinite()).unwrap();
        domain_participant.delete_topic(found_topic).unwrap(); //

        // assert_eq!(found_topic.get_instance_handle(), Err(DdsError::AlreadyDeleted));
        assert_eq!(topic.get_instance_handle(), Err(DdsError::AlreadyDeleted));

        domain_participant.delete_contained_entities().unwrap();
        domain_participant_factory.delete_participant(domain_participant).unwrap();
    }

    #[test]
    fn test_topic_drop_without_delete() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<HelloWorld>(
                "hello_world",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic_handle = topic.get_instance_handle().unwrap();

        drop(topic);
        let contains_topic = domain_participant.contains_entity(topic_handle).unwrap();
        assert!(contains_topic, "Topic deleted!")
    }

    #[test]
    fn test_topic_handle_with_get_status() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(10, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = domain_participant
            .create_topic::<HelloWorld>(
                "hello_world_topic_sub",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let _reader1 = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let _reader2 = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(10));
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Info).try_init();

        log::info!("Status Info: {:?}", topic.get_inconsistent_topic_status().unwrap());
        // Error may occur if the timing of data arrival doesn't match
        assert_eq!(topic.get_inconsistent_topic_status().unwrap().total_count_change(), 0);

        domain_participant.delete_contained_entities().unwrap();
        domain_participant_factory.delete_participant(domain_participant).unwrap();
    }

    #[test]
    fn test_topic_handle_with_listener() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Info).try_init();

        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(10, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        struct TopicListenerStructure {
            _sender: SyncSender<()>,
        }

        impl TopicListener for TopicListenerStructure {
            fn on_inconsistent_topic(
                &self,
                topic: &super::Topic,
                status: &crate::infrastructure::status::InconsistentTopicStatus,
            ) {
                log::info!(
                    "Get Incosistent Status - topic name: {:?}, topic type: {:?}, status: {:?}",
                    topic.get_name(),
                    topic.get_type_name(),
                    status
                )
            }
        }
        let (_sender, _receiver) = sync_channel(0);
        let listener = TopicListenerStructure { _sender };
        let topic = domain_participant
            .create_topic::<HelloWorld>(
                "hello_world_topic_sub",
                "HelloWorld",
                TopicQos::default(),
                Some(Arc::new(listener)),
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let _reader1 = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let _reader2 = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(10));

        domain_participant.delete_contained_entities().unwrap();
        domain_participant_factory.delete_participant(domain_participant).unwrap();
    }

    /// Writer(WITH_KEY) on dp1 vs Reader(NO_KEY) on dp2
    #[test]
    fn test_topic_kind_mismatch_writer_keyed_reader_no_key() {
        let dpf = DomainParticipantFactory::get_instance();

        // dp1: keyed writer
        let dp1 = dpf
            .create_participant(20, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic1 = dp1
            .create_topic::<HelloWorldWithKey>(
                "topic_kind_mismatch_wk_rnk",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher =
            dp1.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorldWithKey>(
                &topic1,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // dp2: no-key reader
        let dp2 = dpf
            .create_participant(20, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic2 = dp2
            .create_topic::<HelloWorld>(
                "topic_kind_mismatch_wk_rnk",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber2 =
            dp2.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
        let _reader2 = subscriber2
            .create_datareader::<HelloWorld>(
                &topic2,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(5));

        let status = topic1.get_inconsistent_topic_status().unwrap();
        assert!(
            status.total_count >= 1,
            "Expected InconsistentTopicStatus total_count >= 1, got {}",
            status.total_count
        );

        dp1.delete_contained_entities().unwrap();
        dpf.delete_participant(dp1).unwrap();
        dp2.delete_contained_entities().unwrap();
        dpf.delete_participant(dp2).unwrap();
    }

    /// Writer(NO_KEY) on dp1 vs Reader(WITH_KEY) on dp2
    #[test]
    fn test_topic_kind_mismatch_writer_no_key_reader_keyed() {
        let dpf = DomainParticipantFactory::get_instance();

        // dp1: no-key writer
        let dp1 = dpf
            .create_participant(21, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic1 = dp1
            .create_topic::<HelloWorld>(
                "topic_kind_mismatch_wnk_rk",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher =
            dp1.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic1,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // dp2: keyed reader
        let dp2 = dpf
            .create_participant(21, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic2 = dp2
            .create_topic::<HelloWorldWithKey>(
                "topic_kind_mismatch_wnk_rk",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber2 =
            dp2.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
        let _reader2 = subscriber2
            .create_datareader::<HelloWorldWithKey>(
                &topic2,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(5));

        let status = topic1.get_inconsistent_topic_status().unwrap();
        assert!(
            status.total_count >= 1,
            "Expected InconsistentTopicStatus total_count >= 1, got {}",
            status.total_count
        );

        dp1.delete_contained_entities().unwrap();
        dpf.delete_participant(dp1).unwrap();
        dp2.delete_contained_entities().unwrap();
        dpf.delete_participant(dp2).unwrap();
    }

    /// Writer(WITH_KEY) + Reader(WITH_KEY): same participant + remote participant
    #[test]
    fn test_topic_kind_match_both_keyed() {
        let dpf = DomainParticipantFactory::get_instance();

        // dp1: keyed writer + keyed reader (local match)
        let dp1 = dpf
            .create_participant(22, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic1 = dp1
            .create_topic::<HelloWorldWithKey>(
                "topic_kind_match_both_keyed",
                "HelloWorldWithKey",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher =
            dp1.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorldWithKey>(
                &topic1,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber1 =
            dp1.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
        let _reader1 = subscriber1
            .create_datareader::<HelloWorldWithKey>(
                &topic1,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // dp2: keyed reader (remote match)
        let dp2 = dpf
            .create_participant(22, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic2 = dp2
            .create_topic::<HelloWorldWithKey>(
                "topic_kind_match_both_keyed",
                "HelloWorldWithKey",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber2 =
            dp2.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
        let _reader2 = subscriber2
            .create_datareader::<HelloWorldWithKey>(
                &topic2,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(5));

        let status = topic1.get_inconsistent_topic_status().unwrap();
        assert_eq!(
            status.total_count, 0,
            "Expected no InconsistentTopicStatus, got total_count={}",
            status.total_count
        );

        dp1.delete_contained_entities().unwrap();
        dpf.delete_participant(dp1).unwrap();
        dp2.delete_contained_entities().unwrap();
        dpf.delete_participant(dp2).unwrap();
    }

    /// Writer(NO_KEY) + Reader(NO_KEY): same participant + remote participant
    #[test]
    fn test_topic_kind_match_both_no_key() {
        let dpf = DomainParticipantFactory::get_instance();

        // dp1: no-key writer + no-key reader (local match)
        let dp1 = dpf
            .create_participant(23, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic1 = dp1
            .create_topic::<HelloWorld>(
                "topic_kind_match_both_no_key",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher =
            dp1.create_publisher(PublisherQos::default(), None, StatusMask::default()).unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic1,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber1 =
            dp1.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
        let _reader1 = subscriber1
            .create_datareader::<HelloWorld>(
                &topic1,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // dp2: no-key reader (remote match)
        let dp2 = dpf
            .create_participant(23, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic2 = dp2
            .create_topic::<HelloWorld>(
                "topic_kind_match_both_no_key",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber2 =
            dp2.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
        let _reader2 = subscriber2
            .create_datareader::<HelloWorld>(
                &topic2,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(5));

        let status = topic1.get_inconsistent_topic_status().unwrap();
        assert_eq!(
            status.total_count, 0,
            "Expected no InconsistentTopicStatus, got total_count={}",
            status.total_count
        );

        dp1.delete_contained_entities().unwrap();
        dpf.delete_participant(dp1).unwrap();
        dp2.delete_contained_entities().unwrap();
        dpf.delete_participant(dp2).unwrap();
    }
}
