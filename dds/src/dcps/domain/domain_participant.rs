//! DomainParticipant - The central entity for DDS communication.
//!
//! A `DomainParticipant` represents an application's participation in a DDS domain. It is the
//! factory for creating `Topic`, `Publisher`, and `Subscriber` objects, and serves as the
//! container for all DDS entities within a domain.
//!
//! # Overview
//!
//! The DomainParticipant is the starting point for all DDS operations after obtaining the
//! factory. Each participant belongs to a specific domain (identified by domain_id) and can
//! only communicate with other participants in the same domain.
//!
//! # Key Responsibilities
//!
//! - **Entity Factory**: Creates topics, publishers, and subscribers
//! - **Type Registration**: Manages type support for data types used in the domain
//! - **QoS Management**: Configures quality of service policies for entities
//! - **Lifecycle Control**: Enables/disables entities and manages their lifecycle
//! - **Content Filtering**: Creates ContentFilteredTopic for filtered data subscription
//!
//! # Basic Usage
//!
//! ```no_run
//! use int2dds::domain::domain_participant_factory::DomainParticipantFactory;
//! use int2dds::topic::type_support::DdsType;
//!
//! #[derive(DdsType)]
//! struct MyData {
//!     id: u32,
//!     value: String,
//! }
//!
//! // Get factory and create participant
//! let factory = DomainParticipantFactory::get_instance();
//! let participant = factory.create_participant(0, Default::default(), None, Default::default()).unwrap();
//!
//! // Create topic
//! let topic = participant.create_topic::<MyData>("MyTopic", "MyData", Default::default(), None, Default::default()).unwrap();
//!
//! // Create publisher and subscriber
//! let publisher = participant.create_publisher(Default::default(), None, Default::default()).unwrap();
//! let subscriber = participant.create_subscriber(Default::default(), None, Default::default()).unwrap();
//!
//! // Clean up
//! participant.delete_subscriber(subscriber).unwrap();
//! participant.delete_publisher(publisher).unwrap();
//! participant.delete_topic(topic).unwrap();
//! // or
//! participant.delete_contained_entities().unwrap();
//! factory.delete_participant(participant).unwrap();
//! ```

use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, MutexGuard, RwLock, Weak,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use super::{domain_participant_listener::DomainParticipantListener, qos::DomainParticipantQos};
use crate::{
    common::{
        builtin::topic::{
            participant_builtin_topic_data::ParticipantBuiltinTopicData,
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
            topic_builtin_topic_data::TopicBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    core::{
        error::{DdsError, DdsResult},
        time::{Duration, Time},
        types::{DomainId, LENGTH_UNLIMITED},
    },
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::{
        entity::{
            impl_dds_entity, impl_dds_entity_impl, BaseEntity, EnableChild, Entity, EntityInternal,
            UpdateStatus,
        },
        qos_policy::{
            DeadlineQosPolicy, DestinationOrderQosPolicy, DestinationOrderQosPolicyKind,
            DurabilityQosPolicy, DurabilityQosPolicyKind, EntityFactoryQosPolicy, HistoryQosPolicy,
            HistoryQosPolicyKind, LivelinessQosPolicy, LivelinessQosPolicyKind, OwnershipQosPolicy,
            OwnershipQosPolicyKind, Qos, ReaderDataLifecycleQosPolicy, ReliabilityQosPolicy,
            ReliabilityQosPolicyKind, ResourceLimitsQosPolicy, TimeBasedFilterQosPolicy,
        },
        status::StatusMask,
        status_condition::StatusCondition,
    },
    publication::{
        publisher::Publisher,
        publisher_listener::PublisherListener,
        qos::{PublisherQos, PUBLISHER_QOS_DEFAULT},
    },
    rtps::{
        builtin::data::participant_message_data::ParticipantMessageData,
        common::{guid::Guid, rtps_error_code::RtpsErrorCode},
        dcps_bridge::dcps_bridge::DcpsBridge,
        entities::{
            entity::Entity as RtpsEntity, participant::Participant as RtpsParticipant,
            reader::Reader,
        },
    },
    subscription::{
        qos::{DataReaderQos, SubscriberQos, SUBSCRIBER_QOS_DEFAULT},
        subscriber::Subscriber,
        subscriber_listener::SubscriberListener,
    },
    topic::{
        content_filtered_topic::ContentFilteredTopic,
        multi_topic::MultiTopic,
        qos::{TopicQos, TOPIC_QOS_DEFAULT},
        topic::Topic,
        topic_description::{TopicDescription, TopicDescriptionInternal},
        topic_listener::TopicListener,
        type_support::{DdsType, TypeSupport},
    },
};

#[derive(Clone)]
pub struct DomainParticipant {
    // Indicates whether this entity is a built-in entity.
    //
    // Currently always `false` as DDS spec does not define built-in
    // DomainParticipant exposed to users.
    //
    // TODO: Reserved for future DCPS-RTPS built-in entity mapping
    // if needed (e.g., exposing built-in participant for diagnostics).
    is_builtin: bool,
    guid: Arc<Guid>,
    domain_id: DomainId,
    qos: Arc<Mutex<DomainParticipantQos>>,
    listener: Arc<RwLock<Option<Arc<dyn DomainParticipantListener>>>>,
    mask: Arc<RwLock<StatusMask>>,
    status_condition: Arc<Mutex<StatusCondition<DomainParticipantQos>>>,
    pub(crate) self_ref: Option<Arc<DomainParticipant>>,
    enabled: Arc<AtomicBool>,
    deleted: Arc<AtomicBool>,
    // rtps_participant: Arc<Mutex<Option<RtpsParticipant>>>,
    dcps_bridge: Arc<Mutex<Option<DcpsBridge>>>,
    builtin_subscriber: Arc<Mutex<Option<Subscriber>>>,
    builtin_topics: Arc<Mutex<Vec<Topic>>>,
    publishers: Arc<Mutex<Vec<Weak<Publisher>>>>,
    publishers_by_handle: Arc<Mutex<HashMap<InstanceHandle, Weak<Publisher>>>>,
    subscribers: Arc<Mutex<Vec<Weak<Subscriber>>>>,
    subscribers_by_handle: Arc<Mutex<HashMap<InstanceHandle, Weak<Subscriber>>>>,
    topics: Arc<Mutex<Vec<Weak<Topic>>>>,
    topics_by_handle: Arc<Mutex<HashMap<InstanceHandle, Weak<Topic>>>>,
    filtered_topics: Arc<Mutex<Vec<Weak<ContentFilteredTopic>>>>,
    filtered_topics_by_topic_handle:
        Arc<Mutex<HashMap<InstanceHandle, Vec<Weak<ContentFilteredTopic>>>>>,
    // multi_topics: Arc<Mutex<Vec<Weak<MultiTopic>>>>,
    orphaned_entities: Arc<Mutex<OrphanedEntities>>,
    types: Arc<RwLock<HashMap<String, Arc<dyn TypeSupport>>>>,
    default_subscriber_qos: Arc<Mutex<SubscriberQos>>,
    default_publisher_qos: Arc<Mutex<PublisherQos>>,
    default_topic_qos: Arc<Mutex<TopicQos>>,
    next_instance_id: Arc<AtomicU32>,
}

impl Debug for DomainParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DomainParticipant {{ domain_id: {:?}, guid: {:?} }}",
            self.domain_id,
            self.guid()
        )
    }
}

impl Drop for DomainParticipant {
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
            let factory = DomainParticipantFactory::get_instance();
            factory.handle_participant_drop(
                &self.domain_id,
                &InstanceHandle::from_guid(&self.guid().unwrap()),
            );
        }
    }
}

#[derive(Default, Clone)]
struct OrphanedEntities {
    topics: Vec<Arc<Topic>>,
    publishers: Vec<Arc<Publisher>>,
    subscribers: Vec<Arc<Subscriber>>,
    content_filtered_topics: Vec<Arc<ContentFilteredTopic>>,
}

impl OrphanedEntities {
    // Check if a specific entity is in orphaned and remove it
    fn add_publisher(&mut self, target: Arc<Publisher>) {
        if !self.publishers.iter().any(|c| Arc::ptr_eq(c, &target)) {
            self.publishers.push(target);
        }
    }

    fn add_subscriber(&mut self, target: Arc<Subscriber>) {
        if !self.subscribers.iter().any(|c| Arc::ptr_eq(c, &target)) {
            self.subscribers.push(target);
        }
    }

    fn add_topic(&mut self, target: Arc<Topic>) {
        if !self.topics.iter().any(|c| Arc::ptr_eq(c, &target)) {
            self.topics.push(target);
        }
    }

    fn add_content_filtered_topic(&mut self, target: Arc<ContentFilteredTopic>) {
        if !self.content_filtered_topics.iter().any(|c| Arc::ptr_eq(c, &target)) {
            self.content_filtered_topics.push(target);
        }
    }

    fn remove_topic(&mut self, target: &Topic) -> bool {
        if let Some(pos) = self.topics.iter().position(|t| &**t == target) {
            self.topics.remove(pos);
            true
        } else {
            false
        }
    }

    fn remove_content_filtered_topic(&mut self, target: &ContentFilteredTopic) -> bool {
        if let Some(pos) = self.content_filtered_topics.iter().position(|t| &**t == target) {
            self.content_filtered_topics.remove(pos);
            true
        } else {
            false
        }
    }

    fn remove_publisher(&mut self, target: &Publisher) -> bool {
        if let Some(pos) = self.publishers.iter().position(|p| &**p == target) {
            self.publishers.remove(pos);
            true
        } else {
            false
        }
    }

    fn remove_subscriber(&mut self, target: &Subscriber) -> bool {
        if let Some(pos) = self.subscribers.iter().position(|s| &**s == target) {
            self.subscribers.remove(pos);
            true
        } else {
            false
        }
    }
}

impl PartialEq for DomainParticipant {
    fn eq(&self, other: &Self) -> bool {
        self.domain_id == other.domain_id
            && self.get_instance_handle() == other.get_instance_handle()
    }
}

impl Eq for DomainParticipant {}

impl_dds_entity!(DomainParticipant, DomainParticipantQos);
impl EnableChild for DomainParticipant {
    fn enable_rtps_entities(&self) -> DdsResult<()> {
        let mut dcps_bridge =
            self.dcps_bridge.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let dcps_bridge = dcps_bridge
            .as_mut()
            .ok_or(DdsError::Error("DcpsBridge is not initialized".to_string()))?;
        match dcps_bridge.init() {
            Ok(()) => Ok(()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }
    fn enable_child_entities(&self) -> DdsResult<()> {
        // Helper macro to reduce repetitive code
        macro_rules! enable_entities {
            ($collection:expr, $entity_type:literal) => {
                $collection
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .iter()
                    .filter_map(|entity| entity.upgrade())
                    .try_for_each(|entity| entity.enable())
                    .map_err(|e| {
                        log::error!("Failed to enable {}: {:?}", $entity_type, e);
                        e
                    })?;
            };
        }

        enable_entities!(self.topics, "topic");
        enable_entities!(self.publishers, "publisher");
        enable_entities!(self.subscribers, "subscriber");

        /*
            {
                match self.topics.lock() {
                    Ok(topics) => {
                        for topic in topics.iter() {
                            topic.enable()?;
                        }
                    }
                    Err(e) => return Err(DdsError::Error(e.to_string())),
                }
            }
            {
                match self.publishers.lock() {
                    Ok(publishers) => {
                        for publisher in publishers.iter() {
                            publisher.enable()?;
                        }
                    }
                    Err(e) => return Err(DdsError::Error(e.to_string())),
                }
            }
            {
                match self.subscribers.lock() {
                    Ok(subscribers) => {
                        for subscriber in subscribers.iter() {
                            subscriber.enable()?;
                        }
                    }
                    Err(e) => return Err(DdsError::Error(e.to_string())),
                }
            }
        */
        Ok(())
    }
}
impl UpdateStatus for DomainParticipant {}

impl DomainParticipant {
    pub(crate) fn new(
        is_builtin: bool,
        domain_id: DomainId,
        qos: DomainParticipantQos,
        listener: Option<Arc<dyn DomainParticipantListener>>,
        mask: StatusMask,
    ) -> DdsResult<Self> {
        let dcps_bridge = DcpsBridge::new(domain_id as u32, &qos.property);
        let guid = dcps_bridge.get_participant().map_err(|e| DdsError::Error(e.message))?.guid();

        let mut participant = Self {
            is_builtin,
            guid: Arc::new(guid),
            domain_id,
            qos: Arc::new(Mutex::new(qos)),
            listener: Arc::new(RwLock::new(listener)),
            mask: Arc::new(RwLock::new(mask)),
            status_condition: Arc::new(Mutex::new(StatusCondition::new(None))),
            self_ref: None,
            enabled: Arc::new(AtomicBool::new(false)),
            deleted: Arc::new(AtomicBool::new(false)),
            // rtps_participant: Arc::new(Mutex::new(None)),
            dcps_bridge: Arc::new(Mutex::new(Some(dcps_bridge))),
            builtin_subscriber: Arc::new(Mutex::new(None)),
            builtin_topics: Arc::new(Mutex::new(Vec::new())),
            publishers: Arc::new(Mutex::new(Vec::new())),
            publishers_by_handle: Arc::new(Mutex::new(HashMap::new())),
            subscribers: Arc::new(Mutex::new(Vec::new())),
            subscribers_by_handle: Arc::new(Mutex::new(HashMap::new())),
            topics: Arc::new(Mutex::new(Vec::new())),
            topics_by_handle: Arc::new(Mutex::new(HashMap::new())),
            filtered_topics: Arc::new(Mutex::new(Vec::new())),
            filtered_topics_by_topic_handle: Arc::new(Mutex::new(HashMap::new())),
            // multi_topics: Arc::new(Mutex::new(Vec::new())),
            orphaned_entities: Arc::new(Mutex::new(OrphanedEntities::default())),
            types: Arc::new(RwLock::new(HashMap::new())),
            default_subscriber_qos: Arc::new(Mutex::new(SubscriberQos::default())),
            default_publisher_qos: Arc::new(Mutex::new(PublisherQos::default())),
            default_topic_qos: Arc::new(Mutex::new(TopicQos::default())),
            next_instance_id: Arc::new(AtomicU32::new(0)),
        };

        let participant_arc = Arc::new(participant.clone());
        let weak_ref = Arc::downgrade(&participant_arc);
        {
            let mut status_condition = participant.status_condition.lock().unwrap();
            *status_condition = StatusCondition::new(Some(weak_ref));
        }
        participant.self_ref = Some(participant_arc.clone()); // Without Arc, the new() function ends and memory is freed. StatusCondition's entity field returns None.
                                                              // Builtin-Endpoints

        Self::initialize_builtin_entities(&participant_arc)?;

        Ok(participant)
    }

    #[allow(clippy::field_reassign_with_default, clippy::needless_borrow)]
    fn initialize_builtin_entities(participant: &Arc<Self>) -> DdsResult<()> {
        let rtps_participant = participant.get_rtps_participant()?;
        let endpoints = rtps_participant.builtin_endpoints();
        let sedp_builtin_publications_reader = endpoints.sedp_builtin_publications_reader.clone();
        let sedp_builtin_subscriptions_reader = endpoints.sedp_builtin_subscriptions_reader.clone();
        // let sedp_builtin_topics_reader = endpoints.sedp_builtin_topics_reader.clone();
        let spdp_builtin_participant_reader = endpoints.spdp_builtin_participant_reader.clone();
        let builtin_participant_message_reader =
            endpoints.builtin_participant_message_reader.clone();

        // Create builtin topics (type registration is handled internally)
        let dcps_participant_topic = Self::create_builtin_topic::<ParticipantBuiltinTopicData>(
            participant,
            "DCPSParticipant",
            "SPDPdiscoveredParticipantData",
        )?;
        let dcps_publication_topic = Self::create_builtin_topic::<PublicationBuiltinTopicData>(
            participant,
            "DCPSPublication",
            "DiscoveredWriterData",
        )?;
        let dcps_subscription_topic = Self::create_builtin_topic::<SubscriptionBuiltinTopicData>(
            participant,
            "DCPSSubscription",
            "DiscoveredReaderData",
        )?;
        // TODO
        // let dcps_topic_topic = Self::create_builtin_topic::<TopicBuiltinTopicData>(
        //     participant,
        //     "DCPSTopic",
        //     "DiscoveredTopicData",
        // )?;
        let dcps_participant_message_topic = Self::create_builtin_topic::<ParticipantMessageData>(
            participant,
            "DCPSParticipantMessage",
            "BuiltinParticipantMessageReader",
        )?;

        // 2.2.5 Built-in Topics
        let mut subscriber_qos = SubscriberQos::default();
        // TODO
        // subscriber_qos.presentation = PresentationQosPolicy {
        //     access_scope: PresentationQosAccessScopeKind::Topic,
        //     coherent_access: false,
        //     ordered_access: false,
        // };
        subscriber_qos.entity_factory =
            EntityFactoryQosPolicy { autoenable_created_entities: true };

        let builtin_subscriber = Subscriber::new(
            true,
            subscriber_qos,
            None,
            StatusMask::all(),
            participant.create_instance_handle()?,
            &participant,
        );

        // Directly enable builtin subscriber (bypass parent check during initialization)
        builtin_subscriber.enabled.store(true, Ordering::SeqCst);

        // 2.2.5 Built-in Topics
        let mut reader_qos = DataReaderQos::default();
        reader_qos.durability =
            DurabilityQosPolicy { kind: DurabilityQosPolicyKind::TransientLocal };
        reader_qos.deadline = DeadlineQosPolicy { period: Duration::infinite() };
        reader_qos.ownership = OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Shared };
        reader_qos.liveliness = LivelinessQosPolicy {
            kind: LivelinessQosPolicyKind::Automatic,
            lease_duration: Duration::from_seconds(100), // mutable, unspecified
        };
        reader_qos.time_based_filter =
            TimeBasedFilterQosPolicy { minimum_separation: Duration::zero() };
        reader_qos.reliability = ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration::from_millis(100),
        };
        reader_qos.destination_order =
            DestinationOrderQosPolicy { kind: DestinationOrderQosPolicyKind::ByReceptionTimestamp };
        reader_qos.history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(1) };
        reader_qos.resource_limits = ResourceLimitsQosPolicy {
            max_instances: LENGTH_UNLIMITED,
            max_samples: LENGTH_UNLIMITED,
            max_samples_per_instance: LENGTH_UNLIMITED,
        };
        reader_qos.reader_data_lifecycle = ReaderDataLifecycleQosPolicy {
            autopurge_nowriter_samples_delay: Duration {
                sec: Duration::INFINITE_SEC,
                nanosec: Duration::INFINITE_NSEC,
            },
            autopurge_disposed_samples_delay: Duration {
                sec: Duration::INFINITE_SEC,
                nanosec: Duration::INFINITE_NSEC,
            },
        };

