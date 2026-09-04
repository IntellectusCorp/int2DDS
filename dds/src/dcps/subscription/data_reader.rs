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

use arc_swap::ArcSwap;
use bytes::Bytes;
use std::{
    any::{Any, TypeId},
    cmp::Ordering as CmpOrdering,
    collections::{BTreeSet, HashMap, HashSet},
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
        time::{Duration, Time},
    },
    infrastructure::{
        deadline_monitor::DeadlineMonitor,
        domain_entity::DomainEntity,
        entity::{
            impl_check_parent_enabled, impl_dds_entity, impl_dds_entity_impl, BaseEntity,
            EnableChild, Entity, EntityInternal, EntityLifecycle, UpdateStatus,
        },
        history_cache::HistoryCache as DcpsHistoryCache,
        qos_policy::Qos,
        status::{
            LivelinessChangedStatus, RequestedDeadlineMissedStatus, RequestedIncompatibleQosStatus,
            RequestedIncompatibleTypeStatus, SampleLostStatus, SampleRejectedStatus, StatusInfo,
            StatusKind, StatusMask, SubscriptionMatchedStatus,
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
        logic::wlp_logic::LivelinessTransition,
    },
    subscription::{
        data_reader_history::{DataReaderHistoryCache, ReaderChangeId},
        data_sample::{DataSample, SamplePayload},
        read_condition::ReadConditionTrait,
        sample_info::{InstanceInfo, SampleInfo, StateMaskExt},
    },
    topic::{
        content_filtered_topic::ContentFilteredTopic,
        topic::Topic,
        topic_description::TopicDescription,
        type_support::{DdsType, TypeSupport},
    },
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
};

pub enum BoundedSerialized {
    Fit(Bytes, SampleInfo),
    TooSmall { required: usize },
}

// Pub/Sub must contain multiple types of DataWriter/Reader<Foo>,
// so we use trait objects for runtime polymorphism instead of generics
pub trait DataReaderBase: DomainEntity + Send + Any {
    fn get_liveliness_changed_status(&self) -> DdsResult<LivelinessChangedStatus>;
    fn get_sample_rejected_status(&self) -> DdsResult<SampleRejectedStatus>;
    fn get_sample_lost_status(&self) -> DdsResult<SampleLostStatus>;
    fn get_requested_deadline_missed_status(&self) -> DdsResult<RequestedDeadlineMissedStatus>;
    fn get_requested_incompatible_qos_status(&self) -> DdsResult<RequestedIncompatibleQosStatus>;
    fn get_requested_incompatible_type_status(&self) -> DdsResult<RequestedIncompatibleTypeStatus>;
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
    fn mark_deleted_and_await_operation_completion(&self);
    fn is_builtin(&self) -> bool;
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
    // Indicates whether this entity is a built-in entity.
    //
    // Built-in entities are managed internally and have restricted operations:
    // - Cannot be deleted (delete_datareader)
    // - Cannot modify QoS (set_qos)
    //
    // See also: DomainParticipant::get_builtin_subscriber()
    is_builtin: bool,
    guid: Guid,
    qos: Arc<ArcSwap<DataReaderQos>>,
    update_lock: Arc<Mutex<()>>,
    listener: Arc<RwLock<Option<Arc<dyn DataReaderListener<Foo = Foo>>>>>,
    mask: Arc<RwLock<StatusMask>>,
    pub status_condition: Arc<Mutex<StatusCondition<DataReaderQos>>>,
    read_conditions: Arc<Mutex<Vec<Arc<dyn ReadConditionTrait + Send + Sync>>>>,
    pub(crate) self_ref: Arc<Mutex<Option<Arc<DataReader<Foo>>>>>,
    enabled: Arc<AtomicBool>,
    lifecycle: Arc<EntityLifecycle>,
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
    requested_incompatible_type_status: Arc<Mutex<RequestedIncompatibleTypeStatus>>,
    subscription_matched_status: Arc<Mutex<SubscriptionMatchedStatus>>,
    sample_lost_status: Arc<Mutex<SampleLostStatus>>,
    deadline_monitor: Arc<Mutex<Option<DeadlineMonitor>>>,
    change_callback: Option<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>>,
    #[allow(clippy::type_complexity)]
    status_callback: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
    _phantom: PhantomData<fn() -> Foo>, // Temporary
    datareader_cache: Arc<Mutex<DataReaderHistoryCache<Foo>>>,
}

