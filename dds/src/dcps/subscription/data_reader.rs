//! DataReader - The interface for receiving data samples from a topic.
//!
//! A `DataReader<T>` is the primary interface for receiving data from publishers. It provides
//! type-safe access to data samples, with support for various reading modes, filtering, and
//! QoS policies.
//!
//! # Overview
//!
//! DataReaders are created by a `Subscriber` and associated with a specific `Topic` or
//! `ContentFilteredTopic`. Each DataReader receives data of a specific type (the generic
//! parameter `T`), which must implement the `DdsType` trait.
//!
//! # Reading vs Taking
//!
//! DataReader provides two primary methods for accessing data:
//!
//! - **`read()`**: samples remain in the cache and can be read again
//! - **`take()`**: samples are removed from the cache
//!
//! # Key Features
//!
//! - **Type-Safe Reception**: Generic over data type `T`
//! - **Flexible Reading**: Read or take with various state filters
//! - **Instance Management**: Track and query keyed data instances
//! - **Conditions**: Read, query, and status conditions for event-driven reading
//! - **Status Notifications**: Callbacks for data available, subscription matched, etc.

use std::{
    any::{Any, TypeId},
    cmp::Ordering as CmpOrdering,
    collections::{BTreeSet, HashMap},
    fmt::Debug,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
};

use super::{
    data_reader_listener::DataReaderListener,
    qos::DataReaderQos,
    query_condition::QueryCondition,
    read_condition::ReadCondition,
    sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    subscriber::Subscriber,
};
use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
    },
    infrastructure::{
        deadline_monitor::DeadlineMonitor,
        domain_entity::DomainEntity,
        entity::{
            impl_check_parent_enabled, impl_dds_entity, impl_dds_entity_impl, BaseEntity,
            EnableChild, Entity, EntityInternal, UpdateStatus,
        },
        history_cache::HistoryCache as DcpsHistoryCache,
        qos_policy::{DestinationOrderQosPolicyKind, HistoryQosPolicyKind, Qos},
        status::{
            LivelinessChangedStatus, RequestedDeadlineMissedStatus, RequestedIncompatibleQosStatus,
            SampleLostStatus, SampleRejectedStatus, StatusInfo, StatusKind, StatusMask,
            SubscriptionMatchedStatus,
        },
        status_condition::StatusCondition,
    },
    rtps::{
        builtin::data::content_filtered_topic::ContentFilterProperty,
        common::{guid::Guid, sequence::SequenceNumber, types::ChangeKind},
        entities::{
            history::{cache_change::CacheChange, history_cache::HistoryCache as _},
            reader::Reader as RtpsReader,
        },
    },
    subscription::{
        data_reader_history::DataReaderHistoryCache,
        data_sample::DataSample,
        read_condition::ReadConditionTrait,
        sample_info::{InstanceInfo, SampleInfo, StateMaskExt},
    },
    topic::{
        content_filtered_topic::ContentFilteredTopic,
        topic::Topic,
        topic_description::TopicDescription,
        type_support::{DdsType, TypeSupport},
    },
};

// Pub/Sub must contain multiple types of DataWriter/Reader<Foo>,
// so we use trait objects for runtime polymorphism instead of generics
pub trait DataReaderBase: DomainEntity + Send + Any {
    fn get_liveliness_changed_status(&self) -> DdsResult<LivelinessChangedStatus>;
    fn get_sample_rejected_status(&self) -> DdsResult<SampleRejectedStatus>;
    fn get_sample_lost_status(&self) -> DdsResult<SampleLostStatus>;
    fn get_requested_deadline_missed_status(&self) -> DdsResult<RequestedDeadlineMissedStatus>;
    fn get_requested_incompatible_qos_status(&self) -> DdsResult<RequestedIncompatibleQosStatus>;
    fn get_subscription_matched_status(&self) -> DdsResult<SubscriptionMatchedStatus>;
    fn get_matched_publication_data(
        &self,
        publication_handle: InstanceHandle,
    ) -> DdsResult<PublicationBuiltinTopicData>;
    fn create_readcondition(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<ReadCondition>;
    fn create_querycondition(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        query_expression: &str,
        query_parameters: Vec<String>,
    ) -> DdsResult<QueryCondition>;
    fn get_matched_publications(&self) -> DdsResult<Vec<InstanceHandle>>;
    fn wait_for_historical_data(&self, max_wait: Duration) -> DdsResult<()>;
    fn get_topicdescription(&self) -> DdsResult<Arc<dyn TopicDescription>>;
    fn get_subscriber(&self) -> DdsResult<Subscriber>;
    fn delete_contained_entities(&self) -> DdsResult<()>;
}

pub(crate) trait DataReaderInternal: DataReaderBase {
    fn disable(&self) -> DdsResult<()>;
    fn clone_boxed(&self) -> Box<dyn DataReaderBase<Qos = DataReaderQos> + Send>;
    fn as_any(&self) -> &dyn Any;
    fn notify_data_available(&self);
    fn get_type_id(&self) -> TypeId;
    fn delete(&self);
    fn is_deleted(&self) -> DdsResult<()>;
    fn get_topic(&self) -> DdsResult<Topic>;
    fn get_readconditions(&self) -> DdsResult<Vec<Arc<dyn ReadConditionTrait + Send + Sync>>>;
    fn delete_readcondition_internal(
        &self,
        condition: Arc<dyn ReadConditionTrait + Send + Sync>,
    ) -> DdsResult<()>;
    fn owns_read_condition(
        &self,
        condition: &Arc<dyn ReadConditionTrait + Send + Sync>,
    ) -> DdsResult<()>;
}

// #[derive(Clone)]
pub struct DataReader<Foo> {
    guid: Guid,
    qos: Arc<Mutex<DataReaderQos>>,
    listener: Arc<RwLock<Option<Arc<dyn DataReaderListener<Foo = Foo>>>>>,
    mask: Arc<RwLock<StatusMask>>,
    pub status_condition: Arc<Mutex<StatusCondition<DataReaderQos>>>,
    read_conditions: Arc<Mutex<Vec<Arc<dyn ReadConditionTrait + Send + Sync>>>>,
    pub(crate) self_ref: Arc<Mutex<Option<Arc<DataReader<Foo>>>>>,
    enabled: Arc<AtomicBool>,
    deleted: Arc<AtomicBool>,
    instance_infos: Arc<Mutex<HashMap<InstanceHandle, InstanceInfo>>>,
    read_samples: Arc<Mutex<HashMap<Guid, BTreeSet<SequenceNumber>>>>,
    topic: Option<Weak<Topic>>,
    content_filtered_topic: Option<Weak<ContentFilteredTopic>>,
    type_support: Arc<dyn TypeSupport + Send>,
    subscriber: Option<Weak<Subscriber>>,
    rtps_reader: Arc<Mutex<Option<Weak<dyn RtpsReader + Send + Sync>>>>,
    liveliness_changed_status: Arc<Mutex<LivelinessChangedStatus>>,
    sample_rejected_status: Arc<Mutex<SampleRejectedStatus>>,
    requested_deadline_missed_status: Arc<Mutex<RequestedDeadlineMissedStatus>>,
    requested_incompatible_qos_status: Arc<Mutex<RequestedIncompatibleQosStatus>>,
    subscription_matched_status: Arc<Mutex<SubscriptionMatchedStatus>>,
    sample_lost_status: Arc<Mutex<SampleLostStatus>>,
    deadline_monitor: Arc<Mutex<Option<DeadlineMonitor>>>,
    change_callback: Option<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>>,
    status_callback: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
    _phantom: PhantomData<fn() -> Foo>, // Temporary
    datareader_cache: Arc<Mutex<DataReaderHistoryCache<Foo>>>,
}

impl<Foo> Debug for DataReader<Foo> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataReader")
            .field("guid", &self.guid)
            .field("qos", &self.qos.lock().unwrap())
            .field(
                "listener",
                &self.listener.read().unwrap().as_ref().map(|_| "Arc<dyn DataReaderListener>"),
            )
            .field("mask", &self.mask.read().unwrap())
            .field("status_condition", &self.status_condition.lock().unwrap())
            .field("self_ref", &self.self_ref.lock().unwrap().as_ref().map(|_| "Arc<DataReader>"))
            .field("enabled", &self.enabled.load(std::sync::atomic::Ordering::Acquire))
            .field("deleted", &self.deleted.load(std::sync::atomic::Ordering::Acquire))
            .field("topic", &self.topic.as_ref().map(|_| "Weak<Topic>"))
            .field("type_support", &"Arc<dyn TypeSupport>")
            .field("publisher", &self.subscriber.as_ref().map(|_| "Weak<Subscriber>"))
            .field("rtps_reader", &"Weak<dyn RtpsReader>")
            .field("instance_infos", &self.instance_infos.lock().unwrap())
            .field("read_samples", &self.read_samples.lock().unwrap())
            .field("liveliness_changed_status", &self.liveliness_changed_status.lock().unwrap())
            .field(
                "requested_deadline_missed_status",
                &self.requested_deadline_missed_status.lock().unwrap(),
            )
            .field(
                "requested_incompatible_qos_status",
                &self.requested_incompatible_qos_status.lock().unwrap(),
            )
            .field("subscription_matched_status", &self.subscription_matched_status.lock().unwrap())
            .field("sample_lost_status", &self.sample_lost_status.lock().unwrap())
            .field("_phantom", &self._phantom)
            .finish()
    }
}

impl<Foo: 'static + Clone + Debug> Clone for DataReader<Foo> {
    fn clone(&self) -> Self {
        Self {
            guid: self.guid,
            qos: self.qos.clone(),
            listener: self.listener.clone(),
            mask: self.mask.clone(),
            status_condition: self.status_condition.clone(),
            read_conditions: self.read_conditions.clone(),
            self_ref: self.self_ref.clone(),
            enabled: self.enabled.clone(),
            deleted: self.deleted.clone(),
            instance_infos: self.instance_infos.clone(),
            read_samples: self.read_samples.clone(),
            topic: self.topic.clone(),
            content_filtered_topic: self.content_filtered_topic.clone(),
            type_support: self.type_support.clone(),
            subscriber: self.subscriber.clone(),
            rtps_reader: self.rtps_reader.clone(),
            liveliness_changed_status: self.liveliness_changed_status.clone(),
            sample_rejected_status: self.sample_rejected_status.clone(),
            sample_lost_status: self.sample_lost_status.clone(),
            requested_deadline_missed_status: self.requested_deadline_missed_status.clone(),
            requested_incompatible_qos_status: self.requested_incompatible_qos_status.clone(),
            subscription_matched_status: self.subscription_matched_status.clone(),
            deadline_monitor: self.deadline_monitor.clone(),
            change_callback: self.change_callback.clone(),
            status_callback: self.status_callback.clone(),
            _phantom: self._phantom,
            datareader_cache: self.datareader_cache.clone(),
        }
    }
}

// When the user goes out of scope and auto-drops without calling delete_datareader,
// it should not be deleted from Subscriber.
impl<Foo> Drop for DataReader<Foo> {
    fn drop(&mut self) {
        // Only handle drop for the last reference (not clones)
        if let Some(guard) = self.self_ref.lock().ok() {
            if let Some(ref self_arc) = guard.as_ref() {
                if Arc::strong_count(self_arc) > 1 {
                    return;
                }
            }
        } else {
            return; // Never fully initialized
        }

        if !self.deleted.load(Ordering::SeqCst) {
            if let Some(ref subscriber_weak) = self.subscriber {
                if let Some(subscriber) = subscriber_weak.upgrade() {
                    if let Some(ref topic_weak) = self.topic {
                        if let Some(topic) = topic_weak.upgrade() {
                            match topic.get_instance_handle() {
                                Ok(topic_handle) => {
                                    let reader_handle = InstanceHandle::from_guid(&self.guid);
                                    subscriber.handle_reader_drop(
                                        topic.get_name(),
                                        &topic_handle,
                                        &reader_handle,
                                    );
                                }
                                Err(e) => println!("Failed to get topic handle: {:?}", e),
                            }
                        }
                    }
                }
            }
        }
    }
}