        let _participant_reader = builtin_subscriber
            .create_builtin_datareader::<ParticipantBuiltinTopicData>(
                &dcps_participant_topic,
                reader_qos.clone(),
                spdp_builtin_participant_reader.clone() as Arc<dyn Reader + Send + Sync>,
            )?;
        let _publication_reader = builtin_subscriber
            .create_builtin_datareader::<PublicationBuiltinTopicData>(
                &dcps_publication_topic,
                reader_qos.clone(),
                sedp_builtin_publications_reader.clone() as Arc<dyn Reader + Send + Sync>,
            )?;
        let _subscription_reader = builtin_subscriber
            .create_builtin_datareader::<SubscriptionBuiltinTopicData>(
                &dcps_subscription_topic,
                reader_qos.clone(),
                sedp_builtin_subscriptions_reader.clone() as Arc<dyn Reader + Send + Sync>,
            )?;
        // TODO
        // let _topic_reader = builtin_subscriber
        //     .create_builtin_datareader::<TopicBuiltinTopicData>(
        //         &dcps_topic_topic,
        //         reader_qos,
        //         sedp_builtin_topics_reader.clone() as Arc<dyn Reader + Send + Sync>,
        //     )?;
        let _participant_message_reader = builtin_subscriber
            .create_builtin_datareader::<ParticipantMessageData>(
                &dcps_participant_message_topic,
                reader_qos.clone(),
                builtin_participant_message_reader.clone() as Arc<dyn Reader + Send + Sync>,
            )?;

        *participant.builtin_subscriber.lock().map_err(|e| DdsError::Error(e.to_string()))? =
            Some(builtin_subscriber);

