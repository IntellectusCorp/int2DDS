//! DataWriter - The interface for publishing data samples to a topic.
//!
//! A `DataWriter<T>` is the primary interface for sending data to subscribers. It provides
//! type-safe publication of data samples, with support for reliable or best-effort delivery,
//! instance lifecycle management, and QoS policies.
//!
//! # Overview
//!
//! DataWriters are created by a `Publisher` and associated with a specific `Topic`. Each
//! DataWriter publishes data of a specific type (the generic parameter `T`), which must
//! implement the `DdsType` trait.
//!
//! # Key Features
//!
//! - **Type-Safe Publishing**: Generic over data type `T`
//! - **Reliable/Best-Effort**: Configurable via QoS policies
//! - **Instance Management**: Register, write, unregister, dispose
//! - **Acknowledgment**: Wait for readers to acknowledge data
//! - **Status Notifications**: Callbacks for publication matched, deadline missed, etc.

use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    marker::PhantomData,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
};

use arc_swap::ArcSwap;

use log::debug;

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
        types::InstanceState,
    },
    infrastructure::{
        deadline_monitor::DeadlineMonitor,
        domain_entity::DomainEntity,
        entity::{
            impl_check_parent_enabled, impl_dds_entity, impl_dds_entity_impl, BaseEntity,
            EnableChild, Entity, EntityInternal, UpdateStatus,
        },
        history_cache::HistoryCache as _,
        qos_policy::{
            DataRepresentationId, LivelinessQosPolicyKind, Qos, ReliabilityQosPolicyKind,
        },
        status::{
            LivelinessLostStatus, OfferedDeadlineMissedStatus, OfferedIncompatibleQosStatus,
            PublicationMatchedStatus, StatusInfo, StatusKind, StatusMask,
        },
        status_condition::StatusCondition,
    },
    publication::data_writer_history::DataWriterHistoryCache,
    rtps::{
        common::{
            entity_kind::EntityKind,
            guid::Guid,
            sequence::SequenceNumber,
            time::RtpsTime,
            types::{ChangeKind, SerializedData},
        },
        entities::writer::Writer as RtpsWriter,
        logic::wlp_logic::WlpLogic,
    },
    topic::{
        topic::Topic,
        type_support::{SerializationFormat, TypeSupport},
    },
    DdsType,
};

use super::{data_writer_listener::DataWriterListener, publisher::Publisher, qos::DataWriterQos};

// Since Pub/Sub must contain multiple types of DataWriter/Reader<Foo>,
// trait objects are used for runtime polymorphism instead of generics
pub trait DataWriterBase: DomainEntity + Send + Any {
    fn assert_liveliness(&self) -> DdsResult<()>;
    fn get_liveliness_lost_status(&self) -> DdsResult<LivelinessLostStatus>;
    fn get_offered_deadline_missed_status(&self) -> DdsResult<OfferedDeadlineMissedStatus>;
    fn get_offered_incompatible_qos_status(&self) -> DdsResult<OfferedIncompatibleQosStatus>;
    fn get_publication_matched_status(&self) -> DdsResult<PublicationMatchedStatus>;
    fn get_topic(&self) -> DdsResult<Topic>;
    fn get_publisher(&self) -> DdsResult<Publisher>;
    fn get_matched_subscriptions(&self) -> DdsResult<Vec<InstanceHandle>>;
    fn get_matched_subscription_data(
        &self,
        subscription_handle: InstanceHandle,
    ) -> DdsResult<SubscriptionBuiltinTopicData>;
    fn wait_for_acknowledgments(&self, max_wait: Duration) -> DdsResult<()>;
}

pub(crate) trait DataWriterInternal: DataWriterBase {
    fn disable(&self) -> DdsResult<()>;
    fn clone_boxed(&self) -> Box<dyn DataWriterInternal<Qos = DataWriterQos> + Send>;
    fn as_any(&self) -> &dyn Any;
    fn get_type_id(&self) -> TypeId;
    fn delete(&self);
    fn is_deleted(&self) -> DdsResult<()>;
    fn is_builtin(&self) -> bool;
}

pub struct DataWriter<Foo> {
    // Indicates whether this entity is a built-in entity.
    //
    // Currently always `false` as DDS spec does not define built-in
    // DataWriter exposed to users.
    //
    // TODO: Reserved for future DCPS-RTPS built-in entity mapping
    // if needed (e.g., exposing built-in writers for diagnostics).
    is_builtin: bool,
    guid: Guid,
    qos: Arc<ArcSwap<DataWriterQos>>,
    // Serializes set_qos so that (cache store + update_rtps_entity) executes
    // as a unit. get_qos reads are lock-free via ArcSwap.
    update_lock: Arc<Mutex<()>>,
    listener: Arc<RwLock<Option<Arc<dyn DataWriterListener<Foo = Foo>>>>>,
    mask: Arc<RwLock<StatusMask>>,
    status_condition: Arc<Mutex<StatusCondition<DataWriterQos>>>,
    pub(crate) self_ref: Arc<Mutex<Option<Arc<DataWriter<Foo>>>>>,
    enabled: Arc<AtomicBool>,
    deleted: Arc<AtomicBool>,
    topic: Option<Weak<Topic>>,
    type_support: Arc<dyn TypeSupport>,
    publisher: Option<Weak<Publisher>>,
    rtps_writer: Arc<Mutex<Option<Weak<dyn RtpsWriter + Send + Sync>>>>,
    key_instances: Arc<Mutex<HashMap<SerializedData, InstanceHandle>>>, // <serialized_key, ih>
    #[allow(clippy::type_complexity)]
    instances: Arc<Mutex<HashMap<InstanceHandle, (SerializedData, Time, InstanceState)>>>, // for instance managing
    liveliness_lost_status: Arc<Mutex<LivelinessLostStatus>>,
    offered_deadline_missed_status: Arc<Mutex<OfferedDeadlineMissedStatus>>,
    offered_incompatible_qos_status: Arc<Mutex<OfferedIncompatibleQosStatus>>,
    publication_matched_status: Arc<Mutex<PublicationMatchedStatus>>,
    deadline_monitor: Arc<Mutex<Option<DeadlineMonitor>>>,
    _phantom: PhantomData<fn() -> Foo>,
    datawriter_cache: Arc<Mutex<DataWriterHistoryCache<Foo>>>,
    wlp_logic: Option<WlpLogic>,
}

impl<Foo> Debug for DataWriter<Foo> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DataWriter")
            .field("guid", &self.guid)
            .field("qos", &**self.qos.load())
            .field(
                "listener",
                &self.listener.read().unwrap().as_ref().map(|_| "Arc<dyn DataWriterListener>"),
            )
            .field("mask", &self.mask.read().unwrap())
            .field("status_condition", &self.status_condition.lock().unwrap())
            .field("self_ref", &self.self_ref.lock().unwrap().as_ref().map(|_| "Arc<DataWriter>"))
            .field("enabled", &self.enabled.load(std::sync::atomic::Ordering::Acquire))
            .field("deleted", &self.deleted.load(std::sync::atomic::Ordering::Acquire))
            .field("topic", &self.topic.as_ref().map(|_| "Weak<Topic>"))
            .field("type_support", &"Arc<dyn TypeSupport>")
            .field("publisher", &self.publisher.as_ref().map(|_| "Weak<Publisher>"))
            .field("rtps_writer", &"Weak<dyn RtpsWriter>")
            .field("key_instances", &self.key_instances.lock().unwrap())
            .field("instances", &self.instances.lock().unwrap())
            .field("liveliness_lost_status", &self.liveliness_lost_status.lock().unwrap())
            .field(
                "offered_deadline_missed_status",
                &self.offered_deadline_missed_status.lock().unwrap(),
            )
            .field(
                "offered_incompatible_qos_status",
                &self.offered_incompatible_qos_status.lock().unwrap(),
            )
            .field("publication_matched_status", &self.publication_matched_status.lock().unwrap())
            .field("_phantom", &self._phantom)
            .finish()
    }
}

impl<Foo: 'static + Clone> Clone for DataWriter<Foo> {
    fn clone(&self) -> Self {
        Self {
            is_builtin: self.is_builtin,
            guid: self.guid,
            qos: self.qos.clone(),
            update_lock: self.update_lock.clone(),
            listener: self.listener.clone(),
            mask: self.mask.clone(),
            status_condition: self.status_condition.clone(),
            self_ref: self.self_ref.clone(),
            enabled: self.enabled.clone(),
            deleted: self.deleted.clone(),
            topic: self.topic.clone(),
            type_support: self.type_support.clone(),
            publisher: self.publisher.clone(),
            rtps_writer: self.rtps_writer.clone(),
            key_instances: self.key_instances.clone(),
            instances: self.instances.clone(),
            liveliness_lost_status: self.liveliness_lost_status.clone(),
            offered_deadline_missed_status: self.offered_deadline_missed_status.clone(),
            offered_incompatible_qos_status: self.offered_incompatible_qos_status.clone(),
            publication_matched_status: self.publication_matched_status.clone(),
            deadline_monitor: self.deadline_monitor.clone(),
            _phantom: self._phantom,
            datawriter_cache: self.datawriter_cache.clone(),
            wlp_logic: self.wlp_logic.clone(),
        }
    }
}

// When user doesn't call delete_datawriter and automatic drop occurs when going out of scope,
// it must not be deleted from Publisher.
impl<Foo> Drop for DataWriter<Foo> {
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

        if !self.deleted.load(Ordering::SeqCst) {
            if let Some(ref publisher_weak) = self.publisher {
                if let Some(publisher) = publisher_weak.upgrade() {
                    if let Some(ref topic_weak) = self.topic {
                        if let Some(topic) = topic_weak.upgrade() {
                            match topic.get_instance_handle() {
                                Ok(topic_handle) => {
                                    let writer_handle = InstanceHandle::from_guid(&self.guid);
                                    publisher.handle_writer_drop(
                                        topic.get_name(),
                                        &topic_handle,
                                        &writer_handle,
                                    );
                                }
                                Err(e) => log::error!("Failed to get topic handle: {:?}", e),
                            }
                        }
                    }
                }
            }
        }
    }
}