impl_dds_entity!(DataReader<Foo>, DataReaderQos, Foo: 'static + Clone + Debug);
impl<Foo: 'static + Clone + Debug> DomainEntity for DataReader<Foo> {}
impl<Foo: 'static + Clone + Debug> EnableChild for DataReader<Foo> {
    fn enable_rtps_entities(&self) -> DdsResult<()> {
        let topic_description = self.get_topicdescription()?;
        let cft = topic_description.as_any().downcast_ref::<ContentFilteredTopic>();
        let subscriber = self.get_subscriber()?;
        let participant = subscriber.get_participant()?;

        let content_filter_property = if let Some(content_filtered_topic) = cft {
            let related_topic = content_filtered_topic.get_related_topic()?;
            Some(ContentFilterProperty {
                content_filtered_topic_name: content_filtered_topic.get_name().to_owned(),
                related_topic_name: related_topic.get_name().to_owned(),
                filter_class_name: "DDSSQL".to_string(),
                filter_expression: content_filtered_topic.get_filter_expression()?,
                expression_parameters: content_filtered_topic.get_expression_parameters()?,
            })
        } else {
            None
        };

        // Downcast to get underlying Topic for QoS
        let topic_qos = if let Some(topic) = topic_description.as_any().downcast_ref::<Topic>() {
            topic.get_qos()?
        } else if let Some(content_filtered_topic) = cft {
            content_filtered_topic.get_related_topic()?.get_qos()?
        } else {
            return Err(DdsError::Error("Unsupported TopicDescription type".to_string()));
        };

        let topic_name = if let Some(topic) = topic_description.as_any().downcast_ref::<Topic>() {
            topic.get_name().to_string()
        } else if let Some(content_filtered_topic) = cft {
            content_filtered_topic.get_related_topic()?.get_name().to_string()
        } else {
            return Err(DdsError::Error("Unsupported TopicDescription type".to_string()));
        };

        let mut subscription_builtin_topic_data =
            SubscriptionBuiltinTopicData::new(&self.get_qos()?, &subscriber.get_qos()?, &topic_qos);
        subscription_builtin_topic_data.set_topic_name(topic_name);
        subscription_builtin_topic_data
            .set_type_name(topic_description.get_type_name().to_string());
        subscription_builtin_topic_data.set_endpoint_guid(self.guid);

        let status_callback = self
            .status_callback
            .as_ref()
            .ok_or(DdsError::Error("Status callback is not initialized".to_string()))?
            .clone();
        let change_callback = self
            .change_callback
            .as_ref()
            .ok_or(DdsError::Error("Change callback is not initialized".to_string()))?
            .clone();

        // Use Weak to avoid lifetime issues in closure
        let mut dcps_bridge = participant.get_dcps_bridge()?;
        let rtps_reader = match dcps_bridge.as_mut() {
            Some(dcps_bridge) => dcps_bridge
                .create_rtps_reader(
                    subscription_builtin_topic_data,
                    content_filter_property,
                    Some(change_callback),
                    Some(status_callback),
                )
                .map_err(|e| DdsError::Error(e.message))?,
            None => return Err(DdsError::Error("DCPS Bridge is not initialized".to_string())),
        };

        drop(dcps_bridge);

        {
            let reader_cache = rtps_reader.reader_cache();
            reader_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?.set_datareader_cache(
                Arc::downgrade(&self.datareader_cache)
                    as Weak<
                        Mutex<
                            dyn DcpsHistoryCache<CacheChangeInputType = Arc<Mutex<CacheChange>>>
                                + Send
                                + Sync,
                        >,
                    >,
            );
        }

        {
            *self.rtps_reader.lock().map_err(|e| DdsError::Error(e.to_string()))? =
                Some(Arc::downgrade(&rtps_reader));
        }

        Ok(())
    }
    fn update_rtps_entity(&self, qos: &Self::Qos) -> DdsResult<()> {
        let subscriber = self.get_subscriber()?;
        let topic = self.get_topic()?;
        let mut subscription_builtin_topic_data =
            SubscriptionBuiltinTopicData::new(qos, &subscriber.get_qos()?, &topic.get_qos()?);
        subscription_builtin_topic_data.set_topic_name(topic.get_name().to_string());
        subscription_builtin_topic_data.set_type_name(topic.get_type_name().to_string());
        subscription_builtin_topic_data.set_endpoint_guid(self.guid);
        let rtps_reader = self.get_rtps_reader()?;
        rtps_reader
            .set_subscription_builtin_topic_data(subscription_builtin_topic_data.clone())
            .map_err(|e| DdsError::Error(e.message))?;

        let topic_description = self.get_topicdescription()?;
        let cft = topic_description.as_any().downcast_ref::<ContentFilteredTopic>();
        let content_filter_property = if let Some(content_filtered_topic) = cft {
            let related_topic = content_filtered_topic.get_related_topic()?;
            Some(ContentFilterProperty {
                content_filtered_topic_name: content_filtered_topic.get_name().to_owned(),
                related_topic_name: related_topic.get_name().to_owned(),
                filter_class_name: "DDSSQL".to_string(),
                filter_expression: content_filtered_topic.get_filter_expression()?,
                expression_parameters: content_filtered_topic.get_expression_parameters()?,
            })
        } else {
            None
        };

        let participant = subscriber.get_participant()?;
        let dcps_bridge = participant.get_dcps_bridge()?;
        let dcps_bridge = dcps_bridge
            .as_ref()
            .ok_or(DdsError::Error("DCPS Bridge is not initialized".to_string()))?;
        dcps_bridge
            .update_reader(&rtps_reader, subscription_builtin_topic_data, content_filter_property)
            .map_err(|e| DdsError::Error(e.message))?;

        Ok(())
    }

    impl_check_parent_enabled!(get_subscriber);
}
impl<Foo: 'static + Clone + Debug> UpdateStatus for DataReader<Foo> {
    fn update_status(
        &self,
        status: StatusKind,
        info: Option<Arc<dyn StatusInfo>>,
    ) -> DdsResult<()> {
        match status {
            StatusKind::INCONSISTENT_TOPIC => self.get_topic()?.update_status(status, info),
            StatusKind::REQUESTED_DEADLINE_MISSED => {
                let info = Arc::downcast::<RequestedDeadlineMissedStatus>(
                    info.ok_or(DdsError::BadParameter)?,
                )
                .map_err(|_| DdsError::BadParameter)?;
                self.handle_requested_deadline_missed_status(info)
            }
            StatusKind::REQUESTED_INCOMPATIBLE_QOS => {
                let info = Arc::downcast::<RequestedIncompatibleQosStatus>(
                    info.ok_or(DdsError::BadParameter)?,
                )
                .map_err(|_| DdsError::BadParameter)?;
                self.handle_requested_incompatible_qos_status(info)
            }
            StatusKind::SAMPLE_LOST => {
                if info.is_some() {
                    return Err(DdsError::BadParameter);
                }
                self.handle_sample_lost_status()
            }
            StatusKind::SAMPLE_REJECTED => {
                let info =
                    Arc::downcast::<SampleRejectedStatus>(info.ok_or(DdsError::BadParameter)?)
                        .map_err(|_| DdsError::BadParameter)?;
                self.handle_sample_rejected_status(info)
            }
            StatusKind::DATA_AVAILABLE => {
                if info.is_some() {
                    return Err(DdsError::BadParameter);
                }
                self.handle_data_available_status()
            }
            StatusKind::LIVELINESS_CHANGED => {
                let info =
                    Arc::downcast::<LivelinessChangedStatus>(info.ok_or(DdsError::BadParameter)?)
                        .map_err(|_| DdsError::BadParameter)?;

                if info.alive_count_change() == -1 && info.not_alive_count_change() == 1 {
                    if let Ok(datareader_cache) = self.datareader_cache.lock() {
                        datareader_cache.remove_writer_from_owner_candidates(
                            info.last_publication_handle().to_guid(),
                        )?;
                    }
                }

                self.handle_liveliness_changed_status(info)
            }
            StatusKind::SUBSCRIPTION_MATCHED => {
                let info =
                    Arc::downcast::<SubscriptionMatchedStatus>(info.ok_or(DdsError::BadParameter)?)
                        .map_err(|_| DdsError::BadParameter)?;
                self.handle_subscription_matched_status(info)
            }
            status => {
                log::warn!("Unknown status received for DataReader - StatusKind: {:?}", status);
                Err(DdsError::BadParameter)
            }
        }
    }
}