impl<Foo> Debug for DataReader<Foo> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataReader")
            .field("guid", &self.guid)
            .field("qos", &**self.qos.load())
            .field(
                "listener",
                &self.listener.read().unwrap().as_ref().map(|_| "Arc<dyn DataReaderListener>"),
            )
            .field("mask", &self.mask.read().unwrap())
            .field("status_condition", &self.status_condition.lock().unwrap())
            .field("self_ref", &self.self_ref.lock().unwrap().as_ref().map(|_| "Arc<DataReader>"))
            .field("enabled", &self.enabled.load(std::sync::atomic::Ordering::Acquire))
            .field("deleted", &self.lifecycle.is_deleted().is_err())
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
            .field(
                "requested_incompatible_type_status",
                &self.requested_incompatible_type_status.lock().unwrap(),
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
            is_builtin: self.is_builtin,
            guid: self.guid,
            qos: self.qos.clone(),
            update_lock: self.update_lock.clone(),
            listener: self.listener.clone(),
            mask: self.mask.clone(),
            status_condition: self.status_condition.clone(),
            read_conditions: self.read_conditions.clone(),
            self_ref: self.self_ref.clone(),
            enabled: self.enabled.clone(),
            lifecycle: self.lifecycle.clone(),
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
            requested_incompatible_type_status: self.requested_incompatible_type_status.clone(),
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
    #[allow(clippy::match_result_ok)]
    fn drop(&mut self) {
        // Builtin entities are managed separately, skip orphan handling
        if self.is_builtin {
            return;
        }

        // Only handle drop for the last reference (not clones)
        if let Some(guard) = self.self_ref.lock().ok() {
            if let Some(self_arc) = guard.as_ref() {
                if Arc::strong_count(self_arc) > 1 {
                    return;
                }
            }
        } else {
            return; // Never fully initialized
        }

        if self.lifecycle.is_deleted().is_ok() {
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
        let rtps_reader = if self.is_builtin {
            // Builtin: RTPS reader was already set during creation
            self.get_rtps_reader()?
        } else {
            // Non-builtin: Create RTPS reader
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
            let topic_qos = if let Some(topic) = topic_description.as_any().downcast_ref::<Topic>()
            {
                topic.get_qos_arc()?
            } else if let Some(content_filtered_topic) = cft {
                content_filtered_topic.get_related_topic()?.get_qos_arc()?
            } else {
                return Err(DdsError::Error("Unsupported TopicDescription type".to_string()));
            };

            let topic_name = if let Some(topic) = topic_description.as_any().downcast_ref::<Topic>()
            {
                topic.get_name().to_string()
            } else if let Some(content_filtered_topic) = cft {
                content_filtered_topic.get_related_topic()?.get_name().to_string()
            } else {
                return Err(DdsError::Error("Unsupported TopicDescription type".to_string()));
            };

            let reader_qos = self.get_qos_arc()?;
            let subscriber_qos = subscriber.get_qos_arc()?;
            let mut subscription_builtin_topic_data =
                SubscriptionBuiltinTopicData::new(&reader_qos, &subscriber_qos, &topic_qos);
            subscription_builtin_topic_data.set_topic_name(topic_name);
            subscription_builtin_topic_data
                .set_type_name(topic_description.get_type_name().to_string());
            subscription_builtin_topic_data.set_endpoint_guid(self.guid);

            // Set TypeIdentifier and TypeObject for DDS-XTypes discovery
            if let Some(type_id) = self.type_support.get_type_identifier() {
                subscription_builtin_topic_data.set_type_identifier(Some(type_id));
            }
            if let Some(type_obj) = self.type_support.get_type_object() {
                subscription_builtin_topic_data.set_type_object(Some(type_obj));
            }
            let type_closure = self.type_support.get_type_object_closure();
            subscription_builtin_topic_data
                .set_type_information(crate::xtypes::TypeInformation::from_closure(&type_closure));
            if let Ok(rtps_participant) = participant.get_rtps_participant() {
                rtps_participant.register_local_type_objects(&type_closure);
            }

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

            // Store RTPS reader reference
            *self.rtps_reader.lock().map_err(|e| DdsError::Error(e.to_string()))? =
                Some(Arc::downgrade(&rtps_reader));

            rtps_reader
        };

        // Common: Connect datareader cache to RTPS reader cache
        {
            let reader_cache = rtps_reader.reader_cache();
            reader_cache
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?
                .set_datareader_cache(Arc::downgrade(&self.datareader_cache)
                    as Weak<Mutex<dyn DcpsHistoryCache + Send + Sync>>);
        }

        Ok(())
    }
    fn update_rtps_entity(&self, qos: &Self::Qos) -> DdsResult<()> {
        let subscriber = self.get_subscriber()?;
        let topic = self.get_topic()?;
        let subscriber_qos = subscriber.get_qos_arc()?;
        let topic_qos = topic.get_qos_arc()?;
        let mut subscription_builtin_topic_data =
            SubscriptionBuiltinTopicData::new(qos, &subscriber_qos, &topic_qos);
        subscription_builtin_topic_data.set_topic_name(topic.get_name().to_string());
        subscription_builtin_topic_data.set_type_name(topic.get_type_name().to_string());
        subscription_builtin_topic_data.set_endpoint_guid(self.guid);

        // Set TypeIdentifier and TypeObject for DDS-XTypes discovery
        if let Some(type_id) = self.type_support.get_type_identifier() {
            subscription_builtin_topic_data.set_type_identifier(Some(type_id));
        }
        if let Some(type_obj) = self.type_support.get_type_object() {
            subscription_builtin_topic_data.set_type_object(Some(type_obj));
        }
        subscription_builtin_topic_data.set_type_information(
            crate::xtypes::TypeInformation::from_closure(
                &self.type_support.get_type_object_closure(),
            ),
        );

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
            StatusKind::REQUESTED_INCOMPATIBLE_TYPE => {
                let info = Arc::downcast::<RequestedIncompatibleTypeStatus>(
                    info.ok_or(DdsError::BadParameter)?,
                )
                .map_err(|_| DdsError::BadParameter)?;
                self.handle_requested_incompatible_type_status(info)
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

                let transition = LivelinessTransition::from_deltas(
                    info.alive_count_change(),
                    info.not_alive_count_change(),
                );

                // Lost / UnmatchAlive / UnmatchNotAlive.
                if matches!(
                    transition,
                    Some(
                        LivelinessTransition::Lost
                            | LivelinessTransition::UnmatchAlive
                            | LivelinessTransition::UnmatchNotAlive
                    )
                ) {
                    if let Ok(datareader_cache) = self.datareader_cache.lock() {
                        datareader_cache.revoke_writer_ownership(
                            info.last_publication_handle().to_guid(),
                            None,
                            true,
                            true,
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
    /// # use int2dds::domain::qos::DomainParticipantQos;
    /// # use int2dds::infrastructure::status::StatusMask;
    /// # use int2dds::topic::qos::TopicQos;
    /// # use int2dds::subscription::qos::{SubscriberQos, DataReaderQos};
    /// # use int2dds::topic::type_support::DdsType;
    /// # use int2dds::subscription::sample_info::SampleStateKind;
    /// # use int2dds::subscription::sample_info::ViewStateKind;
    /// # use int2dds::subscription::sample_info::InstanceStateKind;
    /// # use int2dds::core::types::LENGTH_UNLIMITED;
    /// # #[derive(DdsType)]
    /// # struct MyData { #[dds(key)] id: u32, message: String }
    /// # let factory = DomainParticipantFactory::get_instance();
    /// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
    /// # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", TopicQos::default(), None, StatusMask::default()).unwrap();
    /// # let subscriber = participant.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
    /// # let reader = subscriber.create_datareader::<MyData>(&topic, DataReaderQos::default(), None, StatusMask::default()).unwrap();
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
        let _operation = self.lifecycle.begin_operation()?;
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

    /// Get the raw serialized key bytes for a given instance handle.
    ///
    /// Serialized counterpart of [`get_key_value`](Self::get_key_value) for the FFI
    /// raw-serialized path, mirroring the writer's `get_key_value_serialized`. For
    /// native (derive) types this returns the CDR-serialized key; for raw FFI types
    /// (which cannot deserialize/re-serialize a key) it returns the stored instance
    /// handle bytes, which still round-trip through `lookup_instance_serialized`.
    pub fn get_key_value_serialized(&self, handle: InstanceHandle) -> DdsResult<Arc<[u8]>> {
        let _operation = self.lifecycle.begin_operation()?;
        self.is_enabled()?;

        if handle.is_nil() {
            return Err(DdsError::BadParameter);
        }

        let instance_info = self.get_instance_infos()?;
        if let Some(info) = instance_info.get(&handle) {
            if info.key.is_empty() {
                return Err(DdsError::Error("Unknown Key".to_string()));
            }
            Ok(info.key.clone())
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
    /// # use int2dds::domain::qos::DomainParticipantQos;
    /// # use int2dds::infrastructure::status::StatusMask;
    /// # use int2dds::topic::qos::TopicQos;
    /// # use int2dds::subscription::qos::{SubscriberQos, DataReaderQos};
    /// # use int2dds::topic::type_support::DdsType;
    /// # use int2dds::common::instance_handle::InstanceHandle;
    /// # #[derive(DdsType)]
    /// # struct MyData { #[dds(key)] id: u32, message: String }
    /// # let factory = DomainParticipantFactory::get_instance();
    /// # let participant = factory.create_participant(0, DomainParticipantQos::default(), None, StatusMask::default()).unwrap();
    /// # let topic = participant.create_topic::<MyData>("MyTopic", "MyData", TopicQos::default(), None, StatusMask::default()).unwrap();
    /// # let subscriber = participant.create_subscriber(SubscriberQos::default(), None, StatusMask::default()).unwrap();
    /// # let reader = subscriber.create_datareader::<MyData>(&topic, DataReaderQos::default(), None, StatusMask::default()).unwrap();
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
        let _operation = self.lifecycle.begin_operation()?;
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

    /// Look up an instance handle from the stored serialized key bytes.
    ///
    /// Serialized counterpart of [`lookup_instance`](Self::lookup_instance) for the FFI
    /// raw-serialized path. The reader stores each instance's canonical key CDR (via the
    /// type support's `serialize_key`, the same projection the wire uses), so this matches
    /// on the stored key bytes — as returned by [`get_key_value_serialized`](Self::get_key_value_serialized).
    /// Returns `InstanceHandle::NIL` if the instance is not known to this reader.
    pub fn lookup_instance_serialized(&self, key: &[u8]) -> DdsResult<InstanceHandle> {
        let _operation = self.lifecycle.begin_operation()?;

        if key.is_empty() {
            return Ok(InstanceHandle::NIL);
        }

        let instances = self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for (handle, info) in instances.iter() {
            if info.key.as_ref() == key {
                return Ok(*handle);
            }
        }

        Ok(InstanceHandle::NIL)
    }

    // For Entity
    pub fn set_listener(
        &self,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<()> {
        let _operation = self.lifecycle.begin_operation()?;
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
        let _operation = self.lifecycle.begin_operation()?;
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
    pub fn get_requested_incompatible_type_status(
        &self,
    ) -> DdsResult<RequestedIncompatibleTypeStatus> {
        <Self as DataReaderBase>::get_requested_incompatible_type_status(self)
    }

    #[inline]
    pub fn get_subscription_matched_status(&self) -> DdsResult<SubscriptionMatchedStatus> {
        <Self as DataReaderBase>::get_subscription_matched_status(self)
    }

    /// Returns the 16-byte RTPS GUID of this DataReader. This is the same endpoint
    /// GUID advertised over SEDP discovery (the `endpoint_guid` of this reader's
    /// `SubscriptionBuiltinTopicData`). Read-only accessor; mirrors `DataWriter::guid`.
    pub fn guid(&self) -> Guid {
        self.guid
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

        // No count cap here. Evicting by count cannot tell a still-cached sample from one the
        // cache has dropped, and evicting a cached one flips it back to NOT_READ so the next
        // `read()` hands it out a second time. `prune_read_samples_to_cache` bounds this map
        // against the cache instead, which is the only cut that is unobservable.
        Ok(())
    }

    /// Drops read-state for samples the cache no longer holds.
    ///
    /// `read_samples` exists so a second `read()` can report `READ_SAMPLE_STATE`. Its only
    /// bound was `get_max_samples`, which resolved to `usize::MAX` under the default
    /// `LENGTH_UNLIMITED` limits, so the map grew for the reader's lifetime.
    ///
    /// The bound has to come from the cache rather than a count. `get_sample_state` answers
    /// `READ` both for a sample recorded here and for a sample that is no longer cached, so
    /// entries below the cache's low-water mark cannot change any answer -- dropping them is
    /// unobservable. Dropping an entry whose sample is *still* cached is not: it would report
    /// `NOT_READ` again and hand the sample out a second time.
    ///
    /// `changes` is the snapshot the caller already holds, so this needs no extra cache lock,
    /// and being a pre-removal snapshot only makes the mark conservative.
    fn prune_read_samples_to_cache(&self, changes: &[Arc<CacheChange>]) {
        let Ok(mut read_samples) = self.read_samples.lock() else { return };

        // Only `read()` records anything here, so a reader that only ever `take()`s -- the
        // common case -- has nothing to prune. Bailing before the map is built keeps this off
        // that path entirely rather than hashing a Guid per cached change for an empty result.
        if read_samples.is_empty() {
            return;
        }

        let mut low_water: HashMap<Guid, SequenceNumber> = HashMap::new();
        for change in changes {
            low_water
                .entry(change.writer_guid())
                .and_modify(|lowest| {
                    if change.sequence_number() < *lowest {
                        *lowest = change.sequence_number();
                    }
                })
                .or_insert_with(|| change.sequence_number());
        }

        read_samples.retain(|writer_guid, seen| {
            match low_water.get(writer_guid) {
                // Every sample this writer had has left the cache, so nothing recorded for it
                // is reachable any more. Dropping the entry also bounds the outer map, which
                // otherwise kept one entry per writer GUID ever matched.
                None => false,
                // `split_off` allocates a fresh set and moves every retained element, so in the
                // steady state -- nothing below the mark -- it would do maximal work for no
                // benefit. Only pay it when something is actually prunable.
                Some(lowest) => {
                    if seen.first().is_some_and(|oldest| oldest < lowest) {
                        *seen = seen.split_off(lowest);
                    }
                    !seen.is_empty()
                }
            }
        });
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

    #[allow(clippy::type_complexity)]
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
    fn take_requested_incompatible_type_status(
        &self,
    ) -> DdsResult<RequestedIncompatibleTypeStatus> {
        let mut status_guard = self
            .requested_incompatible_type_status
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
        // StatusCondition. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change instead of it being published only after
        // the listener has already consumed and cleared the status.
        self.set_communication_status_propagation(&StatusKind::REQUESTED_DEADLINE_MISSED, true)?;

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
                // DDS 1.4 2.2.4.1: the StatusChangedFlag is reset to FALSE when the
                // listener returns. Only this entity's flag -- the Subscriber and
                // DomainParticipant own theirs and may still hold unconsumed events
                // from sibling readers.
                self.set_communication_status(&StatusKind::REQUESTED_DEADLINE_MISSED, false)?;
            }
        }

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
        // StatusCondition. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change instead of it being published only after
        // the listener has already consumed and cleared the status.
        self.set_communication_status_propagation(&StatusKind::REQUESTED_INCOMPATIBLE_QOS, true)?;

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
                // DDS 1.4 2.2.4.1: the StatusChangedFlag is reset to FALSE when the
                // listener returns. Only this entity's flag -- the Subscriber and
                // DomainParticipant own theirs and may still hold unconsumed events
                // from sibling readers.
                self.set_communication_status(&StatusKind::REQUESTED_INCOMPATIBLE_QOS, false)?;
            }
        }

        Ok(())
    }
    fn handle_requested_incompatible_type_status(
        &self,
        _info: Arc<RequestedIncompatibleTypeStatus>,
    ) -> DdsResult<()> {
        {
            let mut status_guard = self
                .requested_incompatible_type_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::REQUESTED_INCOMPATIBLE_TYPE, true)?;

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
        // StatusCondition. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change instead of it being published only after
        // the listener has already consumed and cleared the status.
        self.set_communication_status_propagation(&StatusKind::SAMPLE_LOST, true)?;

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
                // DDS 1.4 2.2.4.1: the StatusChangedFlag is reset to FALSE when the
                // listener returns. Only this entity's flag -- the Subscriber and
                // DomainParticipant own theirs and may still hold unconsumed events
                // from sibling readers.
                self.set_communication_status(&StatusKind::SAMPLE_LOST, false)?;
            }
        }

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
        // StatusCondition. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change instead of it being published only after
        // the listener has already consumed and cleared the status.
        self.set_communication_status_propagation(&StatusKind::SAMPLE_REJECTED, true)?;

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
                // DDS 1.4 2.2.4.1: the StatusChangedFlag is reset to FALSE when the
                // listener returns. Only this entity's flag -- the Subscriber and
                // DomainParticipant own theirs and may still hold unconsumed events
                // from sibling readers.
                self.set_communication_status(&StatusKind::SAMPLE_REJECTED, false)?;
            }
        }

        Ok(())
    }
    fn handle_data_available_status(&self) -> DdsResult<()> {
        // StatusCondition first. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change. Publishing it after the listeners also made the
        // flag hostage to them completing -- a listener that panics is contained at the
        // callback boundary, and the waiter would then block forever on a reader whose samples
        // are sitting in the cache. `handle_liveliness_changed_status` uses this same order.
        self.set_read_communication_status(true)?;

        // Listener
        if let Some(listener) = self.get_listener()? {
            listener.on_data_available(self);
        }
        let subscriber = self.subscriber_arc()?;
        if let Some(listener) = subscriber.get_listener()? {
            listener.on_data_available(self);
            listener.on_data_on_readers(&subscriber);
        }
        let participant = subscriber.participant_arc()?;
        if let Some(listener) = participant.get_listener()? {
            listener.on_data_available(self);
            listener.on_data_on_readers(&subscriber);
        }

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
        // StatusCondition. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change instead of it being published only after
        // the listener has already consumed and cleared the status.
        self.set_communication_status_propagation(&StatusKind::LIVELINESS_CHANGED, true)?;

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
                // DDS 1.4 2.2.4.1: the StatusChangedFlag is reset to FALSE when the
                // listener returns. Only this entity's flag -- the Subscriber and
                // DomainParticipant own theirs and may still hold unconsumed events
                // from sibling readers.
                self.set_communication_status(&StatusKind::LIVELINESS_CHANGED, false)?;
            }
        }

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
        // StatusCondition. DDS 1.4 2.2.4.1: the flag becomes TRUE when the status
        // changes, which is before any listener runs, so a thread already blocked in
        // WaitSet::wait observes the change instead of it being published only after
        // the listener has already consumed and cleared the status.
        self.set_communication_status_propagation(&StatusKind::SUBSCRIPTION_MATCHED, true)?;

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
                // DDS 1.4 2.2.4.1: the StatusChangedFlag is reset to FALSE when the
                // listener returns. Only this entity's flag -- the Subscriber and
                // DomainParticipant own theirs and may still hold unconsumed events
                // from sibling readers.
                self.set_communication_status(&StatusKind::SUBSCRIPTION_MATCHED, false)?;
            }
        }

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
        let subscriber = self.subscriber_arc()?;
        subscriber.set_communication_status(status_kind, trigger_value)?;
        // 3. DomainParticipant StatusCondition
        subscriber.participant_arc()?.set_communication_status(status_kind, trigger_value)?;

        Ok(())
    }

    fn set_read_communication_status(&self, trigger_value: bool) -> DdsResult<()> {
        // 1. DataReader StatusCondition
        self.set_communication_status(&StatusKind::DATA_AVAILABLE, trigger_value)?;

        // 2. Subscriber and 3. DomainParticipant StatusConditions.
        //
        // `Entity::set_communication_status` reaches the condition through `get_statuscondition`,
        // which takes a mutex and clones a 4-`Arc` `StatusCondition` just to run one atomic bit
        // update. The subscriber and the participant each take two status kinds, so calling it
        // once per kind pays that lock-and-clone twice for the very same condition. Hoisting one
        // clone per entity turns five lock+clone pairs per sample into three -- and every reader
        // under the participant does this on every sample it delivers.
        //
        // The two kinds stay two separate calls on purpose. `add_communication_status` fires the
        // WaitSet callback only when `enabled_statuses` contains *every* bit of its argument
        // (`StatusMask::contains` is all-of), so folding the kinds into one mask would silently
        // stop waking a condition that enabled only one of them.
        let subscriber = self.subscriber_arc()?;
        let subscriber_condition = subscriber.get_statuscondition()?;
        subscriber_condition
            .set_communication_status(&StatusKind::DATA_ON_READERS, trigger_value)?;
        subscriber_condition
            .set_communication_status(&StatusKind::DATA_AVAILABLE, trigger_value)?;

        let participant = subscriber.participant_arc()?;
        let participant_condition = participant.get_statuscondition()?;
        participant_condition
            .set_communication_status(&StatusKind::DATA_ON_READERS, trigger_value)?;
        participant_condition
            .set_communication_status(&StatusKind::DATA_AVAILABLE, trigger_value)?;

        Ok(())
    }

    pub(crate) fn get_datareader_cache(
        &self,
    ) -> DdsResult<Arc<Mutex<DataReaderHistoryCache<Foo>>>> {
        Ok(self.datareader_cache.clone())
    }

    pub(crate) fn get_available_changes(&self) -> DdsResult<Vec<Arc<CacheChange>>> {
        let topic_ordered = self.subscriber_topic_ordered();
        let datareader_cache = self.get_datareader_cache();
        let arc = datareader_cache?;
        let mut guard = arc.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        // Enforce Lifespan QoS at read time so expired samples are never returned,
        // even if the periodic cleanup timer hasn't fired yet.
        guard.purge_expired_on_read()?;
        // TOPIC ordered_access presents samples in topic-wide DESTINATION_ORDER across instances;
        // otherwise return the per-instance storage order.
        if topic_ordered {
            Ok(guard.get_changes_for_topic_scoped_ordered_access())
        } else {
            Ok(guard.get_changes())
        }
    }

    /// Upgraded parent handle without the deep clone [`DataReaderBase::get_subscriber`] performs.
    ///
    /// Same failure mode and message -- `Error` when the parent `Weak` has expired -- but two
    /// atomic read-modify-writes instead of ~28. Needs no drop guard, unlike the participant
    /// equivalent: the value inside the `Arc` has `self_ref: None`, so `Drop for Subscriber`
    /// early-returns either way.
    ///
    /// Only for internal call sites that never read `Subscriber::self_ref`.
    pub(crate) fn subscriber_arc(&self) -> DdsResult<Arc<Subscriber>> {
        self.subscriber.as_ref().and_then(|weak_ref| weak_ref.upgrade()).ok_or_else(|| {
            DdsError::Error("Subscriber reference is invalid or expired".to_string())
        })
    }

    fn subscriber_topic_ordered(&self) -> bool {
        self.subscriber_arc().and_then(|s| s.presentation_topic_ordered()).unwrap_or(false)
    }

    pub(crate) fn is_subscriber_coherent(&self) -> bool {
        self.subscriber_arc().and_then(|s| s.presentation_coherent_access()).unwrap_or(false)
    }

    pub fn has_cached_data(&self) -> DdsResult<bool> {
        let cache = self.get_datareader_cache()?;
        let mut guard = cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        // Expired samples must not count as available data.
        guard.purge_expired_on_read()?;

        Ok(guard.has_changes())
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

    /// Removes all changes of the given instance from both DataReader and RTPS reader caches.
    /// This acquires both cache locks, so avoid calling from RTPS Reader contexts to prevent deadlock.
    pub(crate) fn remove_change_of_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        let change_id_set_to_remove =
            if let Ok(mut datareader_cache) = self.get_datareader_cache()?.lock() {
                let ids = datareader_cache.get_change_id_set_of_instance(instance_handle)?;
                datareader_cache.remove_all_changes_of_instance(instance_handle)?;
                ids
            } else {
                HashSet::new()
            };

        self.remove_change_from_rtps_reader_cache_by_id_set(change_id_set_to_remove)?;
        Ok(())
    }

    // Remove the SampleInfo bookkeeping for an instance.
    pub(crate) fn remove_instance_info(&self, instance_handle: InstanceHandle) {
        if let Ok(mut instance_infos) = self.instance_infos.lock() {
            instance_infos.remove(&instance_handle);
        }
    }

    // Fully reclaim an instance after its NOT_ALIVE_NO_WRITERS autopurge delay: drop its
    // samples and all per-instance state, so future samples are treated as a new instance.
    pub(crate) fn reclaim_instance(&self, instance_handle: InstanceHandle) -> DdsResult<()> {
        self.remove_change_of_instance(instance_handle)?;
        if let Ok(cache) = self.get_datareader_cache()?.lock() {
            cache.remove_all_instance_resources(instance_handle);
        }
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
                Ok(())
            } else {
                Err(DdsError::Error(res.err().unwrap().to_string()))
            }
        } else {
            Err(DdsError::Error("RTPS Reader is not initialized".to_string()))
        }
    }

    /// Removes changes by the given change IDs from RTPS reader cache.
    /// This should be only called when removing changes from data reader side to rtps reader side to avoid deadlock.
    fn remove_change_from_rtps_reader_cache_by_id_set(
        &self,
        change_id_set: HashSet<ReaderChangeId>,
    ) -> DdsResult<()> {
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
            let res = cache_guard.remove_change_by_id_set(change_id_set);
            if res.is_ok() {
                Ok(())
            } else {
                Err(DdsError::Error(res.err().unwrap().to_string()))
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

        if !instance_handle.is_nil() || !self.type_support.is_compute_key_provided() {
            return Ok(instance_handle);
        }

        // Dispose/unregister/filtered samples carry a key-only payload; only Alive
        // (and AliveFiltered) samples carry the full data record.
        if matches!(change.kind(), ChangeKind::Alive | ChangeKind::AliveFiltered) {
            let data = self.type_support.deserialize(change.data_value(), None)?;
            Ok(self.type_support.compute_key(&*data))
        } else {
            // Dispose/unregister carry a wire serializedKey (with encapsulation header).
            let key_any = self.type_support.deserialize_key_payload(change.data_value())?;
            Ok(self.type_support.compute_key(&*key_any))
        }
    }

    // Mark synthetic invalid-data sample pending.
    pub(crate) fn mark_pending_notification(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        let mut instance_infos =
            self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(info) = instance_infos.get_mut(&instance_handle) {
            info.pending_notification = true;
        }
        Ok(())
    }

    // Clear pending_notification after synthetic sample emitted.
    pub(crate) fn clear_pending_notification(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        let mut instance_infos =
            self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(info) = instance_infos.get_mut(&instance_handle) {
            info.pending_notification = false;
        }
        Ok(())
    }

    // Drain pending synthetic notifications matching the filters; clears flags.
    fn drain_pending_notifications(
        &self,
        instance_infos: &HashMap<InstanceHandle, InstanceInfo>,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        instance_filter: Option<InstanceHandle>,
        max: i32,
    ) -> DdsResult<Vec<SampleInfo>> {
        let mut synthetic_sample_infos = Vec::new();

        if max <= 0 || !sample_states.matches(SampleStateKind::NOT_READ_SAMPLE_STATE) {
            return Ok(synthetic_sample_infos);
        }

        for (instance_handle, info) in instance_infos.iter() {
            if synthetic_sample_infos.len() as i32 >= max {
                break;
            }

            if !info.pending_notification {
                continue;
            }

            if matches!(instance_filter, Some(expected) if expected != *instance_handle) {
                continue;
            }

            if !view_states.matches(info.view_state)
                || !instance_states.matches(info.instance_state)
            {
                continue;
            }

            // Suppress synthetic on rebirth; only surface while still NOT_ALIVE_NO_WRITERS.
            if info.instance_state != InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE {
                continue;
            }

            synthetic_sample_infos.push(SampleInfo {
                sample_state: SampleStateKind::NOT_READ_SAMPLE_STATE,
                view_state: info.view_state,
                instance_state: info.instance_state,
                disposed_generation_count: info.disposed_generation_count,
                no_writers_generation_count: info.no_writers_generation_count,
                sample_rank: 0,
                generation_rank: 0,
                absolute_generation_rank: 0,
                source_timestamp: Time::now(),
                instance_handle: *instance_handle,
                publication_handle: InstanceHandle::NIL,
                valid_data: false,
            });
            self.clear_pending_notification(*instance_handle)?;
        }
        Ok(synthetic_sample_infos)
    }

    // Returns true if the instance state actually changed (a rejected no-op
    // transition returns false), so callers can avoid synthesizing notifications.
    pub(crate) fn update_instance_state(
        &self,
        instance_handle: InstanceHandle,
        new_state: InstanceStateKind,
        cache_change: Option<&CacheChange>,
    ) -> DdsResult<bool> {
        // Non-keyed topic doesn't have instance state
        // if instance_handle.is_nil() {
        //     return Err(DdsError::BadParameter);
        // }

        let mut instance_infos =
            self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let info = instance_infos.entry(instance_handle).or_insert_with(|| InstanceInfo {
            key: Arc::new([]),
            view_state: ViewStateKind::NEW_VIEW_STATE,
            instance_state: InstanceStateKind::ALIVE_INSTANCE_STATE,
            disposed_generation_count: 0,
            no_writers_generation_count: 0,
            pending_notification: false,
        });

        let prev_state = info.instance_state;

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
                    // Drop pending synthetic; rebirth supersedes the prior transition.
                    info.pending_notification = false;
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

                if info.key.is_empty() && instance_handle.is_nil() {
                    // Non-keyed Type: Save NIL handle
                    info.key = Arc::from(instance_handle.value().as_slice());
                } else if info.key.is_empty() && cache_change.is_some() {
                    log::debug!("Extracting and serializing key from data");
                    let data = self.type_support.deserialize(
                        cache_change
                            .as_ref()
                            .ok_or(DdsError::Error(
                                "CacheChange is not properly initialized".to_string(),
                            ))?
                            .data_value(),
                        None,
                    );
                    match data {
                        Ok(deserialized) => {
                            // Native Type : deserialize success → serialize_key
                            #[allow(clippy::disallowed_names)]
                            let foo = deserialized
                                .downcast::<Foo>()
                                .map(|boxed| *boxed)
                                .map_err(|_| DdsError::Error("Type downcast failed".to_string()))?;
                            log::trace!("Deserialized data: {:?}", &foo);
                            let ser_key = self.type_support.serialize_key(&foo as &dyn Any)?;
                            info.key = ser_key;
                        }
                        Err(_) if !instance_handle.is_nil() => {
                            // FFI Type: deserialize not supported → Use already calculated handle hash
                            log::debug!("Using pre-computed instance handle as key (FFI path)");
                            info.key = Arc::from(instance_handle.value().as_slice());
                        }
                        Err(e) => return Err(e), // Unexpected errors
                    }
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
                        info.key = Arc::from(
                            cache_change
                                .as_ref()
                                .ok_or(DdsError::Error(
                                    "CacheChange is not properly initialized".to_string(),
                                ))?
                                .data_value(),
                        );
                    }

                    let monitor_guard =
                        self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                    if let Some(monitor) = monitor_guard.as_ref() {
                        monitor.cancel_instance(&instance_handle);
                    }

                    let reader_qos = self.get_qos_arc()?;
                    let reader_data_lifecycle_qos = &reader_qos.reader_data_lifecycle;
                    if !reader_data_lifecycle_qos.autopurge_disposed_samples_delay.is_infinite() {
                        let std_duration = std::time::Duration::from_nanos(
                            reader_data_lifecycle_qos.autopurge_disposed_samples_delay.as_nanos()
                                as u64,
                        );
                        self.add_autopurge_timer(
                            std_duration,
                            TimerId::AutopurgeDisposed { reader_guid: self.guid },
                            instance_handle,
                            false,
                        )?;
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
                        info.key = Arc::from(
                            cache_change
                                .as_ref()
                                .ok_or(DdsError::Error(
                                    "CacheChange is not properly initialized".to_string(),
                                ))?
                                .data_value(),
                        );
                    }

                    let monitor_guard =
                        self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                    if let Some(monitor) = monitor_guard.as_ref() {
                        // Keep deadline tracking for synthetic no-writer transitions on non-keyed data.
                        let synthetic_non_keyed_no_writers =
                            instance_handle.is_nil() && cache_change.is_none();
                        if !synthetic_non_keyed_no_writers {
                            monitor.cancel_instance(&instance_handle);
                        }
                    }

                    let reader_qos = self.get_qos_arc()?;
                    let reader_data_lifecycle_qos = &reader_qos.reader_data_lifecycle;
                    if !reader_data_lifecycle_qos.autopurge_nowriter_samples_delay.is_infinite() {
                        let std_duration = std::time::Duration::from_nanos(
                            reader_data_lifecycle_qos.autopurge_nowriter_samples_delay.as_nanos()
                                as u64,
                        );
                        self.add_autopurge_timer(
                            std_duration,
                            TimerId::AutopurgeNowriter { reader_guid: self.guid },
                            instance_handle,
                            true,
                        )?;
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

        Ok(info.instance_state != prev_state)
    }

    fn add_autopurge_timer(
        &self,
        std_duration: std::time::Duration,
        timer_id: TimerId,
        instance_handle: InstanceHandle,
        full_reclaim: bool,
    ) -> DdsResult<()> {
        // Get weak reference to self (DataReader)
        let self_ref = self.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let weak_self = self_ref.as_ref().map(Arc::downgrade);

        // Get timer handler lock
        let timer_handler = TimerHandler::get_instance(self.guid.prefix());
        let timer_handler_guard = timer_handler
            .lock()
            .map_err(|e| DdsError::Error(format!("Failed to lock timer handler: {}", e)))?;

        timer_handler_guard.add_timer(timer_id, std_duration, false, move || {
            if let Some(strong) = weak_self.as_ref().and_then(|w| w.upgrade()) {
                // NO_WRITERS reclaims all instance state; DISPOSED purges only the samples.
                let res = if full_reclaim {
                    strong.reclaim_instance(instance_handle)
                } else {
                    strong.remove_change_of_instance(instance_handle)
                };
                if let Err(e) = res {
                    log::error!("Failed to autopurge instance: {:?}", e);
                }
            }
        });
        Ok(())
    }
}

impl<Foo: DdsType> DataReader<Foo> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        is_builtin: bool,
        guid: Guid,
        type_support: Arc<dyn TypeSupport + Send>,
        topic_description: &dyn TopicDescription,
        qos: DataReaderQos,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
        mask: StatusMask,
        subscriber: &Arc<Subscriber>,
        rtps_reader: Option<Arc<dyn RtpsReader + Send + Sync>>,
    ) -> DdsResult<Self> {
        // Builtin entities must have rtps_reader, non-builtin must not
        if is_builtin != rtps_reader.is_some() {
            return Err(DdsError::PreconditionNotMet);
        }

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
            is_builtin,
            guid,
            qos: Arc::new(ArcSwap::from_pointee(qos.clone())),
            update_lock: Arc::new(Mutex::new(())),
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
            rtps_reader: Arc::new(Mutex::new(rtps_reader.map(|r| Arc::downgrade(&r)))),
            enabled: Arc::new(AtomicBool::new(false)),
            lifecycle: Arc::new(EntityLifecycle::default()),
            liveliness_changed_status: Arc::new(Mutex::new(LivelinessChangedStatus::default())),
            sample_rejected_status: Arc::new(Mutex::new(SampleRejectedStatus::default())),
            sample_lost_status: Arc::new(Mutex::new(SampleLostStatus::default())),
            requested_deadline_missed_status: Arc::new(Mutex::new(
                RequestedDeadlineMissedStatus::default(),
            )),
            requested_incompatible_qos_status: Arc::new(Mutex::new(
                RequestedIncompatibleQosStatus::default(),
            )),
            requested_incompatible_type_status: Arc::new(Mutex::new(
                RequestedIncompatibleTypeStatus::default(),
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
                qos.destination_order.kind,
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
            if reader.content_filtered_topic.is_some() {
                datareader_cache.set_content_filter();
            }
            datareader_cache.set_update_status(status_callback.clone());
        }

        let period = reader.get_qos_arc()?.deadline.period;
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
        let _operation = self.lifecycle.begin_operation()?;
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

    // ========================================================================
    // Raw Serialized Data Access (bypasses TypeSupport deserialization)
    // ========================================================================

    /// Take pre-serialized data directly from the cache, bypassing TypeSupport deserialization.
    ///
    /// Returns raw CDR bytes and SampleInfo for each matching sample.
    /// The samples are removed from the cache (take semantics).
    ///
    /// Note: QueryCondition and ContentFilteredTopic filters are NOT applied,
    /// as they require deserialized data.
    pub fn take_serialized(
        &self,
        max_samples: i32,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<(Bytes, SampleInfo)>> {
        self.read_or_take_serialized(
            max_samples,
            sample_states,
            view_states,
            instance_states,
            true,
            None,
        )
    }

    /// Take pre-serialized samples belonging to a single instance.
    ///
    /// Like [`take_serialized`](Self::take_serialized) but restricted to the
    /// instance identified by `handle`. A nil handle returns `BadParameter`; an
    /// unknown handle yields no samples (mirroring `take_instance`).
    pub fn take_instance_serialized(
        &self,
        max_samples: i32,
        handle: InstanceHandle,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<(Bytes, SampleInfo)>> {
        self.read_or_take_serialized(
            max_samples,
            sample_states,
            view_states,
            instance_states,
            true,
            Some(handle),
        )
    }

    /// Read pre-serialized samples belonging to a single instance.
    ///
    /// Like [`read_serialized`](Self::read_serialized) but restricted to the
    /// instance identified by `handle`. A nil handle returns `BadParameter`; an
    /// unknown handle yields no samples (mirroring `read_instance`).
    pub fn read_instance_serialized(
        &self,
        max_samples: i32,
        handle: InstanceHandle,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<(Bytes, SampleInfo)>> {
        self.read_or_take_serialized(
            max_samples,
            sample_states,
            view_states,
            instance_states,
            false,
            Some(handle),
        )
    }

    /// Read pre-serialized data directly from the cache, bypassing TypeSupport deserialization.
    ///
    /// Returns raw CDR bytes and SampleInfo for each matching sample.
    /// The samples remain in the cache and are marked as read.
    ///
    /// Note: QueryCondition and ContentFilteredTopic filters are NOT applied,
    /// as they require deserialized data.
    pub fn read_serialized(
        &self,
        max_samples: i32,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<(Bytes, SampleInfo)>> {
        self.read_or_take_serialized(
            max_samples,
            sample_states,
            view_states,
            instance_states,
            false,
            None,
        )
    }

    /// Take a single pre-serialized sample from the cache.
    pub fn take_next_serialized(&self) -> DdsResult<(Bytes, SampleInfo)> {
        let results = self.read_or_take_serialized(
            1,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            true,
            None,
        )?;
        results.into_iter().next().ok_or(DdsError::NoData)
    }

    /// Take a single pre-serialized sample without copying shared receive payloads.
    pub fn take_next_serialized_bytes(&self) -> DdsResult<(Bytes, SampleInfo)> {
        let (results, _) = self.read_or_take_serialized_bytes(
            1,
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            true,
            None,
            None,
        )?;
        results.into_iter().next().ok_or(DdsError::NoData)
    }

    fn bounded_single_serialized(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        take: bool,
        max_bytes: usize,
    ) -> DdsResult<BoundedSerialized> {
        let (results, too_small) = self.read_or_take_serialized_bytes(
            1,
            sample_states,
            view_states,
            instance_states,
            take,
            Some(max_bytes),
            None,
        )?;
        if let Some(required) = too_small {
            return Ok(BoundedSerialized::TooSmall { required });
        }
        results
            .into_iter()
            .next()
            .map(|(data, info)| BoundedSerialized::Fit(data, info))
            .ok_or(DdsError::NoData)
    }

    pub fn take_next_serialized_bounded(&self, max_bytes: usize) -> DdsResult<BoundedSerialized> {
        self.bounded_single_serialized(
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            true,
            max_bytes,
        )
    }

    pub fn read_next_serialized_bounded(&self, max_bytes: usize) -> DdsResult<BoundedSerialized> {
        self.bounded_single_serialized(
            &[SampleStateKind::NOT_READ_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
            false,
            max_bytes,
        )
    }

    pub fn take_serialized_bounded(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        max_bytes: usize,
    ) -> DdsResult<BoundedSerialized> {
        self.bounded_single_serialized(sample_states, view_states, instance_states, true, max_bytes)
    }

    pub fn read_serialized_bounded(
        &self,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        max_bytes: usize,
    ) -> DdsResult<BoundedSerialized> {
        self.bounded_single_serialized(
            sample_states,
            view_states,
            instance_states,
            false,
            max_bytes,
        )
    }

    fn read_or_take_serialized(
        &self,
        max_samples: i32,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        take: bool,
        instance_handle: Option<InstanceHandle>,
    ) -> DdsResult<Vec<(Bytes, SampleInfo)>> {
        // Hand back the `Bytes` the cache already holds. Rewrapping into a fresh
        // `Arc<[u8]>` copied every sample in full on the way out.
        self.read_or_take_serialized_bytes(
            max_samples,
            sample_states,
            view_states,
            instance_states,
            take,
            None,
            instance_handle,
        )
        .map(|(results, _)| results)
    }

    fn read_or_take_serialized_bytes(
        &self,
        max_samples: i32,
        sample_states: &[SampleStateKind],
        view_states: &[ViewStateKind],
        instance_states: &[InstanceStateKind],
        take: bool,
        max_bytes: Option<usize>,
        instance_handle: Option<InstanceHandle>,
    ) -> DdsResult<(Vec<(Bytes, SampleInfo)>, Option<usize>)> {
        let _operation = self.lifecycle.begin_operation()?;
        self.is_enabled()?;

        if max_samples == 0 {
            return Err(DdsError::BadParameter);
        }

        // Instance-scoped variants reject a nil handle, matching the typed
        // read/take_instance path; an unknown (non-nil) handle simply yields no
        // matching samples via the in-loop filter below.
        if let Some(handle) = instance_handle {
            if handle.is_nil() {
                return Err(DdsError::BadParameter);
            }
        }

        self.set_read_communication_status(false)?;
        let mut result: Vec<(Bytes, SampleInfo)> = Vec::new();
        let mut remaining = if max_samples == -1 { i32::MAX } else { max_samples };

        let changes = self.get_available_changes()?;

        // ContentFilteredTopic is applied on the receive path, so the cache is already filtered.
        let instance_infos = self.get_instance_infos()?;

        for change in changes.iter() {
            if remaining <= 0 {
                break;
            }

            // Instance filter: skip changes belonging to a different instance,
            // before any state check or cache removal (so `take` never consumes
            // samples from other instances).
            if let Some(target) = instance_handle {
                if change.instance_handle() != target {
                    continue;
                }
            }

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
                    pending_notification: false,
                },
            };

            if !sample_states.matches(sample_state)
                || !view_states.matches(info.view_state)
                || !instance_states.matches(info.instance_state)
            {
                continue;
            }

            let has_valid_data = match change.kind() {
                ChangeKind::Alive | ChangeKind::AliveFiltered => true,
                ChangeKind::NotAliveDisposed
                | ChangeKind::NotAliveUnregistered
                | ChangeKind::NotAliveDisposedUnregistered => false,
            };

            let serialized_data = change.data_bytes();

            if let Some(cap) = max_bytes {
                if has_valid_data && serialized_data.len() > cap {
                    return Ok((Vec::new(), Some(serialized_data.len())));
                }
            }

            let sample_info = SampleInfo {
                sample_state,
                view_state: info.view_state,
                instance_state: info.instance_state,
                disposed_generation_count: info.disposed_generation_count,
                no_writers_generation_count: info.no_writers_generation_count,
                sample_rank: 0,
                generation_rank: 0,
                absolute_generation_rank: 0,
                source_timestamp: (*change.source_timestamp().as_ref().ok_or_else(|| {
                    DdsError::Error(
                        "CacheChange's source timestamp is not properly initialized".to_string(),
                    )
                })?)
                .into(),
                instance_handle: change.instance_handle(),
                publication_handle: InstanceHandle::from_guid(&change.writer_guid()),
                valid_data: has_valid_data,
            };

            if take {
                self.remove_change(change.clone())?;
            } else {
                self.mark_sample_as_read(&change.writer_guid(), change.sequence_number())?;
            }

            result.push((serialized_data, sample_info));
            remaining -= 1;
        }

        for sample_info in self.drain_pending_notifications(
            &instance_infos,
            sample_states,
            view_states,
            instance_states,
            None,
            remaining,
        )? {
            result.push((Bytes::new(), sample_info));
        }

        for (_, sample_info) in &result {
            self.mark_instance_as_viewed(sample_info.instance_handle);
        }

        self.prune_read_samples_to_cache(&changes);
        self.reevaluate_all_conditions()?;

        if result.is_empty() {
            Err(DdsError::NoData)
        } else {
            Ok((result, None))
        }
    }

    #[allow(clippy::too_many_arguments)]
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
        log::debug!(
            "read_or_take called: max_samples={}, handle={}, single_instance={}, exact={}, take={}",
            max_samples,
            handle,
            single_instance,
            exact,
            take
        );
        log::debug!(
            "direct_states: sample={:?}, view={:?}, instance={:?}",
            direct_sample_states.is_some(),
            direct_view_states.is_some(),
            direct_instance_states.is_some()
        );
        log::debug!("condition: {:?}", condition.is_some());

        let _operation = self.lifecycle.begin_operation()?;
        self.is_enabled()?;

        if max_samples == 0 {
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
                }
            }
            states
        } else if let (Some(sample_states), Some(view_states), Some(instance_states)) =
            (direct_sample_states, direct_view_states, direct_instance_states)
        {
            log::debug!("Using direct state masks");
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

        log::debug!("Processing {} changes, need {} samples", changes.len(), remaining_samples);

        // Get instance_infos once outside the loop to avoid repeated lock acquisition and cloning
        let instance_infos = self.get_instance_infos()?;

        for (idx, change) in changes.iter().enumerate() {
            if remaining_samples <= 0 {
                log::debug!("Reached sample limit, stopping");
                break;
            }

            if exact && change.instance_handle() != handle {
                log::trace!("Skipping change {}: handle mismatch", idx);
                continue;
            }

            // Check sample state
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
                    pending_notification: false,
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

            // 2. Create DataSample - use optimized version with pre-fetched instance_infos
            match self.change_to_data_sample_with_infos(
                change,
                change.instance_handle(),
                sample_state,
                Some(&instance_infos),
            ) {
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
                    // ContentFilteredTopic is applied on the receive path, so non-matching
                    // samples are never in the cache here.
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

        for sample_info in self.drain_pending_notifications(
            &instance_infos,
            sample_states,
            view_states,
            instance_states,
            if exact { Some(handle) } else { None },
            remaining_samples,
        )? {
            result_samples.push(DataSample::new(None, sample_info, None));
        }

        // 2.2.2.5.1.8 Interpretation of the SampleInfo view_state
        for sample in &result_samples {
            self.mark_instance_as_viewed(sample.sample_info().instance_handle);
        }

        self.prune_read_samples_to_cache(&changes);
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

        let info = {
            let instance_infos =
                self.instance_infos.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            instance_infos.get(&instance_handle).ok_or_else(|| {
                DdsError::Error("InstanceInfo should have been updated already when added to data reader history cache".to_string())
            })?.clone()
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
        change: &Arc<CacheChange>,
        instance_handle: InstanceHandle,
        sample_state: SampleStateKind,
    ) -> DdsResult<DataSample<Foo>> {
        // Delegate to optimized version, fetching instance_infos internally
        self.change_to_data_sample_with_infos(change, instance_handle, sample_state, None)
    }

    /// Optimized version that accepts pre-fetched instance_infos to avoid repeated lock acquisition
    fn change_to_data_sample_with_infos(
        &self,
        change: &Arc<CacheChange>,
        instance_handle: InstanceHandle,
        sample_state: SampleStateKind,
        cached_instance_infos: Option<&HashMap<InstanceHandle, InstanceInfo>>,
    ) -> DdsResult<DataSample<Foo>> {
        // Check if change has valid data based on its kind
        let has_valid_data = match change.kind() {
            ChangeKind::Alive | ChangeKind::AliveFiltered => true,
            ChangeKind::NotAliveDisposed
            | ChangeKind::NotAliveUnregistered
            | ChangeKind::NotAliveDisposedUnregistered => false,
        };

        let data = if has_valid_data {
            // Carry fragment chunks straight through when present, so deserialization
            // happens across them with no contiguous reassembly.
            Some(match change.data_chunks() {
                Some((chunks, cached)) => SamplePayload::Chained {
                    chunks: chunks.iter().cloned().collect(),
                    cached: cached.clone(),
                },
                None => SamplePayload::Contiguous(change.data_bytes()),
            })
        } else {
            None
        };

        // Use cached instance_infos if provided, otherwise fetch
        let owned_instance_infos;
        let instance_infos = match cached_instance_infos {
            Some(infos) => infos,
            None => {
                owned_instance_infos = self.get_instance_infos()?;
                &owned_instance_infos
            }
        };

        let info = instance_infos
            .get(&instance_handle)
            .ok_or(DdsError::Error("Instance not found".to_string()))?;

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
        Ok(DataSample::new(data, sample_info, Some(self.type_support.clone())))
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
        // 1. Candidate instances: those with available changes, plus those carrying a
        // pending synthetic NOT_ALIVE_NO_WRITERS notification whose cache is already empty.
        let mut handles: std::collections::HashSet<InstanceHandle> =
            self.get_available_instance_handles()?.into_iter().collect();
        for (instance_handle, info) in self.get_instance_infos()? {
            if info.pending_notification {
                handles.insert(instance_handle);
            }
        }

        if handles.is_empty() {
            return Ok(InstanceHandle::NIL);
        }

        // 2. Sort instance handles
        let mut sorted_handles: Vec<InstanceHandle> = handles.into_iter().collect();
        sorted_handles.sort();

        // log::debug!("find_next_instance_handle: previous_handle={:?}, sorted_handles={:?}",
        //            previous_handle, sorted_handles);

        // 3. If previous_handle is NIL, return the first instance
        if previous_handle.is_nil() {
            let first = sorted_handles.into_iter().next().unwrap_or(InstanceHandle::NIL);
            // log::debug!("previous_handle is NIL, returning first instance: {:?}", first);
            return Ok(first);
        }

        // 4. Return the smallest available handle strictly greater than previous_handle
        for handle in sorted_handles {
            if handle > previous_handle {
                return Ok(handle);
            }
        }

        // 5. No greater handle remains: the iteration is finished.
        Ok(InstanceHandle::NIL)
    }

    // True if the change should be kept for this reader's ContentFilteredTopic.
    // No CFT, a disabled filter, or non-Alive (key-only) samples always pass.
    pub(crate) fn passes_content_filter(&self, change: &CacheChange) -> DdsResult<bool> {
        let cft = match &self.content_filtered_topic {
            Some(weak) => match weak.upgrade() {
                Some(cft) => cft,
                None => return Ok(true),
            },
            None => return Ok(true),
        };
        if !cft.is_filter_enabled()? {
            return Ok(true);
        }
        if !matches!(change.kind(), ChangeKind::Alive | ChangeKind::AliveFiltered) {
            return Ok(true);
        }
        let expr = cft.get_parsed_expression()?;
        let parameters = cft.get_expression_parameters()?;
        let serialized_data = change.data_bytes();
        let typed = match self.type_support.deserialize(&serialized_data, None) {
            Ok(deserialized) => match deserialized.downcast::<Foo>() {
                Ok(typed) => typed,
                Err(_) => return Ok(true),
            },
            Err(_) => return Ok(true),
        };
        match expr.evaluate(&*typed, &parameters) {
            Ok(false) => Ok(false),
            _ => Ok(true),
        }
    }

    fn get_available_instance_handles(&self) -> DdsResult<Vec<InstanceHandle>> {
        // Check available changes in RTPS reader
        let changes = self.get_available_changes()?;
        let mut handles = std::collections::HashSet::new();

        // Collect instance handles from changes
        for change in changes {
            handles.insert(change.instance_handle());
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
                    pending_notification: false,
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
        let _operation = self.lifecycle.begin_operation()?;

        self.set_communication_status_propagation(&StatusKind::LIVELINESS_CHANGED, false)?;
        self.take_liveliness_changed_status()
    }

    fn get_sample_rejected_status(&self) -> DdsResult<SampleRejectedStatus> {
        // out: DdsError_t, status: SampleRejectedStatus
        /*
            This operation provides access to the SAMPLE_REJECTED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        let _operation = self.lifecycle.begin_operation()?;

        self.set_communication_status_propagation(&StatusKind::SAMPLE_REJECTED, false)?;
        self.take_sample_rejected_status()
    }

    fn get_requested_deadline_missed_status(&self) -> DdsResult<RequestedDeadlineMissedStatus> {
        // out: DdsError_t, status: RequestedDeadlineMissedStatus
        /*
            This operation provides access to the REQUESTED_DEADLINE_MISSED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        let _operation = self.lifecycle.begin_operation()?;

        self.set_communication_status_propagation(&StatusKind::REQUESTED_DEADLINE_MISSED, false)?;
        self.take_requested_deadline_missed_status()
    }

    fn get_requested_incompatible_qos_status(&self) -> DdsResult<RequestedIncompatibleQosStatus> {
        // out: DdsError_t, status: RequestedIncompatibleQosStatus
        /*
            This operation provides access to the REQUESTED_INCOMPATIBLE_QOS communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        let _operation = self.lifecycle.begin_operation()?;

        self.set_communication_status_propagation(&StatusKind::REQUESTED_INCOMPATIBLE_QOS, false)?;
        self.take_requested_incompatible_qos_status()
    }

    fn get_requested_incompatible_type_status(&self) -> DdsResult<RequestedIncompatibleTypeStatus> {
        // out: DdsError_t, status: RequestedIncompatibleTypeStatus
        /*
            This operation provides access to the REQUESTED_INCOMPATIBLE_TYPE communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        let _operation = self.lifecycle.begin_operation()?;

        self.set_communication_status_propagation(&StatusKind::REQUESTED_INCOMPATIBLE_TYPE, false)?;
        self.take_requested_incompatible_type_status()
    }

    fn get_subscription_matched_status(&self) -> DdsResult<SubscriptionMatchedStatus> {
        // out: DdsError_t, status: SubscriptionMatchedStatus
        /*
            This operation provides access to the SUBSCRIPTION_MATCHED communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        let _operation = self.lifecycle.begin_operation()?;

        self.set_communication_status_propagation(&StatusKind::SUBSCRIPTION_MATCHED, false)?;
        self.take_subscription_matched_status()
    }

    fn get_sample_lost_status(&self) -> DdsResult<SampleLostStatus> {
        // out: DdsError_t, status: SampleLostStatus
        /*
            This operation provides access to the SAMPLE_LOST communication status.
            Communication status is described in Section 2.2.4.1, Communication Status.
        */
        let _operation = self.lifecycle.begin_operation()?;

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
        let _operation = self.lifecycle.begin_operation()?;
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
        let _operation = self.lifecycle.begin_operation()?;
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
        let _operation = self.lifecycle.begin_operation()?;
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
        let _operation = self.lifecycle.begin_operation()?;
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
        let _operation = self.lifecycle.begin_operation()?;
        Err(DdsError::Unsupported)
    }

    fn get_topicdescription(&self) -> DdsResult<Arc<dyn TopicDescription>> {
        let _operation = self.lifecycle.begin_operation()?;

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
        let _operation = self.lifecycle.begin_operation()?;
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
        let _operation = self.lifecycle.begin_operation()?;
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

        self.lifecycle.mark_deleted_and_await_operation_completion();
    }

    fn mark_deleted_and_await_operation_completion(&self) {
        self.lifecycle.mark_deleted_and_await_operation_completion();
    }

    fn is_builtin(&self) -> bool {
        self.is_builtin
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
        DurabilityQosPolicy, DurabilityQosPolicyKind, HistoryQosPolicy, HistoryQosPolicyKind,
        PresentationQosAccessScopeKind, PresentationQosPolicy, ReliabilityQosPolicy,
        ReliabilityQosPolicyKind,
    };
    use crate::infrastructure::wait_set::WaitSet;
    use crate::publication::data_writer_listener::DataWriterListener;
    use crate::publication::qos::{DataWriterQos, PublisherQos};
    use crate::rtps::entities::reader::StatefulReader;
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

    /// `set_read_communication_status` fans one arrival out to three entity levels with two
    /// status kinds, and every reader under a participant writes the participant's shared
    /// condition on every sample. Pin the exact bits at every level so a refactor of how the
    /// condition is reached cannot quietly drop or merge one of the five updates -- merging the
    /// two kinds into a single mask op looks equivalent but is not, because the WaitSet trigger
    /// test is all-of (`StatusMask::contains`), not any-of.
    #[test]
    fn read_communication_status_sets_and_clears_every_level() {
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(
                unique_domain_id(),
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let topic = participant
            .create_topic::<TestData>(
                "ReadCommStatusTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<TestData>(
                &topic,
                DataReaderQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        reader.set_read_communication_status(true).unwrap();

        assert!(
            reader.get_status_changes().unwrap().contains(StatusMask::DATA_AVAILABLE),
            "reader must report DATA_AVAILABLE"
        );
        let subscriber_changes = subscriber.get_status_changes().unwrap();
        assert!(
            subscriber_changes.contains(StatusMask::DATA_ON_READERS),
            "subscriber must report DATA_ON_READERS"
        );
        assert!(
            subscriber_changes.contains(StatusMask::DATA_AVAILABLE),
            "subscriber must report DATA_AVAILABLE"
        );
        let participant_changes = participant.get_status_changes().unwrap();
        assert!(
            participant_changes.contains(StatusMask::DATA_ON_READERS),
            "participant must report DATA_ON_READERS"
        );
        assert!(
            participant_changes.contains(StatusMask::DATA_AVAILABLE),
            "participant must report DATA_AVAILABLE"
        );

        reader.set_read_communication_status(false).unwrap();

        assert!(
            !reader.get_status_changes().unwrap().contains(StatusMask::DATA_AVAILABLE),
            "reader must clear DATA_AVAILABLE"
        );
        let subscriber_changes = subscriber.get_status_changes().unwrap();
        assert!(
            !subscriber_changes.contains(StatusMask::DATA_ON_READERS),
            "subscriber must clear DATA_ON_READERS"
        );
        assert!(
            !subscriber_changes.contains(StatusMask::DATA_AVAILABLE),
            "subscriber must clear DATA_AVAILABLE"
        );
        let participant_changes = participant.get_status_changes().unwrap();
        assert!(
            !participant_changes.contains(StatusMask::DATA_ON_READERS),
            "participant must clear DATA_ON_READERS"
        );
        assert!(
            !participant_changes.contains(StatusMask::DATA_AVAILABLE),
            "participant must clear DATA_AVAILABLE"
        );

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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

        assert!(status_condition.get_entity().is_err());

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    /// Listener mask for the counting listeners below. Deliberately not `StatusMask::default()`,
    /// which is `ALL`.
    ///
    /// DDS 1.4 2.2.4.1: a plain communication status is consumed by the listener that handles it,
    /// and its StatusChangedFlag is reset when that listener returns. A reader whose mask enables
    /// SUBSCRIPTION_MATCHED therefore never leaves the status latched for a StatusCondition, so
    /// these tests -- which use the listener only to count `on_data_available` but wait on the
    /// reader's own condition for the discovery handshake -- would block in
    /// `WaitSet::wait(Duration::infinite())` forever. LIVELINESS_CHANGED is the same story for
    /// `test_take_next_instance_surfaces_no_writers_after_writer_deleted`.
    ///
    /// The mask is what separates the two mechanisms: the listener takes data notifications, the
    /// WaitSet takes everything else. Note that `handle_data_available_status` invokes the listener
    /// without consulting the mask, so DATA_AVAILABLE is named here for intent rather than because
    /// omitting it would silence the counter.
    const DATA_ONLY_LISTENER_MASK: StatusMask = StatusMask::DATA_AVAILABLE;

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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    /// `read()` records every sample it hands out in `read_samples` so a later `read()` can
    /// report `READ_SAMPLE_STATE`. Nothing ever pruned that map, so it grew for the reader's
    /// lifetime -- roughly 20 bytes per distinct sample read, unbounded.
    ///
    /// The bound must come from the cache, not from a count. `get_sample_state` answers
    /// `READ` both for a sample recorded in `read_samples` and for a sample that is no longer
    /// cached, so entries below the cache's low-water mark cannot change any answer and are
    /// free to drop. Evicting an entry whose sample is *still cached* would flip it back to
    /// `NOT_READ` and re-deliver it, which is why the last assertion here matters more than
    /// the size one.
    #[test]
    fn read_samples_stays_bounded_without_redelivering() {
        const SAMPLES: usize = 60;

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
                "read_samples_growth",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // KeepLast(1) on both ends: the cache holds one sample at a time, so a correctly
        // bounded `read_samples` cannot accumulate either.
        let history = HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(1), strict: false };
        let reliability = ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration::from_seconds(1),
        };

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic,
                DataWriterQos {
                    history: history.clone(),
                    reliability: reliability.clone(),
                    ..Default::default()
                },
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        // `on_data_available` is what the loop below synchronises on. `write` hands the sample
        // to the send path and returns while delivery lands on the receive thread afterwards,
        // so reading straight after `write` times the loopback rather than the bound under test.
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic,
                DataReaderQos { history, reliability, ..Default::default() },
                Some(Arc::new(SubListener { counter_sender })),
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        // Bounded: `Duration::infinite()` here would wedge the whole test binary if matching
        // ever regressed, and `cargo test` has no per-test timeout to save it.
        wait_set.wait(Duration::from_seconds(10)).expect("writer never matched the reader");
        wait_set.detach_condition(condition).unwrap();

        // The reader's side of the match too, not just the writer's: a sample from a writer the
        // reader has not matched yet is dropped on arrival, which would leave the loop below
        // reading an empty cache for a reason that has nothing to do with the bound.
        let condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(10)).expect("reader never matched the writer");
        wait_set.detach_condition(condition).unwrap();

        // Each iteration waits for its own sample to land before reading, so the loop performs
        // `SAMPLES` real reads instead of however many happen to win a race against delivery.
        // That is what keeps the bound below meaningful: the defect this test guards grew
        // `read_samples` once per sample *read*, so a run whose reads mostly returned `NoData`
        // would satisfy the bound with the defect fully present.
        let mut reads = 0usize;
        for index in 0..SAMPLES as u32 {
            writer
                .write(&HelloWorld { index, message: "growth".to_string() }, InstanceHandle::NIL)
                .unwrap();
            // A missed signal is not a failure by itself -- the listener is a synchronisation
            // aid, and the floor asserted after the loop is what decides whether enough was read.
            let _ = counter_receiver.recv_timeout(std::time::Duration::from_secs(10));
            // `read` is the non-consuming path, and the only one that records read state.
            let read = data_reader.read(
                1,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            );
            if read.is_ok() {
                reads += 1;
            }
        }
        // Well clear of the bound asserted below, so the two cannot both hold unless
        // `read_samples` is bounded by what the cache retains rather than by the read count.
        assert!(
            reads >= SAMPLES / 2,
            "only {reads} of {SAMPLES} reads handed out a sample, too few for the bound below \
             to mean anything"
        );

        // Backstop for a missed listener signal, which the loop above deliberately tolerates:
        // without one, the final sample can still be in flight here. A sample that reads
        // NOT_READ because it has only just arrived is not the defect under test, so the guards
        // below must not run against a cache that is still filling. With every signal seen this
        // succeeds on its first read and costs nothing.
        let settle_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut read_last = false;
        while !read_last && std::time::Instant::now() < settle_deadline {
            if let Ok(samples) = data_reader.read(
                1,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            ) {
                read_last = samples
                    .iter()
                    .any(|sample| matches!(sample.data(), Ok(d) if d.index == SAMPLES as u32 - 1));
            }
            if !read_last {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
        assert!(
            read_last,
            "the last written sample never reached the reader, so the guard below would assert \
             against a cache that is still filling rather than against redelivery"
        );

        let recorded: usize =
            data_reader.read_samples.lock().unwrap().values().map(|set| set.len()).sum();

        // Bounded by what a KeepLast(1) cache can retain, not by the number of samples read.
        // Slack covers samples still in flight when the loop ends; the pre-fix behaviour
        // recorded one entry per sample, so any bound well under SAMPLES discriminates.
        assert!(
            recorded <= 8,
            "read_samples holds {recorded} sequence numbers for a depth-1 cache, so it is \
             growing with every sample read rather than with what the cache retains"
        );

        // The guard that must hold before and after any bounding change: whatever is still
        // cached and already read stays READ. A cap that evicts a live entry re-delivers it.
        // Unconditional -- gating it on the preceding read succeeding is how a bounding bug
        // would slip through unnoticed.
        assert!(
            data_reader
                .read(
                    1,
                    &[SampleStateKind::NOT_READ_SAMPLE_STATE],
                    &[ViewStateKind::ANY_VIEW_STATE],
                    &[InstanceStateKind::ANY_INSTANCE_STATE],
                )
                .is_err(),
            "a cached sample that was already read came back as NOT_READ, so bounding \
             read_samples re-delivers already-read samples"
        );

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() }; // Published by Writer

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        // Set up test data
        let data1 = HelloWorld { index: 0, message: "HelloWorld".to_string() };
        let data2 = HelloWorld { index: 1, message: "HelloWorld".to_string() };

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    // take_next_instance must advance past an instance whose samples were already
    // taken, using the returned handle as previous_handle even though it is gone from the cache.
    #[test]
    fn test_take_next_instance_advances_past_taken_handle() {
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
                "HelloWorldWithKeyAdvance",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();

        // Two distinct instances (keys 0 and 1), one sample each.
        let data_a = HelloWorldWithKey { index: 0, message: "A".to_string() };
        let data_b = HelloWorldWithKey { index: 1, message: "B".to_string() };
        let handle_a = writer.register_instance(&data_a).unwrap();
        let handle_b = writer.register_instance(&data_b).unwrap();
        writer.write(&data_a, handle_a).unwrap();
        writer.write(&data_b, handle_b).unwrap();

        let mut count = 0;
        while counter_receiver.recv().is_ok() {
            count += 1;
            if count == 2 {
                break;
            }
        }

        // Take the first (smallest-handle) instance; capture its handle, then it is removed.
        let first = data_reader
            .take_next_instance(
                10,
                InstanceHandle::NIL,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();
        assert_eq!(first.len(), 1);
        let first_handle = first[0].sample_info().instance_handle;
        let first_index = first[0].data().unwrap().index;

        // Advance with the now-vanished handle: must return the OTHER instance, not NoData.
        let second = data_reader
            .take_next_instance(
                10,
                first_handle,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();
        assert_eq!(second.len(), 1);
        let second_index = second[0].data().unwrap().index;

        assert_ne!(first_index, second_index);
        assert!(first_index == 0 || first_index == 1);
        assert!(second_index == 0 || second_index == 1);

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }

    #[test]
    fn test_take_next_instance_surfaces_no_writers_after_writer_deleted() {
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
                "HelloWorldWithKeyNoWriters",
                "HelloWorldWithKeyType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
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
            .create_datawriter::<HelloWorldWithKey>(&topic, writer_qos, None, StatusMask::default())
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
        let (counter_sender, counter_receiver) = std::sync::mpsc::sync_channel::<()>(100);
        let read_listener = SubKeyListener { counter_sender };
        let data_reader = subscriber
            .create_datareader::<HelloWorldWithKey>(
                &topic,
                reader_qos,
                Some(Arc::new(read_listener)),
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let sub_condition = data_reader.get_statuscondition().unwrap().clone();
        sub_condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(sub_condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        data_reader.get_subscription_matched_status().unwrap();
        wait_set.detach_condition(sub_condition).unwrap();

        // One sample for a single instance, then drain it so its cache is empty.
        let data = HelloWorldWithKey { index: 0, message: "A".to_string() };
        let handle = writer.register_instance(&data).unwrap();
        writer.write(&data, handle).unwrap();
        counter_receiver.recv().unwrap();

        let alive = data_reader
            .take_next_instance(
                10,
                InstanceHandle::NIL,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();
        assert_eq!(alive.len(), 1);
        assert!(alive[0].sample_info().valid_data);

        // Delete the writer and wait for the reader to process the departure. The reader sets the
        // instance to NOT_ALIVE_NO_WRITERS while handling LIVELINESS_CHANGED, before that status
        // wakes the wait set, so the synthetic notification is set once wait returns.
        let liveliness = data_reader.get_statuscondition().unwrap().clone();
        liveliness.set_enabled_statuses(StatusMask::LIVELINESS_CHANGED).unwrap();
        wait_set.attach_condition(liveliness.clone()).unwrap();
        publisher.delete_datawriter(writer).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        wait_set.detach_condition(liveliness).unwrap();

        // The instance's cache is empty, so NO_WRITERS only exists as a synthetic pending sample.
        let no_writers = data_reader
            .take_next_instance(
                10,
                InstanceHandle::NIL,
                &[SampleStateKind::ANY_SAMPLE_STATE],
                &[ViewStateKind::ANY_VIEW_STATE],
                &[InstanceStateKind::ANY_INSTANCE_STATE],
            )
            .unwrap();
        assert_eq!(no_writers.len(), 1);
        let info = no_writers[0].sample_info();
        assert!(!info.valid_data);
        assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
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
                DATA_ONLY_LISTENER_MASK,
            )
            .unwrap();

        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        let wait_set = WaitSet::new();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set.wait(Duration::infinite()).unwrap();
        writer.get_publication_matched_status().unwrap();
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
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
        {
            let mut dcps_bridge = domain_participant.get_dcps_bridge().unwrap();
            let dcps_bridge = dcps_bridge.as_mut().unwrap();
            dcps_bridge
                .delete_rtps_reader(
                    "hello_world".to_string(),
                    reader.get_instance_handle().unwrap().to_guid().entity_id(),
                )
                .unwrap();
        }
        let rtps_reader = reader.get_rtps_reader();
        assert!(rtps_reader.is_err());

        drop(reader);
        domain_participant.delete_contained_entities().unwrap();
        domain_participant_factory.delete_participant(domain_participant).unwrap();
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
                    "Get DataOnReaders Status - Guid {}",
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

        domain_participant.delete_contained_entities().unwrap();
        domain_participant_factory.delete_participant(domain_participant).unwrap();
    }

    #[test]
    fn volatile_late_joiner_receives_gap_for_open_set_member() {
        // A volatile reader that joins mid coherent set must never receive a DATA whose
        // set start was GAPped. The writer GAPs those members instead, so on the reader
        // side the open-set member (sn5) arrives as an irrelevant (GAP) change, not DATA,
        // and nothing is buffered for the incomplete set.
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
                "volatile_coherent_gap",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        // The publisher must offer the same coherent presentation as the subscriber requests,
        // otherwise presentation QoS is incompatible and the endpoints never match.
        let publisher_qos = PublisherQos {
            presentation: PresentationQosPolicy {
                access_scope: PresentationQosAccessScopeKind::Topic,
                coherent_access: true,
                ordered_access: false,
            },
            ..Default::default()
        };
        let publisher =
            participant.create_publisher(publisher_qos, None, StatusMask::default()).unwrap();

        let writer_qos = DataWriterQos {
            durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile },
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let writer = publisher
            .create_datawriter::<HelloWorld>(&topic, writer_qos, None, StatusMask::default())
            .unwrap();

        // Open a coherent set and write sn1..4 before any reader has matched.
        publisher.begin_coherent_changes().unwrap();
        for index in 0..4 {
            let data = HelloWorld { index, message: "member".to_string() };
            writer.write(&data, InstanceHandle::NIL).unwrap();
        }

        // A coherent subscriber so the reader buffers set members when it does receive DATA.
        let subscriber_qos = SubscriberQos {
            presentation: PresentationQosPolicy {
                access_scope: PresentationQosAccessScopeKind::Topic,
                coherent_access: true,
                ordered_access: false,
            },
            ..Default::default()
        };
        let subscriber =
            participant.create_subscriber(subscriber_qos, None, StatusMask::default()).unwrap();
        let reader_qos = DataReaderQos {
            durability: DurabilityQosPolicy { kind: DurabilityQosPolicyKind::Volatile },
            history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll, strict: true },
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: Duration::from_seconds(1),
            },
            ..Default::default()
        };
        let data_reader = subscriber
            .create_datareader::<HelloWorld>(&topic, reader_qos, None, StatusMask::default())
            .unwrap();

        // Wait for the reader to late-join: its match horizon is fixed at sn4.
        let wait_set = WaitSet::new();
        let condition = writer.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set
            .wait(Duration::from_seconds(10))
            .expect("timed out waiting for PUBLICATION_MATCHED");
        wait_set.detach_condition(condition).unwrap();
        let condition = data_reader.get_statuscondition().unwrap().clone();
        condition.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(condition.clone()).unwrap();
        wait_set
            .wait(Duration::from_seconds(10))
            .expect("timed out waiting for SUBSCRIPTION_MATCHED");
        wait_set.detach_condition(condition).unwrap();

        // Write one more member (sn5) while the set is still open.
        let data = HelloWorld { index: 4, message: "member".to_string() };
        writer.write(&data, InstanceHandle::NIL).unwrap();

        let writer_guid = writer.guid();
        let rtps_reader = data_reader.get_rtps_reader().unwrap();
        let stateful_reader =
            rtps_reader.as_any().downcast_ref::<StatefulReader>().expect("stateful reader");
        let writer_proxies = stateful_reader.writer_proxies();
        let reader_cache = rtps_reader.reader_cache();

        // Poll until sn5 has been received on the reader side (as DATA or GAP).
        let sn5 = SequenceNumber::from_i64(5);
        let mut is_relevant = None;
        for _ in 0..100 {
            is_relevant = writer_proxies
                .lock()
                .unwrap()
                .first()
                .and_then(|wp| wp.received_change_is_relevant(sn5));
            if is_relevant.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        // sn5 must arrive as an irrelevant (GAP) change, and no member of the incomplete
        // set may be buffered on the reader.
        assert_eq!(is_relevant, Some(false), "sn5 should be received via GAP, not DATA");
        assert_eq!(
            reader_cache.lock().unwrap().pending_coherent_len(writer_guid),
            0,
            "no member of the GAPped set should be buffered"
        );

        publisher.end_coherent_changes().unwrap();

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
}