impl_dds_entity!(DataWriter<Foo>, DataWriterQos, Foo: 'static + Clone);
impl<Foo: 'static + Clone> DomainEntity for DataWriter<Foo> {}
impl<Foo: 'static + Clone> EnableChild for DataWriter<Foo> {
    fn enable_rtps_entities(&self) -> DdsResult<()> {
        let publisher = self.get_publisher()?;
        let participant = publisher.get_participant()?;
        let topic = self.get_topic()?;
        let mut publication_builtin_topic_data = PublicationBuiltinTopicData::new(
            &self.get_qos()?,
            &publisher.get_qos()?,
            &topic.get_qos()?,
        );
        publication_builtin_topic_data.set_topic_name(topic.get_name().to_string());
        publication_builtin_topic_data.set_type_name(topic.get_type_name().to_string());
        publication_builtin_topic_data.set_endpoint_guid(self.guid);

        // Set TypeIdentifier and TypeObject for DDS-XTypes discovery
        if let Some(type_id) = self.type_support.get_type_identifier() {
            publication_builtin_topic_data.set_type_identifier(Some(type_id));
        }
        if let Some(type_obj) = self.type_support.get_type_object() {
            publication_builtin_topic_data.set_type_object(Some(type_obj));
        }

        let status_callback = self.create_status_callback()?;

        let mut dcps_bridge = participant.get_dcps_bridge()?;
        let rtps_writer = match dcps_bridge.as_mut() {
            Some(dcps_bridge) => {
                let writer = dcps_bridge
                    .create_rtps_writer(publication_builtin_topic_data, Some(status_callback))
                    .map_err(|e| DdsError::Error(e.message))?;
                writer
            }
            None => return Err(DdsError::Error("DCPS Bridge is not initialized".to_string())),
        };

        drop(dcps_bridge);

        {
            self.datawriter_cache
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?
                .set_rtpswriter(Some(Arc::downgrade(&rtps_writer)));
        }

        {
            *self.rtps_writer.lock().map_err(|e| DdsError::Error(e.to_string()))? =
                Some(Arc::downgrade(&rtps_writer));
        }

        Ok(())
    }
    fn update_rtps_entity(&self, qos: &Self::Qos) -> DdsResult<()> {
        let publisher = self.get_publisher()?;
        let topic = self.get_topic()?;
        let mut publication_builtin_topic_data =
            PublicationBuiltinTopicData::new(qos, &publisher.get_qos()?, &topic.get_qos()?);
        publication_builtin_topic_data.set_topic_name(topic.get_name().to_string());
        publication_builtin_topic_data.set_type_name(topic.get_type_name().to_string());
        publication_builtin_topic_data.set_endpoint_guid(self.guid);

        // Set TypeIdentifier and TypeObject for DDS-XTypes discovery
        if let Some(type_id) = self.type_support.get_type_identifier() {
            publication_builtin_topic_data.set_type_identifier(Some(type_id));
        }
        if let Some(type_obj) = self.type_support.get_type_object() {
            publication_builtin_topic_data.set_type_object(Some(type_obj));
        }

        let rtps_writer = self.get_rtps_writer()?;
        rtps_writer
            .set_publication_builtin_topic_data(publication_builtin_topic_data.clone())
            .map_err(|e| DdsError::Error(e.message))?;

        let participant = publisher.get_participant()?;
        let dcps_bridge = participant.get_dcps_bridge()?;
        let dcps_bridge = dcps_bridge
            .as_ref()
            .ok_or(DdsError::Error("DCPS Bridge is not initialized".to_string()))?;
        dcps_bridge
            .update_writer(&rtps_writer, publication_builtin_topic_data)
            .map_err(|e| DdsError::Error(e.message))?;

        Ok(())
    }

    impl_check_parent_enabled!(get_publisher);
}
impl<Foo: 'static + Clone> UpdateStatus for DataWriter<Foo> {
    fn update_status(
        &self,
        status: StatusKind,
        info: Option<Arc<dyn StatusInfo>>,
    ) -> DdsResult<()> {
        match status {
            StatusKind::INCONSISTENT_TOPIC => self.get_topic()?.update_status(status, info),
            StatusKind::OFFERED_DEADLINE_MISSED => {
                let info = Arc::downcast::<OfferedDeadlineMissedStatus>(
                    info.ok_or(DdsError::BadParameter)?,
                )
                .map_err(|_| DdsError::BadParameter)?;
                self.handle_offered_deadline_missed_status(info)
            }
            StatusKind::OFFERED_INCOMPATIBLE_QOS => {
                let info = Arc::downcast::<OfferedIncompatibleQosStatus>(
                    info.ok_or(DdsError::BadParameter)?,
                )
                .map_err(|_| DdsError::BadParameter)?;
                self.handle_offered_incompatible_qos_status(info)
            }
            StatusKind::LIVELINESS_LOST => {
                if info.is_some() {
                    return Err(DdsError::BadParameter);
                }
                self.handle_liveliness_lost_status()
            }
            StatusKind::PUBLICATION_MATCHED => {
                let info =
                    Arc::downcast::<PublicationMatchedStatus>(info.ok_or(DdsError::BadParameter)?)
                        .map_err(|_| DdsError::BadParameter)?;
                self.handle_publication_matched_status(info)
            }
            status => {
                log::warn!("Unknown status received for DataWriter - StatusKind: {:?}", status);
                Err(DdsError::BadParameter)
            }
        }
    }
}