impl<Foo: 'static + Clone + Debug> DataReader<Foo> {
    /// Retrieves the key values for a data instance identified by its handle.
    ///
    /// This operation returns a data instance with only the key fields populated. The non-key
    /// fields will have their default values. This is useful when you have an instance handle
    /// (e.g., from sample info) and need to determine which instance it represents.
    ///
    /// # Arguments
    ///
    /// * `handle` - The instance handle to look up. Must not be `InstanceHandle::NIL`.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Foo)` with the key fields populated from the specified instance, or a `DdsError`
    /// if the handle is invalid or the instance is not known.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `BadParameter` - The handle is `InstanceHandle::NIL` or doesn't correspond to a known instance
    /// * The reader is not enabled
    /// * The reader has been deleted
    /// * Type deserialization fails
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use int2dds::domain::domain_participant_factory::DomainParticipantFactory;
    /// # use int2dds::topic::type_support::DdsType;
    /// # use int2dds::subscription::sample_info::SampleStateKind;
    /// # use int2dds::subscription::sample_info::ViewStateKind;
    /// # use int2dds::subscription::sample_info::InstanceStateKind;
    /// # use int2dds::core::types::LENGTH_UNLIMITED;
    /// # #[derive(DdsType)]
    /// # struct MyData { #[dds(key)] id: u32, message: String }
    /// # let factory = DomainParticipantFactory::get_instance();
    /// # let participant = factory.create_participant(0, Default::default(), None, Default::default()).unwrap();
    /// # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", Default::default(), None, Default::default()).unwrap();
    /// # let subscriber = participant.create_subscriber(Default::default(), None, Default::default()).unwrap();
    /// # let reader = subscriber.create_datareader::<MyData>(&topic, Default::default(), None, Default::default()).unwrap();
    /// // Take some samples
    /// let samples = reader.take(
    ///     LENGTH_UNLIMITED,
    ///     &[SampleStateKind::ANY_SAMPLE_STATE],
    ///     &[ViewStateKind::ANY_VIEW_STATE],
    ///     &[InstanceStateKind::ANY_INSTANCE_STATE]
    /// ).unwrap();
    ///
    /// for sample in samples {
    ///     let handle = sample.sample_info().instance_handle;
    ///
    ///     // Get the key values for this instance
    ///     let key_holder = reader.get_key_value(handle).unwrap();
    ///     println!("Instance key: {:?}", key_holder.id);
    /// }
    /// ```
    pub fn get_key_value(&self, handle: InstanceHandle) -> DdsResult<Foo> {
        // in: key_holder: <Foo>, handle: InstanceHandle
        // out: DdsError_t, key_holder: <Foo>
        self.is_enabled()?;

        if handle.is_nil() {
            return Err(DdsError::BadParameter);
        }

        let instance_info = self.get_instance_infos()?;

        if let Some(info) = instance_info.get(&handle) {
            let key = info.key.clone();
            if key.is_empty() {
                return Err(DdsError::Error("Unknow Key".to_string()));
            }
            let key = self.type_support.deserialize_key(&key)?;
            let result = key
                .downcast::<Foo>()
                .map(|boxed| *boxed)
                .map_err(|_| DdsError::Error("Type downcast failed".to_string()))?;
            Ok(result)
        } else {
            Err(DdsError::BadParameter)
        }
    }

    /// Looks up the instance handle corresponding to a data instance.
    ///
    /// This operation takes an instance and returns the handle that can be used in subsequent
    /// operations that accept an instance handle as a parameter. The instance parameter is used
    /// only to examine the key fields that identify the instance.
    ///
    /// This operation does NOT register the instance. If the instance has not been previously
    /// registered, or if the service cannot provide a handle for any other reason, this operation
    /// returns `InstanceHandle::NIL`.
    ///
    /// # Arguments
    ///
    /// * `instance` - The data instance whose key fields identify the instance to look up
    ///
    /// # Returns
    ///
    /// Returns `Ok(InstanceHandle)` if the instance is known to the reader, or `InstanceHandle::NIL`
    /// if the instance has not been seen.
    ///
    /// # Errors
    ///
    /// Returns an error if the reader has been deleted.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use int2dds::domain::domain_participant_factory::DomainParticipantFactory;
    /// # use int2dds::topic::type_support::DdsType;
    /// # use int2dds::common::instance_handle::InstanceHandle;
    /// # #[derive(DdsType)]
    /// # struct MyData { #[dds(key)] id: u32, message: String }
    /// # let factory = DomainParticipantFactory::get_instance();
    /// # let participant = factory.create_participant(0, Default::default(), None, Default::default()).unwrap();
    /// # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", Default::default(), None, Default::default()).unwrap();
    /// # let subscriber = participant.create_subscriber(Default::default(), None, Default::default()).unwrap();
    /// # let reader = subscriber.create_datareader::<MyData>(&topic, Default::default(), None, Default::default()).unwrap();
    /// let instance = MyData { id: 1, message: String::new() };
    ///
    /// // Look up the handle for this instance
    /// let handle = reader.lookup_instance(&instance).unwrap();
    ///
    /// if handle == InstanceHandle::NIL {
    ///     println!("Instance not known to this reader");
    /// } else {
    ///     println!("Found handle for instance: {:?}", handle);
    /// }
    /// ```
    pub fn lookup_instance(&self, instance: &Foo) -> DdsResult<InstanceHandle> {
        /*
            This operation takes an instance as an argument and returns a handle that can be used
            for operations that take an instance handle as an argument.
            The instance passed as an argument is used only to examine the fields that define the key.
            This operation does not register the instance.
            If the instance has not been previously registered, or for any other reason the service
            cannot provide an instance handle, this operation returns the special value HANDLE_NIL.
        */
        self.is_deleted()?;
        let handle = self.type_support.compute_key(instance as &dyn Any);

        {
            let instances =
                self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            Ok(match instances.get(&handle) {
                Some(_) => handle,
                None => InstanceHandle::NIL, // Unregistered instance
            })
        }
    }

    // For Entity
    pub fn set_listener(
        &self,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
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
    pub fn get_listener(&self) -> DdsResult<Option<Arc<dyn DataReaderListener<Foo = Foo>>>> {
        self.is_deleted()?;
        match self.listener.read() {
            Ok(guard) => Ok(guard.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    #[inline]
    pub fn get_liveliness_changed_status(&self) -> DdsResult<LivelinessChangedStatus> {
        <Self as DataReaderBase>::get_liveliness_changed_status(self)
    }

    #[inline]
    pub fn get_sample_rejected_status(&self) -> DdsResult<SampleRejectedStatus> {
        <Self as DataReaderBase>::get_sample_rejected_status(self)
    }

    #[inline]
    pub fn get_sample_lost_status(&self) -> DdsResult<SampleLostStatus> {
        <Self as DataReaderBase>::get_sample_lost_status(self)
    }

    #[inline]
    pub fn get_requested_deadline_missed_status(&self) -> DdsResult<RequestedDeadlineMissedStatus> {
        <Self as DataReaderBase>::get_requested_deadline_missed_status(self)
    }

    #[inline]
    pub fn get_requested_incompatible_qos_status(
        &self,
    ) -> DdsResult<RequestedIncompatibleQosStatus> {
        <Self as DataReaderBase>::get_requested_incompatible_qos_status(self)
    }

    #[inline]
    pub fn get_subscription_matched_status(&self) -> DdsResult<SubscriptionMatchedStatus> {
        <Self as DataReaderBase>::get_subscription_matched_status(self)
    }

    #[inline]
    pub fn get_matched_publication_data(
        &self,
        publication_handle: InstanceHandle,
    ) -> DdsResult<PublicationBuiltinTopicData> {
        <Self as DataReaderBase>::get_matched_publication_data(self, publication_handle)
    }

    #[inline]
    pub fn create_readcondition(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<ReadCondition> {
        <Self as DataReaderBase>::create_readcondition(
            self,
            sample_states,
            view_states,
            instance_states,
        )
    }

    #[inline]
    pub fn create_querycondition(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        query_expression: &str,
        query_parameters: Vec<String>,
    ) -> DdsResult<QueryCondition> {
        <Self as DataReaderBase>::create_querycondition(
            self,
            sample_states,
            view_states,
            instance_states,
            query_expression,
            query_parameters,
        )
    }

    #[inline]
    pub fn get_matched_publications(&self) -> DdsResult<Vec<InstanceHandle>> {
        <Self as DataReaderBase>::get_matched_publications(self)
    }

    #[inline]
    pub fn wait_for_historical_data(&self, max_wait: Duration) -> DdsResult<()> {
        <Self as DataReaderBase>::wait_for_historical_data(self, max_wait)
    }

    #[inline]
    pub fn get_topicdescription(&self) -> DdsResult<Arc<dyn TopicDescription>> {
        <Self as DataReaderBase>::get_topicdescription(self)
    }

    #[inline]
    pub fn get_subscriber(&self) -> DdsResult<Subscriber> {
        <Self as DataReaderBase>::get_subscriber(self)
    }

    #[inline]
    pub fn delete_contained_entities(&self) -> DdsResult<()> {
        <Self as DataReaderBase>::delete_contained_entities(self)
    }

    pub(crate) fn is_enabled(&self) -> DdsResult<()> {
        self.is_deleted()?;
        if self.enabled.load(Ordering::SeqCst) {
            {
                let _ = self.get_rtps_reader()?;
            }
            Ok(())
        } else {
            Err(DdsError::NotEnabled)
        }
    }

    pub(crate) fn get_instance_infos(&self) -> DdsResult<HashMap<InstanceHandle, InstanceInfo>> {
        match self.instance_infos.lock() {
            Ok(instance_infos) => Ok(instance_infos.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn is_sample_read(&self, writer_guid: &Guid, seq_num: &SequenceNumber) -> DdsResult<bool> {
        let read_samples = self.read_samples.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        if let Some(writer_samples) = read_samples.get(writer_guid) {
            Ok(writer_samples.contains(seq_num))
        } else {
            Ok(false)
        }
    }

    fn get_sample_state(
        &self,
        writer_guid: &Guid,
        seq_num: &SequenceNumber,
    ) -> DdsResult<SampleStateKind> {
        if let Ok(true) = self.is_sample_read(writer_guid, seq_num) {
            Ok(SampleStateKind::READ_SAMPLE_STATE)
        } else {
            // When is_sample_read is false
            if (self
                .get_change(*seq_num, *writer_guid)
                .map_err(|e| DdsError::Error(e.to_string()))?)
            .is_some()
            {
                Ok(SampleStateKind::NOT_READ_SAMPLE_STATE)
            } else {
                Ok(SampleStateKind::READ_SAMPLE_STATE) // If not in cache, already taken = treat as Read
            }
        }
    }

    fn mark_sample_as_read(&self, writer_guid: &Guid, seq_num: SequenceNumber) -> DdsResult<()> {
        let mut read_samples =
            self.read_samples.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let writer_samples = read_samples.entry(*writer_guid).or_insert_with(BTreeSet::new);

        // Check if already exists, then insert
        writer_samples.insert(seq_num);

        // QoS-based size limit
        let max_samples = self.get_max_samples()?;

        if writer_samples.len() > max_samples {
            self.cleanup_old_read_samples(writer_samples, max_samples)?;
        }

        Ok(())
    }

    fn get_max_samples(&self) -> DdsResult<usize> {
        let qos = self.get_qos()?;

        let samples_limit = match qos.history.kind {
            HistoryQosPolicyKind::KeepLast(depth) => depth,
            HistoryQosPolicyKind::KeepAll => {
                if qos.resource_limits.max_samples_per_instance == -1 {
                    i32::MAX
                } else {
                    qos.resource_limits.max_samples_per_instance
                }
            }
        };

        let max_instances = if qos.resource_limits.max_instances == -1 {
            i32::MAX
        } else {
            qos.resource_limits.max_instances
        };

        let max_samples = i32::min(
            qos.resource_limits.max_samples,
            max_instances.saturating_mul(samples_limit), // Prevent overflow
        );

        Ok(max_samples as usize)
    }

    fn cleanup_old_read_samples(
        &self,
        writer_samples: &mut BTreeSet<SequenceNumber>,
        max_samples: usize,
    ) -> DdsResult<()> {
        if writer_samples.len() >= max_samples {
            if let Some(&oldest_seq_num) = writer_samples.iter().next() {
                writer_samples.remove(&oldest_seq_num);
                log::debug!(
                    "Removed oldest read sample with sequence number: {:?}",
                    oldest_seq_num
                );
            }
        }

        Ok(())
    }

    fn mark_instance_as_viewed(&self, handle: InstanceHandle) {
        if let Ok(mut instance_infos) = self.instance_infos.lock() {
            if let Some(info) = instance_infos.get_mut(&handle) {
                // If instance was NEW, mark it as NOT_NEW after being viewed
                if info.view_state == ViewStateKind::NEW_VIEW_STATE {
                    info.view_state = ViewStateKind::NOT_NEW_VIEW_STATE;
                }
            }
        }
    }

    pub(crate) fn get_rtps_reader(&self) -> DdsResult<Arc<dyn RtpsReader + Send + Sync>> {
        let rtps_reader = self
            .rtps_reader
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .as_ref()
            .ok_or(DdsError::Error("RTPS Reader is not initialized".to_string()))?
            .upgrade()
            .ok_or(DdsError::Error("RTPS Reader is not initialized".to_string()))?;
        Ok(rtps_reader)
    }

    pub(crate) fn create_status_callback(
        &self,
    ) -> DdsResult<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>> {
        let self_ref = self.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let self_ref = self_ref
            .as_ref()
            .ok_or(DdsError::Error("DataReader is not properly initialized".to_string()))?;
        let weak_self = Arc::downgrade(self_ref);
        Ok(Arc::new(move |status, info| {
            if let Some(entity) = weak_self.upgrade() {
                let _ = entity.update_status(status, info);
            }
        }))
    }

    fn take_liveliness_changed_status(&self) -> DdsResult<LivelinessChangedStatus> {
        let mut status_guard =
            self.liveliness_changed_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset count_change
        status_guard.alive_count_change = 0;
        status_guard.not_alive_count_change = 0;

        Ok(result)
    }
    fn take_sample_rejected_status(&self) -> DdsResult<SampleRejectedStatus> {
        let mut status_guard =
            self.sample_rejected_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset total_count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_sample_lost_status(&self) -> DdsResult<SampleLostStatus> {
        let mut status_guard =
            self.sample_lost_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset total_count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_requested_deadline_missed_status(&self) -> DdsResult<RequestedDeadlineMissedStatus> {
        let mut status_guard = self
            .requested_deadline_missed_status
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset total_count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_requested_incompatible_qos_status(&self) -> DdsResult<RequestedIncompatibleQosStatus> {
        let mut status_guard = self
            .requested_incompatible_qos_status
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?;
        let result = status_guard.clone();

        // Reset total_count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_subscription_matched_status(&self) -> DdsResult<SubscriptionMatchedStatus> {
        let mut status_guard =
            self.subscription_matched_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset total_count_change
        status_guard.current_count_change = 0;
        status_guard.total_count_change = 0;
        Ok(result)
    }

    fn handle_requested_deadline_missed_status(
        &self,
        info: Arc<RequestedDeadlineMissedStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .requested_deadline_missed_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            status_guard.last_instance_handle = info.last_instance_handle();
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::REQUESTED_DEADLINE_MISSED) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_requested_deadline_missed(self, &status);
                listener_called = true;
            }
            let subscriber = self.get_subscriber()?;
            if let Some(listener) = subscriber.get_listener()? {
                listener.on_requested_deadline_missed(self, &status);
                listener_called = true;
            }
            let participant = subscriber.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_requested_deadline_missed(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_requested_deadline_missed_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::REQUESTED_DEADLINE_MISSED, true)?;

        // Ownership is lost when deadline is missed
        if let Ok(datareader_cache) = self.datareader_cache.lock() {
            datareader_cache.revoke_current_owner_from_instance(info.last_instance_handle())?;
        }

        Ok(())
    }
    fn handle_requested_incompatible_qos_status(
        &self,
        info: Arc<RequestedIncompatibleQosStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .requested_incompatible_qos_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            status_guard.last_policy_id = info.last_policy_id();
            status_guard.policies = info.policies();
            status_guard.clone()
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::REQUESTED_INCOMPATIBLE_QOS) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_requested_incompatible_qos(self, &status);
                listener_called = true;
            }
            let subscriber = self.get_subscriber()?;
            if let Some(listener) = subscriber.get_listener()? {
                listener.on_requested_incompatible_qos(self, &status);
                listener_called = true;
            }
            let participant = subscriber.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_requested_incompatible_qos(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_requested_incompatible_qos_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::REQUESTED_INCOMPATIBLE_QOS, true)?;

        Ok(())
    }
    fn handle_sample_lost_status(&self) -> DdsResult<()> {
        let status = {
            let mut status_guard =
                self.sample_lost_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::SAMPLE_LOST) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_sample_lost(self, &status);
                listener_called = true;
            }
            let subscriber = self.get_subscriber()?;
            if let Some(listener) = subscriber.get_listener()? {
                listener.on_sample_lost(self, &status);
                listener_called = true;
            }
            let participant = subscriber.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_sample_lost(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_sample_lost_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::SAMPLE_LOST, true)?;

        Ok(())
    }
    fn handle_sample_rejected_status(&self, info: Arc<SampleRejectedStatus>) -> DdsResult<()> {
        let status = {
            let mut status_guard =
                self.sample_rejected_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            status_guard.last_reason = info.last_reason();
            status_guard.last_instance_handle = info.last_instance_handle();
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::SAMPLE_REJECTED) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_sample_rejected(self, &status);
                listener_called = true;
            }
            let subscriber = self.get_subscriber()?;
            if let Some(listener) = subscriber.get_listener()? {
                listener.on_sample_rejected(self, &status);
                listener_called = true;
            }
            let participant = subscriber.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_sample_rejected(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_sample_rejected_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::SAMPLE_REJECTED, true)?;

        Ok(())
    }
    fn handle_data_available_status(&self) -> DdsResult<()> {
        // Listener
        if let Some(listener) = self.get_listener()? {
            listener.on_data_available(self);
        }
        let subscriber = self.get_subscriber()?;
        if let Some(listener) = subscriber.get_listener()? {
            listener.on_data_available(self);
            listener.on_data_on_readers(&subscriber);
        }
        let participant = subscriber.get_participant()?;
        if let Some(listener) = participant.get_listener()? {
            listener.on_data_available(self);
            listener.on_data_on_readers(&subscriber);
        }

        // StatusCondition
        self.set_read_communication_status(true)?;

        Ok(())
    }

    fn handle_liveliness_changed_status(
        &self,
        info: Arc<LivelinessChangedStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .liveliness_changed_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.alive_count =
                (status_guard.alive_count + info.alive_count_change()).max(0);
            status_guard.not_alive_count =
                (status_guard.not_alive_count + info.not_alive_count_change()).max(0);
            status_guard.alive_count_change += info.alive_count_change();
            status_guard.not_alive_count_change += info.not_alive_count_change();
            status_guard.last_publication_handle = info.last_publication_handle();
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::LIVELINESS_CHANGED) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_liveliness_changed(self, &status);
                listener_called = true;
            }
            let subscriber = self.get_subscriber()?;
            if let Some(listener) = subscriber.get_listener()? {
                listener.on_liveliness_changed(self, &status);
                listener_called = true;
            }
            let participant = subscriber.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_liveliness_changed(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_liveliness_changed_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::LIVELINESS_CHANGED, true)?;

        Ok(())
    }
    pub(crate) fn handle_subscription_matched_status(
        &self,
        info: Arc<SubscriptionMatchedStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .subscription_matched_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.current_count =
                (status_guard.current_count + info.current_count_change()).max(0);
            status_guard.current_count_change += info.current_count_change();
            if info.current_count_change() > 0 {
                status_guard.total_count += info.current_count_change();
                status_guard.total_count_change += info.current_count_change();
            }
            status_guard.last_publication_handle = info.last_publication_handle();
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::SUBSCRIPTION_MATCHED) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_subscription_matched(self, &status);
                listener_called = true;
            }
            let subscriber = self.get_subscriber()?;
            if let Some(listener) = subscriber.get_listener()? {
                listener.on_subscription_matched(self, &status);
                listener_called = true;
            }
            let participant = subscriber.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_subscription_matched(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_subscription_matched_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::SUBSCRIPTION_MATCHED, true)?;

        Ok(())
    }

    fn set_communication_status_propagation(
        &self,
        status_kind: &StatusKind,
        trigger_value: bool,
    ) -> DdsResult<()> {
        // 1. DataReader StatusCondition
        self.set_communication_status(status_kind, trigger_value)?;
        // 2. Subscriber StatusCondition
        let subscriber = self.get_subscriber()?;
        subscriber.set_communication_status(status_kind, trigger_value)?;
        // 3. DomainParticipant StatusCondition
        subscriber.get_participant()?.set_communication_status(status_kind, trigger_value)?;

        Ok(())
    }

    fn set_read_communication_status(&self, trigger_value: bool) -> DdsResult<()> {
        // 1. DataReader StatusCondition
        self.set_communication_status(&StatusKind::DATA_AVAILABLE, trigger_value)?;

        // 2. Subscriber StatusCondition
        let subscriber = self.get_subscriber()?;
        subscriber.set_communication_status(&StatusKind::DATA_ON_READERS, trigger_value)?;
        subscriber.set_communication_status(&StatusKind::DATA_AVAILABLE, trigger_value)?;

        // 3. DomainParticipant StatusCondition
        let participant = subscriber.get_participant()?;
        participant.set_communication_status(&StatusKind::DATA_ON_READERS, trigger_value)?;
        participant.set_communication_status(&StatusKind::DATA_AVAILABLE, trigger_value)?;

        Ok(())
    }

    pub(crate) fn get_datareader_cache(
        &self,
    ) -> DdsResult<Arc<Mutex<DataReaderHistoryCache<Foo>>>> {
        Ok(self.datareader_cache.clone())
    }

    pub(crate) fn get_available_changes(&self) -> DdsResult<Vec<Arc<CacheChange>>> {
        let datareader_cache = self.get_datareader_cache();
        let arc = datareader_cache?;
        let guard = arc.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(guard.get_changes().clone())
    }

    pub(crate) fn get_change(
        &self,
        seq_num: SequenceNumber,
        writer_guid: Guid,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        let datareader_cache = self.get_datareader_cache();
        let arc = datareader_cache?;
        let guard = arc.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let change = guard.get_change(seq_num, writer_guid);
        Ok(change)
    }

    /// Removes change from both DataReader and RTPS reader caches.
    /// This acquires both cache locks, so avoid calling from RTPS Reader contexts to prevent deadlock.
    pub(crate) fn remove_change(&self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        if let Ok(mut datareader_cache) = self.get_datareader_cache()?.lock() {
            datareader_cache.remove_change(a_change.clone())?;
        }

        self.remove_change_from_rtps_reader_cache(a_change)?;
        Ok(())
    }

    /// This should be only called when removing a change from data reader side to rtps reader side to avoid deadlock.
    /// RTPS reader history keeps acquiring datareader cache lock on socket listening thread,
    /// So never try to acquire rtps reader cache lock while holding datareader cache lock.
    fn remove_change_from_rtps_reader_cache(&self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        if let Ok(weak_rtps_reader) = self.rtps_reader.lock().as_ref() {
            let weak_rtps_reader = weak_rtps_reader
                .as_ref()
                .ok_or(DdsError::Error("RTPS Reader is not initialized".to_string()))?;
            let rtps_reader = weak_rtps_reader
                .upgrade()
                .ok_or(DdsError::Error("Failed to upgrade rtps reader weak".to_string()))?;
            let rtps_reader_cache = rtps_reader.reader_cache();
            let mut cache_guard = rtps_reader_cache.lock().map_err(|e| {
                DdsError::Error(format!("Failed to lock rtps reader cache mutex: {}", e))
            })?;
            let res = cache_guard.remove_change(a_change);
            if res.is_ok() {
                return Ok(());
            } else {
                return Err(DdsError::Error(res.err().unwrap().to_string()));
            }
        } else {
            Err(DdsError::Error("RTPS Reader is not initialized".to_string()))
        }
    }

    pub(crate) fn fallback_instance_handle(
        &self,
        change: &CacheChange,
    ) -> DdsResult<InstanceHandle> {
        let instance_handle = change.instance_handle();

        // Fallback: If InlineQos has no key_hash, compute from SerializedData (RTPS 9.6.4.8)
        if instance_handle.is_nil() && self.type_support.is_compute_key_provided() {
            log::info!("Instance handle is NIL, computing from serialized data (fallback)");
            let data = self.type_support.deserialize(change.data_value())?;
            let computed_handle = self.type_support.compute_key(&*data);
            // log::info!("Computed instance handle from data: {:?}", computed_handle);
            Ok(computed_handle)
        } else {
            // log::info!("Using instance_handle from InlineQos (no fallback needed)");
            Ok(instance_handle)
        }
    }

    pub(crate) fn update_instance_state(
        &self,
        instance_handle: InstanceHandle,
        new_state: InstanceStateKind,
        cache_change: Option<&CacheChange>,
    ) -> DdsResult<()> {
        // Non-keyed topic doesn't have instance state
        if instance_handle.is_nil() {
            return Err(DdsError::BadParameter);
        }

        let mut instance_infos =
            self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let info = instance_infos.entry(instance_handle).or_insert_with(|| InstanceInfo {
            key: Arc::new([]),
            view_state: ViewStateKind::NEW_VIEW_STATE,
            instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
            disposed_generation_count: 0,
            no_writers_generation_count: 0,
        });

        // Update InstanceState based on change kind
        log::trace!("Updating instance state based on change kind: {:?}", new_state);
        match new_state {
            InstanceStateKind::ALIVE_INSTANCE_STATE => {
                log::debug!("Setting instance state to ALIVE");

                // 2.2.2.5.1.5 Interpretation of the SampleInfo disposed_generation_count and no_writers_generation_count
                if info.instance_state == InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE {
                    info.disposed_generation_count += 1;
                    log::trace!("Instance was previously NOT_ALIVE_DISPOSED, incrementing disposed_generation_count to {}",
                                   info.disposed_generation_count);
                }

                if info.instance_state == InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE {
                    log::trace!("Instance was previously NOT_ALIVE_NO_WRITERS, incrementing no_writers_generation_count to {}",
                                   info.no_writers_generation_count);
                    info.no_writers_generation_count += 1;
                }

                // 2.2.2.5.1.8 Interpretation of the SampleInfo view_state
                let was_not_alive = info.instance_state != InstanceStateKind::ALIVE_INSTANCE_STATE;
                let has_been_disposed_or_unregistered =
                    info.disposed_generation_count > 0 || info.no_writers_generation_count > 0;

                if was_not_alive && has_been_disposed_or_unregistered {
                    log::debug!("Instance reborn: resetting view_state to NEW (disposed_gen={}, no_writers_gen={})",
                                   info.disposed_generation_count, info.no_writers_generation_count);
                    info.view_state = ViewStateKind::NEW_VIEW_STATE;
                }

                info.instance_state = InstanceStateKind::ALIVE_INSTANCE_STATE;

                if info.key.is_empty() && cache_change.is_some() {
                    log::debug!("Extracting and serializing key from data");
                    let data = self.type_support.deserialize(
                        cache_change
                            .as_ref()
                            .ok_or(DdsError::Error(
                                "CacheChange is not properly initialized".to_string(),
                            ))?
                            .data_value(),
                    )?;
                    let foo = data
                        .downcast::<Foo>()
                        .map(|boxed| *boxed)
                        .map_err(|_| DdsError::Error("Type downcast failed".to_string()))?;
                    log::trace!("Deserialized data: {:?}", &foo);
                    let ser_key = self.type_support.serialize_key(&foo as &dyn Any)?;
                    info.key = ser_key;
                }

                let monitor_guard =
                    self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                if let Some(monitor) = monitor_guard.as_ref() {
                    monitor.track_instance(&instance_handle);
                }
            }
            InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE => {
                if info.instance_state == InstanceStateKind::ALIVE_INSTANCE_STATE {
                    log::debug!("Setting instance state to NOT_ALIVE_DISPOSED");
                    info.instance_state = InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE;
                    if info.key.is_empty() && cache_change.is_some() {
                        info.key = cache_change
                            .as_ref()
                            .ok_or(DdsError::Error(
                                "CacheChange is not properly initialized".to_string(),
                            ))?
                            .data_value_arc();
                    }

                    let monitor_guard =
                        self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                    if let Some(monitor) = monitor_guard.as_ref() {
                        monitor.cancel_instance(&instance_handle);
                    }

                    if !self
                        .get_qos()?
                        .reader_data_lifecycle
                        .autopurge_disposed_samples_delay
                        .is_infinite()
                    {
                        //
                    }
                } else {
                    log::debug!(
                        "Not alive state transition only occurs from ALIVE to NOT_ALIVE,cannot change to NOT_ALIVE_DISPOSED from {:?}",
                        info.instance_state
                    );
                }
            }
            InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE => {
                if info.instance_state == InstanceStateKind::ALIVE_INSTANCE_STATE {
                    log::debug!("Setting instance state to NOT_ALIVE_NO_WRITERS");
                    info.instance_state = InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE;
                    if info.key.is_empty() && cache_change.is_some() {
                        info.key = cache_change
                            .as_ref()
                            .ok_or(DdsError::Error(
                                "CacheChange is not properly initialized".to_string(),
                            ))?
                            .data_value_arc();
                    }

                    let monitor_guard =
                        self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                    if let Some(monitor) = monitor_guard.as_ref() {
                        monitor.cancel_instance(&instance_handle);
                    }

                    if !self
                        .get_qos()?
                        .reader_data_lifecycle
                        .autopurge_nowriter_samples_delay
                        .is_infinite()
                    {
                        //
                    }
                } else {
                    log::debug!(
                        "Not alive state transition only occurs from ALIVE to NOT_ALIVE, cannot change to NOT_ALIVE_NO_WRITERS from {:?}",
                        info.instance_state
                    );
                }
            }
            _ => {
                log::trace!("Not yet implemented for the input state {:?}", new_state);
            }
        }

        Ok(())
    }
}