        Ok(())
    }

    pub(crate) fn has_active_entities(&self) -> DdsResult<bool> {
        self.is_deleted()?;
        {
            // Check non-builtin publishers
            match self.get_publishers() {
                Ok(publishers) => {
                    if publishers.iter().any(|p| !p.is_builtin()) {
                        return Ok(true);
                    }
                }
                Err(err) => return Err(err),
            }
            // Check non-builtin subscribers
            match self.get_subscribers() {
                Ok(subscribers) => {
                    if subscribers.iter().any(|s| !s.is_builtin()) {
                        return Ok(true);
                    }
                }
                Err(e) => return Err(e),
            }
            // Check non-builtin topics
            match self.get_topics() {
                Ok(topics) => {
                    if topics.iter().any(|t| !t.is_builtin()) {
                        return Ok(true);
                    }
                }
                Err(e) => return Err(e),
            }
            // match self.get_filtered_topics() {
            //     Ok(filtered_topics) => {
            //         if !filtered_topics.is_empty() {
            //             return Ok(true);
            //         }
            //     }
            //     Err(e) => return Err(e),
            // }
            // match self.get_multi_topics() {
            //     Ok(multi_topics) => {
            //         if !multi_topics.is_empty() {
            //             return Ok(true);
            //         }
            //     }
            //     Err(e) => return Err(e),
            // }
            Ok(false)
        }
    }

    // TODO: RTPS layer implementation must be completed first (Built-in)
    pub fn ignore_participant(&self, _handle: InstanceHandle) -> DdsResult<()> {
        /*
            This operation allows the application to instruct the service to locally ignore a specific remote DomainParticipant.
            After this operation is called, the service will behave locally as if the remote participant does not exist.
            That is, all Topics, Publications, and Subscriptions originating from that domain participant will be ignored.

            This operation can be used with the remote participant discovery feature provided through the "DCPSParticipant" Built-in Topic,
            for example, to implement access control functionality.
            Application data can be associated with the DomainParticipant through the USER_DATA QoS policy,
            and this data is propagated as a field of the built-in topic, allowing the application to implement its own access control policies.
            For details, see Section 2.2.5 Built-in Topics.

            The domain participant to ignore is identified by the handle argument,
            which is contained in the SampleInfo obtained when reading data samples from the built-in DataReader for the "DCPSParticipant" topic.
            The built-in DataReader can be read through the same read/take operations as a regular DataReader,
            and these data access operations are described in Section 2.2.2.5 Subscription Module.

            The ignore_participant operation does not need to be reversible.
            The service does not provide a means to reverse this operation.

            Error codes that may be returned in addition to standard error codes: OUT_OF_RESOURCES.
        */
        // match &self.rtps_participant {
        //     None => DdsError::NotEnabled,
        //     Some(participant) => {
        //         if participant.ignore_participant(instance_handle_to_guid(handle).guid_prefix) {
        //             DdsError::Ok
        //         } else {
        //             DdsError::BadParameter
        //         }
        //     }
        // }

        // Expected code..
        // match self.is_enabled() {
        //     DdsError::Ok => (),
        //     err_code => return Err(err_code),
        // }
        // self.rtps_participant.ignore_participant(handle.to_guid().prefix())

        Err(DdsError::Unsupported)
    }

    // TODO: RTPS layer implementation must be completed first (Built-in)
    pub fn ignore_publication(&self, _handle: InstanceHandle) -> DdsResult<()> {
        /*
            ignore_publication
            This operation allows the application to instruct the service to locally ignore a remote publication.
            A publication is defined by the combination of a topic name and the user data and partition set configured in the Publisher
            (see the "DCPSPublication" Built-in Topic in Section 2.2.5).

            After this operation is called, all data written by that publication will be ignored.

            The DataWriter to be ignored is identified through the handle argument.
            This handle is the one that appears in the SampleInfo obtained when reading data from the "DCPSPublication" topic through the built-in DataReader.

            The ignore_publication operation does not require reversibility.
            The service does not provide a way to reverse this.

            Error codes that may be returned in addition to standard errors:

            OUT_OF_RESOURCES
        */
        // match self.is_enabled() {
        //     DdsError::Ok => (),
        //     err_code => return Err(err_code),
        // }

        Err(DdsError::Unsupported)
    }

    // TODO: RTPS layer implementation must be completed first (Built-in)
    pub fn ignore_subscription(&self, _handle: InstanceHandle) -> DdsResult<()> {
        /*
            This operation allows the application to instruct the service to locally ignore a remote subscription.
            A subscription is defined by the combination of a topic name and the user data and partition set configured in the Subscriber
            (see the "DCPSSubscription" Built-in Topic in Section 2.2.5).

            After this operation is called, all data received from that subscription will be ignored.

            The DataReader to be ignored is identified through the handle argument.
            This handle is the one that appears in the SampleInfo obtained when reading data from the "DCPSSubscription" topic through the built-in DataReader.

            The ignore_subscription operation does not require reversibility.
            The service does not provide a way to reverse this.

            Error codes that may be returned in addition to standard errors:

            OUT_OF_RESOURCES
        */
        // match self.is_enabled() {
        //     DdsError::Ok => (),
        //     err_code => return Err(err_code),
        // }

        Err(DdsError::Unsupported)
    }

    /// Creates a new `Publisher` for publishing data.
    ///
    /// A publisher is a container for data writers. It manages the creation and lifecycle
    /// of data writers, and provides a way to group related data writers together.
    ///
    /// The publisher will be automatically enabled if the participant's QoS policy
    /// `autoenable_created_entities` is set to true (which is the default).
    ///
    /// # Arguments
    ///
    /// * `qos` - Quality of Service policies for the publisher. Use `PublisherQos::default()` for defaults.
    /// * `listener` - Optional listener for status notifications. Pass `None` if not needed.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Publisher)` on success, or a `DdsError` if creation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The participant has been deleted
    /// * The QoS policies are inconsistent
    /// * System resources are insufficient
    pub fn create_publisher(
        &self,
        qos: PublisherQos,
        listener: Option<Arc<dyn PublisherListener>>,
        mask: StatusMask,
    ) -> DdsResult<Publisher> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        qos.is_consistent()?;
        let handle = self.create_instance_handle()?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("DomainParticipant is not properly initialized".to_string()))?;
        let publisher = Publisher::new(false, qos, listener, mask, handle, self_ref);
        let qos = self.get_qos()?;
        if let Ok(()) = self.is_enabled() {
            if qos.entity_factory.autoenable_created_entities {
                publisher.enable()?;
            }
        }

        let publisher_ref = publisher
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Publisher is not properly initialized".to_string()))?
            .clone();
        let weak_publisher = Arc::downgrade(&publisher_ref);
        {
            let mut publishers =
                self.publishers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            publishers.push(weak_publisher.clone());
        }
        {
            let mut publishers_by_handle =
                self.publishers_by_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            publishers_by_handle.insert(handle, weak_publisher.clone());
        }

        Ok(publisher)
    }

    /// Creates a new `Publisher` using QoS settings from a loaded profile.
    ///
    /// This is a convenience method that retrieves QoS from the profile and delegates
    /// to [`create_publisher`](Self::create_publisher).
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path in the format `"Library::Profile"` or `"Library::Profile::QosName"`.
    ///   See [`QosProvider`](crate::config::json::QosProvider) for supported path formats.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Errors
    ///
    /// Returns an error if the profile is not found or publisher creation fails.
    pub fn create_publisher_with_profile(
        &self,
        qos_path: &str,
        listener: Option<Arc<dyn PublisherListener>>,
        mask: StatusMask,
    ) -> DdsResult<Publisher> {
        let qos = self.get_publisher_qos_from_profile(qos_path)?;
        self.create_publisher(qos, listener, mask)
    }

    pub fn delete_publisher(&self, mut publisher: Publisher) -> DdsResult<()> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        match self.try_delete_publisher(&mut publisher) {
            Ok(()) => Ok(()),
            Err(e) => {
                let pub_ref = publisher
                    .self_ref
                    .as_ref()
                    .ok_or(DdsError::Error("Publisher is not properly initialized".to_string()))?
                    .clone();
                self.orphaned_entities
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .add_publisher(pub_ref);
                Err(e)
            }
        }
    }

    fn try_delete_publisher(&self, publisher: &mut Publisher) -> DdsResult<()> {
        {
            let mut orphaned = self.orphaned_entities.lock().unwrap();
            orphaned.remove_publisher(publisher)
        };

        if publisher.get_participant()?.get_instance_handle()? != self.get_instance_handle()? {
            return Err(DdsError::PreconditionNotMet);
        }
        if publisher.has_active_entities()? {
            return Err(DdsError::PreconditionNotMet);
        }

        let handle = publisher.get_instance_handle()?;

        let mut publishers = self
            .publishers
            .lock()
            .map_err(|_| DdsError::Error("Failed to lock publishers".to_string()))?;
        let mut publishers_by_handle = self
            .publishers_by_handle
            .lock()
            .map_err(|_| DdsError::Error("Failed to lock publishers_by_handle".to_string()))?;

        // Check existence by handle
        if !publishers_by_handle.contains_key(&handle) {
            return Err(DdsError::Error("Publisher not found".to_string()));
        }

        // Clean up dead references and remove the publisher
        publishers.retain(|weak_publisher| {
            match weak_publisher.upgrade() {
                Some(p) => p.get_instance_handle().is_ok_and(|h| h != handle),
                None => false, // Remove dead references
            }
        });

        publishers_by_handle.remove(&handle);

        publisher.delete();
        Ok(())
    }

    pub(crate) fn handle_publisher_drop(&self, publisher_handle: &InstanceHandle) {
        let mut found_publisher = None;
        if let Ok(publishers) = self.publishers.lock() {
            for weak_publisher in publishers.iter() {
                if let Some(strong_publisher) = weak_publisher.upgrade() {
                    if let Ok(handle) = strong_publisher.get_instance_handle() {
                        if handle == *publisher_handle {
                            found_publisher = Some(strong_publisher);
                            break;
                        }
                    }
                }
            }
        }

        if found_publisher.is_none() {
            if let Ok(publishers_by_topic_handle) = self.publishers_by_handle.lock() {
                if let Some(weak_publisher) = publishers_by_topic_handle.get(publisher_handle) {
                    if let Some(strong_publisher) = weak_publisher.upgrade() {
                        if let Ok(handle) = strong_publisher.get_instance_handle() {
                            if handle == *publisher_handle {
                                found_publisher = Some(strong_publisher);
                            }
                        }
                    }
                }
            }
        }

        // If found, add Publisher to orphaned_entities
        if let Some(publisher) = found_publisher {
            if let Ok(mut orphaned_entities) = self.orphaned_entities.lock() {
                orphaned_entities.add_publisher(publisher);
            }
        }
        // If not found, it has already been properly deleted via delete_publisher, so do nothing
    }

    /// Creates a new `Subscriber` for receiving data.
    ///
    /// A subscriber is a container for data readers. It manages the creation and lifecycle
    /// of data readers, and provides a way to group related data readers together.
    ///
    /// The subscriber will be automatically enabled if the participant's QoS policy
    /// `autoenable_created_entities` is set to true (which is the default).
    ///
    /// # Arguments
    ///
    /// * `qos` - Quality of Service policies for the subscriber. Use `SubscriberQos::default()` for defaults.
    /// * `listener` - Optional listener for status notifications. Pass `None` if not needed.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Subscriber)` on success, or a `DdsError` if creation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The participant has been deleted
    /// * The QoS policies are inconsistent
    /// * System resources are insufficient
    pub fn create_subscriber(
        &self,
        qos: SubscriberQos,
        listener: Option<Arc<dyn SubscriberListener>>,
        mask: StatusMask,
    ) -> DdsResult<Subscriber> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        qos.is_consistent()?;
        let handle = self.create_instance_handle()?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("DomainParticipant not properly initialized".to_string()))?;
        let subscriber = Subscriber::new(false, qos, listener, mask, handle, self_ref);
        let qos = self.get_qos()?;
        if let Ok(()) = self.is_enabled() {
            if qos.entity_factory.autoenable_created_entities {
                subscriber.enable()?;
            }
        }

        let subscriber_ref = subscriber
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Subscriber is not properly initialized".to_string()))?
            .clone();
        let weak_subscriber = Arc::downgrade(&subscriber_ref);
        {
            let mut subscribers =
                self.subscribers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            subscribers.push(weak_subscriber.clone());
        }
        {
            let mut subscribers_by_handle =
                self.subscribers_by_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            subscribers_by_handle.insert(handle, weak_subscriber.clone());
        }

        Ok(subscriber)
    }

    /// Creates a new `Subscriber` using QoS settings from a loaded profile.
    ///
    /// This is a convenience method that retrieves QoS from the profile and delegates
    /// to [`create_subscriber`](Self::create_subscriber).
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path in the format `"Library::Profile"` or `"Library::Profile::QosName"`.
    ///   See [`QosProvider`](crate::config::json::QosProvider) for supported path formats.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Errors
    ///
    /// Returns an error if the profile is not found or subscriber creation fails.
    pub fn create_subscriber_with_profile(
        &self,
        qos_path: &str,
        listener: Option<Arc<dyn SubscriberListener>>,
        mask: StatusMask,
    ) -> DdsResult<Subscriber> {
        let qos = self.get_subscriber_qos_from_profile(qos_path)?;
        self.create_subscriber(qos, listener, mask)
    }

    pub fn delete_subscriber(&self, mut subscriber: Subscriber) -> DdsResult<()> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        match self.try_delete_subscriber(&mut subscriber) {
            Ok(()) => Ok(()),
            Err(e) => {
                let subscriber_ref = subscriber
                    .self_ref
                    .as_ref()
                    .ok_or(DdsError::Error("Subscriber is not properly initialized".to_string()))?
                    .clone();
                self.orphaned_entities
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .add_subscriber(subscriber_ref);
                Err(e)
            }
        }
    }

    fn try_delete_subscriber(&self, subscriber: &mut Subscriber) -> DdsResult<()> {
        self.is_deleted()?;

        {
            let mut orphaned = self.orphaned_entities.lock().unwrap();
            orphaned.remove_subscriber(subscriber)
        };

        if subscriber.get_participant()?.get_instance_handle()? != self.get_instance_handle()? {
            return Err(DdsError::PreconditionNotMet);
        }

        if subscriber.has_active_entities()? {
            return Err(DdsError::PreconditionNotMet);
        }

        let handle = subscriber.get_instance_handle()?;

        let mut subscribers = self
            .subscribers
            .lock()
            .map_err(|_| DdsError::Error("Failed to lock Subscribers".to_string()))?;
        let mut subscribers_by_handle = self
            .subscribers_by_handle
            .lock()
            .map_err(|_| DdsError::Error("Failed to lock subscribers_by_handle".to_string()))?;

        // Check existence by handle
        if !subscribers_by_handle.contains_key(&handle) {
            return Err(DdsError::Error("Subscriber not found".to_string()));
        }

        // Clean up dead references and remove the subscriber
        subscribers.retain(|weak_subscriber| {
            match weak_subscriber.upgrade() {
                Some(p) => p.get_instance_handle().is_ok_and(|h| h != handle),
                None => false, // Remove dead references
            }
        });

        subscribers_by_handle.remove(&handle);

        subscriber.delete();
        Ok(())
    }

    pub(crate) fn handle_subscriber_drop(&self, subscriber_handle: &InstanceHandle) {
        let mut found_subscriber = None;
        if let Ok(subscribers) = self.subscribers.lock() {
            for weak_subscriber in subscribers.iter() {
                if let Some(strong_subscriber) = weak_subscriber.upgrade() {
                    if let Ok(handle) = strong_subscriber.get_instance_handle() {
                        if handle == *subscriber_handle {
                            found_subscriber = Some(strong_subscriber);
                            break;
                        }
                    }
                }
            }
        }

        if found_subscriber.is_none() {
            if let Ok(subscribers_by_topic_handle) = self.subscribers_by_handle.lock() {
                if let Some(weak_subscriber) = subscribers_by_topic_handle.get(subscriber_handle) {
                    if let Some(strong_subscriber) = weak_subscriber.upgrade() {
                        if let Ok(handle) = strong_subscriber.get_instance_handle() {
                            if handle == *subscriber_handle {
                                found_subscriber = Some(strong_subscriber);
                            }
                        }
                    }
                }
            }
        }

        // If found, add Subscriber to orphaned_entities
        if let Some(subscriber) = found_subscriber {
            if let Ok(mut orphaned_entities) = self.orphaned_entities.lock() {
                orphaned_entities.add_subscriber(subscriber);
            }
        }
        // If not found, it has already been properly deleted via delete_subscriber, so do nothing
    }

    pub fn get_builtin_subscriber(&self) -> DdsResult<Subscriber> {
        /*
            This operation enables access to the built-in Subscriber.
            Each DomainParticipant contains multiple built-in Topic objects and DataReader objects for accessing those Topics.
            All these DataReader objects belong to a single built-in Subscriber.

            Built-in Topics are used to convey information about other DomainParticipant, Topic, DataReader, and DataWriter objects.
            Descriptions of these built-in objects are covered in Section 2.2.5, Built-in Topics.
        */
        self.is_deleted()?;
        self.builtin_subscriber
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .clone()
            .ok_or(DdsError::PreconditionNotMet)
    }

    // TODO: In the future, when MultiTopic are implemented, extend this function to
    // TODO: Should enable searching for TopicDescription of this type.
    pub fn lookup_topicdescription(
        &self,
        topic_name: &str,
    ) -> DdsResult<Option<Arc<dyn TopicDescription>>> {
        /*
            The lookup_topicdescription operation enables access to previously locally created TopicDescription objects based on name.
            This operation takes the name of a TopicDescription as an argument.

            If a TopicDescription with the same name already exists, it provides access to that object.
            If it does not exist, it returns a platform-defined .nil. value, and this operation never blocks the caller.

            The lookup_topicdescription operation can be used to find locally created Topic, ContentFilteredTopic, and MultiTopic objects.

            Unlike the find_topic operation, lookup_topicdescription only searches for Topics created locally,
            Should not create a new TopicDescription.

            TopicDescription returned through lookup_topicdescription does not need to be deleted separately.
            However, if there are no DataReaders or DataWriters bound to that TopicDescription,
            It can be deleted through operations such as delete_topic, in which case it is actually deleted and subsequent lookup operations will fail.

            If the operation fails to find a TopicDescription, a platform-defined .nil. value is returned.
        */

        self.is_deleted()?;
        match self.find_topic_description_by_name(topic_name) {
            Ok(Some(topic)) => Ok(Some(topic)),
            Ok(None) => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn create_multitopic(
        &self,
        _topic_name: &str,
        _type_name: &str,
        _subscription_expression: &str,
        _expression_parameters: Vec<String>,
    ) -> DdsResult<MultiTopic> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        // let multi_topic = MultiTopic::new(
        //     type_name,
        //     topic_name,
        //     subscription_expression,
        //     expression_parameters,
        // );

        // related_topic.add_reference(); //need

        // let multi_topic_arc = Arc::new(multi_topic.clone());
        // let mut multi_guard = self.multi_topics.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        // multi_guard.push(multi_topic_arc);

        // multi_topic
        Err(DdsError::Unsupported)
    }

    pub fn delete_multitopic(&self, _multi_topic: MultiTopic) -> DdsResult<()> {
        // in: multitopic: Multitopic
        // out: DdsError_t
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    pub fn create_contentfilteredtopic<Foo>(
        &self,
        topic_name: &str,
        related_topic: &Topic,
        filter_expression: &str,
        expression_parameters: Vec<String>,
    ) -> DdsResult<ContentFilteredTopic> {
        self.is_deleted()?;

        let topic_arc = self.find_internal_topic(related_topic)?;

        if related_topic.get_participant()?.get_instance_handle()? != self.get_instance_handle()? {
            return Err(DdsError::PreconditionNotMet);
        }

        let handle = self.create_instance_handle()?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("DomainParticipant not properly initialized".to_string()))?;

        let filtered_topic = ContentFilteredTopic::new(
            topic_name,
            &topic_arc,
            filter_expression,
            expression_parameters,
            handle,
            self_ref,
        )?;

        let filtered_topic_ref = filtered_topic
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("ContentFilteredTopic is not properly initialized".to_string()))?
            .clone();
        let weak_filtered_topic = Arc::downgrade(&filtered_topic_ref);

        {
            let mut filtered_topics =
                self.filtered_topics.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            filtered_topics.push(weak_filtered_topic.clone());
        }

        {
            let mut filtered_topics_by_topic_handle =
                self.filtered_topics_by_topic_handle
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?;
            filtered_topics_by_topic_handle
                .entry(topic_arc.get_instance_handle()?)
                .or_insert_with(Vec::new)
                .push(weak_filtered_topic.clone());
        }

        Ok(filtered_topic)
    }

    pub fn delete_contentfilteredtopic(
        &self,
        mut content_filtered_topic: ContentFilteredTopic,
    ) -> DdsResult<()> {
        self.is_deleted()?;

        match self.try_delete_contentfilteredtopic(&mut content_filtered_topic) {
            Ok(()) => Ok(()),
            Err(e) => {
                let content_filtered_topic_ref = content_filtered_topic
                    .self_ref
                    .as_ref()
                    .ok_or(DdsError::Error("Topic is not properly initialized".to_string()))?
                    .clone();
                self.orphaned_entities
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .content_filtered_topics
                    .push(content_filtered_topic_ref);
                Err(e)
            }
        }
    }

    fn try_delete_contentfilteredtopic(
        &self,
        content_filtered_topic: &mut ContentFilteredTopic,
    ) -> DdsResult<()> {
        {
            let mut orphaned = self.orphaned_entities.lock().unwrap();
            orphaned.remove_content_filtered_topic(content_filtered_topic)
        };

        if content_filtered_topic.get_participant()?.get_instance_handle()?
            != self.get_instance_handle()?
        {
            return Err(DdsError::PreconditionNotMet);
        }
        if self.contains_topicdescription(content_filtered_topic)? {
            return Err(DdsError::PreconditionNotMet);
        }

        let related_topic_handle =
            content_filtered_topic.get_related_topic()?.get_instance_handle()?;
        let handle = content_filtered_topic.topic_instance_handle()?;

        {
            let mut content_filtered_topics = self
                .filtered_topics
                .lock()
                .map_err(|_| DdsError::Error("Failed to lock topics".to_string()))?;
            let mut filtered_topics_by_topic_handle =
                self.filtered_topics_by_topic_handle
                    .lock()
                    .map_err(|_| DdsError::Error("Failed to lock topics_by_handle".to_string()))?;

            if !filtered_topics_by_topic_handle.contains_key(&related_topic_handle) {
                return Err(DdsError::Error("Topic not found".to_string()));
            }

            content_filtered_topics.retain(|weak_cft| match weak_cft.upgrade() {
                Some(cft) => cft.topic_instance_handle().is_ok_and(|h| h != handle),
                None => false,
            });

            if let Some(cft_list) = filtered_topics_by_topic_handle.get_mut(&related_topic_handle) {
                cft_list.retain(|weak_cft| match weak_cft.upgrade() {
                    Some(cft) => cft.topic_instance_handle().is_ok_and(|h| h != handle),
                    None => false,
                });

                if cft_list.is_empty() {
                    filtered_topics_by_topic_handle.remove(&related_topic_handle);
                }
            }
        }

        content_filtered_topic.delete();
        Ok(())
    }

    pub(crate) fn handle_contentfilteredtopic_drop(
        &self,
        topic_handle: &InstanceHandle,
        content_filtered_topic_handle: &InstanceHandle,
    ) {
        let mut found_topic = None;
        if let Ok(filtered_topics) = self.filtered_topics.lock() {
            for weak_filtered_topic in filtered_topics.iter() {
                if let Some(strong_filtered_topic) = weak_filtered_topic.upgrade() {
                    if let Ok(handle) = strong_filtered_topic.topic_instance_handle() {
                        if handle == *content_filtered_topic_handle {
                            found_topic = Some(strong_filtered_topic);
                            break;
                        }
                    }
                }
            }
        }

        if found_topic.is_none() {
            if let Ok(filtered_topics_by_topic_handle) = self.filtered_topics_by_topic_handle.lock()
            {
                if let Some(weak_filtered_topics) =
                    filtered_topics_by_topic_handle.get(topic_handle)
                {
                    for weak_filtered_topic in weak_filtered_topics.iter() {
                        {
                            if let Some(strong_filtered_topic) = weak_filtered_topic.upgrade() {
                                if let Ok(handle) = strong_filtered_topic.topic_instance_handle() {
                                    if handle == *content_filtered_topic_handle {
                                        found_topic = Some(strong_filtered_topic);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if let Some(topic) = found_topic {
            if let Ok(mut orphaned_entities) = self.orphaned_entities.lock() {
                orphaned_entities.add_content_filtered_topic(topic);
            }
        }
    }

    pub fn assert_liveliness(&self) -> DdsResult<()> {
        /*
            This operation manually asserts the Liveliness of the DomainParticipant.
            This operation is used in conjunction with the LIVELINESS QoS policy (see Section 2.2.3, Supported QoS) to inform the service that the entity is still active.

            This operation should only be used when the DomainParticipant contains DataWriter entities with LIVELINESS set to MANUAL_BY_PARTICIPANT,
            It only affects the liveliness of that DataWriter entity. Otherwise this operation has no effect.

            Note:
            The act of writing data through a DataWriter write operation automatically asserts the liveliness of that DataWriter and its DomainParticipant.
            Therefore, use of assert_liveliness is only necessary when the application does not write data regularly.

            For details, see Section 2.2.3.11 LIVELINESS.
        */
        self.is_enabled()?;

        if !self.has_manual_by_participant_writers()? {
            return Ok(());
        }
        let rtps_participant = self.get_rtps_participant()?;

        // Update liveliness at participant level (affects all MANUAL_BY_PARTICIPANT writers)
        if rtps_participant.assert_liveliness() {
            Ok(())
        } else {
            Err(DdsError::Error("Failed to assert participant liveliness".to_string()))
        }
    }

    pub fn delete_contained_entities(&self) -> DdsResult<()> {
        /*
            This operation deletes all entities created through "create" operations on that DomainParticipant.
            That is, this operation deletes all contained Publisher, Subscriber, Topic, ContentFilteredTopic, and MultiTopic objects.
            Before deleting each contained entity, this operation recursively calls the corresponding delete_contained_entities operation for each contained entity (if applicable). This pattern is applied recursively.
            In this way, the delete_contained_entities operation on a DomainParticipant deletes all entities recursively contained in that DomainParticipant, including DataWriter, DataReader, and QueryCondition and ReadCondition objects belonging to those DataReaders.

            This operation returns PRECONDITION_NOT_MET if any of the contained entities is in a state where it cannot be deleted.

            If delete_contained_entities returns successfully, the application can delete the DomainParticipant knowing that no contained entities exist anymore.
        */
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        {
            match self.get_publishers() {
                Ok(publishers) => {
                    for publisher in publishers {
                        publisher.delete_contained_entities()?;
                        self.delete_publisher((*publisher).clone())?;
                    }
                }
                Err(err) => return Err(err),
            }
            match self.get_subscribers() {
                Ok(subscribers) => {
                    for subscriber in subscribers {
                        subscriber.delete_contained_entities()?;
                        self.delete_subscriber((*subscriber).clone())?;
                    }
                }
                Err(e) => return Err(e),
            }
            match self.get_filtered_topics() {
                Ok(filtered_topics) => {
                    for filtered_topic in filtered_topics {
                        self.delete_contentfilteredtopic((*filtered_topic).clone())?;
                    }
                }
                Err(e) => return Err(e),
            }
            match self.get_topics() {
                Ok(topics) => {
                    for topic in topics {
                        self.delete_topic((*topic).clone())?;
                    }
                }
                Err(e) => return Err(e),
            }
            // match self.get_multi_topics() {
            //     Ok(multi_topics) => {
            //         if !multi_topics.is_empty() {
            //             return Ok(true);
            //         }
            //     }
            //     Err(e) => return Err(e),
            // }
            Ok(())
        }
    }

    // TODO: RTPS layer implementation must precede this (Built-in)
    pub fn ignore_topic(&self, _handle: InstanceHandle) -> DdsResult<()> {
        /*
            This operation allows the application to instruct the service to ignore a specific Topic locally.
            Thereafter, all publications or subscriptions to that Topic are ignored locally.

            If the application knows it will never publish or subscribe to data for a specific Topic,
            Can be used to save local resources.

            The Topic to be ignored is identified through the handle argument.
            This handle is the handle that appears in the SampleInfo acquired when reading data from the "DCPSTopic" topic through the built-in DataReader.

            The ignore_topic operation does not require being reversible.
            The service does not provide a way to reverse this.

            Error codes that can be returned in addition to standard errors:

            OUT_OF_RESOURCES
        */

        // match self.is_enabled() {
        //     DdsError::Ok => (),
        //     err_code => return Err(err_code),
        // }
        Err(DdsError::Unsupported)
    }

    /// Creates a new `Topic` with the specified name and type.
    ///
    /// Topics are the central concept in DDS for data distribution. A topic represents a
    /// named data stream of a particular type. Publishers write data to topics, and subscribers
    /// read data from topics. Communication occurs between publishers and subscribers that use
    /// the same topic name and compatible types.
    ///
    /// The topic will be automatically enabled if the participant's QoS policy
    /// `autoenable_created_entities` is set to true (which is the default).
    ///
    /// # Type Parameters
    ///
    /// * `Foo` - The data type for this topic. Must implement the `DdsType` trait (typically derived).
    ///
    /// # Arguments
    ///
    /// * `topic_name` - The name of the topic. Must be unique within the domain.
    /// * `type_name` - The name of the data type. Can be the same as the Rust type name.
    /// * `qos` - Quality of Service policies for the topic. Use `TopicQos::default()` for defaults.
    /// * `listener` - Optional listener for status notifications. Pass `None` if not needed.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Topic)` on success, or a `DdsError` if creation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The participant has been deleted
    /// * The QoS policies are inconsistent
    /// * Type registration fails
    /// * A topic with the same name but different type already exists
    pub fn create_topic<Foo>(
        &self,
        topic_name: &str,
        type_name: &str,
        qos: TopicQos,
        listener: Option<Arc<dyn TopicListener>>,
        mask: StatusMask,
    ) -> DdsResult<Topic>
    where
        Foo: DdsType,
    {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        qos.is_consistent()?;
        let handle = self.create_instance_handle()?;

        let type_support = Foo::TypeSupport::default();
        self.register_type(Arc::new(type_support), type_name)?;
        if self.find_typesupport(type_name).is_none() {
            return Err(DdsError::PreconditionNotMet);
        }

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("DomainParticipant not properly initialized".to_string()))?;
        let topic = Topic::new(false, topic_name, type_name, qos, listener, mask, handle, self_ref);
        let qos = self.get_qos()?;
        if let Ok(()) = self.is_enabled() {
            if qos.entity_factory.autoenable_created_entities {
                topic.enable()?;
            }
        }

        let topic_ref = topic
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Topic is not properly initialized".to_string()))?
            .clone();
        let weak_topic = Arc::downgrade(&topic_ref);
        {
            let mut topics = self.topics.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            topics.push(weak_topic.clone());
        }
        {
            let mut topics_by_handle =
                self.topics_by_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            topics_by_handle.insert(handle, weak_topic.clone());
        }

        Ok(topic)
    }

    /// Creates a new `Topic` using QoS settings from a loaded profile.
    ///
    /// This is a convenience method that retrieves QoS from the profile and delegates
    /// to [`create_topic`](Self::create_topic).
    ///
    /// # Arguments
    ///
    /// * `topic_name` - The name of the topic.
    /// * `type_name` - The registered type name for this topic.
    /// * `qos_path` - QoS path in the format `"Library::Profile"` or `"Library::Profile::QosName"`.
    ///   See [`QosProvider`](crate::config::json::QosProvider) for supported path formats.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Errors
    ///
    /// Returns an error if the profile is not found or topic creation fails.
    pub fn create_topic_with_profile<Foo>(
        &self,
        topic_name: &str,
        type_name: &str,
        qos_path: &str,
        listener: Option<Arc<dyn TopicListener>>,
        mask: StatusMask,
    ) -> DdsResult<Topic>
    where
        Foo: DdsType,
    {
        let qos = self.get_topic_qos_from_profile(qos_path)?;
        self.create_topic::<Foo>(topic_name, type_name, qos, listener, mask)
    }

    /// Creates a builtin topic for internal use.
    ///
    /// Builtin topics are used for discovery protocol (DCPSParticipant, DCPSPublication,
    /// DCPSSubscription, DCPSParticipantMessage). They are created during participant
    /// initialization and cannot be deleted by user code.
    fn create_builtin_topic<Foo>(
        participant: &Arc<Self>,
        topic_name: &str,
        type_name: &str,
    ) -> DdsResult<Topic>
    where
        Foo: DdsType,
    {
        let handle = participant.create_instance_handle()?;

        // Register type support for builtin type
        let type_support = Foo::TypeSupport::default();
        participant.register_type(Arc::new(type_support), type_name)?;

        let topic = Topic::new(
            true, // is_builtin
            topic_name,
            type_name,
            TopicQos::default(),
            None,
            StatusMask::all(),
            handle,
            participant,
        );

        // Add weak references to topics collection (for find_internal_topic lookup)
        let topic_ref = topic
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Topic is not properly initialized".to_string()))?
            .clone();
        let weak_topic = Arc::downgrade(&topic_ref);
        {
            let mut topics =
                participant.topics.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            topics.push(weak_topic.clone());
        }
        {
            let mut topics_by_handle =
                participant.topics_by_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            topics_by_handle.insert(handle, weak_topic);
        }

        // Store owned topic in builtin_topics for proper cleanup
        {
            let mut builtin_topics =
                participant.builtin_topics.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            builtin_topics.push(topic.clone());
        }

        Ok(topic)
    }

    pub fn delete_topic(&self, mut topic: Topic) -> DdsResult<()> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        match self.try_delete_topic(&mut topic) {
            Ok(()) => Ok(()),
            Err(e) => {
                let topic_ref = topic
                    .self_ref
                    .as_ref()
                    .ok_or(DdsError::Error("Topic is not properly initialized".to_string()))?
                    .clone();
                self.orphaned_entities
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .add_topic(topic_ref);
                Err(e)
            }
        }
    }

    fn try_delete_topic(&self, topic: &mut Topic) -> DdsResult<()> {
        {
            let mut orphaned = self.orphaned_entities.lock().unwrap();
            orphaned.remove_topic(topic)
        };

        if topic.get_participant()?.get_instance_handle()? != self.get_instance_handle()? {
            return Err(DdsError::PreconditionNotMet);
        }
        if self.contains_topic(topic)? {
            return Err(DdsError::PreconditionNotMet);
        }

        let type_name = topic.get_type_name();
        let handle = topic.get_instance_handle()?;
        let should_unregister_type = !self.has_other_topics_with_type(type_name, &handle)?;

        {
            let mut topics = self
                .topics
                .lock()
                .map_err(|_| DdsError::Error("Failed to lock topics".to_string()))?;
            let mut topics_by_handle = self
                .topics_by_handle
                .lock()
                .map_err(|_| DdsError::Error("Failed to lock topics_by_handle".to_string()))?;

            // Check existence with handle
            if !topics_by_handle.contains_key(&handle) {
                return Err(DdsError::Error("Topic not found".to_string()));
            }

            // Clean up dead references and remove topic
            topics.retain(|weak_topic| {
                match weak_topic.upgrade() {
                    Some(p) => p.get_instance_handle().is_ok_and(|h| h != handle),
                    None => false, // Remove dead reference
                }
            });

            topics_by_handle.remove(&handle);
        }

        // Unregister type only when not used by other topics
        if should_unregister_type {
            self.unregister_type(type_name)?;
        }

        topic.delete();
        Ok(())
    }

    pub(crate) fn handle_topic_drop(&self, topic_handle: &InstanceHandle) {
        let mut found_topic = None;
        if let Ok(topics) = self.topics.lock() {
            for weak_topic in topics.iter() {
                if let Some(strong_topic) = weak_topic.upgrade() {
                    if let Ok(handle) = strong_topic.get_instance_handle() {
                        if handle == *topic_handle {
                            found_topic = Some(strong_topic);
                            break;
                        }
                    }
                }
            }
        }

        if found_topic.is_none() {
            if let Ok(topics_by_topic_handle) = self.topics_by_handle.lock() {
                if let Some(weak_topic) = topics_by_topic_handle.get(topic_handle) {
                    if let Some(strong_topic) = weak_topic.upgrade() {
                        if let Ok(handle) = strong_topic.get_instance_handle() {
                            if handle == *topic_handle {
                                found_topic = Some(strong_topic);
                            }
                        }
                    }
                }
            }
        }

        // If found, add Publisher to orphaned_entities
        if let Some(topic) = found_topic {
            if let Ok(mut orphaned_entities) = self.orphaned_entities.lock() {
                orphaned_entities.add_topic(topic);
            }
        }
        // If not found, it has already been deleted by delete_topic, so do nothing
    }

    // Can be changed to support remote Topic propagation in the future
    pub fn find_topic(&self, topic_name: &str, timeout: Duration) -> DdsResult<Topic> {
        /*
           The find_topic operation enables access to an active Topic that already exists or is ready to exist, based on name.
           This operation takes the Topic name and a timeout value as arguments.

           If a Topic with the same name already exists, it provides access to that Topic.
           If it does not exist, it blocks the caller waiting until another mechanism creates the Topic (or until the specified timeout occurs).
           This "other mechanism" could be a separate thread, configuration tool, or other middleware service.

           Note that a Topic is a local object that acts as a "proxy" referring to a global concept.
           Depending on the middleware implementation, it may choose to propagate Topics and make remotely created Topics available locally.

           A Topic acquired through find_topic must be deleted through delete_topic, which releases local resources.
           If the same Topic is acquired multiple times through find_topic or create_topic, delete_topic must be called a corresponding number of times.

           Regardless of whether the middleware provides Topic propagation, the delete_topic operation only deletes the local proxy object.
           If the operation reaches a timeout, it returns a platform-defined .nil. value.
        */
        self.is_deleted()?;

        if let Ok(Some(topic)) = self.find_topic_by_name(topic_name) {
            return Ok((*topic).clone());
        }

        if timeout.is_zero() {
            return Err(DdsError::Timeout);
        }

        let start_time = Time::now();

        loop {
            if let Ok(Some(topic)) = self.find_topic_by_name(topic_name) {
                return Ok((*topic).clone());
            }

            if !timeout.is_infinite() {
                let elapsed = start_time.elapsed()?;

                if elapsed >= timeout {
                    return Err(DdsError::Timeout);
                }
            }

            // Prevent busy loop
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    fn find_topic_by_name(&self, topic_name: &str) -> DdsResult<Option<Arc<Topic>>> {
        match self.get_topics() {
            Ok(topics) => {
                let result = topics.iter().find(|t| t.get_name() == topic_name).cloned();

                match result {
                    Some(topic) => Ok(Some(topic)),
                    None => Ok(None),
                }
            }
            Err(err_code) => Err(err_code),
        }
    }

    fn find_topic_description_by_name(
        &self,
        topic_name: &str,
    ) -> DdsResult<Option<Arc<dyn TopicDescription>>> {
        match self.get_topics() {
            Ok(topics) => {
                let result = topics.iter().find(|t| t.get_name() == topic_name).cloned();

                if let Some(topic) = result {
                    return Ok(Some(topic));
                }
            }
            Err(err_code) => return Err(err_code),
        }

        match self.get_filtered_topics() {
            Ok(topics) => {
                let result = topics.iter().find(|t| t.get_name() == topic_name).cloned();

                if let Some(topic) = result {
                    return Ok(Some(topic));
                }
            }
            Err(err_code) => return Err(err_code),
        }

        Ok(None)
    }

    // TODO: RTPS layer implementation must precede this (Built-in)
    pub fn get_discovered_participants(&self) -> DdsResult<Vec<InstanceHandle>> {
        /*
            Among the DomainParticipants discovered in the domain,
            Retrieves a list of DomainParticipants that the application has not specified to "ignore" through the ignore_participant operation.
            If the infrastructure does not maintain connectivity information locally
        */
        // TODO: Filter out participants ignored via ignore_participant operation
        self.is_deleted()?;
        let rtps_participant = self.get_rtps_participant()?;
        let proxy_datas = rtps_participant.remote_participant_proxy_datas();
        let result = match proxy_datas.lock() {
            Ok(datas) => {
                let handles = datas
                    .iter()
                    .map(|data| InstanceHandle::from_guid(&data.participant_guid()))
                    .collect();
                Ok(handles)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        };
        result
    }

    pub fn get_discovered_participant_data(
        &self,
        participant_handle: InstanceHandle,
    ) -> DdsResult<ParticipantBuiltinTopicData> {
        /*
            This operation retrieves information about DomainParticipants discovered on the network.
            The Participant must belong to the same domain as the DomainParticipant on which this operation is called,
            and must not be "ignored" through the ignore_participant operation.
            participant_handle must reference a DomainParticipant that meets these conditions,
            Otherwise the operation fails and returns PRECONDITION_NOT_MET.
            The get_discovered_participants operation can be used to find currently discovered DomainParticipants.
            If the infrastructure does not maintain the information needed to fill participant_data,
        */
        // TODO: Filter out participants ignored via ignore_participant operation
        self.is_deleted()?;
        let participant_guid = participant_handle.to_guid();
        let rtps_participant = self.get_rtps_participant()?;
        let proxy_datas = rtps_participant.remote_participant_proxy_datas();
        let datas_guard = proxy_datas.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let proxy_data = datas_guard
            .iter()
            .find(|data| data.participant_guid() == participant_guid)
            .ok_or(DdsError::PreconditionNotMet)?;
        Ok(ParticipantBuiltinTopicData::new(participant_guid, proxy_data.user_data().clone()))
    }

    // TODO: RTPS layer implementation must precede this (Built-in)
    pub fn get_discovered_topics(&self) -> DdsResult<Vec<InstanceHandle>> {
        /*
            Among the Topics discovered in the domain,
            retrieves a list of Topics that the application has not specified to "ignore" through the ignore_topic operation.
        */
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    // TODO: RTPS layer implementation must precede this (Built-in)
    pub fn get_discovered_topic_data(
        &self,
        _topic_handle: InstanceHandle,
    ) -> DdsResult<TopicBuiltinTopicData> {
        /*
            This operation retrieves information about Topics discovered on the network.
            The Topic must be created by a Participant in the same domain as the DomainParticipant on which this operation is called, and must not be "ignored" through the ignore_topic operation.
            topic_handle must reference a Topic that meets these conditions, otherwise the operation fails and returns PRECONDITION_NOT_MET.
            The get_discovered_topics operation can be used to find currently discovered Topics.
            If the infrastructure does not maintain the information needed to fill topic_data, this operation may fail and return UNSUPPORTED.
            This operation may also fail if the infrastructure does not maintain connectivity information locally, in which case it returns UNSUPPORTED.
        */
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    pub fn contains_entity(&self, handle: InstanceHandle) -> DdsResult<bool> {
        self.is_deleted()?;

        match self.get_publishers_by_handle() {
            Ok(publishers_by_handle) => {
                if publishers_by_handle.contains_key(&handle) {
                    return Ok(true);
                }
            }
            Err(err) => return Err(err),
        }
        match self.get_subscribers_by_handle() {
            Ok(subscribers_by_handle) => {
                if subscribers_by_handle.contains_key(&handle) {
                    return Ok(true);
                }
            }
            Err(err) => return Err(err),
        }
        match self.get_topics_by_handle() {
            Ok(topics_by_handle) => {
                if topics_by_handle.contains_key(&handle) {
                    return Ok(true);
                }
            }
            Err(err) => return Err(err),
        }
        match self.get_publishers() {
            Ok(publishers) => {
                for publisher in publishers.iter() {
                    if Ok(true) == publisher.contains_entity(handle) {
                        return Ok(true);
                    }
                }
            }
            Err(err) => return Err(err),
        }
        match self.get_subscribers() {
            Ok(subscribers) => {
                for subscriber in subscribers.iter() {
                    if Ok(true) == subscriber.contains_entity(handle) {
                        return Ok(true);
                    }
                }
            }
            Err(err) => return Err(err),
        }
        Ok(false)
    }

    pub fn get_current_time(&self) -> DdsResult<Time> {
        self.is_deleted()?;

        let now_system_time = SystemTime::now();
        match now_system_time.duration_since(UNIX_EPOCH) {
            Ok(unix_time) => {
                Ok(Time { sec: unix_time.as_secs() as i32, nanosec: unix_time.subsec_nanos() })
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn set_default_publisher_qos(&self, qos: PublisherQos) -> DdsResult<()> {
        self.is_deleted()?;

        if qos == PUBLISHER_QOS_DEFAULT {
            return self.reset_default_publisher_qos();
        }
        match qos.is_consistent() {
            Ok(()) => match self.default_publisher_qos.lock() {
                Ok(mut default_qos) => {
                    *default_qos = qos;
                    Ok(())
                }
                Err(e) => Err(DdsError::Error(e.to_string())),
            },
            Err(err_code) => Err(err_code),
        }
    }

    fn reset_default_publisher_qos(&self) -> DdsResult<()> {
        match self.default_publisher_qos.lock() {
            Ok(mut default_qos) => {
                *default_qos = PUBLISHER_QOS_DEFAULT;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn get_default_publisher_qos(&self) -> DdsResult<PublisherQos> {
        self.is_deleted()?;

        Ok(self.default_publisher_qos.lock().map_err(|e| DdsError::Error(e.to_string()))?.clone())
    }

    /// Retrieves `PublisherQos` from a loaded profile.
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    ///
    /// Returns an error if the participant is deleted or the profile is not found.
    pub fn get_publisher_qos_from_profile(&self, qos_path: &str) -> DdsResult<PublisherQos> {
        self.is_deleted()?;
        DomainParticipantFactory::get_instance().get_publisher_qos_from_profile(qos_path)
    }

    pub fn set_default_subscriber_qos(&self, qos: SubscriberQos) -> DdsResult<()> {
        self.is_deleted()?;

        if qos == SUBSCRIBER_QOS_DEFAULT {
            return self.reset_default_subscriber_qos();
        }
        match qos.is_consistent() {
            Ok(()) => match self.default_subscriber_qos.lock() {
                Ok(mut default_qos) => {
                    *default_qos = qos;
                    Ok(())
                }
                Err(e) => Err(DdsError::Error(e.to_string())),
            },
            Err(err_code) => Err(err_code),
        }
    }

    fn reset_default_subscriber_qos(&self) -> DdsResult<()> {
        match self.default_subscriber_qos.lock() {
            Ok(mut default_qos) => {
                *default_qos = SUBSCRIBER_QOS_DEFAULT;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn get_default_subscriber_qos(&self) -> DdsResult<SubscriberQos> {
        self.is_deleted()?;

        Ok(self.default_subscriber_qos.lock().map_err(|e| DdsError::Error(e.to_string()))?.clone())
    }

    /// Retrieves `SubscriberQos` from a loaded profile.
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    ///
    /// Returns an error if the participant is deleted or the profile is not found.
    pub fn get_subscriber_qos_from_profile(&self, qos_path: &str) -> DdsResult<SubscriberQos> {
        self.is_deleted()?;
        DomainParticipantFactory::get_instance().get_subscriber_qos_from_profile(qos_path)
    }

    pub fn set_default_topic_qos(&self, qos: TopicQos) -> DdsResult<()> {
        self.is_deleted()?;

        if qos == TOPIC_QOS_DEFAULT {
            return self.reset_default_topic_qos();
        }
        match qos.is_consistent() {
            Ok(()) => match self.default_topic_qos.lock() {
                Ok(mut default_qos) => {
                    *default_qos = qos;
                    Ok(())
                }
                Err(e) => Err(DdsError::Error(e.to_string())),
            },
            Err(err_code) => Err(err_code),
        }
    }

    fn reset_default_topic_qos(&self) -> DdsResult<()> {
        match self.default_topic_qos.lock() {
            Ok(mut default_qos) => {
                *default_qos = TOPIC_QOS_DEFAULT;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn get_default_topic_qos(&self) -> DdsResult<TopicQos> {
        self.is_deleted()?;

        Ok(self.default_topic_qos.lock().map_err(|e| DdsError::Error(e.to_string()))?.clone())
    }

    /// Retrieves `TopicQos` from a loaded profile.
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    ///
    /// Returns an error if the participant is deleted or the profile is not found.
    pub fn get_topic_qos_from_profile(&self, qos_path: &str) -> DdsResult<TopicQos> {
        self.is_deleted()?;
        DomainParticipantFactory::get_instance().get_topic_qos_from_profile(qos_path)
    }

    pub fn get_domain_id(&self) -> DdsResult<DomainId> {
        self.is_deleted()?;
        Ok(self.domain_id)
    }

    // For Entity
    pub fn set_listener(
        &self,
        listener: Option<Arc<dyn DomainParticipantListener>>,
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
    pub fn get_listener(&self) -> DdsResult<Option<Arc<dyn DomainParticipantListener>>> {
        self.is_deleted()?;

        match self.listener.read() {
            Ok(guard) => Ok(guard.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_publishers(&self) -> DdsResult<Vec<Arc<Publisher>>> {
        match self.publishers.lock() {
            Ok(mut publishers) => {
                let mut valid_publishers = Vec::new();
                publishers.retain(|weak| {
                    if let Some(arc) = weak.upgrade() {
                        valid_publishers.push(arc);
                        true // Keep valid Weak in vector
                    } else {
                        false // Remove invalid Weak from vector
                    }
                });
                Ok(valid_publishers)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_subscribers(&self) -> DdsResult<Vec<Arc<Subscriber>>> {
        match self.subscribers.lock() {
            Ok(mut subscribers) => {
                let mut valid_subscribers = Vec::new();
                subscribers.retain(|weak| {
                    if let Some(arc) = weak.upgrade() {
                        valid_subscribers.push(arc);
                        true // Keep valid Weak in vector
                    } else {
                        false // Remove invalid Weak from vector
                    }
                });
                Ok(valid_subscribers)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_topics(&self) -> DdsResult<Vec<Arc<Topic>>> {
        match self.topics.lock() {
            Ok(mut topics) => {
                let mut valid_topics = Vec::new();
                topics.retain(|weak| {
                    if let Some(arc) = weak.upgrade() {
                        valid_topics.push(arc);
                        true // Keep valid Weak in vector
                    } else {
                        false // Remove invalid Weak from vector
                    }
                });
                Ok(valid_topics)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_filtered_topics(&self) -> DdsResult<Vec<Arc<ContentFilteredTopic>>> {
        match self.filtered_topics.lock() {
            Ok(mut filtered_topics) => {
                let mut valid_topics = Vec::new();
                filtered_topics.retain(|weak| {
                    if let Some(arc) = weak.upgrade() {
                        valid_topics.push(arc);
                        true // Keep valid Weak in vector
                    } else {
                        false // Remove invalid Weak from vector
                    }
                });
                Ok(valid_topics)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_publishers_by_handle(&self) -> DdsResult<HashMap<InstanceHandle, Arc<Publisher>>> {
        match self.publishers_by_handle.lock() {
            Ok(mut publishers_by_handle) => {
                let mut result = HashMap::new();
                publishers_by_handle.retain(|handle, weak| {
                    if let Some(arc) = weak.upgrade() {
                        result.insert(*handle, arc);
                        true // Keep valid Weak in HashMap
                    } else {
                        false // Remove invalid Weak from HashMap
                    }
                });

                Ok(result)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_subscribers_by_handle(&self) -> DdsResult<HashMap<InstanceHandle, Arc<Subscriber>>> {
        match self.subscribers_by_handle.lock() {
            Ok(mut subscribers_by_handle) => {
                let mut result = HashMap::new();
                subscribers_by_handle.retain(|handle, weak| {
                    if let Some(arc) = weak.upgrade() {
                        result.insert(*handle, arc);
                        true // Keep valid Weak in HashMap
                    } else {
                        false // Remove invalid Weak from HashMap
                    }
                });

                Ok(result)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn get_topics_by_handle(&self) -> DdsResult<HashMap<InstanceHandle, Arc<Topic>>> {
        match self.topics_by_handle.lock() {
            Ok(mut topics_by_handle) => {
                let mut result = HashMap::new();
                topics_by_handle.retain(|handle, weak| {
                    if let Some(arc) = weak.upgrade() {
                        result.insert(*handle, arc);
                        true // Keep valid Weak in HashMap
                    } else {
                        false // Remove invalid Weak from HashMap
                    }
                });

                Ok(result)
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn contains_topic(&self, topic: &Topic) -> DdsResult<bool> {
        // Check if there are ContentFilteredTopic, Multitopic, DataWriter, or DataReader using this Topic
        let publishers = self.get_publishers()?;
        for publisher in publishers {
            if publisher.has_writers_for_topic(topic)? {
                return Ok(true);
            }
        }

        let subscribers = self.get_subscribers()?;
        for subscriber in subscribers {
            if subscriber.has_readers_for_topic(topic)? {
                return Ok(true);
            }
        }

        match self.filtered_topics_by_topic_handle.lock() {
            Ok(filtered_topics_by_topic_handle) => {
                if filtered_topics_by_topic_handle.contains_key(&topic.get_instance_handle()?) {
                    return Ok(true);
                }
            }
            Err(e) => return Err(DdsError::Error(e.to_string())),
        }

        // match self.multitopics.lock() {
        //     Ok(multi_topics_by_topic_handle) => {
        //         if multi_topics_by_topic_handle.contains_key(&topic.get_instance_handle()?) {
        //             return Ok(true);
        //         }
        //     }
        //     Err(e) => return Err(DdsError::Error(e.to_string())),
        // }
        Ok(false)
    }

    fn contains_topicdescription(
        &self,
        topic_description: &dyn TopicDescription,
    ) -> DdsResult<bool> {
        let subscribers = self.get_subscribers()?;
        for subscriber in subscribers {
            if subscriber.has_readers_for_topic(topic_description)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Register a custom TypeSupport implementation with the DomainParticipant.
    ///
    /// This method allows registering a TypeSupport for a type before creating topics.
    /// When a topic is created with the same type_name, the already-registered TypeSupport
    /// will be used instead of the default one.
    ///
    /// This is particularly useful for FFI scenarios where the TypeSupport implementation
    /// needs to be provided at runtime rather than compile time.
    ///
    /// # Arguments
    /// * `type_support` - The TypeSupport implementation to register
    /// * `type_name` - The name to register the type under (must be non-empty)
    ///
    /// # Errors
    /// * `DdsError::BadParameter` - If type_name is empty
    /// * `DdsError::PreconditionNotMet` - If a different TypeSupport is already registered
    ///   for this type_name (same TypeSupport is OK)
    pub fn register_type_support(
        &self,
        type_support: Arc<dyn TypeSupport>,
        type_name: &str,
    ) -> DdsResult<()> {
        self.register_type(type_support, type_name)
    }

    /// Registers a `DynamicTypeSupport` for dynamic type handling.
    ///
    /// This is a convenience method for registering type support for dynamic data.
    /// The type name is automatically extracted from the DynamicTypeSupport.
    ///
    /// # Arguments
    ///
    /// * `type_support` - The `DynamicTypeSupport` created from a `TypeObject`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use int2dds::xtypes::DynamicTypeSupport;
    ///
    /// // Create DynamicTypeSupport from a TypeObject received during discovery
    /// let type_support = DynamicTypeSupport::from_type_object(type_object)?;
    ///
    /// // Register the dynamic type
    /// participant.register_dynamic_type(Arc::new(type_support))?;
    /// ```
    pub fn register_dynamic_type(
        &self,
        type_support: Arc<crate::xtypes::DynamicTypeSupport>,
    ) -> DdsResult<()> {
        let type_name = type_support.get_type_name().to_string();
        self.register_type(type_support, &type_name)
    }

    /// Creates a Topic for use with `DynamicData`.
    ///
    /// This method creates a topic without requiring a compile-time type. It is used
    /// when working with `DynamicTypeSupport` for dynamic data handling.
    ///
    /// # Arguments
    ///
    /// * `topic_name` - Name of the topic to create.
    /// * `type_support` - The `DynamicTypeSupport` for this topic.
    /// * `qos` - QoS policies for the topic.
    /// * `listener` - Optional listener for topic events.
    /// * `mask` - Status mask for listener callbacks.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use int2dds::xtypes::DynamicTypeSupport;
    ///
    /// let type_support = DynamicTypeSupport::from_type_object(type_object)?;
    /// let type_support_arc = Arc::new(type_support);
    ///
    /// // Register and create topic
    /// participant.register_dynamic_type(type_support_arc.clone())?;
    /// let topic = participant.create_topic_dynamic(
    ///     "SensorData",
    ///     type_support_arc,
    ///     TopicQos::default(),
    ///     None,
    ///     StatusMask::default(),
    /// )?;
    /// ```
    pub fn create_topic_dynamic(
        &self,
        topic_name: &str,
        type_support: Arc<crate::xtypes::DynamicTypeSupport>,
        qos: TopicQos,
        listener: Option<Arc<dyn TopicListener>>,
        mask: StatusMask,
    ) -> DdsResult<Topic> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        qos.is_consistent()?;
        let handle = self.create_instance_handle()?;

        let type_name = type_support.get_type_name().to_string();
        self.register_type(type_support, &type_name)?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("DomainParticipant not properly initialized".to_string()))?;
        let topic =
            Topic::new(false, topic_name, &type_name, qos, listener, mask, handle, self_ref);
        let qos = self.get_qos()?;
        if let Ok(()) = self.is_enabled() {
            if qos.entity_factory.autoenable_created_entities {
                topic.enable()?;
            }
        }

        // Store topic reference
        let topic_ref = topic
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Topic is not properly initialized".to_string()))?
            .clone();
        let weak_topic = Arc::downgrade(&topic_ref);
        {
            let mut topics = self.topics.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            topics.push(weak_topic.clone());
        }
        {
            let mut topics_by_handle =
                self.topics_by_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            topics_by_handle.insert(handle, weak_topic.clone());
        }

        Ok(topic)
    }

    /// Creates a `DynamicTypeSupport` from a `TypeObject`.
    ///
    /// This is a convenience method for creating dynamic type support from
    /// TypeObjects received during discovery. The TypeObject is typically
    /// obtained from `PublicationBuiltinTopicData::type_object()` after
    /// reading from the builtin subscriber's DCPSPublication DataReader.
    ///
    /// # Arguments
    ///
    /// * `type_object` - The `TypeObject` received during discovery.
    ///
    /// # Returns
    ///
    /// A `DynamicTypeSupport` that can be used to create DataReader or
    /// DataWriter for `DynamicData`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use int2dds::xtypes::{DynamicTypeSupport, TypeObject};
    ///
    /// // Get TypeObject from discovered publication
    /// let publication_data = datareader.get_matched_publication_data(handle)?;
    /// if let Some(type_object) = publication_data.type_object() {
    ///     // Create DynamicTypeSupport from TypeObject
    ///     let type_support = participant.create_dynamic_type_from_type_object(
    ///         type_object.clone()
    ///     )?;
    ///
    ///     // Register and use for dynamic data handling
    ///     let type_support_arc = Arc::new(type_support);
    ///     participant.register_dynamic_type(type_support_arc.clone())?;
    ///     let topic = participant.create_topic_dynamic(
    ///         "SensorTopic",
    ///         type_support_arc.clone(),
    ///         TopicQos::default(),
    ///         None,
    ///         StatusMask::default(),
    ///     )?;
    ///     let reader = subscriber.create_datareader_dynamic(
    ///         &topic,
    ///         type_support_arc,
    ///         DataReaderQos::default(),
    ///         None,
    ///         StatusMask::default(),
    ///     )?;
    /// }
    /// ```
    pub fn create_dynamic_type_from_type_object(
        &self,
        type_object: crate::xtypes::TypeObject,
    ) -> DdsResult<crate::xtypes::DynamicTypeSupport> {
        crate::xtypes::DynamicTypeSupport::from_type_object(type_object)
    }

    pub(crate) fn register_type(
        &self,
        type_support: Arc<dyn TypeSupport>,
        type_name: &str,
    ) -> DdsResult<()> {
        if type_name.is_empty() {
            return Err(DdsError::BadParameter);
        }

        // Use write lock of RwLock
        let mut types = self.types.write().unwrap();

        if let Some(existing_type) = types.get(type_name) {
            // type_id() method is available (from Arc<dyn TypeSupport>)
            if existing_type.type_id() == type_support.type_id() {
                return Ok(());
            }
            return Err(DdsError::PreconditionNotMet);
        }

        types.insert(type_name.to_string(), type_support);
        Ok(())
    }

    pub(crate) fn unregister_type(&self, type_name: &str) -> DdsResult<()> {
        let mut types = self.types.write().unwrap();

        if types.get(type_name).is_some() {
            types.remove(type_name);
            return Ok(());
        }
        Err(DdsError::PreconditionNotMet)
    }

    fn has_other_topics_with_type(
        &self,
        type_name: &str,
        excluding_handle: &InstanceHandle,
    ) -> DdsResult<bool> {
        let topics =
            self.topics.lock().map_err(|_| DdsError::Error("Failed to lock topics".to_string()))?;

        for weak_topic in topics.iter() {
            if let Some(topic) = weak_topic.upgrade() {
                if let Ok(topic_handle) = topic.get_instance_handle() {
                    // Check excluding topics to be removed
                    if topic_handle != *excluding_handle && topic.get_type_name() == type_name {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    pub(crate) fn find_internal_topic(&self, external_topic: &Topic) -> DdsResult<Arc<Topic>> {
        let topic_arc = match self.topics.lock() {
            Ok(topics) => topics
                .iter()
                .filter_map(|weak_topic| weak_topic.upgrade())
                .find(|topic| **topic == *external_topic)
                .ok_or(DdsError::BadParameter)?,
            Err(e) => return Err(DdsError::Error(e.to_string())),
        };

        Ok(topic_arc)
    }

    pub(crate) fn find_internal_cft(
        &self,
        external_topic: &ContentFilteredTopic,
    ) -> DdsResult<Arc<ContentFilteredTopic>> {
        let topic_arc = match self.filtered_topics.lock() {
            Ok(filtered_topics) => filtered_topics
                .iter()
                .filter_map(|weak_topic| weak_topic.upgrade())
                .find(|topic| **topic == *external_topic)
                .ok_or(DdsError::BadParameter)?,
            Err(e) => return Err(DdsError::Error(e.to_string())),
        };

        Ok(topic_arc)
    }

    pub(crate) fn find_typesupport(&self, type_name: &str) -> Option<Arc<dyn TypeSupport>> {
        // Use read lock of RwLock (multiple threads can read simultaneously)
        self.types.read().unwrap().get(type_name).cloned()
    }

    fn create_instance_handle(&self) -> DdsResult<InstanceHandle> {
        let id = self.next_instance_id.fetch_add(1, Ordering::SeqCst);

        // Copy GUID value to handle
        let mut handle = self.get_instance_handle()?;

        // Set vendor specific flag and ID value
        handle[15] = 0x01;
        handle[14] = (id & 0xFF) as u8;
        handle[13] = ((id >> 8) & 0xFF) as u8;
        handle[12] = ((id >> 16) & 0xFF) as u8;

        Ok(handle)
    }

    pub(crate) fn is_enabled(&self) -> DdsResult<()> {
        self.is_deleted()?;
        if self.enabled.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(DdsError::NotEnabled)
        }
    }

    pub(crate) fn disable(&self) -> DdsResult<()> {
        self.set_listener(None, StatusMask::default())?;

        {
            match self.publishers.lock() {
                Ok(publishers) => {
                    for weak_publisher in publishers.iter() {
                        if let Some(publisher) = weak_publisher.upgrade() {
                            publisher.disable()?;
                        }
                    }
                }
                Err(e) => return Err(DdsError::Error(e.to_string())),
            }
        }

        {
            match self.subscribers.lock() {
                Ok(subscribers) => {
                    for weak_subscriber in subscribers.iter() {
                        if let Some(subscriber) = weak_subscriber.upgrade() {
                            subscriber.disable()?;
                        }
                    }
                }
                Err(e) => return Err(DdsError::Error(e.to_string())),
            }
        }
        Ok(())
    }

    pub(crate) fn get_dcps_bridge(&self) -> DdsResult<MutexGuard<'_, Option<DcpsBridge>>> {
        self.dcps_bridge.lock().map_err(|e| DdsError::Error(e.to_string()))
    }

    pub(crate) fn get_rtps_participant(&self) -> DdsResult<RtpsParticipant> {
        let bridge_guard = self.get_dcps_bridge()?;
        match bridge_guard.as_ref() {
            Some(bridge) => bridge.get_participant().map_err(|e| DdsError::Error(e.message)),
            None => Err(DdsError::Error("DCPS Bridge is not initialized".to_string())),
        }
    }

    fn has_manual_by_participant_writers(&self) -> DdsResult<bool> {
        let publishers = self.get_publishers()?;
        for publisher in publishers {
            let writers = publisher.get_data_writers()?;
            for writer in writers {
                if writer.get_qos()?.liveliness.kind == LivelinessQosPolicyKind::ManualByParticipant
                {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub fn guid(&self) -> DdsResult<Guid> {
        Ok(*self.guid)
    }

    pub(crate) fn delete(&mut self) -> DdsResult<()> {
        // Clean up builtin entities to break self-reference cycles
        self.cleanup_builtin_entities();

        let mut bridge_guard =
            self.dcps_bridge.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        if let Some(dcps_bridge) = bridge_guard.as_mut() {
            dcps_bridge.disable().map_err(|rtps_err| {
                // Handle sending handler related errors specifically
                match rtps_err.code {
                    RtpsErrorCode::LockError | RtpsErrorCode::ThreadJoinError => {
                        DdsError::PreconditionNotMet
                    }
                    _ => DdsError::Error("Failed to disable RTPS participant".to_string()), // Or other appropriate mapping
                }
            })?;
        }

        *bridge_guard = None;
        self.self_ref = None;
        self.deleted.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// Cleans up builtin entities to break self-reference cycles and prevent memory leaks.
    fn cleanup_builtin_entities(&self) {
        // Clean up builtin subscriber and its datareaders
        if let Ok(mut guard) = self.builtin_subscriber.lock() {
            if let Some(mut subscriber) = guard.take() {
                subscriber.cleanup_builtin_entities();
            }
        }

        // Clean up builtin topics
        if let Ok(mut builtin_topics) = self.builtin_topics.lock() {
            for mut topic in builtin_topics.drain(..) {
                topic.delete();
            }
        }
    }

    fn is_deleted(&self) -> DdsResult<()> {
        if self.deleted.load(Ordering::SeqCst) {
            Err(DdsError::AlreadyDeleted)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod domain_participant_tests {
    use super::*;
    use crate::domain::domain_participant_factory::DomainParticipantFactory;
    use crate::infrastructure::qos_policy::EntityFactoryQosPolicy;
    use crate::publication::qos::DataWriterQos;
    use crate::subscription::data_reader::DataReaderInternal;
    use crate::subscription::qos::DataReaderQos;
    use crate::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
    use crate::test_utils::unique_domain_id;
    use std::time::{Duration as StdDuration, Instant};
    use std::{sync::Arc, thread};

    #[derive(DdsType)]
    pub struct HelloWorld {
        pub index: u32,
        pub message: String,
    }

    #[test]
    #[ignore]
    fn test_domain_participant_equality() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant with same settings
        let participant1 = factory
            .create_participant(1, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // Clone of same participant - DomainParticipant's clone
        let participant1_clone = participant1.clone();

        // Different participant (same settings but different instance)
        let participant2 = factory
            .create_participant(1, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // Participant with different domain_id
        let participant3 = factory
            .create_participant(1, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // Participant with different QoS
        let mut different_qos = DomainParticipantQos::default();
        different_qos.entity_factory.autoenable_created_entities = false; // Set different from default

        let participant4 =
            factory.create_participant(1, different_qos, None, StatusMask::default()).unwrap();

        // Test 1: Cloned participant has same internal Arc pointers
        assert!(participant1 == participant1_clone);

        // Test 2: Different participant instances should differ
        assert!(participant1 != participant2);

        // Test 3: Participants with different domain_id should differ
        assert!(participant1 != participant3);

        // Test 4: Participants with different QoS should differ
        assert!(participant1 != participant4);

        // Test 5: Search test in participant vector
        let participants = vec![participant1.clone(), participant3.clone()];

        assert!(participants.contains(&participant1), "Should find the participant in the vector");

        assert!(
            !participants.contains(&participant2),
            "Should not find a different participant in the vector"
        );

        assert!(
            participants.contains(&participant1_clone),
            "Should find the cloned participant in the vector"
        );

        // Test 6: Verify internal Arc pointers are the same in cloned object
        assert!(Arc::ptr_eq(&participant1.publishers, &participant1_clone.publishers));
        assert!(Arc::ptr_eq(&participant1.subscribers, &participant1_clone.subscribers));
        assert!(Arc::ptr_eq(&participant1.topics, &participant1_clone.topics));
    }

    #[test]
    #[ignore]
    fn test_participant_after_modification() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(2, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let participant_clone = participant.clone();

        // let publisher = participant
        //     .create_publisher(PublisherQos::default(), None, StatusMask::default())
        //     .unwrap();
        // Add Publisher to participant

        assert!(participant == participant_clone);
    }

    #[test]
    fn autoenable_created_entities_false() {
        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();

        let mut domain_participant_qos = DomainParticipantQos::default();
        domain_participant_qos.entity_factory =
            EntityFactoryQosPolicy { autoenable_created_entities: false };

        let participant = factory
            .create_participant(domain_id, domain_participant_qos, None, StatusMask::default())
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorld>(
                "hello_world",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let res = publisher.is_enabled();
        assert!(res.is_err());

        let writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let res = writer.is_enabled();
        assert!(res.is_err());

        // write() should fail when not enabled
        let res = writer
            .write(&HelloWorld { index: 0, message: "hello".to_string() }, InstanceHandle::NIL);
        assert!(res.is_err());

        // Manually enable the writer
        let res = writer.enable();
        assert!(res.is_err());

        participant.enable().unwrap();
        let res = participant.is_enabled();
        assert!(res.is_ok());
        publisher.enable().unwrap();

        // Now write() should succeed
        let res_2 = writer
            .write(&HelloWorld { index: 0, message: "hello".to_string() }, InstanceHandle::NIL);
        assert!(res_2.is_ok());
    }

    #[test]
    fn autoenable_created_entities_false_pub() {
        let domain_id: i32 = 87;
        let factory = DomainParticipantFactory::get_instance();

        let domain_participant_qos = DomainParticipantQos::default();
        let participant = factory
            .create_participant(domain_id, domain_participant_qos, None, StatusMask::default())
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorld>(
                "hello_world",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let mut publisher_qos = PublisherQos::default();
        publisher_qos.entity_factory =
            EntityFactoryQosPolicy { autoenable_created_entities: false };

        let publisher =
            participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();

        let res = publisher.is_enabled();
        assert!(res.is_ok());

        let writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let res = writer.is_enabled();
        assert!(res.is_err());

        // write() should fail when not enabled
        let res = writer
            .write(&HelloWorld { index: 0, message: "hello".to_string() }, InstanceHandle::NIL);
        assert!(res.is_err());

        // Manually enable the writer
        let res = writer.enable();
        assert!(res.is_ok());

        // Now write() should succeed
        let res_2 = writer
            .write(&HelloWorld { index: 0, message: "hello".to_string() }, InstanceHandle::NIL);
        assert!(res_2.is_ok());
    }

    use crate::dcps::topic::type_support::DdsType;

    #[derive(DdsType)]
    pub struct TestData {
        #[dds(key)]
        id: u32,
    }

    #[test]
    #[ignore]
    fn test_find_topic_existing() {
        // Create test participant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(11, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let _topic = participant
            .create_topic::<TestData>(
                "TestTopic",
                "TestDataType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Test find_topic (already existing topic)
        match participant.find_topic("TestTopic", Duration::zero()) {
            Ok(found_topic) => {
                assert_eq!(found_topic.get_name(), "TestTopic");
                println!("Test passed: Successfully found existing topic");
            }
            Err(code) => {
                panic!("Test failed: Could not find existing topic, error code: {:?}", code);
            }
        }
    }

    #[test]
    #[ignore]
    fn test_find_topic_timeout_zero() {
        // Create test participant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(11, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // Test find_topic (non-existent topic, timeout 0)
        match participant.find_topic("NonExistentTopic", Duration::zero()) {
            Ok(_) => {
                panic!("Test failed: Incorrectly reported finding non-existent topic");
            }
            Err(code) => {
                assert_eq!(code, DdsError::Timeout);
                println!("Test passed: Timeout occurred appropriately with timeout 0");
            }
        }
    }

    #[test]
    #[ignore]
    fn test_find_topic_with_timeout() {
        // Create test participant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(11, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // Start separate thread to create topic after delay
        let participant_clone = participant.clone();
        let create_thread = thread::Builder::new()
            .name("delayed_topic_creator".to_string())
            .spawn(move || {
                // Create topic after waiting 300ms
                thread::sleep(StdDuration::from_millis(300));
                let _delayed_topic = participant_clone
                    .create_topic::<TestData>(
                        "DelayedTopic",
                        "TestDataType",
                        TopicQos::default(),
                        None,
                        StatusMask::default(),
                    )
                    .unwrap();
                thread::sleep(StdDuration::from_millis(600));
            })
            .expect("Failed to spawn delayed_topic_creator thread");
        // Record start time
        let start = Instant::now();

        // Call find_topic (timeout: 500ms)
        match participant.find_topic("DelayedTopic", Duration::from_millis(500)) {
            Ok(found_topic) => {
                let elapsed = start.elapsed();
                assert_eq!(found_topic.get_name(), "DelayedTopic");
                println!("Test passed: Found delayed topic after {}ms", elapsed.as_millis());

                // Check if found after approximately 300ms (allow some margin)
                assert!(elapsed.as_millis() >= 290, "Found too quickly: {}ms", elapsed.as_millis());
            }
            Err(code) => {
                panic!("Test failed: Could not find topic, error code: {:?}", code);
            }
        }

        // Wait for creation thread to complete
        create_thread.join().expect("Failed to join delayed_topic_creator thread");
    }

    #[test]
    #[ignore]
    fn test_find_topic_timeout_occurs() {
        // Create test participant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(11, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // Record start time
        let start = Instant::now();

        // Call find_topic (timeout: 200ms, topic will not be created)
        match participant.find_topic("WillTimeoutTopic", Duration::from_millis(200)) {
            Ok(_) => {
                panic!("Test failed: Incorrectly reported finding non-created topic");
            }
            Err(code) => {
                let elapsed = start.elapsed();
                assert_eq!(code, DdsError::Timeout);
                println!(
                    "Test passed: Timeout occurred appropriately, elapsed time: {}ms",
                    elapsed.as_millis()
                );

                // Check if timeout occurred after approximately 200ms (allow some margin)
                assert!(
                    elapsed.as_millis() >= 190,
                    "Timeout occurred too quickly: {}ms",
                    elapsed.as_millis()
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn test_domain_participant_status_condition() {
        // Create DomainParticipant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(13, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        // 1. Check if status_condition is initialized
        assert!(
            participant.get_statuscondition().unwrap().get_entity().is_ok(),
            "StatusCondition's entity is not set"
        );

        // 2. Check if entity is set to the correct type
        if let Ok(entity) = participant.get_statuscondition().unwrap().get_entity() {
            let entity_guid = entity.get_instance_handle().unwrap();
            let participant_guid = participant.get_instance_handle().unwrap();

            assert_eq!(
                entity_guid, participant_guid,
                "Entity referenced by StatusCondition is not the original DomainParticipant"
            );
        } else {
            panic!("StatusCondition's entity is None");
        }
    }

    #[test]
    #[ignore]
    fn test_publisher_participant() {
        // Create DomainParticipant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(13, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        // 1. Check if publisher participant exists
        assert!(publisher.get_participant().is_ok(), "Publisher's domain_participant is not set");

        // 2. Original participant == entity == publisher's participant
        if let Ok(entity) = participant.get_statuscondition().unwrap().get_entity() {
            let entity_guid = entity.get_instance_handle().unwrap();
            let participant_guid = participant.get_instance_handle().unwrap();
            let pub_part_guid = publisher.get_participant().unwrap().get_instance_handle().unwrap();

            assert_eq!(
                entity_guid, participant_guid,
                "Entity referenced by StatusCondition is not the original DomainParticipant"
            );
            assert_eq!(
                pub_part_guid, participant_guid,
                "Publisher's DomainParticipant != original DomainParticipant"
            );
            assert_eq!(
                entity_guid, pub_part_guid,
                "Entity referenced by StatusCondition differs from Publisher's Domain Participant"
            );
        } else {
            panic!("StatusCondition's entity is None");
        }
    }

    #[test]
    #[ignore]
    fn test_publisher_eq_participant_publisher() {
        // Create DomainParticipant
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(13, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        assert!(publisher.get_participant().is_ok(), "Publisher's domain_participant is not set");

        let pub_participant = publisher.get_participant().unwrap();

        let pp_publisher = &pub_participant.get_publishers().unwrap()[0];

        assert_eq!(
            publisher.get_instance_handle().unwrap(),
            pp_publisher.get_instance_handle().unwrap(),
            "Publisher returned from DomainParticipant differs from the Publisher."
        );
    }

    #[test]
    #[ignore]
    fn test_status_condition_entity_lifecycle() {
        // Variables to store references
        let status_condition;
        let participant_guid;

        // Create and use participant within block
        {
            let factory = DomainParticipantFactory::get_instance();

            // Create participant
            let participant = factory
                .create_participant(
                    15,
                    DomainParticipantQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();
            participant_guid = participant.get_instance_handle().unwrap();

            // Store StatusCondition object
            status_condition = participant.get_statuscondition().unwrap().clone();

            // Verify reference is valid
            let entity_result = status_condition.get_entity();

            // Check if same entity using guid
            if let Ok(entity) = entity_result.as_ref() {
                let entity_guid = entity.get_instance_handle().unwrap();
                assert_eq!(
                    entity_guid, participant_guid,
                    "The guid of the entity referenced by StatusCondition differs from the original DomainParticipant"
                );
            } else {
                panic!("Entity reference Err");
            }

            factory.delete_participant(participant).unwrap();
        }

        // Check weak reference state after participant is dropped
        assert!(
            status_condition.get_entity().is_err(),
            "Entity reference should be Err after DomainParticipant is dropped"
        );
    }

    #[test]
    #[ignore]
    fn test_publisher_dp_lifecycle() {
        // Variables to store references
        let publisher;
        let participant_guid;

        // Create and use participant within block
        {
            let factory = DomainParticipantFactory::get_instance();

            // Create participant
            let participant = factory
                .create_participant(
                    15,
                    DomainParticipantQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();
            publisher = participant
                .create_publisher(PublisherQos::default(), None, StatusMask::default())
                .unwrap();

            participant_guid = participant.get_instance_handle().unwrap();

            // Verify reference is valid
            let pub_part = publisher.get_participant();

            let pub_part_guid = pub_part.as_ref().unwrap().get_instance_handle().unwrap();

            assert_eq!(
                pub_part_guid, participant_guid,
                "The guid of DomainParticipant referenced by Publisher differs from the original DomainParticipant"
            );

            participant.delete_publisher(publisher.clone()).unwrap();
            factory.delete_participant(participant).unwrap();
        }
        // Check weak reference state after participant is dropped
        assert!(
            publisher.get_participant().is_err(),
            "Weak reference should be None after DomainParticipant is dropped"
        );
    }

    #[test]
    fn test_contentfilteredtopic_lifecycle() {
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

        // Create a topic
        let topic = participant
            .create_topic::<HelloWorld>(
                "test_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Create ContentFilteredTopic
        let cft = participant
            .create_contentfilteredtopic::<HelloWorld>(
                "filtered_topic",
                &topic,
                "index > %0",
                vec!["10".to_string()],
            )
            .unwrap();

        // Verify ContentFilteredTopic is created
        assert_eq!(cft.get_name(), "filtered_topic");
        assert_eq!(cft.get_type_name(), "HelloWorld");
        assert_eq!(cft.get_filter_expression().unwrap(), "index > %0");

        // Topic should not be deletable while ContentFilteredTopic exists
        let delete_result = participant.delete_topic(topic.clone());
        assert!(
            delete_result.is_err(),
            "Topic should not be deletable while ContentFilteredTopic exists"
        );

        // Delete ContentFilteredTopic
        participant.delete_contentfilteredtopic(cft).unwrap();

        // Now Topic should be deletable
        assert!(
            participant.delete_topic(topic).is_ok(),
            "Topic should be deletable after ContentFilteredTopic is deleted"
        );

        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_contentfilteredtopic_drop_without_explicit_delete() {
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
            .create_topic::<HelloWorld>(
                "test_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        {
            // ContentFilteredTopic created in inner scope
            let _cft = participant
                .create_contentfilteredtopic::<HelloWorld>(
                    "filtered_topic",
                    &topic,
                    "index > %0",
                    vec!["10".to_string()],
                )
                .unwrap();

            // Topic should not be deletable
            assert!(
                participant.delete_topic(topic.clone()).is_err(),
                "Topic should not be deletable while ContentFilteredTopic exists"
            );
        } // ContentFilteredTopic dropped here without explicit delete

        // Topic should still not be deletable (ContentFilteredTopic was dropped but not explicitly deleted)
        assert!(
            participant.delete_topic(topic.clone()).is_err(),
            "Topic should not be deletable when ContentFilteredTopic was only dropped"
        );
    }

    #[test]
    fn test_topic_drop_without_explicit_delete() {
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

        {
            let _topic = participant
                .create_topic::<HelloWorld>(
                    "test_topic",
                    "HelloWorld",
                    TopicQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();
        } // Topic dropped here without explicit delete

        // Topic should still exist in participant
        let found_topic = participant.find_topic("test_topic", Duration::from_millis(0));
        assert!(found_topic.is_ok(), "Topic should still exist after being dropped");
    }

    #[test]
    fn test_multiple_contentfilteredtopics_on_same_topic() {
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
            .create_topic::<HelloWorld>(
                "test_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Create multiple ContentFilteredTopics on the same topic
        let cft1 = participant
            .create_contentfilteredtopic::<HelloWorld>(
                "filtered_topic_1",
                &topic,
                "index > %0",
                vec!["10".to_string()],
            )
            .unwrap();

        let cft2 = participant
            .create_contentfilteredtopic::<HelloWorld>(
                "filtered_topic_2",
                &topic,
                "index < %0",
                vec!["100".to_string()],
            )
            .unwrap();

        // Topic should not be deletable while any ContentFilteredTopic exists
        assert!(
            participant.delete_topic(topic.clone()).is_err(),
            "Topic should not be deletable while ContentFilteredTopics exist"
        );

        // Delete one ContentFilteredTopic
        participant.delete_contentfilteredtopic(cft1).unwrap();

        // Topic should still not be deletable
        assert!(
            participant.delete_topic(topic.clone()).is_err(),
            "Topic should not be deletable while one ContentFilteredTopic still exists"
        );

        // Delete the other ContentFilteredTopic
        participant.delete_contentfilteredtopic(cft2).unwrap();

        // Now Topic should be deletable
        assert!(
            participant.delete_topic(topic).is_ok(),
            "Topic should be deletable after all ContentFilteredTopics are deleted"
        );

        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_lookup_topicdescription() {
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

        // Create a Topic
        let topic = participant
            .create_topic::<HelloWorld>(
                "test_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Lookup Topic
        let found = participant.lookup_topicdescription("test_topic").unwrap();
        assert!(found.is_some(), "Should find the created Topic");
        assert_eq!(found.unwrap().get_name(), "test_topic");

        // Create ContentFilteredTopic
        let cft = participant
            .create_contentfilteredtopic::<HelloWorld>(
                "filtered_topic",
                &topic,
                "index > %0",
                vec!["10".to_string()],
            )
            .unwrap();

        // Lookup ContentFilteredTopic
        let found_cft = participant.lookup_topicdescription("filtered_topic").unwrap();
        assert!(found_cft.is_some(), "Should find the created ContentFilteredTopic");
        assert_eq!(found_cft.unwrap().get_name(), "filtered_topic");

        // Lookup non-existent
        let not_found = participant.lookup_topicdescription("non_existent");
        assert!(
            not_found.is_err() || not_found.unwrap().is_none(),
            "Should not find non-existent topic"
        );

        participant.delete_contentfilteredtopic(cft).unwrap();
        participant.delete_topic(topic).unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_datareader_with_contentfilteredtopic() {
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
            .create_topic::<HelloWorld>(
                "test_topic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let cft = participant
            .create_contentfilteredtopic::<HelloWorld>(
                "filtered_topic",
                &topic,
                "index > %0",
                vec!["10".to_string()],
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        // Create DataReader with ContentFilteredTopic
        let reader = subscriber
            .create_datareader::<HelloWorld>(
                &cft,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Verify get_topicdescription returns the ContentFilteredTopic
        let topic_desc = reader.get_topicdescription().unwrap();
        assert_eq!(topic_desc.get_name(), "filtered_topic");
        assert_eq!(topic_desc.get_type_name(), "HelloWorld");
        subscriber.delete_datareader(reader).unwrap();
        participant.delete_contentfilteredtopic(cft).unwrap();
        participant.delete_topic(topic).unwrap();
        participant.delete_subscriber(subscriber).unwrap();
        factory.delete_participant(participant).unwrap();
    }

    impl DomainParticipant {
        #[cfg(test)]
        pub fn get_topic_strong_count(&self, topic_name: &str) -> Option<usize> {
            let topics_by_name = self.find_topic_by_name(topic_name).unwrap();
            match topics_by_name {
                Some(topic) => Some(Arc::strong_count(&topic)),
                None => None,
            }
        }
    }

    #[test]
    #[ignore]
    fn test_topic_creation_and_deletion() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(16, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = participant
            .create_topic::<TestData>(
                "RefCountTestTopic",
                "RefCountTestType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Topic deletion succeeds
        assert!(participant.delete_topic(topic).is_ok());
        assert!(participant.get_topic_strong_count("RefCountTestTopic").is_none());
    }

    #[test]
    #[ignore]
    fn test_topic_deletion_with_partial_cleanup() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(16, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = participant
            .create_topic::<TestData>(
                "RefCountTestTopic",
                "RefCountTestType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader1 = subscriber
            .create_datareader::<TestData>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let reader2 = subscriber
            .create_datareader::<TestData>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer1 = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let writer2 = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Delete some only
        publisher.delete_datawriter(writer1).unwrap();
        publisher.delete_datawriter(writer2).unwrap();
        subscriber.delete_datareader(reader1).unwrap();

        // Deletion should fail since reader2 is still alive
        assert!(matches!(participant.delete_topic(topic), Err(DdsError::PreconditionNotMet)));

        // Delete last reader
        subscriber.delete_datareader(reader2).unwrap();
        // drop(reader2);
        let topic = participant.find_topic("RefCountTestTopic", Duration::infinite()).unwrap();

        // Deletion now succeeds
        assert!(participant.delete_topic(topic).is_ok());
    }

    #[test]
    #[ignore]
    fn test_nonexistent_topic_deletion() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(16, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let other_participant = factory
            .create_participant(17, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = other_participant
            .create_topic::<TestData>(
                "RefCountTestTopic",
                "RefCountTestType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Attempt to delete non-existent topic
        assert!(matches!(participant.delete_topic(topic), Err(_)));
    }

    #[test]
    #[ignore]
    fn test_delete_contained_entities_participant() {
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(18, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = participant
            .create_topic::<HelloWorld>(
                "HelloWorld",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let _writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let _con1 = reader
            .create_readcondition(
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();
        let _con2 = reader
            .create_readcondition(
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();

        assert!(!reader.get_readconditions().unwrap().is_empty());
        assert!(!subscriber.get_data_readers().unwrap().is_empty());
        assert!(!participant.get_subscribers().unwrap().is_empty());
        assert!(!participant.get_publishers().unwrap().is_empty());

        participant.delete_contained_entities().unwrap();

        assert!(subscriber.get_data_readers().is_err());
        assert!(participant.get_subscribers().unwrap().is_empty());
        assert!(participant.get_publishers().unwrap().is_empty());
    }

    #[test]
    fn test_get_participant_guid() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(15, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let instance_handle = participant.get_instance_handle().unwrap();
        println!("instance_handle: {:?}", instance_handle);
        assert_eq!(instance_handle.is_nil(), false);
    }

    #[test]
    fn test_delete_dcps_bridge() {
        let factory = DomainParticipantFactory::get_instance();

        // Create participant
        let participant = factory
            .create_participant(15, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let participant_clone = participant.clone();

        factory.delete_participant(participant_clone).unwrap();

        let dcps_bridge = participant.get_dcps_bridge().unwrap();
        assert!(dcps_bridge.is_none());
    }

    #[test]
    fn test_get_entity_guid() {
        let domain_id = unique_domain_id();
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "TestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let _writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let _writer2 = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let _reader = subscriber
            .create_datareader::<TestData>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let _reader2 = subscriber
            .create_datareader::<TestData>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        println!("domain_participant guid: {:?}", domain_participant.guid());
    }

    impl DomainParticipant {
        fn is_type_registered(&self, type_name: &str) -> bool {
            let types = self.types.read().unwrap();
            types.contains_key(type_name)
        }
    }

    #[test]
    fn test_type_unregistered_when_last_topic_deleted() {
        let domain_id = unique_domain_id();
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let type_name = "TestType";

        // Create only one topic
        let topic = domain_participant
            .create_topic::<TestData>(
                "TestTopic",
                type_name,
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Check if type is registered
        assert!(domain_participant.is_type_registered(type_name));

        // Delete topic
        domain_participant.delete_topic(topic).unwrap();

        // Check if type is unregistered
        assert!(!domain_participant.is_type_registered(type_name));
    }

    #[test]
    fn test_multiple_types_independent_cleanup() {
        let domain_id = unique_domain_id();
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let type_name = "TestMultipleType";

        let topic1 = domain_participant
            .create_topic::<TestData>(
                "TestTopic1",
                type_name,
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let _topic2 = domain_participant
            .create_topic::<TestData>(
                "TestTopic2",
                type_name,
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Delete topic1
        domain_participant.delete_topic(topic1).unwrap();

        // Should still be registered
        assert!(domain_participant.is_type_registered(type_name));
    }

    // ==================== Builtin Subscriber Tests (DDS 2.2.2.2.1.13) ====================

    /// Test that get_builtin_subscriber returns successfully
    #[test]
    fn test_get_builtin_subscriber_success() {
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

        // Should return the builtin subscriber without error
        let builtin_subscriber = participant.get_builtin_subscriber();
        assert!(builtin_subscriber.is_ok(), "get_builtin_subscriber should succeed");

        factory.delete_participant(participant).unwrap();
    }

    /// Test that builtin subscriber contains expected DataReaders
    #[test]
    fn test_builtin_subscriber_contains_datareaders() {
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

        let builtin_subscriber = participant.get_builtin_subscriber().unwrap();

        // Check that builtin DataReaders exist via lookup_datareader
        // DCPSParticipant
        let participant_reader =
            builtin_subscriber.lookup_datareader::<ParticipantBuiltinTopicData>("DCPSParticipant");
        assert!(participant_reader.is_ok(), "DCPSParticipant reader lookup should succeed");

        // DCPSPublication
        let publication_reader =
            builtin_subscriber.lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication");
        assert!(publication_reader.is_ok(), "DCPSPublication reader lookup should succeed");

        // DCPSSubscription
        let subscription_reader = builtin_subscriber
            .lookup_datareader::<SubscriptionBuiltinTopicData>("DCPSSubscription");
        assert!(subscription_reader.is_ok(), "DCPSSubscription reader lookup should succeed");

        factory.delete_participant(participant).unwrap();
    }

    /// Test that builtin subscriber has correct QoS settings per DDS 2.2.5
    #[test]
    fn test_builtin_subscriber_qos() {
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

        let builtin_subscriber = participant.get_builtin_subscriber().unwrap();
        let qos = builtin_subscriber.get_qos().unwrap();

        // ENTITY_FACTORY: autoenable_created_entities = TRUE
        assert!(
            qos.entity_factory.autoenable_created_entities,
            "Builtin subscriber should have autoenable_created_entities = true"
        );

        factory.delete_participant(participant).unwrap();
    }

    /// Test that builtin DataReaders have correct QoS settings per DDS 2.2.5
    #[test]
    fn test_builtin_datareader_qos() {
        use crate::infrastructure::qos_policy::{
            DestinationOrderQosPolicyKind, DurabilityQosPolicyKind, HistoryQosPolicyKind,
            OwnershipQosPolicyKind, ReliabilityQosPolicyKind,
        };

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

        let builtin_subscriber = participant.get_builtin_subscriber().unwrap();

        // Get DCPSSubscription reader and check its QoS
        let subscription_reader = builtin_subscriber
            .lookup_datareader::<SubscriptionBuiltinTopicData>("DCPSSubscription")
            .unwrap();

        let qos = subscription_reader.get_qos().unwrap();

        // DURABILITY: TRANSIENT_LOCAL
        assert_eq!(
            qos.durability.kind,
            DurabilityQosPolicyKind::TransientLocal,
            "Builtin reader should have TRANSIENT_LOCAL durability"
        );

        // DEADLINE: infinite
        assert!(qos.deadline.period.is_infinite(), "Builtin reader should have infinite deadline");

        // OWNERSHIP: SHARED
        assert_eq!(
            qos.ownership.kind,
            OwnershipQosPolicyKind::Shared,
            "Builtin reader should have SHARED ownership"
        );

        // RELIABILITY: RELIABLE
        assert_eq!(
            qos.reliability.kind,
            ReliabilityQosPolicyKind::Reliable,
            "Builtin reader should have RELIABLE reliability"
        );

        // DESTINATION_ORDER: BY_RECEPTION_TIMESTAMP
        assert_eq!(
            qos.destination_order.kind,
            DestinationOrderQosPolicyKind::ByReceptionTimestamp,
            "Builtin reader should have BY_RECEPTION_TIMESTAMP destination order"
        );

        // HISTORY: KEEP_LAST depth=1
        assert!(
            matches!(qos.history.kind, HistoryQosPolicyKind::KeepLast(1)),
            "Builtin reader should have KEEP_LAST(1) history"
        );

        // TIME_BASED_FILTER: minimum_separation = 0
        assert!(
            qos.time_based_filter.minimum_separation.is_zero(),
            "Builtin reader should have zero time_based_filter"
        );

        // RESOURCE_LIMITS: all LENGTH_UNLIMITED
        assert_eq!(
            qos.resource_limits.max_instances, LENGTH_UNLIMITED,
            "Builtin reader should have unlimited max_instances"
        );
        assert_eq!(
            qos.resource_limits.max_samples, LENGTH_UNLIMITED,
            "Builtin reader should have unlimited max_samples"
        );
        assert_eq!(
            qos.resource_limits.max_samples_per_instance, LENGTH_UNLIMITED,
            "Builtin reader should have unlimited max_samples_per_instance"
        );

        // READER_DATA_LIFECYCLE: autopurge delays = infinite
        assert!(
            qos.reader_data_lifecycle.autopurge_nowriter_samples_delay.is_infinite(),
            "Builtin reader should have infinite autopurge_nowriter_samples_delay"
        );
        assert!(
            qos.reader_data_lifecycle.autopurge_disposed_samples_delay.is_infinite(),
            "Builtin reader should have infinite autopurge_disposed_samples_delay"
        );

        factory.delete_participant(participant).unwrap();
    }

    /// Test that builtin subscriber cannot be deleted
    #[test]
    fn test_builtin_subscriber_cannot_be_deleted() {
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

        let builtin_subscriber = participant.get_builtin_subscriber().unwrap();

        // Attempting to delete builtin subscriber should fail
        let result = participant.delete_subscriber(builtin_subscriber);
        assert!(result.is_err(), "Deleting builtin subscriber should fail");

        factory.delete_participant(participant).unwrap();
    }

    /// Test that builtin subscriber QoS cannot be modified
    #[test]
    fn test_builtin_subscriber_qos_immutable() {
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

        let builtin_subscriber = participant.get_builtin_subscriber().unwrap();

        // Attempting to modify QoS should fail
        let mut new_qos = builtin_subscriber.get_qos().unwrap();
        new_qos.entity_factory.autoenable_created_entities = false;

        let result = builtin_subscriber.set_qos(new_qos);
        assert!(result.is_err(), "Modifying builtin subscriber QoS should fail");

        factory.delete_participant(participant).unwrap();
    }

    /// Test get_builtin_subscriber on deleted participant
    #[test]
    fn test_get_builtin_subscriber_on_deleted_participant() {
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

        let participant_clone = participant.clone();
        factory.delete_participant(participant).unwrap();

        // Should return error on deleted participant
        let result = participant_clone.get_builtin_subscriber();
        assert!(result.is_err(), "get_builtin_subscriber on deleted participant should fail");
    }
}