// DataWriter abstract class
impl<Foo: 'static + Clone> DataWriter<Foo> {
    // DataWriter should be specialized for each data type.
    // Trait defining methods that should be defined in auto-generated class for <Foo>
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        is_builtin: bool,
        guid: Guid,
        type_support: Arc<dyn TypeSupport + Send>,
        topic: &Arc<Topic>,
        qos: DataWriterQos,
        listener: Option<Arc<dyn DataWriterListener<Foo = Foo>>>,
        mask: StatusMask,
        publisher: &Arc<Publisher>,
        wlp_logic: Option<WlpLogic>,
    ) -> DdsResult<Self> {
        let writer = Self {
            is_builtin,
            guid,
            qos: Arc::new(ArcSwap::from_pointee(qos.clone())),
            update_lock: Arc::new(Mutex::new(())),
            listener: Arc::new(RwLock::new(listener)),
            mask: Arc::new(RwLock::new(mask)),
            status_condition: Arc::new(Mutex::new(StatusCondition::new(None))),
            self_ref: Arc::new(Mutex::new(None)),
            type_support: type_support.clone(),
            topic: Some(Arc::downgrade(topic)),
            publisher: Some(Arc::downgrade(publisher)),
            rtps_writer: Arc::new(Mutex::new(None)),
            key_instances: Arc::new(Mutex::new(HashMap::new())),
            instances: Arc::new(Mutex::new(HashMap::new())),
            enabled: Arc::new(AtomicBool::new(false)),
            deleted: Arc::new(AtomicBool::new(false)),
            liveliness_lost_status: Arc::new(Mutex::new(LivelinessLostStatus::default())),
            offered_deadline_missed_status: Arc::new(Mutex::new(
                OfferedDeadlineMissedStatus::default(),
            )),
            offered_incompatible_qos_status: Arc::new(Mutex::new(
                OfferedIncompatibleQosStatus::default(),
            )),
            publication_matched_status: Arc::new(Mutex::new(PublicationMatchedStatus::default())),
            deadline_monitor: Arc::new(Mutex::new(None)),
            _phantom: PhantomData,
            datawriter_cache: Arc::new(Mutex::new(DataWriterHistoryCache::<Foo>::new(
                Weak::new(),
                qos.reliability,
                qos.history,
                qos.resource_limits,
                type_support.is_compute_key_provided(),
            ))),
            wlp_logic,
        };
        let writer_arc = Arc::new(writer.clone());
        let weak_ref = Arc::downgrade(&writer_arc);
        {
            let mut status_condition =
                writer.status_condition.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            *status_condition = StatusCondition::new(Some(weak_ref.clone()));
        }
        {
            let mut datawriter_cache =
                writer.datawriter_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            datawriter_cache.set_datawriter(weak_ref);
        }
        {
            let mut self_ref =
                writer.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            *self_ref = Some(writer_arc);
        }
        let period = writer.get_qos()?.deadline.period;
        if !period.is_infinite()
            && (guid.entity_kind() == EntityKind::USER_DEFINED_WRITER_WITH_KEY
                || guid.entity_kind() == EntityKind::USER_DEFINED_WRITER_NO_KEY)
        {
            let status_callback = writer.create_status_callback()?;
            let mut deadline_monitor =
                writer.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            *deadline_monitor = Some(DeadlineMonitor::new(period, status_callback, true));
        }
        Ok(writer)
    }

    fn resolve_serialization_format(
        data_representation: &[DataRepresentationId],
        extensibility: crate::serialize::xcdr::ExtensibilityKind,
    ) -> DdsResult<SerializationFormat> {
        let supported = if data_representation.is_empty() {
            &[DataRepresentationId::XcdrDataRepresentation][..]
        } else {
            data_representation
        };

        for representation in supported {
            match representation {
                DataRepresentationId::XcdrDataRepresentation => {
                    return Ok(SerializationFormat::Cdr);
                }
                DataRepresentationId::Xcdr2DataRepresentation => {
                    let use_delimiters = matches!(
                        extensibility,
                        crate::serialize::xcdr::ExtensibilityKind::Appendable
                            | crate::serialize::xcdr::ExtensibilityKind::Mutable
                    );
                    return Ok(SerializationFormat::Xcdr {
                        extensibility_kind: extensibility,
                        use_delimiters,
                    });
                }
                DataRepresentationId::XmlDataRepresentation => {
                    log::warn!(
                        "XML DataRepresentation is not supported; ignoring preference for now"
                    );
                }
            }
        }

        Err(DdsError::Error("No supported DataRepresentationId found in QoS policy".to_string()))
    }

    fn validate_timestamp(timestamp: &Time) -> DdsResult<()> {
        if timestamp.is_infinite() || timestamp.sec < 0 || !timestamp.is_valid() {
            return Err(DdsError::BadParameter);
        }
        Ok(())
    }

    fn resolve_handle(
        instance_handle: InstanceHandle,
        user_handle: InstanceHandle,
    ) -> DdsResult<InstanceHandle> {
        if user_handle.is_nil() {
            Ok(instance_handle)
        } else if user_handle != instance_handle {
            Err(DdsError::PreconditionNotMet)
        } else {
            Ok(instance_handle)
        }
    }

    fn resolve_dispose_key(
        &self,
        serialized_key: SerializedData,
        computed_handle: InstanceHandle,
        user_handle: InstanceHandle,
    ) -> DdsResult<(SerializedData, InstanceHandle)> {
        let instance_handle = {
            let key_instances =
                self.key_instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            key_instances.get(&serialized_key).copied().unwrap_or(computed_handle)
        };
        let resolved = Self::resolve_handle(instance_handle, user_handle)?;
        Ok((serialized_key, resolved))
    }

    fn resolve_unregister_key(
        &self,
        serialized_key: SerializedData,
        user_handle: InstanceHandle,
    ) -> DdsResult<(SerializedData, InstanceHandle)> {
        let instance_handle = {
            let key_instances =
                self.key_instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            match key_instances.get(&serialized_key) {
                Some(h) => *h,
                None => return Err(DdsError::BadParameter),
            }
        };
        let resolved = Self::resolve_handle(instance_handle, user_handle)?;
        Ok((serialized_key, resolved))
    }

    /// Disposes of a data instance, indicating it is no longer valid.
    ///
    /// This operation requests that the middleware delete the data instance. The actual deletion
    /// is deferred until the instance is no longer being used anywhere in the system. DataReaders
    /// that already know about the instance will be notified of the disposal through changes in
    /// the instance state.
    ///
    /// This operation does not modify the instance's value - the `data` parameter is used only
    /// to identify the instance through its key fields. The service automatically assigns a
    /// source timestamp when this operation is called.
    ///
    /// # Arguments
    ///
    /// * `data` - The data instance to dispose (used only for identifying the instance via key fields)
    /// * `handle` - Instance handle identifying the instance:
    ///   - Use `InstanceHandle::NIL` to automatically identify the instance from the data's key fields
    ///   - Use a handle returned by `register_instance()` to explicitly specify the instance
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or a `DdsError` if the operation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * Same error conditions as `unregister_instance()` for invalid handles
    /// * `Timeout` - Same conditions as `write()` when RELIABILITY QoS is RELIABLE
    /// * `OutOfResources` - Resource limits exceeded
    /// * The writer has been deleted
    pub fn dispose(&self, data: &Foo, handle: InstanceHandle) -> DdsResult<()> {
        /*
            This operation requests the middleware to delete the data (actual deletion is deferred until the data is no longer in use anywhere in the system).
            Typically, the application learns of the deletion through DataReader objects that already knew about the instance (for details, see 2.2.2.5 Subscription Module).
            This operation does not modify the value of the instance. The instance parameter is passed solely for identifying the instance.
            When this operation is used, the Service automatically provides a source_timestamp value. This value can be accessed by DataReader objects through the source_timestamp attribute in SampleInfo.
            Constraints on the handle parameter value and corresponding error behavior are the same as specified in the unregister_instance operation (2.2.2.4.2.7).
            This operation can block and return TIMEOUT under the same conditions as the write operation (2.2.2.4.2.11).
            Additionally, this operation can return OUT_OF_RESOURCES error under the same conditions as the write operation.
        */
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time().unwrap();
        self.dispose_w_timestamp(data, handle, timestamp)
    }

    pub fn dispose_w_timestamp(
        &self,
        data: &Foo,
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        // let start_time = Time::now();
        self.is_enabled()?;

        if !self.type_support.is_compute_key_provided() {
            log::warn!("dispose on no-key topic has no effect");
            return Ok(());
        }

        Self::validate_timestamp(&timestamp)?;

        let serialized_key = self.type_support.serialize_key(data as &dyn Any)?;
        let computed_handle = self.type_support.compute_key(data as &dyn Any);
        let (serialized_key, resolved_handle) =
            self.resolve_dispose_key(serialized_key, computed_handle, handle)?;

        self.dispose_inner(serialized_key, resolved_handle, timestamp)
    }

    /// Publishes a data sample to the topic.
    ///
    /// This is the primary method for sending data to subscribers. The service automatically
    /// assigns a source timestamp when this operation is called. This timestamp is available
    /// to DataReaders through the `source_timestamp` field in `SampleInfo`.
    ///
    /// As a side effect, this operation asserts liveliness for the DataWriter, its Publisher,
    /// and the DomainParticipant.
    ///
    /// # Arguments
    ///
    /// * `data` - The data sample to publish
    /// * `handle` - Instance handle identifying the data instance:
    ///   - Use `InstanceHandle::NIL` to automatically identify the instance from the data's key fields
    ///   - Use a handle returned by `register_instance()` to explicitly specify the instance
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success, or a `DdsError` if the write operation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `PreconditionNotMet` - The handle is valid but doesn't match the data instance
    /// * `BadParameter` - The handle is invalid
    /// * `Timeout` - Reliability QoS is RELIABLE and max_blocking_time expired before space became available
    /// * `OutOfResources` - Resource limits exceeded and space won't become available
    /// * The writer has been deleted
    ///
    /// # Blocking Behavior
    ///
    /// If the RELIABILITY QoS is set to RELIABLE, this operation may block if:
    /// * Data would be lost or RESOURCE_LIMITS would be exceeded
    /// * The operation blocks for up to `max_blocking_time` specified in the RELIABILITY QoS
    /// * If space isn't available within that time, returns `Timeout` error
    pub fn write(&self, data: &Foo, handle: InstanceHandle) -> DdsResult<()> {
        // in: data: <Foo>, handle: InstanceHandle
        // out: DdsError_t
        /*
            This operation modifies the value of a data instance. When this operation is used, the Service automatically provides a source_timestamp value.
            This value can be accessed by DataReader objects through the source_timestamp attribute in SampleInfo.
            For more details on data timestamps, refer to 2.2.2.5 (Subscription Module) and 2.2.3.17 for the DESTINATION_ORDER QoS policy.
            This operation should be provided in the specialized class generated for the specific application data type being written.
            By doing so, the data passed through the data argument will have the correct application-defined type (e.g., Foo).
                -> A class specialized for a specific data type like DataWriter<Foo>..
            As a side effect, this operation asserts liveliness for the DataWriter itself, its Publisher, and the DomainParticipant.
            The handle parameter can use the special value HANDLE_NIL.
            This means the instance identification is automatically inferred from instance_data (through the key).
            If the handle is a value other than HANDLE_NIL, it must be the value returned by register_instance when the instance was registered.
            Otherwise, the behavior is as follows:
            - If the handle corresponds to an existing instance but does not match the instance pointed to by the data argument,
              the behavior is generally undefined, but if the Service implementation can detect this, the operation should fail and return error code PRECONDITION_NOT_MET.
            - If the handle corresponds to a non-existent instance, the behavior is generally undefined, but if detectable, error code BAD_PARAMETER should be returned.
            If the RELIABILITY QoS kind is set to RELIABLE, this write operation can block when data would be lost or the limits specified by RESOURCE_LIMITS are exceeded.
            In such cases, the max_blocking_time setting of RELIABILITY determines the maximum duration the write operation can block until space becomes available.
            If space to store the data cannot be secured within that time, the operation fails and returns a TIMEOUT error.
            Specifically, the DataWriter's write operation may block in the following cases (though this list is not exhaustive):
            - If RESOURCE_LIMITS.max_samples < RESOURCE_LIMITS.max_instances * HISTORY.depth, when max_samples limit is exceeded, the Service may discard some samples from any instance (but maintain at least one sample per instance).
              If space is still insufficient after that, the write operation may block.
            - If RESOURCE_LIMITS.max_samples < RESOURCE_LIMITS.max_instances, the DataWriter may block regardless of HISTORY depth.
            Instead of blocking, the write operation may terminate immediately and return an OUT_OF_RESOURCES error, provided both of the following conditions are met:
            1. The reason for blocking is exceeding RESOURCE_LIMITS.
            2. The Service determines that the required resources are unlikely to become available within max_blocking_time (e.g., impossible unless the user unregisters an instance).
            If the provided handle is valid but does not match the instance referenced by data, the behavior is generally undefined, but if the Service can detect this, a PRECONDITION_NOT_MET error is returned.
            If the handle is invalid, the behavior is undefined or, if detectable, a BAD_PARAMETER error is returned.
        */
        // source_timestamp value provided somewhere... (source_timestamp attribute of SampleInfo)
        // Assert liveliness to Pub, DomainParticipant
        // if handle==InstanceHandle::NIL -> instance identification: automatically inferred through data.key
        // else
        //     if register_instance() != handle
        //
        // self.rtps_writer(handle, data related).history_cache.add_change(cache) // Rtps v2.5 Overview
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time()?;
        self.write_w_timestamp(data, handle, timestamp)
    }

    pub fn write_w_timestamp(
        &self,
        data: &Foo,
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        self.write_w_timestamp_inner(data, handle, timestamp)?;
        Ok(())
    }

    /// Write data and return the SampleIdentity (writer GUID + sequence number).
    /// `on_identity_assigned` is called with the allocated identity before serialization,
    /// allowing the caller to embed it into the data (e.g. RequestHeader.request_id). (7.8.1)
    /// Used by DDS-RPC for request-reply correlation.
    pub fn write_and_obtain_sample_identity(
        &self,
        data: &mut Foo,
        handle: InstanceHandle,
        on_identity_assigned: impl FnOnce(&mut Foo, Guid, SequenceNumber),
    ) -> DdsResult<(Guid, SequenceNumber)> {
        self.is_enabled()?;
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time()?;
        Self::validate_timestamp(&timestamp)?;

        let format = {
            let qos = self.qos.load();
            let extensibility = self.type_support.get_extensibility_kind();
            Self::resolve_serialization_format(&qos.data_representation.value, extensibility)?
        };

        let key_info = if self.type_support.is_compute_key_provided() {
            let serialized_key = self.type_support.serialize_key(data as &dyn Any)?;
            let computed_handle = self.type_support.compute_key(data as &dyn Any);
            Some((serialized_key, computed_handle))
        } else {
            None
        };

        let (instance_handle, _) = self.resolve_write_instance(key_info, handle, timestamp)?;

        let type_support = self.type_support.clone();
        let seq_num;
        {
            let rtps_writer = self.get_rtps_writer()?;
            let change = rtps_writer.new_change_with_rpc_callback(
                ChangeKind::Alive,
                instance_handle,
                Some(timestamp.into()),
                Box::new(move |guid, seq| {
                    on_identity_assigned(data, guid, seq);
                    type_support
                        .serialize(data as &dyn Any, Some(&format))
                        .unwrap_or_default()
                        .to_vec()
                }),
            );
            seq_num = change.sequence_number();
            let mut datawriter_cache =
                self.datawriter_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            datawriter_cache.add_change_with_cleanup(Arc::new(change))?;
        }

        self.update_liveliness()?;
        Ok((self.guid, seq_num))
    }

    // pub: needed by DDS-RPC for SampleIdentity construction
    pub fn guid(&self) -> Guid {
        self.guid
    }

    fn write_w_timestamp_inner(
        &self,
        data: &Foo,
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<SequenceNumber> {
        self.is_enabled()?;
        Self::validate_timestamp(&timestamp)?;

        let format = {
            let qos = self.qos.load();
            let extensibility = self.type_support.get_extensibility_kind();
            Self::resolve_serialization_format(&qos.data_representation.value, extensibility)?
        };

        let key_info = if self.type_support.is_compute_key_provided() {
            let serialized_key = self.type_support.serialize_key(data as &dyn Any)?;
            let computed_handle = self.type_support.compute_key(data as &dyn Any);
            Some((serialized_key, computed_handle))
        } else {
            None
        };

        let (instance_handle, _) = self.resolve_write_instance(key_info, handle, timestamp)?;

        let seq_num = self.add_change(
            ChangeKind::Alive,
            data as &dyn Any,
            &format,
            instance_handle,
            Some(timestamp.into()),
        )?;
        debug!("add_change completed in datawriter");

        self.update_liveliness()?;
        debug!("update_liveliness completed in datawriter");

        Ok(seq_num)
    }

    /// Write pre-serialized data directly, bypassing TypeSupport serialization.
    ///
    /// This is the primary write path for FFI/C users who serialize data in C
    /// using CDR utilities. The `serialized_data` must be a valid CDR-encoded
    /// byte sequence including the 4-byte encapsulation header.
    ///
    /// # Arguments
    /// * `serialized_data` - CDR-encoded bytes (with encapsulation header)
    /// * `serialized_key` - CDR-encoded key bytes for instance identification.
    ///   If `None`, `InstanceHandle::NIL` is used (unkeyed topic).
    pub fn write_serialized(
        &self,
        serialized_data: &[u8],
        serialized_key: Option<&[u8]>,
    ) -> DdsResult<()> {
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time()?;
        self.write_serialized_w_timestamp(serialized_data, serialized_key, timestamp)
    }

    /// Write pre-serialized data with an explicit timestamp.
    pub fn write_serialized_w_timestamp(
        &self,
        serialized_data: &[u8],
        serialized_key: Option<&[u8]>,
        timestamp: Time,
    ) -> DdsResult<()> {
        self.is_enabled()?;
        Self::validate_timestamp(&timestamp)?;

        let key_info = match serialized_key {
            Some(key_bytes) if !key_bytes.is_empty() => {
                let key_data: SerializedData = Arc::from(key_bytes);
                let computed_handle = Self::compute_instance_handle_from_key(key_bytes);
                Some((key_data, computed_handle))
            }
            _ => None,
        };

        let (instance_handle, _) =
            self.resolve_write_instance(key_info, InstanceHandle::NIL, timestamp)?;

        self.add_change_serialized(
            ChangeKind::Alive,
            serialized_data,
            instance_handle,
            Some(timestamp.into()),
        )?;

        self.update_liveliness()?;

        Ok(())
    }

    /// Compute an InstanceHandle from raw key bytes.
    /// If key_bytes fits in 16 bytes, it is used directly as the KeyHash.
    /// Otherwise, MD5 hash is computed.
    fn compute_instance_handle_from_key(key_bytes: &[u8]) -> InstanceHandle {
        if key_bytes.is_empty() {
            return InstanceHandle::NIL;
        }
        let mut hash = [0u8; 16];
        if key_bytes.len() <= 16 {
            hash[..key_bytes.len()].copy_from_slice(key_bytes);
        } else {
            let digest = md5::compute(key_bytes);
            hash.copy_from_slice(&digest.0);
        }
        InstanceHandle::new(hash)
    }

    /// Register an instance using raw serialized key bytes.
    pub fn register_instance_serialized(&self, key: &[u8]) -> DdsResult<InstanceHandle> {
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time()?;
        self.register_instance_serialized_w_timestamp(key, timestamp)
    }

    /// Register an instance using raw serialized key bytes with explicit timestamp.
    pub fn register_instance_serialized_w_timestamp(
        &self,
        key: &[u8],
        timestamp: Time,
    ) -> DdsResult<InstanceHandle> {
        self.is_enabled()?;

        if key.is_empty() {
            return Ok(InstanceHandle::NIL);
        }

        Self::validate_timestamp(&timestamp)?;

        let key_data: SerializedData = Arc::from(key);
        let handle = Self::compute_instance_handle_from_key(key);

        self.register_instance_inner(key_data, handle, timestamp)
    }

    /// Dispose an instance using raw serialized key bytes.
    pub fn dispose_serialized(&self, key: &[u8], handle: InstanceHandle) -> DdsResult<()> {
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time()?;
        self.dispose_serialized_w_timestamp(key, handle, timestamp)
    }

    /// Dispose an instance using raw serialized key bytes with explicit timestamp.
    pub fn dispose_serialized_w_timestamp(
        &self,
        key: &[u8],
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        self.is_enabled()?;

        if key.is_empty() {
            return Ok(());
        }

        Self::validate_timestamp(&timestamp)?;

        let serialized_key: SerializedData = Arc::from(key);
        let computed_handle = Self::compute_instance_handle_from_key(key);
        let (serialized_key, resolved_handle) =
            self.resolve_dispose_key(serialized_key, computed_handle, handle)?;

        self.dispose_inner(serialized_key, resolved_handle, timestamp)
    }

    /// Unregister an instance using raw serialized key bytes.
    pub fn unregister_instance_serialized(
        &self,
        key: &[u8],
        handle: InstanceHandle,
    ) -> DdsResult<()> {
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time()?;
        self.unregister_instance_serialized_w_timestamp(key, handle, timestamp)
    }

    /// Unregister an instance using raw serialized key bytes with explicit timestamp.
    pub fn unregister_instance_serialized_w_timestamp(
        &self,
        key: &[u8],
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        self.is_enabled()?;

        if key.is_empty() {
            return Ok(());
        }

        Self::validate_timestamp(&timestamp)?;

        let serialized_key: SerializedData = Arc::from(key);
        let (serialized_key, resolved_handle) =
            self.resolve_unregister_key(serialized_key, handle)?;

        self.unregister_instance_inner(serialized_key, resolved_handle, timestamp)
    }

    /// Lookup an instance handle from raw serialized key bytes.
    pub fn lookup_instance_serialized(&self, key: &[u8]) -> DdsResult<InstanceHandle> {
        if key.is_empty() {
            return Ok(InstanceHandle::NIL);
        }

        let serialized_key: SerializedData = Arc::from(key);
        let key_instances =
            self.key_instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        Ok(key_instances.get(&serialized_key).copied().unwrap_or(InstanceHandle::NIL))
    }

    /// Get the serialized key bytes for a given instance handle.
    pub fn get_key_value_serialized(&self, handle: InstanceHandle) -> DdsResult<Arc<[u8]>> {
        let instances = self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        match instances.get(&handle) {
            Some((key_data, _, _)) => Ok(key_data.clone()),
            None => Err(DdsError::BadParameter),
        }
    }

    pub fn get_key_value(&self, key_holder: &mut Foo, handle: InstanceHandle) -> DdsResult<()> {
        // in: key_holder: <Foo>, handle: InstanceHandle
        // out: DdsError_t, key_holder: <Foo>
        /*
            This operation can be used to retrieve the instance key corresponding to an instance_handle.
            This operation only fills in the fields that constitute the key within the key_holder instance.
            If the InstanceHandle_t given as a_handle does not correspond to an existing data object known by the DataWriter, this operation may return BAD_PARAMETER.
            If the implementation cannot check for an invalid handle, the result in this situation is undefined (i.e., may exhibit unspecified behavior).
        */
        self.is_enabled()?;

        if handle.is_nil() {
            return Err(DdsError::BadParameter);
        }

        {
            let instances = self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            if let Some((serialized_key, _time, _state)) = instances.get(&handle) {
                let instance_data = self.type_support.deserialize_key(serialized_key)?;
                if let Some(foo_data) = instance_data.downcast_ref::<Foo>() {
                    *key_holder = foo_data.clone();
                    Ok(()) // Return Ok on success
                } else {
                    Err(DdsError::Error(format!(
                        "Failed to downcast to {:?} type",
                        self.type_support.get_type_name()
                    )))
                }
            } else {
                Err(DdsError::BadParameter)
            }
        }
    }
    /// Look up an instance handle using pre-serialized key bytes.
    pub fn lookup_instance(&self, instance: &Foo) -> DdsResult<InstanceHandle> {
        // in: instance: <Foo>
        // out: InstanceHandle
        /*
            This operation takes an instance as an argument and returns a handle that can be used in subsequent operations that take an instance handle as an argument.
            The passed instance is used solely for examining the fields that define the key.
            This operation does not register the instance.
            If the instance has not been previously registered,
            or if for any other reason the service cannot provide an instance handle,
            the service returns the special value HANDLE_NIL.
        */
        self.is_deleted()?;
        let handle = self.type_support.compute_key(instance as &dyn Any);

        // Verify that the handle is actually registered
        {
            let instances = self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            Ok(match instances.get(&handle) {
                Some((_, _, _)) => handle,
                None => InstanceHandle::NIL, // unregistered instance
            })
        }
    }

    // For Entity
    pub fn set_listener(
        &self,
        listener: Option<Arc<dyn DataWriterListener<Foo = Foo>>>,
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
    pub fn get_listener(&self) -> DdsResult<Option<Arc<dyn DataWriterListener<Foo = Foo>>>> {
        self.is_deleted()?;
        match self.listener.read() {
            Ok(guard) => Ok(guard.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    #[inline]
    pub fn assert_liveliness(&self) -> DdsResult<()> {
        <Self as DataWriterBase>::assert_liveliness(self)
    }

    #[inline]
    pub fn get_liveliness_lost_status(&self) -> DdsResult<LivelinessLostStatus> {
        <Self as DataWriterBase>::get_liveliness_lost_status(self)
    }

    #[inline]
    pub fn get_offered_deadline_missed_status(&self) -> DdsResult<OfferedDeadlineMissedStatus> {
        <Self as DataWriterBase>::get_offered_deadline_missed_status(self)
    }

    #[inline]
    pub fn get_offered_incompatible_qos_status(&self) -> DdsResult<OfferedIncompatibleQosStatus> {
        <Self as DataWriterBase>::get_offered_incompatible_qos_status(self)
    }

    #[inline]
    pub fn get_publication_matched_status(&self) -> DdsResult<PublicationMatchedStatus> {
        <Self as DataWriterBase>::get_publication_matched_status(self)
    }

    #[inline]
    pub fn get_topic(&self) -> DdsResult<Topic> {
        <Self as DataWriterBase>::get_topic(self)
    }

    #[inline]
    pub fn get_publisher(&self) -> DdsResult<Publisher> {
        <Self as DataWriterBase>::get_publisher(self)
    }

    #[inline]
    pub fn get_matched_subscriptions(&self) -> DdsResult<Vec<InstanceHandle>> {
        <Self as DataWriterBase>::get_matched_subscriptions(self)
    }

    #[inline]
    pub fn get_matched_subscription_data(
        &self,
        subscription_handle: InstanceHandle,
    ) -> DdsResult<SubscriptionBuiltinTopicData> {
        <Self as DataWriterBase>::get_matched_subscription_data(self, subscription_handle)
    }

    /// Blocks the calling thread until all data written by this DataWriter has been acknowledged
    /// by all matched reliable DataReaders, or until the specified timeout expires.
    ///
    /// This operation is useful when you need to ensure that all previously written data has been
    /// delivered and acknowledged before proceeding. This is commonly used before shutting down
    /// a publisher to ensure all data has been delivered.
    ///
    /// The operation only waits for acknowledgments from DataReaders with RELIABLE reliability QoS.
    /// Best-effort readers do not send acknowledgments.
    ///
    /// # Arguments
    ///
    /// * `max_wait` - Maximum duration to wait for acknowledgments. Use `Duration::infinite()` to wait indefinitely.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if all data has been acknowledged, or a `DdsError` if the timeout expires
    /// or an error occurs.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `Timeout` - The max_wait duration expired before all data was acknowledged
    /// * The writer has been deleted
    /// * The writer is not enabled
    #[inline]
    pub fn wait_for_acknowledgments(&self, max_wait: Duration) -> DdsResult<()> {
        <Self as DataWriterBase>::wait_for_acknowledgments(self, max_wait)
    }

    pub(crate) fn is_enabled(&self) -> DdsResult<()> {
        self.is_deleted()?;
        if self.enabled.load(Ordering::SeqCst) {
            {
                let _ = self.get_rtps_writer()?;
            }
            Ok(())
        } else {
            Err(DdsError::NotEnabled)
        }
    }

    /// Pool-based add_change skeleton: acquire from pool → reset → fill buffer → add to history.
    /// Avoids per-write heap allocation by reusing CacheChange and its internal buffer.
    /// `fill` writes the payload into the reused buffer (already cleared by reset).
    fn add_change_pooled_with(
        &self,
        kind: ChangeKind,
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
        fill: impl FnOnce(&mut Vec<u8>) -> DdsResult<()>,
    ) -> DdsResult<SequenceNumber> {
        let rtps_writer = self.get_rtps_writer()?;

        // 1. Acquire from pool (fast: Vec::pop)
        let mut change = {
            let mut cache =
                self.datawriter_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            cache.acquire_change()
        };

        // 2. Allocate sequence number
        let seq_num = rtps_writer.allocate_sequence_number();

        // 3. Reset metadata + fill the reused buffer
        change.reset(kind, rtps_writer.guid(), handle, seq_num, source_timestamp);
        fill(change.data_mut())?;
        change.apply_fragmentation(rtps_writer.data_max_size_serialized() as usize);

        // 4. Add to history (may evict → release back to pool)
        {
            let mut cache =
                self.datawriter_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            cache.add_change_with_cleanup(Arc::new(change))?;
        }

        Ok(seq_num)
    }

    /// Pooled add_change for typed data: fills the buffer via TypeSupport serialization.
    fn add_change(
        &self,
        kind: ChangeKind,
        data: &dyn Any,
        format: &SerializationFormat,
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
    ) -> DdsResult<SequenceNumber> {
        self.add_change_pooled_with(kind, handle, source_timestamp, |buf| {
            self.type_support.serialize_into(data, buf, Some(format))
        })
    }

    /// Pooled add_change for pre-serialized bytes: copies the bytes into the reused buffer.
    /// Used by the serialized write path and dispose/unregister (empty payload).
    fn add_change_serialized(
        &self,
        kind: ChangeKind,
        bytes: &[u8],
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
    ) -> DdsResult<SequenceNumber> {
        self.add_change_pooled_with(kind, handle, source_timestamp, |buf| {
            buf.extend_from_slice(bytes);
            Ok(())
        })
    }

    fn register_instance_to_datawriter_cache(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        let mut datawriter_cache =
            self.datawriter_cache.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        datawriter_cache.register_instance(instance_handle)?;

        Ok(())
    }

    pub(crate) fn get_rtps_writer(&self) -> DdsResult<Arc<dyn RtpsWriter + Send + Sync>> {
        let rtps_writer = self
            .rtps_writer
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .as_ref()
            .ok_or(DdsError::Error("RTPS Writer is not initialized".to_string()))?
            .upgrade()
            .ok_or(DdsError::Error("RTPS Writer is not initialized".to_string()))?;
        Ok(rtps_writer)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn create_status_callback(
        &self,
    ) -> DdsResult<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>> {
        let self_ref = self.self_ref.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let self_ref = self_ref
            .as_ref()
            .ok_or(DdsError::Error("DataWriter is not properly initialized".to_string()))?;
        let weak_self = Arc::downgrade(self_ref);
        Ok(Arc::new(move |status, info| {
            if let Some(entity) = weak_self.upgrade() {
                let _ = entity.update_status(status, info);
            }
        }))
    }

    fn take_liveliness_lost_status(&self) -> DdsResult<LivelinessLostStatus> {
        let mut status_guard =
            self.liveliness_lost_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_offered_deadline_missed_status(&self) -> DdsResult<OfferedDeadlineMissedStatus> {
        let mut status_guard = self
            .offered_deadline_missed_status
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_offered_incompatible_qos_status(&self) -> DdsResult<OfferedIncompatibleQosStatus> {
        let mut status_guard = self
            .offered_incompatible_qos_status
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?;
        let result = status_guard.clone();

        // Reset count_change
        status_guard.total_count_change = 0;
        Ok(result)
    }
    fn take_publication_matched_status(&self) -> DdsResult<PublicationMatchedStatus> {
        let mut status_guard =
            self.publication_matched_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // Reset count_change
        status_guard.current_count_change = 0;
        status_guard.total_count_change = 0;
        Ok(result)
    }

    fn set_communication_status_propagation(
        &self,
        status_kind: &StatusKind,
        trigger_value: bool,
    ) -> DdsResult<()> {
        // 1. DataWriter StatusCondition
        self.get_statuscondition()?.set_communication_status(status_kind, trigger_value)?;
        // 2. Publisher StatusCondition
        let publisher = self.get_publisher()?;
        publisher.set_communication_status(status_kind, trigger_value)?;
        // 3. DomainParticipant StatusCondition
        publisher.get_participant()?.set_communication_status(status_kind, trigger_value)?;

        Ok(())
    }

    fn handle_offered_deadline_missed_status(
        &self,
        info: Arc<OfferedDeadlineMissedStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .offered_deadline_missed_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            status_guard.last_instance_handle = info.last_instance_handle();
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::OFFERED_DEADLINE_MISSED) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_offered_deadline_missed(self, &status);
                listener_called = true;
            }
            let publisher = self.get_publisher()?;
            if let Some(listener) = publisher.get_listener()? {
                listener.on_offered_deadline_missed(self, &status);
                listener_called = true;
            }
            let participant = publisher.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_offered_deadline_missed(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_offered_deadline_missed_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::OFFERED_DEADLINE_MISSED, true)?;

        Ok(())
    }

    fn handle_offered_incompatible_qos_status(
        &self,
        info: Arc<OfferedIncompatibleQosStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .offered_incompatible_qos_status
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
        if mask.contains(StatusKind::OFFERED_INCOMPATIBLE_QOS) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_offered_incompatible_qos(self, &status);
                listener_called = true;
            }
            let publisher = self.get_publisher()?;
            if let Some(listener) = publisher.get_listener()? {
                listener.on_offered_incompatible_qos(self, &status);
                listener_called = true;
            }
            let participant = publisher.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_offered_incompatible_qos(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_offered_incompatible_qos_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::OFFERED_INCOMPATIBLE_QOS, true)?;

        Ok(())
    }

    fn handle_liveliness_lost_status(&self) -> DdsResult<()> {
        let status = {
            let mut status_guard =
                self.liveliness_lost_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.total_count += 1;
            status_guard.total_count_change += 1;
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::LIVELINESS_LOST) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_liveliness_lost(self, &status);
                listener_called = true;
            }
            let publisher = self.get_publisher()?;
            if let Some(listener) = publisher.get_listener()? {
                listener.on_liveliness_lost(self, &status);
                listener_called = true;
            }
            let participant = publisher.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_liveliness_lost(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_liveliness_lost_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::LIVELINESS_LOST, true)?;

        Ok(())
    }

    fn handle_publication_matched_status(
        &self,
        info: Arc<PublicationMatchedStatus>,
    ) -> DdsResult<()> {
        let status = {
            let mut status_guard = self
                .publication_matched_status
                .lock()
                .map_err(|e| DdsError::Error(e.to_string()))?;

            status_guard.current_count =
                (status_guard.current_count + info.current_count_change()).max(0);
            status_guard.current_count_change += info.current_count_change();
            if info.current_count_change() > 0 {
                status_guard.total_count += info.current_count_change();
                status_guard.total_count_change += info.current_count_change();
            }
            status_guard.last_subscription_handle = info.last_subscription_handle();
            *status_guard
        };

        // Listener
        let mask = self.get_listener_mask()?;
        if mask.contains(StatusKind::PUBLICATION_MATCHED) {
            let mut listener_called = false;
            if let Some(listener) = self.get_listener()? {
                listener.on_publication_matched(self, &status);
                listener_called = true;
            }
            let publisher = self.get_publisher()?;
            if let Some(listener) = publisher.get_listener()? {
                listener.on_publication_matched(self, &status);
                listener_called = true;
            }
            let participant = publisher.get_participant()?;
            if let Some(listener) = participant.get_listener()? {
                listener.on_publication_matched(self, &status);
                listener_called = true;
            }

            if listener_called {
                let _ = self.take_publication_matched_status()?;
            }
        }

        // StatusCondition
        self.set_communication_status_propagation(&StatusKind::PUBLICATION_MATCHED, true)?;

        Ok(())
    }

    pub(crate) fn get_datawriter_cache(
        &self,
    ) -> DdsResult<Arc<Mutex<DataWriterHistoryCache<Foo>>>> {
        Ok(self.datawriter_cache.clone())
    }

    fn update_liveliness(&self) -> DdsResult<()> {
        match self.get_qos()?.liveliness.kind {
            LivelinessQosPolicyKind::Automatic => Ok(()),
            LivelinessQosPolicyKind::ManualByParticipant => {
                let wlp = self
                    .wlp_logic
                    .as_ref()
                    .ok_or(DdsError::Error("WLP not initialized".to_string()))?;
                // Update Participant liveliness
                match wlp.update_local_participant_liveliness() {
                    Ok(()) => Ok(()),
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
            }
            LivelinessQosPolicyKind::ManualByTopic => {
                // Update own writer liveliness (LivelinessMonitor)
                let wlp = self
                    .wlp_logic
                    .as_ref()
                    .ok_or(DdsError::Error("WLP not initialized".to_string()))?;
                // Update Participant liveliness
                let writer_guid = self.get_rtps_writer()?.guid();
                match wlp.renew_asserting_writer(&writer_guid) {
                    Ok(()) => Ok(()),
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
            }
        }
    }

    /// Common logic for register_instance after key serialization and handle computation.
    fn register_instance_inner(
        &self,
        serialized_key: SerializedData,
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<InstanceHandle> {
        self.register_instance_to_datawriter_cache(handle)?;

        {
            let mut instances =
                self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            match instances.get(&handle) {
                Some((_, _, InstanceState::Registered)) => {
                    return Ok(handle);
                }
                Some((_, _, InstanceState::Unregistered))
                | Some((_, _, InstanceState::Disposed))
                | None => {}
            }

            instances
                .insert(handle, (serialized_key.clone(), timestamp, InstanceState::Registered));

            let mut key_instances =
                self.key_instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            key_instances.insert(serialized_key, handle);

            let monitor_guard =
                self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            if let Some(monitor) = monitor_guard.as_ref() {
                monitor.track_instance(&handle);
            }

            log::debug!("Registering Instance - handle: {:?}", handle);
        }
        Ok(handle)
    }

    /// Common logic for dispose after key resolution and handle validation.
    fn dispose_inner(
        &self,
        serialized_key: SerializedData,
        resolved_handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        let mut instances = self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        match instances.get(&resolved_handle) {
            Some((_, _, InstanceState::Registered)) | Some((_, _, InstanceState::Unregistered)) => {
            }
            Some((_, _, InstanceState::Disposed)) => {
                return Ok(());
            }
            None => {
                return Err(DdsError::BadParameter);
            }
        }

        instances
            .insert(resolved_handle, (serialized_key.clone(), timestamp, InstanceState::Disposed));

        let monitor_guard =
            self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if !resolved_handle.is_nil() {
            if let Some(monitor) = monitor_guard.as_ref() {
                monitor.cancel_instance(&resolved_handle);
            }
        }

        self.add_change_serialized(
            ChangeKind::NotAliveDisposed,
            &[],
            resolved_handle,
            Some(timestamp.into()),
        )?;

        self.update_liveliness()?;

        Ok(())
    }

    /// Common logic for unregister_instance after key resolution and handle validation.
    fn unregister_instance_inner(
        &self,
        serialized_key: SerializedData,
        resolved_handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        let mut instances = self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        match instances.get(&resolved_handle) {
            Some((_, _, InstanceState::Registered)) => {}
            Some((_, _, _)) => {
                return Err(DdsError::BadParameter);
            }
            None => {
                return Err(DdsError::BadParameter);
            }
        }

        instances.insert(
            resolved_handle,
            (serialized_key.clone(), timestamp, InstanceState::Unregistered),
        );

        let monitor_guard =
            self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(monitor) = monitor_guard.as_ref() {
            monitor.cancel_instance(&resolved_handle);
        }

        let change_kind =
            if self.get_qos()?.writer_data_lifecycle.autodispose_unregistered_instances {
                ChangeKind::NotAliveDisposedUnregistered
            } else {
                ChangeKind::NotAliveUnregistered
            };

        self.add_change_serialized(change_kind, &[], resolved_handle, Some(timestamp.into()))?;

        self.update_liveliness()?;

        Ok(())
    }

    /// Common logic for write instance resolution: auto-register if needed, validate handle,
    /// and update deadline monitor.
    ///
    /// `key_info`: If the topic has keys, `Some((serialized_key, computed_handle))`.
    ///             For no-key topics, `None`.
    /// `handle`: The user-provided handle (may be NIL).
    /// `timestamp`: Source timestamp.
    ///
    /// Returns `(instance_handle, is_new_instance)`.
    fn resolve_write_instance(
        &self,
        key_info: Option<(SerializedData, InstanceHandle)>,
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<(InstanceHandle, bool)> {
        let mut instance_handle = InstanceHandle::NIL;
        let mut is_new_instance = false;

        if let Some((serialized_key, computed_handle)) = key_info {
            let existing_handle = {
                let key_instances =
                    self.key_instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                key_instances.get(&serialized_key).copied()
            };

            instance_handle = if let Some(h) = existing_handle {
                h
            } else {
                {
                    let mut instances =
                        self.instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                    instances.insert(
                        computed_handle,
                        (serialized_key.clone(), timestamp, InstanceState::Registered),
                    );
                }
                {
                    let mut key_instances =
                        self.key_instances.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                    key_instances.insert(serialized_key, computed_handle);
                }
                is_new_instance = true;
                computed_handle
            };
        }

        if !handle.is_nil() && handle != instance_handle {
            return Err(DdsError::PreconditionNotMet);
        }

        let monitor_guard =
            self.deadline_monitor.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(monitor) = monitor_guard.as_ref() {
            if is_new_instance {
                monitor.track_instance(&instance_handle);
            }
            monitor.reschedule_instance(&instance_handle);
        }

        Ok((instance_handle, is_new_instance))
    }
}

impl<Foo> DataWriter<Foo>
where
    Foo: DdsType,
{
    /// Registers a data instance with the DataWriter.
    ///
    /// This operation informs the service that the application intends to modify a particular
    /// instance. This gives the service an opportunity to pre-configure resources for improved
    /// performance. The operation returns a handle that can be used in subsequent `write()` or
    /// `dispose()` operations for this instance.
    ///
    /// This operation is **idempotent**: calling it on an already-registered instance returns
    /// the existing handle. This allows you to look up the handle for a given instance.
    ///
    /// Using this operation is optional - you can call `write()` directly with `InstanceHandle::NIL`
    /// to have the service identify instances by examining the key fields. However, pre-registering
    /// instances can improve performance when writing frequently to the same instances.
    ///
    /// # Arguments
    ///
    /// * `instance` - The data instance whose key fields identify the instance to register
    ///
    /// # Returns
    ///
    /// Returns `Ok(InstanceHandle)` that can be used in subsequent write/dispose operations,
    /// or `InstanceHandle::NIL` if the service chooses not to allocate a handle.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `Timeout` - Same conditions as `write()` when RELIABILITY QoS is RELIABLE
    /// * `OutOfResources` - Resource limits exceeded
    /// * The writer has been deleted
    pub fn register_instance(&self, instance: &Foo) -> DdsResult<InstanceHandle> {
        /*
            This operation informs the service that the application intends to modify a particular instance.
            This provides the service an opportunity to pre-configure resources for improved performance.
            This operation takes an instance as a parameter (to obtain the key value) and returns a handle that can be used in subsequent write or dispose operations for that instance.
            This operation should be performed before calling all operations that modify the instance, such as write, write_w_timestamp, dispose, and dispose_w_timestamp.
            If the service chooses not to allocate a handle for that instance, the special value HANDLE_NIL may be returned.
            This operation can block under the same conditions as described for the write operation (section 2.2.2.4.2.11), and in that case may return TIMEOUT.
            Additionally, it may return OUT_OF_RESOURCES under the same conditions.
            The register_instance operation is **idempotent**. That is, if called again on an already-registered instance, it simply returns the already-allocated handle.
            This allows it to be used to look up and retrieve the handle allocated to a given instance.
            Using this operation explicitly is optional; the application can call the write operation directly and specify HANDLE_NIL to have the service identify the instance by examining the 'key'.
        */
        // Pre-register an instance for a specific key to optimize performance
        //
        // This function notifies the DDS service in advance that the application plans to send data repeatedly with a specific key
        // Based on this information, the service pre-allocates resources for that key and optimizes internal structures -> improving performance of subsequent write operations
        //
        // # Arguments
        // * `instance` - instance for obtaining the key value
        //
        // # Returns
        // `InstanceHandle` to use in subsequent write/dispose operations
        // `HANDLE_NIL` may be returned if the service decides not to allocate a handle.
        //
        // # Notes
        // - This operation is optional. Write operations work normally without it, but
        //   it provides performance benefits when frequent writes with the same key are expected.
        // - When called on an already-registered instance, returns the existing allocated handle.
        // - This operation guarantees idempotency (idempotency == property where performing an operation multiple times produces the same result, safe for accidental duplicate calls)
        //
        // # Errors
        // - `TIMEOUT`: timeout can occur under the same conditions as write operations.
        // - `OUT_OF_RESOURCES`: can occur when resources are insufficient.
        let timestamp = self.get_publisher()?.get_participant()?.get_current_time().unwrap();
        self.register_instance_w_timestamp(instance, timestamp)
    }

    pub fn register_instance_w_timestamp(
        &self,
        instance: &Foo,
        timestamp: Time,
    ) -> DdsResult<InstanceHandle> {
        /*
            This operation performs the same function as register_instance,
            and can be used instead of register_instance when the application wants to specify a source_timestamp value.
            The source_timestamp can affect the relative order in which Readers observe events originating from multiple Writers.
            For more details on this, refer to the DESTINATION_ORDER QoS policy (section 2.2.3.17).
            This operation can result in TIMEOUT under the same conditions described for the write operation (section 2.2.2.4.2.11),
            and may also return an OUT_OF_RESOURCES error under the same conditions
        */
        // let start_time = Time::now();
        self.is_enabled()?;

        if !self.type_support.is_compute_key_provided() {
            log::warn!("register_instance on no-key topic has no effect");
            return Ok(InstanceHandle::NIL);
        }

        Self::validate_timestamp(&timestamp)?;

        let serialized_key = self.type_support.serialize_key(instance as &dyn Any)?;
        let handle = self.type_support.compute_key(instance as &dyn Any);

        self.register_instance_inner(serialized_key, handle, timestamp)
    }
    pub fn unregister_instance(&self, instance: &Foo, handle: InstanceHandle) -> DdsResult<()> {
        // in: instance: <Foo>, handle: InstanceHandle
        // out: DdsError_t
        /*
            This operation performs the reverse of register_instance. It should only be called on currently registered instances.
            The unregister_instance operation should be called only once per instance, even if register_instance was called multiple times for that instance.
            This operation informs the service that the DataWriter no longer intends to modify this data instance.
            It also indicates that the service may remove all information about this instance locally.
            After calling unregister_instance, the handle previously allocated for that instance should no longer be used.
            The handle parameter can use the special value HANDLE_NIL.
            This value means to automatically infer the identity of the instance (identified through the key).
            If the handle value is not HANDLE_NIL, it must match the value returned when the instance was registered by register_instance.
            Otherwise, the following behavior occurs:
            If the handle references an instance that actually exists but does not match the instance pointed to by the 'instance' parameter,
            the behavior is generally unspecified, but if the service implementation can detect this, the operation fails and returns error code PRECONDITION_NOT_MET.
            If the handle references a non-existent instance, the behavior is generally unspecified, but
            if the service implementation can detect this, the operation fails and returns error code BAD_PARAMETER.
            After that, if the application wants to modify (write or dispose) the instance, it must register it again or use HANDLE_NIL to identify the instance without registration.
            This operation does not mean the instance has been deleted (that role is performed by dispose).
            unregister_instance simply means the DataWriter has "nothing more to say" about that instance.
            DataReader entities reading that instance will eventually receive a sample with instance state NOT_ALIVE_NO_WRITERS if no DataWriter is writing that instance anymore.
            This operation can affect the ownership of the data instance (section 2.2.3.9 OWNERSHIP and section 2.2.3.23.1 Ownership Interpretation in Redundant Systems).
            If the DataWriter was the exclusive owner of the instance, calling unregister_instance will relinquish that ownership.
            This operation can result in TIMEOUT under the same conditions described for the write operation (section 2.2.2.4.2.11),
            and additionally can return the following error codes: TIMEOUT, PRECONDITION_NOT_MET.
        */
        // let timestamp = self.get_publisher()?.get_participant()?.get_current_time().unwrap();
        let timestamp = Time::now();
        self.unregister_instance_w_timestamp(instance, handle, timestamp)
    }

    pub fn unregister_instance_w_timestamp(
        &self,
        instance: &Foo,
        handle: InstanceHandle,
        timestamp: Time,
    ) -> DdsResult<()> {
        // in: instance: <Foo>, handle: InstanceHandle, timestamp: Time
        // out: DdsError_t
        /*
            This operation performs the same function as unregister_instance,
            and can be used instead of unregister_instance when the application wants to specify a source_timestamp value.
            The source_timestamp can affect the relative order in which Readers observe events originating from multiple Writers.
            For more details, refer to the DESTINATION_ORDER QoS policy (section 2.2.3.17).
            Constraints on the handle parameter value and the corresponding error behavior are the same as specified for the unregister_instance operation (section 2.2.2.4.2.7).
            This operation can result in TIMEOUT under the same conditions described for the write operation (section 2.2.2.4.2.11).
        */
        // let start_time = Time::now();
        self.is_enabled()?;
        if !self.type_support.is_compute_key_provided() {
            log::warn!("unregister on no-key topic has no effect");
            return Ok(());
        }
        Self::validate_timestamp(&timestamp)?;

        let serialized_key = self.type_support.serialize_key(instance as &dyn Any)?;
        let (serialized_key, resolved_handle) =
            self.resolve_unregister_key(serialized_key, handle)?;

        self.unregister_instance_inner(serialized_key, resolved_handle, timestamp)
    }
}

impl<Foo: 'static + Clone> DataWriterBase for DataWriter<Foo> {
    fn assert_liveliness(&self) -> DdsResult<()> {
        // out: DdsError_t
        /*
            This operation manually asserts the liveliness of the DataWriter.
            This is used in conjunction with the LIVELINESS QoS policy (see section 2.2.3, Supported QoS) to inform the service that the entity is still in an active state.
            This operation only needs to be used when the LIVELINESS setting is MANUAL_BY_PARTICIPANT or MANUAL_BY_TOPIC; otherwise, it has no effect.
            Note: When data is written through the write operation on a DataWriter, liveliness is automatically asserted for both that DataWriter itself and its DomainParticipant.
            Therefore, the assert_liveliness operation only needs to be used when the application does not write data regularly.
        */
        self.is_enabled()?;

        match self.get_qos()?.liveliness.kind {
            LivelinessQosPolicyKind::Automatic => Ok(()),
            LivelinessQosPolicyKind::ManualByParticipant => {
                self.get_publisher()?.get_participant()?.assert_liveliness()
            }
            LivelinessQosPolicyKind::ManualByTopic => {
                let rtps_writer = self.get_rtps_writer()?;
                if rtps_writer.assert_liveliness() {
                    Ok(())
                } else {
                    Err(DdsError::Error("Failed to assert writer liveliness".to_string()))
                }
            }
        }
    }

    fn get_liveliness_lost_status(&self) -> DdsResult<LivelinessLostStatus> {
        /*
            This operation provides access to the LIVELINESS_LOST communication status.
            Refer to Section 2.2.4.1 Communication Status for a description of communication status.
        */
        self.is_deleted()?;

        let mut status_guard =
            self.liveliness_lost_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // 1. Reset total_count_change
        status_guard.total_count_change = 0;

        self.set_communication_status_propagation(&StatusKind::LIVELINESS_LOST, false)?;

        Ok(result)
    }

    fn get_offered_deadline_missed_status(&self) -> DdsResult<OfferedDeadlineMissedStatus> {
        /*
            This operation provides access to the OFFERED_DEADLINE_MISSED communication status.
            Refer to Section 2.2.4.1 Communication Status for a description of communication status.
        */
        self.is_deleted()?;

        let mut status_guard = self
            .offered_deadline_missed_status
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // 1. Reset total_count_change
        status_guard.total_count_change = 0;

        self.set_communication_status_propagation(&StatusKind::OFFERED_DEADLINE_MISSED, false)?;

        Ok(result)
    }

    fn get_offered_incompatible_qos_status(&self) -> DdsResult<OfferedIncompatibleQosStatus> {
        /*
            This operation provides access to the OFFERED_INCOMPATIBLE_QOS communication status.
            Refer to Section 2.2.4.1 Communication Status for a description of communication status.
        */
        self.is_deleted()?;

        let mut status_guard = self
            .offered_incompatible_qos_status
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?;
        let result = status_guard.clone();

        // 1. Reset total_count_change
        status_guard.total_count_change = 0;

        self.set_communication_status_propagation(&StatusKind::OFFERED_INCOMPATIBLE_QOS, false)?;

        Ok(result)
    }

    fn get_publication_matched_status(&self) -> DdsResult<PublicationMatchedStatus> {
        // out: DdsError_t, status: PublicationMatchedStatus
        /*
            This operation provides access to the PUBLICATION_MATCHED communication status.
            Refer to Section 2.2.4.1 Communication Status for a description of communication status.
        */
        self.is_deleted()?;

        let mut status_guard =
            self.publication_matched_status.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let result = *status_guard;

        // 1. Reset total_count_change
        status_guard.total_count_change = 0;

        self.set_communication_status_propagation(&StatusKind::PUBLICATION_MATCHED, false)?;

        Ok(result)
    }

    fn get_topic(&self) -> DdsResult<Topic> {
        self.is_deleted()?;
        if let Some(weak_ref) = self.topic.as_ref() {
            // Attempt to upgrade Weak<T> to Arc<T>
            if let Some(topic_arc) = weak_ref.upgrade() {
                return Ok((*topic_arc).clone());
            }
        }

        // topic is None or reference has expired
        Err(DdsError::Error("Topic reference is invalid or expired".to_string()))
    }

    fn get_publisher(&self) -> DdsResult<Publisher> {
        self.is_deleted()?;
        if let Some(weak_ref) = self.publisher.as_ref() {
            // Attempt to upgrade Weak<T> to Arc<T>
            if let Some(publisher_arc) = weak_ref.upgrade() {
                return Ok((*publisher_arc).clone());
            }
        }

        // publisher is None or reference has expired
        Err(DdsError::Error("Publisher reference is invalid or expired".to_string()))
    }

    fn get_matched_subscriptions(&self) -> DdsResult<Vec<InstanceHandle>> {
        // subscription_handles: InstanceHandle[]
        // out: DdsError_t, subscription_handles: InstanceHandle[]
        /*
            This operation retrieves the list of subscriptions "associated" with the current DataWriter. "Associated" means subscriptions of the same Topic with compatible QoS,
            which the application has not specified to ignore through the DomainParticipant's ignore_subscription operation.
            The handles in the subscription_handles list are the handles used by the DDS implementation to locally identify the corresponding matched DataReader entities.
            These handles match the values that appear in the instance_handle field of SampleInfo when reading the "DCPSSubscriptions" Builtin Topic.
            If the DDS middleware does not maintain connection information locally, this operation may fail.
        */
        self.is_enabled()?;
        let matched_readers_guids;
        {
            let rtps_writer = self.get_rtps_writer()?;
            matched_readers_guids = rtps_writer.matched_readers_guids();
        }
        let mut subscription_handles = Vec::new();
        for guid in matched_readers_guids {
            subscription_handles.push(InstanceHandle::from_guid(&guid));
        }
        Ok(subscription_handles)
    }

    fn get_matched_subscription_data(
        &self,
        subscription_handle: InstanceHandle,
    ) -> DdsResult<SubscriptionBuiltinTopicData> {
        // in: subscription_data: SubscriptionBuiltinTopicData, subscription_handle: InstanceHandle
        // out: DdsError_t, subscription_data: SubscriptionBuiltinTopicData
        /*
            This operation retrieves information about a subscription "associated" with the current DataWriter.
            "Associated" means a subscription of the same Topic with compatible QoS,
            which the application has not specified to ignore through the DomainParticipant's ignore_subscription operation.
            The subscription_handle must represent a subscription associated with the current DataWriter; otherwise, this operation fails and returns BAD_PARAMETER.
            The get_matched_subscriptions operation can be used to find subscriptions associated with the current DataWriter.
            Additionally, if the middleware does not internally possess the information needed to populate subscription_data, this operation may fail and return UNSUPPORTED.
        */
        self.is_enabled()?;
        let reader_guid = subscription_handle.to_guid();
        {
            let rtps_writer = self.get_rtps_writer()?;
            if let Ok(data) = rtps_writer.get_matched_subscription_data(reader_guid) {
                return Ok(data);
            }
        }
        Err(DdsError::BadParameter)
    }

    fn wait_for_acknowledgments(&self, max_wait: Duration) -> DdsResult<()> {
        // in: max_wait: Duration
        // out: DdsError_t
        /*
        This operation is intended to be used only when the DataWriter's RELIABILITY QoS kind is set to RELIABLE.
        Otherwise, this operation returns RETCODE_OK immediately.
        The wait_for_acknowledgments operation blocks the calling thread,
        waiting until all data written by the DataWriter has been acknowledged by all matched DataReader entities with RELIABILITY QoS kind set to RELIABLE, or until the period specified by the max_wait parameter elapses.
        It terminates when whichever of the two conditions is reached first.
        An OK return value means all written samples have been acknowledged by all reliable matched data readers.
        A TIMEOUT return value indicates that max_wait has elapsed but some data has not yet been acknowledged.
        */
        self.is_enabled()?;
        let reliability = self.get_qos()?.reliability;
        if reliability.kind == ReliabilityQosPolicyKind::Reliable {
            let rtps_writer = self.get_rtps_writer()?;
            if rtps_writer.wait_for_all_acked(max_wait) {
                return Ok(());
            }
        }
        Ok(())
    }
}

impl<Foo: 'static + Clone> DataWriterInternal for DataWriter<Foo> {
    fn clone_boxed(&self) -> Box<dyn DataWriterInternal<Qos = DataWriterQos> + Send> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_type_id(&self) -> TypeId {
        TypeId::of::<Foo>()
    }

    fn disable(&self) -> DdsResult<()> {
        self.set_listener(None, StatusMask::default())
    }

    fn delete(&self) {
        // Shutdown deadline monitor
        let monitor_to_drop = if let Ok(mut monitor_guard) = self.deadline_monitor.lock() {
            monitor_guard.take() // Take ownership, will drop after lock is released
        } else {
            None
        };
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

    fn is_builtin(&self) -> bool {
        self.is_builtin
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::domain_participant_factory::DomainParticipantFactory;
    use crate::domain::qos::DomainParticipantQos;
    use crate::infrastructure::qos_policy::HistoryQosPolicyKind;
    use crate::publication::qos::PublisherQos;
    use crate::topic::qos::TopicQos;
    use std::{sync::atomic::AtomicUsize, thread};

    use super::*;

    // Test struct - for detecting drop
    use crate::dcps::topic::type_support::DdsType;

    #[derive(DdsType)]
    pub struct TestData {
        #[dds(key)]
        id: u32,
    }

    #[test]
    fn test_writer_drop_without_delete() {
        println!("=== Test: Writer drop without delete_writer ===");

        // Standard DDS creation process
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
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

        // Check initial state of orphaned_writers
        let initial_writers_count = {
            let writers = publisher.get_data_writers().unwrap();
            writers.len()
        };
        println!("Initial orphaned writers count: {}", initial_writers_count);

        // Create DataWriter and verify HashMap registration
        {
            let _test_data = TestData { id: 1 };
            let writer = publisher
                .create_datawriter::<TestData>(
                    &topic,
                    DataWriterQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            let topic_name = writer.get_topic().unwrap().get_name().to_string();
            let _writer_handle = writer.get_instance_handle().unwrap();

            // Verify registered in HashMap
            let registered_in_name_map = publisher.lookup_datawriter::<TestData>(&topic_name);

            assert!(registered_in_name_map.is_ok(), "Writer should be registered in HashMap");

            // Here, writer goes out of scope and is dropped - without calling delete_writer
            println!("Writer going out of scope without delete_writer...");
        }

        // Verify added to orphaned_writers
        let writers;
        let final_writers_count = {
            writers = publisher.get_data_writers().unwrap();
            writers.len()
        };
        println!("Final orphaned writers count: {}", final_writers_count);

        assert_eq!(
            final_writers_count,
            initial_writers_count + 1,
            "Writer should be moved to orphaned_writers"
        );
    }

    #[test]
    fn test_writer_proper_delete() {
        println!("=== Test: Proper delete_writer usage ===");

        // Standard DDS creation process
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "TestTopic2",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let initial_writers_count = {
            let writers = publisher.get_data_writers().unwrap();
            writers.len()
        };

        // Create DataWriter and properly call delete_writer
        {
            let _test_data = TestData { id: 2 };
            let writer = publisher
                .create_datawriter::<TestData>(
                    &topic,
                    DataWriterQos::default(),
                    None,
                    StatusMask::default(),
                )
                .unwrap();

            let topic_name = writer.get_topic().unwrap().get_name().to_string();

            // Verify registered in HashMap
            let registered_in_name_map =
                publisher.lookup_datawriter::<TestData>(&topic_name).is_ok();
            assert!(registered_in_name_map, "Writer doesn't registered before delete");

            // Properly call delete_writer - removed from HashMap here
            let delete_result = publisher.delete_datawriter(writer);
            println!("Delete result: {:?}", delete_result);
            assert!(delete_result.is_ok(), "delete_writer should succeed");

            // Verify removed from HashMap
            let registered_after_delete =
                publisher.lookup_datawriter::<TestData>(&topic_name).is_ok();
            println!("Writer still registered after delete: {}", registered_after_delete);
            assert!(!registered_after_delete, "Writer should be removed from HashMap after delete");

            // Here, writer goes out of scope and is dropped
            // However, since it's already removed from HashMap, it should not be added to orphaned_writers
            println!("Writer going out of scope after delete...");
        }

        // Should not be added to orphaned_writers
        let final_writers_count = {
            let writers = publisher.get_data_writers().unwrap();
            writers.len()
        };
        println!("Final orphaned writers count: {}", final_writers_count);

        assert_eq!(
            final_writers_count, initial_writers_count,
            "No writers should be added to orphaned_writers when properly deleted"
        );
    }

    #[test]
    fn test_delete_writer() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
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
        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        {
            let rtps_writer = writer.get_rtps_writer();
            assert!(rtps_writer.is_ok());
        }
        let mut dcps_bridge = domain_participant.get_dcps_bridge().unwrap();
        let dcps_bridge = dcps_bridge.as_mut().unwrap();
        dcps_bridge
            .delete_rtps_writer(
                "TestTopic".to_string(),
                writer.get_instance_handle().unwrap().to_guid().entity_id(),
            )
            .unwrap();
        let rtps_writer = writer.get_rtps_writer();
        assert!(rtps_writer.is_err());
    }

    // Listener for DeadlineQos testing
    struct DeadlineTestListener {
        miss_count: Arc<AtomicUsize>,
    }

    impl DataWriterListener for DeadlineTestListener {
        type Foo = TestData;

        fn on_offered_deadline_missed(
            &self,
            _writer: &DataWriter<Self::Foo>,
            _status: &OfferedDeadlineMissedStatus,
        ) {
            self.miss_count.fetch_add(1, Ordering::SeqCst);
            println!("Deadline missed detected!");
        }
    }

    #[test]
    fn test_deadline_qos_with_writer() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "DeadlineTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        // QoS settings with 500ms deadline (large margin for CI)
        let mut writer_qos = DataWriterQos::default();
        writer_qos.deadline.period = Duration::from_millis(500);

        let deadline_miss_count = Arc::new(AtomicUsize::new(0));
        let listener =
            Arc::new(DeadlineTestListener { miss_count: Arc::clone(&deadline_miss_count) });

        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                writer_qos,
                Some(listener),
                StatusMask::OFFERED_DEADLINE_MISSED,
            )
            .unwrap();

        // Test data
        let data = TestData { id: 1 };

        // First write - refresh deadline
        writer.write(&data, InstanceHandle::NIL).unwrap();
        println!("First write completed");

        // Write again before deadline - miss should not occur
        thread::sleep(std::time::Duration::from_millis(200));
        writer.write(&data, InstanceHandle::NIL).unwrap();
        println!("Second write completed (within deadline)");

        // Verify after short wait
        thread::sleep(std::time::Duration::from_millis(100));
        assert_eq!(
            deadline_miss_count.load(Ordering::SeqCst),
            0,
            "No deadline miss should occur when writing within deadline"
        );

        // Wait to exceed deadline - miss should occur
        println!("Waiting for deadline to expire...");
        thread::sleep(std::time::Duration::from_millis(600));

        // Verify deadline miss
        let miss_count = deadline_miss_count.load(Ordering::SeqCst);
        println!("Deadline miss count: {}", miss_count);
        assert!(
            miss_count >= 1,
            "At least one deadline miss should be detected, got {}",
            miss_count
        );
    }

    #[test]
    fn test_deadline_qos_infinite() {
        // Verify that no errors occur when deadline is infinite
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "InfiniteDeadlineTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        // QoS with infinite deadline (default value)
        let writer_qos = DataWriterQos::default();
        assert!(writer_qos.deadline.period.is_infinite());

        let writer = publisher
            .create_datawriter::<TestData>(&topic, writer_qos, None, StatusMask::default())
            .unwrap();

        let data = TestData { id: 1 };

        // Write, register, dispose, unregister should all work without errors
        let handle = writer.register_instance(&data).unwrap();
        assert!(!handle.is_nil());

        writer.write(&data, InstanceHandle::NIL).unwrap();
        writer.write(&data, handle).unwrap();

        writer.dispose(&data, handle).unwrap();

        // Register again and unregister
        let handle2 = writer.register_instance(&data).unwrap();
        writer.write(&data, handle2).unwrap();
        writer.unregister_instance(&data, handle2).unwrap();

        println!("All operations completed without error with infinite deadline");
    }

    #[test]
    fn test_deadline_qos_with_register_dispose() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "RegisterDisposeDeadlineTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let mut writer_qos = DataWriterQos::default();
        writer_qos.deadline.period = Duration::from_millis(150);

        let deadline_miss_count = Arc::new(AtomicUsize::new(0));
        let listener =
            Arc::new(DeadlineTestListener { miss_count: Arc::clone(&deadline_miss_count) });

        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                writer_qos,
                Some(listener),
                StatusMask::OFFERED_DEADLINE_MISSED,
            )
            .unwrap();

        let data = TestData { id: 42 };

        // Start tracking instance with register_instance
        let handle = writer.register_instance(&data).unwrap();
        println!("Instance registered with handle: {:?}", handle);

        // Wait for deadline - miss should occur
        thread::sleep(std::time::Duration::from_millis(200));
        assert!(
            deadline_miss_count.load(Ordering::SeqCst) >= 1,
            "Deadline miss should occur after registration"
        );

        // Cancel tracking with dispose
        writer.dispose(&data, handle).unwrap();
        println!("Instance disposed");

        // After dispose, deadline miss should no longer occur
        let count_before_dispose = deadline_miss_count.load(Ordering::SeqCst);
        thread::sleep(std::time::Duration::from_millis(200));
        let count_after_dispose = deadline_miss_count.load(Ordering::SeqCst);

        println!(
            "Count before dispose: {}, after dispose: {}",
            count_before_dispose, count_after_dispose
        );

        // After dispose, count should not increase or increase very slightly
        // (may increase about once due to timing issues)
        assert!(
            count_after_dispose - count_before_dispose <= 1,
            "Deadline miss should not occur after dispose"
        );
    }

    #[test]
    fn test_deadline_qos_multiple_instances() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "MultiInstanceDeadlineTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let mut writer_qos = DataWriterQos::default();
        writer_qos.deadline.period = Duration::from_millis(150);

        let deadline_miss_count = Arc::new(AtomicUsize::new(0));
        let listener =
            Arc::new(DeadlineTestListener { miss_count: Arc::clone(&deadline_miss_count) });

        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                writer_qos,
                Some(listener),
                StatusMask::OFFERED_DEADLINE_MISSED,
            )
            .unwrap();

        // Register multiple instances
        let data1 = TestData { id: 1 };
        let data2 = TestData { id: 2 };
        let data3 = TestData { id: 3 };

        writer.write(&data1, InstanceHandle::NIL).unwrap();
        writer.write(&data2, InstanceHandle::NIL).unwrap();
        writer.write(&data3, InstanceHandle::NIL).unwrap();

        println!("Three instances written");

        // Wait for all instances' deadlines to expire
        thread::sleep(std::time::Duration::from_millis(200));

        // Deadline miss should occur for all 3 instances
        let miss_count = deadline_miss_count.load(Ordering::SeqCst);
        println!("Deadline miss count for 3 instances: {}", miss_count);
        assert!(
            miss_count >= 3,
            "At least 3 deadline misses expected (one per instance), got {}",
            miss_count
        );
    }

    #[test]
    fn test_deadline_qos_retrack_on_write() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "RetrackDeadlineTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let mut writer_qos = DataWriterQos::default();
        writer_qos.deadline.period = Duration::from_millis(120);

        let deadline_miss_count = Arc::new(AtomicUsize::new(0));
        let listener =
            Arc::new(DeadlineTestListener { miss_count: Arc::clone(&deadline_miss_count) });

        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                writer_qos,
                Some(listener),
                StatusMask::OFFERED_DEADLINE_MISSED,
            )
            .unwrap();

        let data = TestData { id: 100 };

        // Refresh deadline by writing repeatedly
        for i in 0..5 {
            writer.write(&data, InstanceHandle::NIL).unwrap();
            println!("Write iteration {}", i);
            thread::sleep(std::time::Duration::from_millis(80));
        }

        // Wrote 5 times at 80ms intervals, so deadline (120ms) is not exceeded
        assert_eq!(
            deadline_miss_count.load(Ordering::SeqCst),
            0,
            "No deadline miss should occur when writing regularly within deadline"
        );

        // Now wait to exceed the deadline
        thread::sleep(std::time::Duration::from_millis(150));

        // Deadline miss occurs
        assert!(
            deadline_miss_count.load(Ordering::SeqCst) >= 1,
            "Deadline miss should occur after stopping writes"
        );
    }

    #[test]
    fn test_deadline_qos_shutdown_on_delete() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<TestData>(
                "ShutdownTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let mut writer_qos = DataWriterQos::default();
        writer_qos.deadline.period = Duration::from_millis(100);

        let deadline_miss_count = Arc::new(AtomicUsize::new(0));
        let listener =
            Arc::new(DeadlineTestListener { miss_count: Arc::clone(&deadline_miss_count) });

        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                writer_qos,
                Some(listener),
                StatusMask::OFFERED_DEADLINE_MISSED,
            )
            .unwrap();

        let data = TestData { id: 99 };

        // Register instance
        writer.register_instance(&data).unwrap();
        println!("Instance registered");

        // Wait briefly then call delete (trigger shutdown)
        thread::sleep(std::time::Duration::from_millis(50));
        writer.delete();
        println!("Writer deleted (shutdown called)");

        let count_before_delete = deadline_miss_count.load(Ordering::SeqCst);

        // Wait to exceed deadline after delete
        thread::sleep(std::time::Duration::from_millis(200));

        let count_after_delete = deadline_miss_count.load(Ordering::SeqCst);

        println!(
            "Count before delete: {}, after delete: {}",
            count_before_delete, count_after_delete
        );

        // After delete, deadline miss should not occur
        assert_eq!(
            count_before_delete, count_after_delete,
            "Deadline miss should not occur after delete (shutdown)"
        );
    }

    // ========================================================================
    // Listener mask tests - verify that individual StatusKind mask correctly
    // gates listener callback invocation.
    //
    // These tests call update_status() directly with a specific StatusKind mask
    // to verify that the corresponding listener callback is invoked.
    // Before the fix, handle_offered_incompatible_qos_status and
    // handle_publication_matched_status checked StatusKind::SAMPLE_LOST
    // instead of their correct StatusKind, so callbacks never fired
    // with individual masks.
    // ========================================================================

    use std::sync::atomic::{AtomicBool, Ordering};

    struct MaskTestListener {
        offered_deadline_missed_called: AtomicBool,
        offered_incompatible_qos_called: AtomicBool,
        liveliness_lost_called: AtomicBool,
        publication_matched_called: AtomicBool,
    }

    impl MaskTestListener {
        fn new() -> Self {
            Self {
                offered_deadline_missed_called: AtomicBool::new(false),
                offered_incompatible_qos_called: AtomicBool::new(false),
                liveliness_lost_called: AtomicBool::new(false),
                publication_matched_called: AtomicBool::new(false),
            }
        }
    }

    impl DataWriterListener for MaskTestListener {
        type Foo = TestData;

        fn on_offered_deadline_missed(
            &self,
            _writer: &DataWriter<TestData>,
            _status: &OfferedDeadlineMissedStatus,
        ) {
            self.offered_deadline_missed_called.store(true, Ordering::SeqCst);
        }

        fn on_offered_incompatible_qos(
            &self,
            _writer: &DataWriter<TestData>,
            _status: &OfferedIncompatibleQosStatus,
        ) {
            self.offered_incompatible_qos_called.store(true, Ordering::SeqCst);
        }

        fn on_liveliness_lost(
            &self,
            _writer: &DataWriter<TestData>,
            _status: &LivelinessLostStatus,
        ) {
            self.liveliness_lost_called.store(true, Ordering::SeqCst);
        }

        fn on_publication_matched(
            &self,
            _writer: &DataWriter<TestData>,
            _status: &PublicationMatchedStatus,
        ) {
            self.publication_matched_called.store(true, Ordering::SeqCst);
        }
    }

    fn setup_mask_test() -> (
        crate::domain::domain_participant::DomainParticipant,
        crate::publication::publisher::Publisher,
        crate::topic::topic::Topic,
    ) {
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = participant
            .create_topic::<TestData>(
                "MaskTestTopic",
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        (participant, publisher, topic)
    }

    #[test]
    fn test_mask_offered_incompatible_qos() {
        let (_participant, publisher, topic) = setup_mask_test();

        let listener = Arc::new(MaskTestListener::new());
        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                Some(listener.clone() as Arc<dyn DataWriterListener<Foo = TestData>>),
                StatusKind::OFFERED_INCOMPATIBLE_QOS,
            )
            .unwrap();

        let status = Arc::new(OfferedIncompatibleQosStatus::default());
        writer.update_status(StatusKind::OFFERED_INCOMPATIBLE_QOS, Some(status)).unwrap();

        assert!(
            listener
                .offered_incompatible_qos_called
                .load(Ordering::SeqCst),
            "on_offered_incompatible_qos should be called when mask contains OFFERED_INCOMPATIBLE_QOS"
        );
    }

    #[test]
    fn test_mask_publication_matched() {
        let (_participant, publisher, topic) = setup_mask_test();

        let listener = Arc::new(MaskTestListener::new());
        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                Some(listener.clone() as Arc<dyn DataWriterListener<Foo = TestData>>),
                StatusKind::PUBLICATION_MATCHED,
            )
            .unwrap();

        let status = Arc::new(PublicationMatchedStatus {
            total_count: 0,
            total_count_change: 1,
            current_count: 0,
            current_count_change: 1,
            last_subscription_handle: InstanceHandle::default(),
        });
        writer.update_status(StatusKind::PUBLICATION_MATCHED, Some(status)).unwrap();

        assert!(
            listener.publication_matched_called.load(Ordering::SeqCst),
            "on_publication_matched should be called when mask contains PUBLICATION_MATCHED"
        );
    }

    #[test]
    fn test_mask_liveliness_lost() {
        let (_participant, publisher, topic) = setup_mask_test();

        let listener = Arc::new(MaskTestListener::new());
        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                Some(listener.clone() as Arc<dyn DataWriterListener<Foo = TestData>>),
                StatusKind::LIVELINESS_LOST,
            )
            .unwrap();

        writer.update_status(StatusKind::LIVELINESS_LOST, None).unwrap();

        assert!(
            listener.liveliness_lost_called.load(Ordering::SeqCst),
            "on_liveliness_lost should be called when mask contains LIVELINESS_LOST"
        );
    }

    #[test]
    fn test_mask_offered_deadline_missed() {
        let (_participant, publisher, topic) = setup_mask_test();

        let listener = Arc::new(MaskTestListener::new());
        let writer = publisher
            .create_datawriter::<TestData>(
                &topic,
                DataWriterQos::default(),
                Some(listener.clone() as Arc<dyn DataWriterListener<Foo = TestData>>),
                StatusKind::OFFERED_DEADLINE_MISSED,
            )
            .unwrap();

        let status = Arc::new(OfferedDeadlineMissedStatus::default());
        writer.update_status(StatusKind::OFFERED_DEADLINE_MISSED, Some(status)).unwrap();

        assert!(
            listener
                .offered_deadline_missed_called
                .load(Ordering::SeqCst),
            "on_offered_deadline_missed should be called when mask contains OFFERED_DEADLINE_MISSED"
        );
    }

    // Builds a writer with KeepLast(depth) and returns it plus a way to read the pool size.
    fn create_pool_test_writer(topic_name: &str, depth: i32) -> DataWriter<TestData> {
        let factory = DomainParticipantFactory::get_instance();
        let participant = factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let topic = participant
            .create_topic::<TestData>(
                topic_name,
                "TestData",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let mut writer_qos = DataWriterQos::default();
        writer_qos.history.kind = HistoryQosPolicyKind::KeepLast(depth);
        publisher
            .create_datawriter::<TestData>(&topic, writer_qos, None, StatusMask::default())
            .unwrap()
    }

    // Repeated serialized writes must reuse the pool so the free list stays at the
    // working-set size. A non-pooled path never acquires, leaving the pre-filled pool
    // pinned at `depth`; the balanced path drains it well below `depth`.
    #[test]
    fn write_serialized_keeps_pool_at_working_set() {
        let depth = 4;
        let writer = create_pool_test_writer("PoolSerializedTopic", depth);

        // 4-byte encapsulation header + body; content is not parsed for alive writes
        let serialized = vec![0u8, 1, 0, 0, 42, 0, 0, 0];
        for _ in 0..50 {
            writer.write_serialized(&serialized, None).unwrap();
        }

        let cache = writer.get_datawriter_cache().unwrap();
        let pool_len = cache.lock().unwrap().pool_len();
        assert!(
            pool_len < depth as usize,
            "serialized path should keep pool at working-set size, got {pool_len} (depth {depth})"
        );
    }

    // Typed writes already go through the pooled path; included as the balanced-path
    // counterpart to the serialized regression above.
    #[test]
    fn write_keeps_pool_at_working_set() {
        let depth = 4;
        let writer = create_pool_test_writer("PoolTypedTopic", depth);

        let data = TestData { id: 7 };
        for _ in 0..50 {
            writer.write(&data, InstanceHandle::NIL).unwrap();
        }

        let cache = writer.get_datawriter_cache().unwrap();
        let pool_len = cache.lock().unwrap().pool_len();
        assert!(
            pool_len < depth as usize,
            "typed path should keep pool at working-set size, got {pool_len} (depth {depth})"
        );
    }
}