impl<Foo: DdsType> DataReader<Foo> {
    pub(crate) fn new(
        guid: Guid,
        type_support: Arc<dyn TypeSupport + Send>,
        topic_description: &dyn TopicDescription,
        qos: DataReaderQos,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
        mask: StatusMask,
        subscriber: &Arc<Subscriber>,
    ) -> DdsResult<Self> {
        // Downcast to get Topic reference
        let (topic_weak, cft_weak) = if let Some(topic) =
            topic_description.as_any().downcast_ref::<Topic>()
        {
            // Get Arc<Topic> from participant to create Weak reference
            let participant = topic_description.get_participant()?;
            let topic_arc = participant.find_internal_topic(topic)?;
            (Some(Arc::downgrade(&topic_arc)), None)
        } else if let Some(cft) = topic_description.as_any().downcast_ref::<ContentFilteredTopic>()
        {
            let participant = topic_description.get_participant()?;
            let topic_arc = participant.find_internal_cft(cft)?;
            (None, Some(Arc::downgrade(&topic_arc)))
        } else {
            (None, None)
        };
        let mut reader = Self {
            guid,
            qos: Arc::new(Mutex::new(qos.clone())),
            listener: Arc::new(RwLock::new(listener)),
            mask: Arc::new(RwLock::new(mask)),
            status_condition: Arc::new(Mutex::new(StatusCondition::new(None))),
            read_conditions: Arc::new(Mutex::new(Vec::new())),
            self_ref: Arc::new(Mutex::new(None)),
            type_support: type_support.clone(),
            instance_infos: Arc::new(Mutex::new(HashMap::new())),
            read_samples: Arc::new(Mutex::new(HashMap::new())),
            topic: topic_weak,
            content_filtered_topic: cft_weak,
            subscriber: Some(Arc::downgrade(subscriber)),
            rtps_reader: Arc::new(Mutex::new(None)),
            enabled: Arc::new(AtomicBool::new(false)),
            deleted: Arc::new(AtomicBool::new(false)),
            liveliness_changed_status: Arc::new(Mutex::new(LivelinessChangedStatus::default())),
            sample_rejected_status: Arc::new(Mutex::new(SampleRejectedStatus::default())),
            sample_lost_status: Arc::new(Mutex::new(SampleLostStatus::default())),
            requested_deadline_missed_status: Arc::new(Mutex::new(
                RequestedDeadlineMissedStatus::default(),
            )),
            requested_incompatible_qos_status: Arc::new(Mutex::new(
                RequestedIncompatibleQosStatus::default(),
            )),
            subscription_matched_status: Arc::new(Mutex::new(SubscriptionMatchedStatus::default())),
            deadline_monitor: Arc::new(Mutex::new(None)),
            change_callback: None,
            status_callback: None,
            _phantom: PhantomData,
            datareader_cache: Arc::new(Mutex::new(DataReaderHistoryCache::<Foo>::new(
                Weak::new(),
                qos.reliability,
                qos.history,
                qos.resource_limits,
                type_support.is_compute_key_provided(),
                qos.ownership.kind,
            ))),
        };

        let reader_arc = Arc::new(reader.clone());
        let weak_ref = Arc::downgrade(&reader_arc);
        {
            let mut status_condition =
                reader.status_condition.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            *status_condition = StatusCondition::new(Some(weak_ref.clone()));
        }

        // Without the Arc, the new() function ends and memory is freed. StatusCondition's entity field returns None.
        {
            let mut self_ref =
                reader.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            *self_ref = Some(reader_arc);
        }

        let change_callback = reader.create_change_received_callback()?;
        reader.change_callback = Some(change_callback.clone());
        let status_callback = reader.create_status_callback()?;
        reader.status_callback = Some(status_callback.clone());

        {
            let mut datareader_cache =
                reader.datareader_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            datareader_cache.set_datareader(weak_ref);
            datareader_cache.set_update_status(status_callback.clone());
        }

        let period = reader.get_qos()?.deadline.period;
        if !period.is_infinite() {
            *reader.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))? =
                Some(DeadlineMonitor::new(period, status_callback, false));
        }
        Ok(reader)
    }

    pub fn delete_readcondition<T: Into<Arc<dyn ReadConditionTrait + Send + Sync>>>(
        &self,
        condition: T,
    ) -> DdsResult<()> {
        /*
            This operation deletes a ReadCondition attached to this DataReader.
            Since QueryCondition is a specialization of ReadCondition, this operation can also delete QueryConditions.
            If the ReadCondition is not attached to this DataReader, this operation returns error code PRECONDITION_NOT_MET.
            Error codes that can be returned in addition to the standard error codes: PRECONDITION_NOT_MET.
        */
        self.is_deleted()?;
        let condition_trait: Arc<dyn ReadConditionTrait + Send + Sync> = condition.into();
        self.owns_read_condition(&condition_trait)?;
        {
            let mut conditions =
                self.read_conditions.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            // Try Arc::ptr_eq first, if it fails use StatusCondition's PartialEq
            let pos = conditions.iter().position(|c| {
                Arc::ptr_eq(c, &condition_trait) || {
                    // Compare with StatusCondition PartialEq
                    std::ptr::eq(
                        c.as_ref() as *const dyn ReadConditionTrait as *const (),
                        condition_trait.as_ref() as *const dyn ReadConditionTrait as *const (),
                    ) || format!("{:?}", c) == format!("{:?}", condition_trait)
                }
            });

            if let Some(pos) = pos {
                conditions.remove(pos);
                return Ok(());
            }
        }
        Err(DdsError::Error("ReadCondition not found.".to_string()))
    }

    /// Reads data samples from the DataReader without removing them from the cache.
    ///
    /// This operation accesses data samples that match the specified state filters. Unlike `take()`,
    /// this operation does not remove samples from the reader's cache, allowing them to be read
    /// multiple times. After reading, the sample state changes from `NotRead` to `Read`.
    ///
    /// # Arguments
    ///
    /// * `max_samples` - Maximum number of samples to return. Use `LENGTH_UNLIMITED` (-1) for all available samples.
    /// * `sample_states` - Filter for sample read state (e.g., `&[SampleStateKind::NotRead]`).
    ///   Use `&[SampleStateKind::ANY_SAMPLE_STATE]` for all states.
    /// * `view_states` - Filter for instance view state (e.g., `&[ViewStateKind::New]`).
    ///   Use `&[ViewStateKind::ANY_VIEW_STATE]` for all states.
    /// * `instance_states` - Filter for instance lifecycle state (e.g., `&[InstanceStateKind::Alive]`).
    ///   Use `&[InstanceStateKind::ANY_INSTANCE_STATE]` for all states.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<DataSample<Foo>>)` containing the data samples and their associated `SampleInfo`.
    /// Each `DataSample` contains the data and metadata about the sample.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The reader has been deleted
    /// * The reader is not enabled
    pub fn read(
        &self,
        max_samples: i32,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        self.read_or_take(
            max_samples,
            InstanceHandle::NIL,
            Some(sample_states),
            Some(view_states),
            Some(instance_states),
            None,
            false,
            false,
            false,
        )
    }

    /// Takes data samples from the DataReader, removing them from the cache.
    ///
    /// This operation accesses data samples that match the specified state filters and removes
    /// them from the reader's cache. Unlike `read()`, samples returned by this operation cannot
    /// be accessed again through the reader. This is the most common way to consume data.
    ///
    /// # Arguments
    ///
    /// * `max_samples` - Maximum number of samples to return. Use `LENGTH_UNLIMITED` (-1) for all available samples.
    /// * `sample_states` - Filter for sample read state (e.g., `&[SampleStateKind::NotRead]`).
    ///   Use `&[SampleStateKind::ANY_SAMPLE_STATE]` for all states.
    /// * `view_states` - Filter for instance view state (e.g., `&[ViewStateKind::New]`).
    ///   Use `&[ViewStateKind::ANY_VIEW_STATE]` for all states.
    /// * `instance_states` - Filter for instance lifecycle state (e.g., `&[InstanceStateKind::Alive]`).
    ///   Use `&[InstanceStateKind::ANY_INSTANCE_STATE]` for all states.
    ///
    /// # Returns
    ///
    /// Returns `Ok(Vec<DataSample<Foo>>)` containing the data samples and their associated `SampleInfo`.
    /// Each `DataSample` contains the data and metadata about the sample.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The reader has been deleted
    /// * The reader is not enabled
    pub fn take(
        &self,
        max_samples: i32,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        self.read_or_take(
            max_samples,
            InstanceHandle::NIL,
            Some(sample_states),
            Some(view_states),
            Some(instance_states),
            None,
            false,
            false,
            true,
        )
    }

    // TODO
    pub fn return_loan(&self) -> DdsResult<Vec<DataSample<Foo>>> {
        // in: data_values: <Foo>[], sample_infos: SampleInfo[]
        // out: DdsError_t, data_values: <Foo>[], sample_infos: SampleInfo[]
        self.is_enabled()?;
        /*
            When calling read or take with a collection of max_len=0,
            the DataReader "loans" its internal buffer to provide zero-copy access.
            For this, the arguments of the read and take functions should be changed as follows:
            pub fn read_with_params(
                &self,
                data_values: &mut Vec<DataSample<Foo>>, // For max_len check
                sample_infos: &mut Vec<SampleInfo>,
                // ...
            )
            Due to Rust's characteristics, receiving &mut T as an argument complicates ownership and lifetime management.
            Additionally, receiving pointers as arguments requires unsafe blocks.
        */
        Err(DdsError::Unsupported)
    }

    pub fn read_next_sample(&self) -> DdsResult<DataSample<Foo>> {
        match self.read_or_take(
            1,
            InstanceHandle::NIL,
            Some(&[SampleStateKind::NOT_READ_SAMPLE_STATE]),
            Some(&[ViewStateKind::ANY_VIEW_STATE]),
            Some(&[InstanceStateKind::ANY_INSTANCE_STATE]),
            None,
            false,
            false,
            false,
        ) {
            Ok(samples) => Ok(samples[0].clone()),
            Err(e) => Err(e),
        }
    }

    pub fn take_next_sample(&self) -> DdsResult<DataSample<Foo>> {
        match self.read_or_take(
            1,
            InstanceHandle::NIL,
            Some(&[SampleStateKind::NOT_READ_SAMPLE_STATE]),
            Some(&[ViewStateKind::ANY_VIEW_STATE]),
            Some(&[InstanceStateKind::ANY_INSTANCE_STATE]),
            None,
            false,
            false,
            true,
        ) {
            Ok(samples) => Ok(samples[0].clone()),
            Err(e) => Err(e),
        }
    }

    pub fn read_instance(
        &self,
        max_samples: i32,
        handle: InstanceHandle,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        self.read_or_take(
            max_samples,
            handle,
            Some(sample_states),
            Some(view_states),
            Some(instance_states),
            None,
            true,
            true,
            false,
        )
    }

    pub fn take_instance(
        &self,
        max_samples: i32,
        handle: InstanceHandle,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        self.read_or_take(
            max_samples,
            handle,
            Some(sample_states),
            Some(view_states),
            Some(instance_states),
            None,
            true,
            true,
            true,
        )
    }

    pub fn read_next_instance(
        &self,
        max_samples: i32,
        previous_handle: InstanceHandle,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        self.read_or_take(
            max_samples,
            previous_handle,
            Some(sample_states),
            Some(view_states),
            Some(instance_states),
            None,
            true,
            false,
            false,
        )
    }

    pub fn take_next_instance(
        &self,
        max_samples: i32,
        previous_handle: InstanceHandle,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        self.read_or_take(
            max_samples,
            previous_handle,
            Some(sample_states),
            Some(view_states),
            Some(instance_states),
            None,
            true,
            false,
            true,
        )
    }

    pub fn read_w_condition<T: Into<Arc<dyn ReadConditionTrait + Send + Sync>>>(
        &self,
        max_samples: i32,
        condition: T,
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        let condition_trait: Arc<dyn ReadConditionTrait + Send + Sync> = condition.into();
        self.owns_read_condition(&condition_trait)?;
        self.read_or_take(
            max_samples,
            InstanceHandle::NIL,
            None,
            None,
            None,
            Some(&condition_trait),
            false,
            false,
            false,
        )
    }

    pub fn take_w_condition<T: Into<Arc<dyn ReadConditionTrait + Send + Sync>>>(
        &self,
        max_samples: i32,
        condition: T,
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        let condition_trait: Arc<dyn ReadConditionTrait + Send + Sync> = condition.into();
        self.owns_read_condition(&condition_trait)?;
        self.read_or_take(
            max_samples,
            InstanceHandle::NIL,
            None,
            None,
            None,
            Some(&condition_trait),
            false,
            false,
            true,
        )
    }

    pub fn read_next_instance_w_condition<T: Into<Arc<dyn ReadConditionTrait + Send + Sync>>>(
        &self,
        max_samples: i32,
        previous_handle: InstanceHandle,
        condition: T,
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        let condition_trait: Arc<dyn ReadConditionTrait + Send + Sync> = condition.into();
        self.owns_read_condition(&condition_trait)?;
        self.read_or_take(
            max_samples,
            previous_handle,
            None,
            None,
            None,
            Some(&condition_trait),
            true,
            false,
            false,
        )
    }

    pub fn take_next_instance_w_condition<T: Into<Arc<dyn ReadConditionTrait + Send + Sync>>>(
        &self,
        max_samples: i32,
        previous_handle: InstanceHandle,
        condition: T,
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        let condition_trait: Arc<dyn ReadConditionTrait + Send + Sync> = condition.into();
        self.owns_read_condition(&condition_trait)?;
        self.read_or_take(
            max_samples,
            previous_handle,
            None,
            None,
            None,
            Some(&condition_trait),
            true,
            false,
            true,
        )
    }

    fn read_or_take(
        &self,
        max_samples: i32,
        handle: InstanceHandle,
        direct_sample_states: Option<&[SampleStateKind]>,
        direct_view_states: Option<&[ViewStateKind]>,
        direct_instance_states: Option<&[InstanceStateKind]>,
        condition: Option<&Arc<dyn ReadConditionTrait + Send + Sync>>,
        single_instance: bool,
        exact: bool,
        take: bool,
    ) -> DdsResult<Vec<DataSample<Foo>>> {
        log::debug!("read_or_take called: max_samples={}, handle={:?}, single_instance={}, exact={}, take={}",
                   max_samples, handle, single_instance, exact, take);
        log::debug!(
            "direct_states: sample={:?}, view={:?}, instance={:?}",
            direct_sample_states.is_some(),
            direct_view_states.is_some(),
            direct_instance_states.is_some()
        );
        log::debug!("condition: {:?}", condition.is_some());

        self.is_enabled()?;

        if max_samples == 0 || max_samples > i32::MAX {
            log::warn!("BadParameter: max_samples={}", max_samples);
            return Err(DdsError::BadParameter);
        }

        let (handle, exact) = if single_instance && !exact {
            // for *_next_instance
            let next_handle = self.find_next_instance_handle(handle)?;
            if next_handle.is_nil() {
                log::debug!("NoData: next_handle is nil");
                return Err(DdsError::NoData);
            }
            (next_handle, true)
        } else {
            (handle, exact)
        };

        if exact && handle.is_nil() {
            // for read/take_instance
            log::warn!("BadParameter: exact={} but handle is nil", exact);
            return Err(DdsError::BadParameter);
        }

        self.set_read_communication_status(false)?;
        let mut result_samples: Vec<DataSample<Foo>> = Vec::new();
        // TODO: The size of the collection may be additionally limited by the PRESENTATION QoS policy (2.2.3.6).
        let mut remaining_samples = if max_samples == -1 { i32::MAX } else { max_samples };

        // let rtps_reader = self.get_rtps_reader()?;
        // let mut changes = rtps_reader.available_changes();

        let mut changes = self.get_available_changes()?;
        log::debug!("Available changes count: {}", changes.len());
        // self.sort_changes_by_timestamp(&mut changes)?;

        if changes.is_empty() {
            log::debug!("NoData: no available changes");
            return Err(DdsError::NoData);
        }

        let (sample_states, view_states, instance_states) = if let Some(cond) = condition {
            log::debug!("Using condition masks");
            let states = (
                cond.get_sample_state_mask(),
                cond.get_view_state_mask(),
                cond.get_instance_state_mask(),
            );
            if let Some(query_condition) = cond.as_any().downcast_ref::<QueryCondition>() {
                if let Some(order_by_fields) = query_condition.get_order_by_fields() {
                    log::debug!("QueryCondition ORDER BY detected: {:?}", order_by_fields);
                    self.sort_changes_by_order_fields(&mut changes, order_by_fields)?;
                } else {
                    // Default sorting if no ORDER BY
                    log::debug!("No ORDER BY, using timestamp sort");
                    self.sort_changes_by_timestamp(&mut changes)?;
                }
            } else {
                // Default sorting for ReadCondition
                log::debug!("ReadCondition detected, using timestamp sort");
                self.sort_changes_by_timestamp(&mut changes)?;
            }
            states
        } else if let (Some(sample_states), Some(view_states), Some(instance_states)) =
            (direct_sample_states, direct_view_states, direct_instance_states)
        {
            log::debug!("Using direct state masks");
            self.sort_changes_by_timestamp(&mut changes)?;
            (sample_states, view_states, instance_states)
        } else {
            // No mask
            log::error!("No state masks provided - returning BadParameter");
            return Err(DdsError::BadParameter);
        };

        // let instance_infos = self.get_instance_infos()?; // All DataSample1, 2 of the same instance -> treated as New
        let (qc_expression, qc_parameters) = if let Some(cond) = condition {
            if let Some(query) = cond.as_any().downcast_ref::<QueryCondition>() {
                (Some(query.parsed_expression.clone()), query.get_query_parameters()?)
            } else {
                (None, Vec::new())
            }
        } else {
            (None, Vec::new())
        };

        // Get ContentFilteredTopic expression (independently!)
        let (cft_expression, cft_parameters) = if let Some(cft) = &self.content_filtered_topic {
            let cft = cft
                .upgrade()
                .ok_or(DdsError::Error("ContentFilteredTopic is deleted".to_string()))?;
            (Some(cft.parsed_expression.clone()), cft.get_expression_parameters()?)
        } else {
            (None, Vec::new())
        };

        log::debug!("Processing {} changes, need {} samples", changes.len(), remaining_samples);
        for (idx, change) in changes.iter().enumerate() {
            if remaining_samples <= 0 {
                log::debug!("Reached sample limit, stopping");
                break;
            }

            if exact && change.instance_handle() != handle {
                log::trace!("Skipping change {}: handle mismatch", idx);
                continue;
            }

            // let is_read = self.is_sample_read(&change.writer_guid(), &change.sequence_number())?;
            // let sample_state = if is_read {
            //     SampleStateKind::READ_SAMPLE_STATE
            // } else {
            //     SampleStateKind::NOT_READ_SAMPLE_STATE
            // };
            // if !sample_states.matches(sample_state) {
            //     continue;
            // }
            // Check sample state
            let instance_infos = self.get_instance_infos()?; // Only DataSample1 of the same instance is treated as New, DataSample2 is treated as NotNew.
            let sample_state =
                self.get_sample_state(&change.writer_guid(), &change.sequence_number())?;
            let info = match instance_infos.get(&change.instance_handle()) {
                Some(info) => info,
                None => &InstanceInfo {
                    key: Arc::new([]),
                    view_state: ViewStateKind::NEW_VIEW_STATE,
                    instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
                    disposed_generation_count: 0,
                    no_writers_generation_count: 0,
                },
            };

            // Check basic state masks
            if !sample_states.matches(sample_state)
                || !view_states.matches(info.view_state)
                || !instance_states.matches(info.instance_state)
            {
                log::trace!("Skipping change {}: state mask mismatch (sample={:?}, view={:?}, instance={:?})",
                          idx, sample_state, info.view_state, info.instance_state);
                continue;
            }
            log::trace!("Change {} passed state mask filters", idx);

            // 2. Create DataSample
            match self.change_to_data_sample(change, change.instance_handle(), sample_state) {
                Ok(data_sample) => {
                    if let Some(qc_expr) = &qc_expression {
                        if !qc_expr.evaluate(&data_sample.data()?, &qc_parameters)? {
                            log::trace!(
                                "Skipping change {}: QueryCondition expression failed",
                                idx
                            );
                            continue;
                        }
                        log::trace!("Change {} passed QueryCondition", idx);
                    }
                    if let Some(cft_expr) = &cft_expression {
                        if !cft_expr.evaluate(&data_sample.data()?, &cft_parameters)? {
                            log::trace!(
                                "Skipping change {}: ContentFilteredTopic expression failed",
                                idx
                            );
                            continue;
                        }
                        log::trace!("Change {} passed ContentFilteredTopic", idx);
                    }
                    // self.mark_instance_as_viewed(change.instance_handle());
                    if take {
                        self.remove_change(change.clone())?;
                    } else {
                        self.mark_sample_as_read(&change.writer_guid(), change.sequence_number())?;
                    }
                    result_samples.push(data_sample);
                    remaining_samples -= 1;
                }
                Err(_) => {
                    if take {
                        self.remove_change(change.clone())?;
                    }
                    continue;
                }
            }
        }

        // 2.2.2.5.1.8 Interpretation of the SampleInfo view_state
        for sample in &result_samples {
            self.mark_instance_as_viewed(sample.sample_info().instance_handle);
        }

        self.reevaluate_all_conditions()?;

        if result_samples.is_empty() {
            log::debug!("read_or_take: no samples matched filters, returning NoData");
            Err(DdsError::NoData)
        } else {
            log::debug!("read_or_take: returning {} samples", result_samples.len());
            self.update_all_sample_ranks(&mut result_samples);
            Ok(result_samples)
        }
    }

    pub(crate) fn create_change_received_callback(
        &self,
    ) -> DdsResult<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>> {
        let self_ref = self.self_ref.as_ref().lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let self_ref = self_ref
            .clone()
            .ok_or(DdsError::Error("DataReader is not properly initialized".to_string()))?;
        let weak_self = Arc::downgrade(&self_ref);
        Ok(Arc::new(move |change| {
            if let Some(entity) = weak_self.upgrade() {
                let _ = entity.process_incoming_change(change);
            }
        }))
    }

    pub(crate) fn process_incoming_change(&self, change: Arc<CacheChange>) -> DdsResult<()> {
        log::debug!("Processing instance registration for change");
        let instance_handle = change.instance_handle();

        let has_key = self.type_support.is_compute_key_provided() && !instance_handle.is_nil();
        log::debug!(
            "Type has key: {}, handle is valid: {}",
            self.type_support.is_compute_key_provided(),
            !instance_handle.is_nil()
        );

        let info = if has_key {
            // With key: store in instance_info
            log::debug!("Processing keyed type instance");
            let mut instance_infos =
                self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            let info = instance_infos.get_mut(&instance_handle).ok_or_else(|| {
                DdsError::Error("InstanceInfo should have been updated already when added to data reader history cache".to_string())
            })?;

            info.clone() // Clone and use after releasing lock
        } else {
            // NoKey type
            log::debug!("Processing keyless type instance with temporary InstanceInfo");
            InstanceInfo {
                key: Arc::new([]),
                view_state: ViewStateKind::NEW_VIEW_STATE,
                instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
                disposed_generation_count: 0,
                no_writers_generation_count: 0,
            }
        };

        if let Ok(readconditions) = self.get_readconditions() {
            log::debug!("Checking {} read condition(s)", readconditions.len());
            for (idx, readcondition) in readconditions.iter().enumerate() {
                let sample_state =
                    self.get_sample_state(&change.writer_guid(), &change.sequence_number())?;
                log::trace!(
                    "Condition[{}] - Checking masks: sample={:?}, view={:?}, instance={:?}",
                    idx,
                    sample_state,
                    info.view_state,
                    info.instance_state
                );
                log::trace!(
                    "Condition[{}] - Expected masks: sample={:?}, view={:?}, instance={:?}",
                    idx,
                    readcondition.get_sample_state_mask(),
                    readcondition.get_view_state_mask(),
                    readcondition.get_instance_state_mask()
                );

                if readcondition.get_instance_state_mask().matches(info.instance_state)
                    && readcondition.get_view_state_mask().matches(info.view_state)
                    && readcondition.get_sample_state_mask().matches(sample_state)
                {
                    log::debug!("Condition[{}] state masks matched", idx);

                    if let Some(query_condition) =
                        readcondition.as_any().downcast_ref::<QueryCondition>()
                    {
                        log::debug!("Condition[{}] is QueryCondition, evaluating expression", idx);
                        let data = self.change_to_data_sample(
                            &change,
                            change.instance_handle(),
                            sample_state,
                        )?;
                        match query_condition.evaluate_expression(&data.data()?) {
                            Ok(true) => {
                                log::info!(
                                    "QueryCondition[{}] triggered (expression matched)",
                                    idx
                                );
                                readcondition.set_trigger_value(true);
                            }
                            Ok(false) => {
                                log::debug!("QueryCondition[{}] expression not matched", idx);
                            }
                            Err(e) => {
                                log::warn!("QueryCondition[{}] evaluation failed: {:?}", idx, e);
                            }
                        }
                    } else {
                        // Trigger immediately if regular ReadCondition
                        log::info!("ReadCondition[{}] triggered", idx);
                        readcondition.set_trigger_value(true);
                    }
                } else {
                    log::trace!("Condition[{}] state masks did not match", idx);
                }
            }
        } else {
            log::trace!("No read conditions attached to this DataReader");
        }
        // TODO: In the future, also store DeserializedKey in a map to reduce time
        Ok(())
    }

    fn change_to_data_sample(
        &self,
        change: &CacheChange,
        instance_handle: InstanceHandle,
        sample_state: SampleStateKind,
    ) -> DdsResult<DataSample<Foo>> {
        // Check if change has valid data based on its kind
        let has_valid_data = match change.kind() {
            ChangeKind::Alive | ChangeKind::AliveFiltered => true,
            ChangeKind::NotAliveDisposed
            | ChangeKind::NotAliveUnregistered
            | ChangeKind::NotAliveDisposedUnregistered => false,
        };

        // Deserialize the data
        let data = if has_valid_data { Some(change.data_value_arc()) } else { None };

        let instance_infos = self.get_instance_infos()?;
        let info = if !instance_handle.is_nil() {
            instance_infos
                .get(&instance_handle)
                .ok_or(DdsError::Error("Instance not found".to_string()))?
                .clone()
        } else {
            InstanceInfo {
                key: change.data_value_arc(),
                instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
                view_state: ViewStateKind::NEW_VIEW_STATE,
                disposed_generation_count: 0,
                no_writers_generation_count: 0,
            }
        };

        let sample_info = SampleInfo {
            sample_state,
            view_state: info.view_state,
            instance_state: info.instance_state,
            disposed_generation_count: info.disposed_generation_count,
            no_writers_generation_count: info.no_writers_generation_count,
            sample_rank: 0, // Will be updated later
            generation_rank: 0,
            absolute_generation_rank: 0,
            source_timestamp: (*change.source_timestamp().as_ref().ok_or(DdsError::Error(
                "CacheChange's source timestamp is not properly initialized".to_string(),
            ))?)
            .into(),
            instance_handle,
            publication_handle: InstanceHandle::from_guid(&change.writer_guid()),
            valid_data: has_valid_data,
        };
        Ok(DataSample::new(data, sample_info))
    }

    fn sort_changes_by_timestamp(&self, changes: &mut [Arc<CacheChange>]) -> DdsResult<()> {
        let destination_kind = self.get_qos()?.destination_order.kind;
        changes.sort_by(|a, b| {
            if destination_kind == DestinationOrderQosPolicyKind::ByReceptionTimestamp {
                // Sort by reception_timestamp (oldest to newest)
                a.reception_timestamp()
                    .cmp(&b.reception_timestamp())
                    // If timestamp is same, compare by sequence_number (safety mechanism)
                    .then_with(|| a.sequence_number().cmp(&b.sequence_number()))
            } else {
                // Sort by source_timestamp (oldest to newest)
                a.source_timestamp()
                    .cmp(&b.source_timestamp())
                    // If timestamp is same, compare by sequence_number (safety mechanism)
                    .then_with(|| a.sequence_number().cmp(&b.sequence_number()))
            }
        });
        Ok(())
    }

    /// Sort changes according to ORDER BY fields
    fn sort_changes_by_order_fields(
        &self,
        changes: &mut [Arc<CacheChange>],
        order_by_fields: &[String],
    ) -> DdsResult<()> {
        changes.sort_by(|a, b| {
            // Compare sequentially for each ORDER BY field
            for field in order_by_fields {
                // Create data samples from both changes to extract field values
                let sample_a = match self.change_to_data_sample(
                    a,
                    a.instance_handle(),
                    SampleStateKind::NOT_READ_SAMPLE_STATE,
                ) {
                    Ok(sample) => sample,
                    Err(_) => continue, // Move to next field on conversion failure
                };
                let sample_b = match self.change_to_data_sample(
                    b,
                    b.instance_handle(),
                    SampleStateKind::NOT_READ_SAMPLE_STATE,
                ) {
                    Ok(sample) => sample,
                    Err(_) => continue, // Move to next field on conversion failure
                };

                // Extract field values
                let value_a = match sample_a.data().and_then(|data| data.get_field_value(field)) {
                    Ok(value) => value,
                    Err(_) => continue, // Move to next field if field missing
                };
                let value_b = match sample_b.data().and_then(|data| data.get_field_value(field)) {
                    Ok(value) => value,
                    Err(_) => continue, // Move to next field if field missing
                };

                // Compare parameter values
                let cmp_result = value_a.compare_for_ordering(&value_b);
                if cmp_result != CmpOrdering::Equal {
                    return cmp_result;
                }
            }
            // If all ORDER BY fields are equal, compare by sequence_number (stable sort)
            a.sequence_number().cmp(&b.sequence_number())
        });

        log::info!("Sorted {} changes by ORDER BY fields: {:?}", changes.len(), order_by_fields);
        Ok(())
    }

    // Calculate sample_rank for all samples (based on sorted order)
    fn update_all_sample_ranks(&self, samples: &mut [DataSample<Foo>]) {
        let total_samples = samples.len();
        // Calculate sample_rank in sorted order
        // samples[0] = oldest sample → highest rank
        // samples[last] = newest sample → rank 0
        for (i, sample) in samples.iter_mut().enumerate() {
            sample.sample_info.sample_rank = (total_samples - 1 - i) as i32;
        }
    }

    fn find_next_instance_handle(
        &self,
        previous_handle: InstanceHandle,
    ) -> DdsResult<InstanceHandle> {
        // 1. Collect all available instance handles
        let available_handles = self.get_available_instance_handles()?;

        if available_handles.is_empty() {
            return Ok(InstanceHandle::NIL);
        }

        // 2. Sort instance handles
        let mut sorted_handles = available_handles;
        sorted_handles.sort();

        // log::debug!("find_next_instance_handle: previous_handle={:?}, sorted_handles={:?}",
        //            previous_handle, sorted_handles);

        // 3. If previous_handle is NIL, return the first instance
        if previous_handle.is_nil() {
            let first = sorted_handles.into_iter().next().unwrap_or(InstanceHandle::NIL);
            // log::debug!("previous_handle is NIL, returning first instance: {:?}", first);
            return Ok(first);
        }

        // 4. Find the instance after previous_handle
        let mut found_previous = false;
        for handle in sorted_handles {
            if found_previous {
                // log::debug!("Found next instance after {:?}: {:?}", previous_handle, handle);
                return Ok(handle);
            }
            if handle == previous_handle {
                found_previous = true;
            }
        }

        // 5. Return NIL if no next instance
        // log::debug!("No next instance found after {:?}, returning NIL", previous_handle);
        Ok(InstanceHandle::NIL)
    }

    fn get_available_instance_handles(&self) -> DdsResult<Vec<InstanceHandle>> {
        // Check available changes in RTPS reader
        let changes = self.get_available_changes()?;
        let mut handles = std::collections::HashSet::new();

        // Collect instance handles from changes
        for change in changes {
            if !change.instance_handle().is_nil() {
                handles.insert(change.instance_handle());
            }
        }

        Ok(handles.into_iter().collect())
    }

    fn reevaluate_all_conditions(&self) -> DdsResult<()> {
        if let Ok(conditions) = self.get_readconditions() {
            for condition in conditions {
                let has_matching = self.has_matching_samples_for_condition(&condition)?;
                condition.set_trigger_value(has_matching);
            }
        }
        Ok(())
    }

    fn has_matching_samples_for_condition(
        &self,
        condition: &Arc<dyn ReadConditionTrait + Send + Sync>,
    ) -> DdsResult<bool> {
        let changes = self.get_available_changes()?;
        let instance_infos = self.get_instance_infos()?;

        let read_samples = self.read_samples.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for change in changes {
            let sample_state = if read_samples
                .get(&change.writer_guid())
                .is_some_and(|seq_set| seq_set.contains(&change.sequence_number()))
            {
                SampleStateKind::READ_SAMPLE_STATE
            } else {
                SampleStateKind::NOT_READ_SAMPLE_STATE
            };

            let info = match instance_infos.get(&change.instance_handle()) {
                Some(info) => info,
                None => &InstanceInfo {
                    key: Arc::new([]),
                    view_state: ViewStateKind::NEW_VIEW_STATE,
                    instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
                    disposed_generation_count: 0,
                    no_writers_generation_count: 0,
                },
            };

            // Check state masks
            if condition.get_sample_state_mask().matches(sample_state)
                && condition.get_view_state_mask().matches(info.view_state)
                && condition.get_instance_state_mask().matches(info.instance_state)
            {
                // Also check expression if QueryCondition
                if let Some(query_condition) = condition.as_any().downcast_ref::<QueryCondition>() {
                    let data = self.change_to_data_sample(
                        &change,
                        change.instance_handle(),
                        sample_state,
                    )?;
                    if query_condition.evaluate_expression(&data.data()?).unwrap_or(false) {
                        return Ok(true);
                    }
                } else {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    // TODO: support gereration_ranks in SampleInfo
    fn _calculate_generation_ranks(&self, _info: &InstanceInfo) -> (i32, i32) {
        todo!()
    }
}

impl<Foo: 'static + Clone + Debug> DataReaderBase for DataReader<Foo> {
    fn get_liveliness_changed_status(&self) -> DdsResult<LivelinessChangedStatus> {
        // out: DdsError_t, status: LivelinessChangedStatus
        /*
            This operation provides access to the LIVELINESS_CHANGED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        self.is_deleted()?;

        self.set_communication_status_propagation(&StatusKind::LIVELINESS_CHANGED, false)?;
        self.take_liveliness_changed_status()
    }

    fn get_sample_rejected_status(&self) -> DdsResult<SampleRejectedStatus> {
        // out: DdsError_t, status: SampleRejectedStatus
        /*
            This operation provides access to the SAMPLE_REJECTED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        self.is_deleted()?;

        self.set_communication_status_propagation(&StatusKind::SAMPLE_REJECTED, false)?;
        self.take_sample_rejected_status()
    }

    fn get_requested_deadline_missed_status(&self) -> DdsResult<RequestedDeadlineMissedStatus> {
        // out: DdsError_t, status: RequestedDeadlineMissedStatus
        /*
            This operation provides access to the REQUESTED_DEADLINE_MISSED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        self.is_deleted()?;

        self.set_communication_status_propagation(&StatusKind::REQUESTED_DEADLINE_MISSED, false)?;
        self.take_requested_deadline_missed_status()
    }

    fn get_requested_incompatible_qos_status(&self) -> DdsResult<RequestedIncompatibleQosStatus> {
        // out: DdsError_t, status: RequestedIncompatibleQosStatus
        /*
            This operation provides access to the REQUESTED_INCOMPATIBLE_QOS communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        self.is_deleted()?;

        self.set_communication_status_propagation(&StatusKind::REQUESTED_INCOMPATIBLE_QOS, false)?;
        self.take_requested_incompatible_qos_status()
    }

    fn get_subscription_matched_status(&self) -> DdsResult<SubscriptionMatchedStatus> {
        // out: DdsError_t, status: SubscriptionMatchedStatus
        /*
            This operation provides access to the SUBSCRIPTION_MATCHED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        self.is_deleted()?;

        self.set_communication_status_propagation(&StatusKind::SUBSCRIPTION_MATCHED, false)?;
        self.take_subscription_matched_status()
    }

    fn get_sample_lost_status(&self) -> DdsResult<SampleLostStatus> {
        // out: DdsError_t, status: SampleLostStatus
        /*
            This operation provides access to the SAMPLE_LOST communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        self.is_deleted()?;

        self.set_communication_status_propagation(&StatusKind::SAMPLE_LOST, false)?;
        self.take_sample_lost_status()
    }

    fn get_matched_publications(&self) -> DdsResult<Vec<InstanceHandle>> {
        // in: publication_handles: InstanceHandle[]
        // out: DdsError_t, publication_handles: InstanceHandle[]
        /*
            This operation retrieves information about publications "associated" with this DataReader.
            That is, publications whose Topic matches, QoS is compatible, and that the application has not
            designated to "ignore" through the DomainParticipant's ignore_publication operation.
            The publication_handle must refer to a publication currently associated with this DataReader,
            otherwise this operation fails and returns BAD_PARAMETER.
            The get_matched_publications operation can be used to check publications associated with this DataReader.
            Additionally, this operation may fail and return UNSUPPORTED if the infrastructure does not hold
            the information needed to fill in the publication_data.
        */
        self.is_enabled()?;
        let matched_writers_guids;
        {
            let rtps_reader = self.get_rtps_reader()?;
            matched_writers_guids = rtps_reader.matched_writers_guids();
        }
        let mut publication_handles = Vec::new();
        for guid in matched_writers_guids {
            publication_handles.push(InstanceHandle::from_guid(&guid));
        }
        Ok(publication_handles)
    }

    fn get_matched_publication_data(
        &self,
        publication_handle: InstanceHandle,
    ) -> DdsResult<PublicationBuiltinTopicData> {
        // in: publication_data: PublicationBuiltinTopicData, publication_handle: InstanceHandle
        // out: DdsError_t, publication_data: PublicationBuiltinTopicData
        /*
            This operation retrieves the list of publications "associated" with this DataReader.
            That is, publications whose Topic matches, QoS is compatible, and that the application has not
            designated to "ignore" through the DomainParticipant's ignore_publication operation.
            The handles in the publication_handles list are the values used by the DDS implementation to locally
            identify the associated DataWriter entities.
            These handles match the values that appear in the instance_handle field of SampleInfo when reading
            the "DCPSPublications" builtin topic.
            This operation may fail if the infrastructure does not maintain association information locally.
        */
        self.is_enabled()?;
        let writer_guid = publication_handle.to_guid();
        {
            let rtps_reader = self.get_rtps_reader()?;
            if let Ok(data) = rtps_reader.get_matched_publication_data(writer_guid) {
                return Ok(data);
            }
        }
        Err(DdsError::BadParameter)
    }

    fn create_readcondition(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<ReadCondition> {
        /*
            This operation creates a ReadCondition.
            The returned ReadCondition is attached to and belongs to this DataReader.
            On failure, this operation returns a platform-defined 'nil' value.
        */
        self.is_deleted()?;
        let self_ref = self.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let self_ref = self_ref
            .as_ref()
            .ok_or(DdsError::Error("DataReader is not properly initialized".to_string()))?;
        let self_ref: Arc<dyn DataReaderInternal<Qos = DataReaderQos>> = self_ref.clone();
        let read_condition =
            ReadCondition::new(view_states, instance_states, sample_states, &self_ref);
        let read_condition_arc = Arc::new(read_condition.clone());
        {
            let mut read_conditions =
                self.read_conditions.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            read_conditions.push(read_condition_arc.clone());
        }
        Ok(read_condition)
    }

    fn create_querycondition(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        query_expression: &str,
        query_parameters: Vec<String>,
    ) -> DdsResult<QueryCondition> {
        /*
            This operation creates a QueryCondition.
            The returned QueryCondition is attached to and belongs to this DataReader.
            The syntax for the query_expression and query_parameters parameters is described in Annex B.
            On failure, this operation returns a platform-defined 'nil' value.
        */
        self.is_deleted()?;
        let self_ref = self.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let self_ref = self_ref
            .as_ref()
            .ok_or(DdsError::Error("DataReader is not properly initialized".to_string()))?;
        let self_ref: Arc<dyn DataReaderInternal<Qos = DataReaderQos>> = self_ref.clone();
        let query_condition = QueryCondition::new(
            view_states,
            instance_states,
            sample_states,
            query_expression,
            query_parameters,
            &self_ref,
        )?;
        let query_condition_arc = Arc::new(query_condition.clone());
        {
            let mut read_conditions =
                self.read_conditions.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            read_conditions.push(query_condition_arc.clone());
        }
        Ok(query_condition)
    }

    // TODO
    fn wait_for_historical_data(&self, _max_wait: Duration) -> DdsResult<()> {
        // in: max_wait: Duration
        // out: DdsError_t
        /*
            This operation is intended to be used only on DataReader entities where the PERSISTENCE QoS kind is not VOLATILE.
            As soon as an application enables a non-VOLATILE DataReader,
            the reader begins to receive both data written before the domain was joined (i.e., "historical" data) and new data.
            In some situations, the application logic may need to wait until all "historical data" is received.
            The wait_for_historical_data operation is provided for this purpose.
            This operation blocks the calling thread until all "historical" data has been received,
            or until the time specified by the max_wait parameter has elapsed. Whichever condition is met first terminates the operation.
            A return value of OK indicates that all historical data has been received,
            while a return value of TIMEOUT indicates that not all data was received before max_wait expired.
        */
        // if self.get_qos()?.durability.kind == DurabilityQosPolicyKind::Volatile {

        // }
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    fn get_topicdescription(&self) -> DdsResult<Arc<dyn TopicDescription>> {
        self.is_deleted()?;

        if let Some(weak_cft) = &self.content_filtered_topic {
            return Ok(weak_cft
                .upgrade()
                .ok_or(DdsError::Error("ContentFilteredTopic has been deleted".to_string()))?
                as Arc<dyn TopicDescription>);
        }

        if let Some(weak_topic) = &self.topic {
            return Ok(weak_topic
                .upgrade()
                .ok_or(DdsError::Error("Topic has been deleted".to_string()))?
                as Arc<dyn TopicDescription>);
        }

        Err(DdsError::Error("No topic description".into()))
    }

    fn get_subscriber(&self) -> DdsResult<Subscriber> {
        self.is_deleted()?;
        if let Some(weak_ref) = self.subscriber.as_ref() {
            // Attempt to upgrade Weak<T> to Arc<T>
            if let Some(subscriber_arc) = weak_ref.upgrade() {
                return Ok((*subscriber_arc).clone());
            }
        }
        // If subscriber is None or the reference has expired
        Err(DdsError::Error("Subscriber reference is invalid or expired".to_string()))
    }

    fn delete_contained_entities(&self) -> DdsResult<()> {
        /*
            This operation deletes all entities created through "create" operations on this DataReader.
            That is, this operation deletes all contained ReadCondition and QueryCondition objects.
            If any contained entity is in a state where it cannot be deleted, this operation returns PRECONDITION_NOT_MET.
            Once delete_contained_entities returns successfully, the application knows that this DataReader
            no longer has any contained ReadCondition and QueryCondition objects, and the DataReader can be safely deleted.
        */
        self.is_deleted()?;
        match self.get_readconditions() {
            Ok(conditions) => {
                for condition in conditions {
                    self.delete_readcondition_internal(condition)?;
                }
            }
            Err(err) => return Err(err),
        }
        Ok(())
    }
}

impl<Foo: 'static + Clone + Debug> DataReaderInternal for DataReader<Foo> {
    fn disable(&self) -> DdsResult<()> {
        self.set_listener(None, StatusMask::default())
    }

    fn clone_boxed(&self) -> Box<dyn DataReaderBase<Qos = DataReaderQos> + Send> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn notify_data_available(&self) {
        if let Ok(listener_guard) = self.listener.read() {
            if let Some(listener) = listener_guard.as_ref() {
                listener.on_data_available(self);
            }
        }
    }

    fn get_type_id(&self) -> TypeId {
        TypeId::of::<Foo>()
    }

    fn delete(&self) {
        // Shutdown and drop deadline monitor
        let monitor_to_drop = if let Ok(mut monitor_guard) = self.deadline_monitor.lock() {
            monitor_guard.take() // Take ownership, will drop after lock is released
        } else {
            None
        };
        // Lock is released here, then monitor drops (triggering shutdown and join)
        drop(monitor_to_drop);

        let value_to_drop = self.self_ref.lock().ok().and_then(|mut guard| guard.take());
        drop(value_to_drop);

        self.deleted.store(true, Ordering::SeqCst);
    }

    fn is_deleted(&self) -> DdsResult<()> {
        if self.deleted.load(Ordering::SeqCst) {
            Err(DdsError::AlreadyDeleted)
        } else {
            Ok(())
        }
    }

    fn get_topic(&self) -> DdsResult<Topic> {
        if let Some(weak_ref) = self.topic.as_ref() {
            // Attempt to upgrade Weak<T> to Arc<T>
            if let Some(topic_arc) = weak_ref.upgrade() {
                return Ok((*topic_arc).clone());
            }
        }

        // If topic is None or the reference has expired
        Err(DdsError::Error("Topic reference is invalid or expired".to_string()))
    }

    fn get_readconditions(&self) -> DdsResult<Vec<Arc<dyn ReadConditionTrait + Send + Sync>>> {
        let guard = self.read_conditions.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        Ok(guard.clone())
    }

    fn owns_read_condition(
        &self,
        condition: &Arc<dyn ReadConditionTrait + Send + Sync>,
    ) -> DdsResult<()> {
        if let Ok(read_condition) = Arc::downcast::<ReadCondition>(condition.clone()) {
            if read_condition.get_datareader::<Foo>()?.get_instance_handle()?
                != self.get_instance_handle()?
            {
                return Err(DdsError::PreconditionNotMet);
            }
        } else if let Ok(query_condition) = Arc::downcast::<QueryCondition>(condition.clone()) {
            if query_condition.get_datareader::<Foo>()?.get_instance_handle()?
                != self.get_instance_handle()?
            {
                return Err(DdsError::PreconditionNotMet);
            }
        } else {
            return Err(DdsError::BadParameter);
        }
        Ok(())
    }

    fn delete_readcondition_internal(
        &self,
        condition: Arc<dyn ReadConditionTrait + Send + Sync>,
    ) -> DdsResult<()> {
        /*
            This operation deletes a ReadCondition attached to this DataReader.
            Since QueryCondition is a specialization of ReadCondition, this operation can also delete QueryConditions.
            If the ReadCondition is not attached to this DataReader, this operation returns error code PRECONDITION_NOT_MET.
            Error codes that can be returned in addition to the standard error codes: PRECONDITION_NOT_MET.
        */
        self.is_deleted()?;
        self.owns_read_condition(&condition)?;
        {
            let mut conditions =
                self.read_conditions.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            if let Some(pos) = conditions.iter().position(|c| Arc::ptr_eq(c, &condition)) {
                conditions.remove(pos);
                return Ok(());
            }
        }
        Err(DdsError::Error("ReadCondition not found.".to_string()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::dcps::topic::type_support::DdsType;
    use crate::domain::domain_participant_factory::DomainParticipantFactory;
    use crate::domain::qos::DomainParticipantQos;
    use crate::infrastructure::qos_policy::{
        HistoryQosPolicy, ReliabilityQosPolicy, ReliabilityQosPolicyKind,
    };
    use crate::infrastructure::wait_set::WaitSet;
    use crate::publication::data_writer_listener::DataWriterListener;
    use crate::publication::qos::{DataWriterQos, PublisherQos};
    use crate::subscription::data_reader_listener::DataReaderListener;
    use crate::subscription::qos::SubscriberQos;
    use crate::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
    use crate::subscription::subscriber_listener::SubscriberListener;
    use crate::test_utils::unique_domain_id;
    use crate::topic::qos::TopicQos;
    use std::sync::mpsc::{sync_channel, SyncSender};
    use std::sync::Arc;

    #[derive(DdsType)]
    pub struct TestData {
        #[dds(key)]
        id: u32,
    }

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
    fn test_reader_statuscondition() {
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
        let reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let status_condition = reader.get_statuscondition().unwrap();

        subscriber.delete_datareader(reader).unwrap();

        println!("{:?}", status_condition.get_entity());

        assert!(status_condition.get_entity().is_err())
    }

    struct SubListener {
        counter_sender: SyncSender<()>,
    }

    impl DataReaderListener for SubListener {
        type Foo = HelloWorld;
        fn on_data_available(
            &self,
            _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        ) {
            // Send counter signal whenever data is available
            let _ = self.counter_sender.try_send(());
            println!("Data available! Counter signal sent.");
        }
    }

    #[test]
    fn test_read_samples_change_in_order() {
        // let _ = env_logger::builder().filter_level(log::LevelFilter::Info).try_init();

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
                "hello_world_order",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();

        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 2 {
                break;
            }
        }

        let samples = data_reader
            .read(
                2,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to read samples");

        assert_eq!(samples.len(), 2);

        println!("samples: {:?}", samples);
        assert_eq!(samples[0].sample_info().sample_rank, 1);
        assert_eq!(samples[1].sample_info().sample_rank, 0);
        assert_eq!(samples[0].data().unwrap().index, data1.index);
        assert_eq!(samples[1].data().unwrap().index, data2.index);
    }

    #[test]
    fn test_read_only_first_sample() {
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
                "hello_world_first",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() }; // Published by Writer

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();

        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 2 {
                break;
            }
        }

        // Call read
        let sample1 = data_reader
            .read(
                1,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to read samples");

        let sample2 = data_reader
            .read(
                1,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to read samples");

        assert_eq!(sample1.len(), 1);
        assert_eq!(sample2.len(), 1);
        // sample_rank = rank within the result set I read
        // Size of the set I read: 1 -> sample_rank is 0.
        assert_eq!(sample1[0].sample_info().sample_rank, 0);
        assert_eq!(sample2[0].sample_info().sample_rank, 0);
        assert_eq!(sample1[0].data().unwrap().index, data1.index);
        assert_eq!(sample2[0].data().unwrap().index, data1.index);
    }

    #[test]
    fn test_take_samples() {
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
                "hello_world_takes",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 2 {
                break;
            }
        }

        // Call take
        let samples = data_reader
            .take(
                2,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to take samples");

        // Verify results
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].sample_info().sample_rank, 1);
        assert_eq!(samples[1].sample_info().sample_rank, 0);
        assert_eq!(samples[0].data().unwrap().index, data1.index);
        assert_eq!(samples[1].data().unwrap().index, data2.index);

        // Second take should have no data
        let second_take = data_reader.take(
            2,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );
        assert!(second_take.is_err());
    }

    #[test]
    fn test_take_samples_in_order() {
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
                "hello_world",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();

        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 2 {
                break;
            }
        }

        // Call take
        let samples = data_reader
            .take(
                2,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to take samples");

        // Verify results
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].sample_info().sample_rank, 1);
        assert_eq!(samples[1].sample_info().sample_rank, 0);
        assert_eq!(samples[0].data().unwrap().index, data1.index);
        assert_eq!(samples[1].data().unwrap().index, data2.index);

        // Second take should have no data
        let second_take = data_reader.take(
            2,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );
        assert!(second_take.is_err());
    }

    #[test]
    fn test_read_next_samples() {
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
                "hello_world",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();

        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 2 {
                break;
            }
        }

        // Call read_next_sample 1
        let sample0 = data_reader.read_next_sample().unwrap();
        // Call read_next_sample 2
        let sample1 = data_reader.read_next_sample().unwrap();

        assert_eq!(sample0.sample_info().sample_rank, 0);
        assert_eq!(sample1.sample_info().sample_rank, 0);
        assert_eq!(sample0.data().unwrap().index, data1.index);
        assert_eq!(sample1.data().unwrap().index, data2.index);
    }

    #[test]
    fn test_take_next_samples() {
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
                "hello_world",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();

        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 2 {
                break;
            }
        }

        // Call take_next_sample 1
        let sample0 = data_reader.take_next_sample().unwrap();
        // Call take_next_sample 2
        let sample1 = data_reader.take_next_sample().unwrap();

        assert_eq!(sample0.sample_info().sample_rank, 0);
        assert_eq!(sample1.sample_info().sample_rank, 0);
        assert_eq!(sample0.data().unwrap().index, data1.index);
        assert_eq!(sample1.data().unwrap().index, data2.index);
    }

    struct SubKeyListener {
        counter_sender: SyncSender<()>,
    }

    impl DataReaderListener for SubKeyListener {
        type Foo = HelloWorldWithKey;
        fn on_subscription_matched(
            &self,
            _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
            status: &int2dds::infrastructure::status::SubscriptionMatchedStatus,
        ) {
            if status.current_count() > 0 {
                // info!("Publisher matched! Sending start signal...");
                println!("Publisher matched! Sending start signal...");
                // Callback only sends signal and returns immediately
                // let _ = self.sender.try_send(true);
            } else {
                // info!("No Publishers. Sending stop signal...");
                println!("No Publishers. Sending stop signal...");
                // let _ = self.sender.try_send(false);
            }
        }
        fn on_data_available(
            &self,
            _reader: &int2dds::subscription::data_reader::DataReader<Self::Foo>,
        ) {
            // Send counter signal whenever data is available
            let _ = self.counter_sender.try_send(());
            println!("Data available! Counter signal sent.");
        }
    }

    struct PubKeyListener {
        sender: SyncSender<bool>,
    }

    impl DataWriterListener for PubKeyListener {
        type Foo = HelloWorldWithKey;
        fn on_publication_matched(
            &self,
            _writer: &int2dds::publication::data_writer::DataWriter<Self::Foo>,
            status: &int2dds::infrastructure::status::PublicationMatchedStatus,
        ) {
            if status.current_count() > 0 {
                println!("Subscriber matched! Sending start signal...");
                // Callback only sends signal and returns immediately
                let _ = self.sender.try_send(true);
            } else {
                println!("No subscribers. Sending stop signal...");
                let _ = self.sender.try_send(false);
            }
        }
    }

    #[test]
    fn test_read_instance_with_specific_handle() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeySpec",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();
        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        // Set up test data - different instances
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let _data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };

        let instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        // Call read_instance - read only instance_handle_1
        let samples = data_reader
            .read_instance(
                10,
                instance_handle_1,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to read instance samples");

        // Verify results - should return only 2 samples corresponding to instance_handle_1
        assert_eq!(samples.len(), 2);
        assert_eq!(samples[0].data().unwrap().index, data1.index);
        assert_eq!(samples[1].data().unwrap().index, data3.index);

        // All samples should have the same instance_handle
        for sample in &samples {
            assert_eq!(sample.sample_info().instance_handle, instance_handle_1);
        }
    }

    #[test]
    fn test_take_instance_with_specific_handle() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyHandle",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let _data3 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };

        let instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();
        // Call take_instance - take only instance_handle_2
        let samples = data_reader
            .take_instance(
                10,
                instance_handle_2,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to take instance samples");

        // Verify results - should return only 1 sample corresponding to instance_handle_2
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].data().unwrap().index, data2.index);
        assert_eq!(samples[0].sample_info().instance_handle, instance_handle_2);

        // Second take_instance call - NoData since instance_handle_2 was already taken
        let second_take = data_reader.take_instance(
            10,
            instance_handle_2,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );
        assert!(second_take.is_err());
        assert_eq!(second_take.unwrap_err(), DdsError::NoData);

        // instance_handle_1 should still be available
        let remaining_samples = data_reader
            .take_instance(
                10,
                instance_handle_1,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to take remaining instance samples");

        assert_eq!(remaining_samples.len(), 2);
    }

    #[test]
    fn test_read_instance_with_nil_handle() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKey",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };

        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };

        let _writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, _counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let _data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };

        // Call read_instance with NIL handle - should generate BAD_PARAMETER error
        let result = data_reader.read_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::BadParameter);
    }

    #[test]
    fn test_take_instance_with_nonexistent_handle() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyNon",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let _instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let nonexistent_handle = InstanceHandle::new([2; 16]); // Different handle

        // Call take_instance with non-existent handle - should generate NoData error
        let result = data_reader.take_instance(
            10,
            nonexistent_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::NoData);
    }

    #[test]
    fn test_read_instance_with_sample_state_filter() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeySample",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let _instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();

        // First read - NOT_READ state only
        let unread_samples = data_reader
            .read_instance(
                10,
                instance_handle_1,
                &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to read unread samples");

        assert_eq!(unread_samples.len(), 2);

        // Second read - READ state only (samples become read state after first read)
        let read_samples = data_reader
            .read_instance(
                10,
                instance_handle_1,
                &[SampleStateKind::READ_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .expect("Failed to read read samples");

        assert_eq!(read_samples.len(), 2);
    }

    #[test]
    fn test_read_next_instance_basic() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyBasic",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let _instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();
        let _instance_handle_3 = data_reader.lookup_instance(&data4).unwrap();
        let payload1 = data1.serialize().unwrap();
        let payload2 = data3.serialize().unwrap();
        let payload3 = data2.serialize().unwrap();

        // Start from the first instance
        let result = data_reader.read_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 2);
        println!("{:?}", samples[0].data().unwrap());
        println!("{:?}", samples[1].data().unwrap());
        assert_eq!(*samples[0].data().unwrap().serialize().unwrap(), *payload1);
        assert_eq!(*samples[1].data().unwrap().serialize().unwrap(), *payload2);

        // Read next instance
        let result = data_reader.read_next_instance(
            10,
            instance_handle_1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(*samples[0].data().unwrap().serialize().unwrap(), *payload3);
    }

    #[test]
    fn test_read_next_instance_no_next_instance() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyNoNext",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_3 = data_reader.lookup_instance(&data4).unwrap();

        // Request next instance after the last instance - should generate NoData error
        let result = data_reader.read_next_instance(
            10,
            instance_handle_3,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::NoData);
    }

    #[test]
    fn test_take_next_instance_basic() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKey",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let payload1 = data1.serialize().unwrap();
        let payload2 = data2.serialize().unwrap();
        let _instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let _instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();
        let instance_handle_3 = data_reader.lookup_instance(&data4).unwrap();

        // Take first instance
        let result = data_reader.take_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 2);
        assert_eq!(*samples[0].data().unwrap().serialize().unwrap(), *payload1);

        // Take next instance (first instance already removed)
        let result = data_reader.take_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(*samples[0].data().unwrap().serialize().unwrap(), *payload2);

        // Err since no data comes after data4
        let result = data_reader.take_next_instance(
            10,
            instance_handle_3,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), DdsError::NoData);
    }

    #[test]
    fn test_read_next_instance_with_view_state_filter() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyView",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let _data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let payload2 = data2.serialize().unwrap();
        let instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let _instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();
        let _instance_handle_3 = data_reader.lookup_instance(&data4).unwrap();

        let result = data_reader.read_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::NEW_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 2);

        let result = data_reader.read_next_instance(
            10,
            instance_handle_1,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::NEW_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 1);
        assert_eq!(*samples[0].data().unwrap().serialize().unwrap(), *payload2);
    }

    #[test]
    fn test_take_next_instance_with_multiple_samples_per_instance() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyMulti",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let _data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let payload4: Arc<[u8]> = data4.serialize().unwrap();
        let _instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let _instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();
        let _instance_handle_3 = data_reader.lookup_instance(&data4).unwrap();

        // Take all samples of the first instance (up to 10)
        let result = data_reader.take_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 2); // 2 samples of the first instance

        // Take next instance
        let result = data_reader.take_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 1); // 1 sample of the second instance

        // Take next instance
        let result = data_reader.take_next_instance(
            10,
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 1); // 1 sample of the third instance
        assert_eq!(*samples[0].data().unwrap().serialize().unwrap(), *payload4);
    }

    #[test]
    fn test_read_next_instance_with_nonexistent_previous_handle() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyNext",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let nonexistent_handle = InstanceHandle::new([99; 16]);

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let _instance_handle = data_reader.lookup_instance(&data1).unwrap();
        // Read next instance using non-existent previous handle
        let result = data_reader.read_next_instance(
            10,
            nonexistent_handle,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert_eq!(result.unwrap_err(), DdsError::NoData); // In current implementation, compares by instance_handle size..
    }

    #[test]
    fn test_take_next_instance_with_max_samples_limit() {
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
            .create_topic::<HelloWorldWithKey>(
                "HelloWorldWithKeyTakeMax",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let (sender, _receiver) = sync_channel(0);
        let _listener = PubKeyListener { sender };
        let writer_qos = DataWriterQos {
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender: counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                StatusMask::default(),
            )
            .unwrap();

        let mut condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let mut condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let instance_handle_1 = writer.register_instance(&data1).unwrap();
        let instance_handle_2 = writer.register_instance(&data2).unwrap();
        let instance_handle_3 = writer.register_instance(&data4).unwrap();

        writer.write(&data1, instance_handle_1).unwrap();
        writer.write(&data2, instance_handle_2).unwrap();
        writer.write(&data3, instance_handle_1).unwrap();
        writer.write(&data4, instance_handle_3).unwrap();

        let mut count = 0;
        while let Ok(_) = counter_receiver.recv() {
            count += 1;
            println!("Data received count: {}", count);
            if count == 4 {
                break;
            }
        }

        let data1 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey1".to_string() };
        let data2 = HelloWorldWithKey { index: 1, message: "HelloWorldWithKey2".to_string() };
        let _data3 = HelloWorldWithKey { index: 0, message: "HelloWorldWithKey3".to_string() };
        let data4 = HelloWorldWithKey { index: 2, message: "HelloWorldWithKey4".to_string() };
        let _instance_handle_1 = data_reader.lookup_instance(&data1).unwrap();
        let _instance_handle_2 = data_reader.lookup_instance(&data2).unwrap();
        let _instance_handle_3 = data_reader.lookup_instance(&data4).unwrap();

        // Take only up to 2 samples
        let result = data_reader.take_next_instance(
            1, // max_samples limit
            InstanceHandle::NIL,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        );

        assert!(result.is_ok());
        let samples = result.unwrap();
        assert_eq!(samples.len(), 1); // Return only 1 out of 2
    }
    //

    #[test]
    fn test_delete_contained_entities_reader() {
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
        let reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
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

        reader.delete_contained_entities().unwrap();

        assert!(reader.get_readconditions().unwrap().is_empty());
    }

    #[test]
    fn test_delete_reader() {
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
            .create_topic::<HelloWorld>(
                "hello_world",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        {
            let rtps_reader = reader.get_rtps_reader();
            assert!(rtps_reader.is_ok());
        }
        let mut dcps_bridge = domain_participant.get_dcps_bridge().unwrap();
        let dcps_bridge = dcps_bridge.as_mut().unwrap();
        dcps_bridge
            .delete_rtps_reader(
                "hello_world".to_string(),
                reader.get_instance_handle().unwrap().to_guid().entity_id(),
            )
            .unwrap();
        let rtps_reader = reader.get_rtps_reader();
        assert!(rtps_reader.is_err());
    }

    #[test]
    fn test_data_available_handle_with_listener() {
        struct SubscriberListenerStructure {
            _sender: SyncSender<()>,
        }

        impl SubscriberListener for SubscriberListenerStructure {
            fn on_data_available(&self, the_reader: &dyn DataReaderBase<Qos = DataReaderQos>) {
                if let Some(typed_reader) =
                    (the_reader as &dyn Any).downcast_ref::<DataReader<HelloWorld>>()
                {
                    match typed_reader.take(
                        10,
                        &[SampleStateKind::ANY_SAMPLE_STATE],
                        &[ViewStateKind::ANY_VIEW_STATE],
                        &[InstanceStateKind::ANY_INSTANCE_STATE],
                    ) {
                        Ok(samples) => {
                            for sample in samples.iter() {
                                let sample = sample.data().unwrap();
                                log::info!("Read sample: {:?}", sample);
                            }
                        }
                        Err(e) => {
                            log::error!("Error reading samples: {:?}", e);
                        }
                    }
                }
            }

            fn on_data_on_readers(&self, subscriber: &Subscriber) {
                log::info!(
                    "Get DataOnReaders Status - Guid {:?}",
                    subscriber.get_instance_handle().unwrap().to_guid()
                );
            }
        }

        let (sender, _receiver) = sync_channel(0);
        let listener = SubscriberListenerStructure { _sender: sender };

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
            .create_topic::<HelloWorld>(
                "hello_world_topic_sub",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(
                SubscriberQos::default(),
                Some(Arc::new(listener)),
                StatusMask::default(),
            )
            .unwrap();
        let _reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        std::thread::sleep(std::time::Duration::from_secs(10));
    }
}
