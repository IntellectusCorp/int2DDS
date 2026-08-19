//! Publisher - Container and factory for DataWriter objects.
//!
//! A `Publisher` acts as a container for DataWriter objects that publish data on topics.
//! It provides a way to group related data writers together and manage their lifecycle.
//!
//! # Overview
//!
//! Publishers are created by a `DomainParticipant` and are used to create `DataWriter`
//! objects. All data writers created by the same publisher share the publisher's QoS
//! policies (unless overridden).
//!
//! # Key Features
//!
//! - **DataWriter Factory**: Creates and manages DataWriter objects
//! - **QoS Management**: Publishers have their own QoS policies
//! - **Lifecycle Control**: Publishers can be suspended/resumed
//! - **Entity Lookup**: Find data writers by topic name

use std::{
    any::TypeId,
    collections::HashMap,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
};

use arc_swap::ArcSwap;

use super::{
    data_writer::{DataWriter, DataWriterBase, DataWriterInternal},
    data_writer_listener::DataWriterListener,
    publisher_listener::PublisherListener,
    qos::{DataWriterQos, PublisherQos},
};
use crate::{
    common::instance_handle::InstanceHandle,
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
    },
    dcps::topic::type_support::{DdsType, TypeSupport},
    domain::{
        domain_participant::{DomainParticipant, ParticipantRef},
        domain_participant_factory::DomainParticipantFactory,
    },
    infrastructure::{
        domain_entity::DomainEntity,
        entity::{
            impl_check_parent_enabled, impl_dds_entity, impl_dds_entity_impl, BaseEntity,
            EnableChild, Entity, EntityInternal, UpdateStatus,
        },
        qos_kind::QosKind,
        qos_policy::Qos,
        status::StatusMask,
        status_condition::StatusCondition,
    },
    rtps::common::{entity_kind::EntityKind, guid::Guid},
    topic::{qos::TopicQos, topic::Topic},
};

#[derive(Clone)]
pub struct Publisher {
    // Indicates whether this entity is a built-in entity.
    //
    // Currently always `false` as DDS spec does not define built-in
    // Publisher exposed to users.
    //
    // TODO: Reserved for future DCPS-RTPS built-in entity mapping
    // if needed (e.g., exposing built-in publisher for diagnostics).
    is_builtin: bool,
    guid: Guid,
    qos: Arc<ArcSwap<PublisherQos>>,
    update_lock: Arc<Mutex<()>>,
    listener: Arc<RwLock<Option<Arc<dyn PublisherListener>>>>,
    mask: Arc<RwLock<StatusMask>>,
    status_condition: Arc<Mutex<StatusCondition<PublisherQos>>>,
    pub(crate) self_ref: Option<Arc<Publisher>>,
    enabled: Arc<AtomicBool>,
    deleted: Arc<AtomicBool>,
    #[allow(clippy::type_complexity)]
    writers_by_topic_name:
        Arc<Mutex<HashMap<String, Vec<Weak<dyn DataWriterInternal<Qos = DataWriterQos>>>>>>,
    #[allow(clippy::type_complexity)]
    writers_by_topic_handle:
        Arc<Mutex<HashMap<InstanceHandle, Vec<Weak<dyn DataWriterInternal<Qos = DataWriterQos>>>>>>,
    orphaned_writers: Arc<Mutex<Vec<Arc<dyn DataWriterInternal<Qos = DataWriterQos>>>>>,
    default_datawriter_qos: Arc<Mutex<Option<DataWriterQos>>>,
    participant: Option<Weak<DomainParticipant>>,
    // Nesting depth of begin/end_coherent_changes; the set is open while non-zero.
    coherent_depth: Arc<AtomicU32>,
}

impl Debug for Publisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publisher")
            .field("guid", &self.guid)
            .field("qos", &**self.qos.load())
            .field(
                "listener",
                &self.listener.read().unwrap().as_ref().map(|_| "Arc<dyn PublisherListener>"),
            )
            .field("mask", &self.mask.read().unwrap())
            .field("status_condition", &self.status_condition.lock().unwrap())
            .field("self_ref", &self.self_ref.as_ref().map(|_| "Arc<Publisher>"))
            .field("enabled", &self.enabled.load(std::sync::atomic::Ordering::Acquire))
            .field("deleted", &self.deleted.load(std::sync::atomic::Ordering::Acquire))
            .finish()
    }
}

impl PartialEq for Publisher {
    fn eq(&self, other: &Self) -> bool {
        self.guid == other.guid
    }
}
impl Eq for Publisher {}

impl Drop for Publisher {
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
            if let Some(ref participant_weak) = self.participant {
                if let Some(participant) = participant_weak.upgrade() {
                    let publisher_handle = InstanceHandle::from_guid(&self.guid);
                    participant.handle_publisher_drop(&publisher_handle);
                }
            }
        }
    }
}

impl_dds_entity!(Publisher, PublisherQos);
impl EnableChild for Publisher {
    fn enable_child_entities(&self) -> DdsResult<()> {
        let writers_by_topic_name =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for weak_writers in writers_by_topic_name.values() {
            for weak_writer in weak_writers.iter() {
                if let Some(writer) = weak_writer.upgrade() {
                    writer.enable()?; // Enable only live ones
                }
            }
        }

        Ok(())
    }

    impl_check_parent_enabled!(get_participant);
}
impl UpdateStatus for Publisher {}
impl DomainEntity for Publisher {}

impl Publisher {
    pub(crate) fn new(
        is_builtin: bool,
        qos: PublisherQos,
        listener: Option<Arc<dyn PublisherListener>>,
        mask: StatusMask,
        handle: InstanceHandle,
        participant: &Arc<DomainParticipant>,
    ) -> Self {
        let mut publisher = Self {
            is_builtin,
            qos: Arc::new(ArcSwap::from_pointee(qos)),
            update_lock: Arc::new(Mutex::new(())),
            guid: handle.to_guid(),
            listener: Arc::new(RwLock::new(listener)),
            mask: Arc::new(RwLock::new(mask)),
            status_condition: Arc::new(Mutex::new(StatusCondition::new(None))),
            self_ref: None,
            enabled: Arc::new(AtomicBool::new(false)),
            deleted: Arc::new(AtomicBool::new(false)),
            writers_by_topic_name: Arc::new(Mutex::new(HashMap::new())),
            writers_by_topic_handle: Arc::new(Mutex::new(HashMap::new())),
            orphaned_writers: Arc::new(Mutex::new(Vec::new())),
            default_datawriter_qos: Arc::new(Mutex::new(None)),
            participant: Some(Arc::downgrade(participant)),
            coherent_depth: Arc::new(AtomicU32::new(0)),
        };
        let publisher_arc = Arc::new(publisher.clone());
        let weak_ref = Arc::downgrade(&publisher_arc);
        {
            let mut status_condition = publisher.status_condition.lock().unwrap();
            *status_condition = StatusCondition::new(Some(weak_ref));
        }
        publisher.self_ref = Some(publisher_arc); // Without Arc, the new() function ends and memory is freed. StatusCondition's entity field would return None.
        publisher
    }

    /// Returns whether this publisher is a built-in entity.
    pub(crate) fn is_builtin(&self) -> bool {
        self.is_builtin
    }

    /// Creates a new `DataWriter` for publishing data of type `Foo` to the specified topic.
    ///
    /// A data writer is the primary interface for publishing data samples to a topic. Once created,
    /// you can use the writer's `write()` method to send data samples to subscribers that are
    /// listening to the same topic.
    ///
    /// The data writer will be automatically enabled if the participant's QoS policy
    /// `autoenable_created_entities` is set to true (which is the default).
    ///
    /// # Type Parameters
    ///
    /// * `Foo` - The data type to publish. Must be `Clone` and have a `'static` lifetime.
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to publish to. Must belong to the same `DomainParticipant` as this publisher.
    /// * `qos` - Quality of Service policies for the data writer. Use `DataWriterQos::default()` for defaults.
    /// * `listener` - Optional listener for status notifications (e.g., publication matched, deadline missed). Pass `None` if not needed.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Returns
    ///
    /// Returns `Ok(DataWriter<Foo>)` on success, or a `DdsError` if creation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The publisher has been deleted
    /// * The topic does not belong to the same `DomainParticipant` as this publisher
    /// * Type support for the topic is not found
    /// * The QoS policies are inconsistent
    /// * The DCPS bridge is not initialized
    /// Resolution chain for `QosKind::Default`: registered default → configured
    /// default profile → spec default. `QosKind::Specific` is used as-is.
    ///
    /// Shared by `create_datawriter` and `create_datawriter_dynamic` so both entry
    /// points resolve the default sentinel identically.
    fn resolve_datawriter_qos(&self, qos: QosKind<DataWriterQos>) -> DataWriterQos {
        match qos {
            QosKind::Specific(q) => q,
            QosKind::Default => {
                if let Some(registered) =
                    self.default_datawriter_qos.lock().ok().and_then(|g| g.clone())
                {
                    registered
                } else if let Ok(profile_qos) =
                    DomainParticipantFactory::get_instance().get_datawriter_qos_from_profile("")
                {
                    profile_qos
                } else {
                    DataWriterQos::default()
                }
            }
        }
    }

    pub fn create_datawriter<Foo: 'static + Clone>(
        &self,
        topic: &Topic,
        qos: impl Into<QosKind<DataWriterQos>>,
        listener: Option<Arc<dyn DataWriterListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataWriter<Foo>> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        let qos = self.resolve_datawriter_qos(qos.into());

        let type_support = self.get_participant()?.find_typesupport(topic.get_type_name());
        if type_support.is_none() {
            return Err(DdsError::Error("Failed to create DataWriter: Topic does not belong to the same DomainParticipant as Publisher.".to_string()));
        }
        let type_support =
            type_support.ok_or(DdsError::Error("TypeSupport not found for Topic".to_string()))?;

        self.create_datawriter_impl(type_support, topic, qos, listener, mask)
    }

    /// Internal implementation for creating a DataWriter with a provided TypeSupport.
    fn create_datawriter_impl<Foo: 'static + Clone>(
        &self,
        type_support: Arc<dyn TypeSupport>,
        topic: &Topic,
        qos: DataWriterQos,
        listener: Option<Arc<dyn DataWriterListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataWriter<Foo>> {
        let _ = self.cleanup_dead_writers();

        let topic_arc = self.get_participant()?.find_internal_topic(topic)?;

        qos.is_consistent()?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Publisher is not properly initialized".to_string()))?;

        let participant = self.get_participant()?;
        let entity_kind = if type_support.is_compute_key_provided() {
            EntityKind::USER_DEFINED_WRITER_WITH_KEY
        } else {
            EntityKind::USER_DEFINED_WRITER_NO_KEY
        };
        let dcps_bridge = participant.get_dcps_bridge()?;
        let guid = dcps_bridge
            .as_ref()
            .ok_or(DdsError::Error("DCPS Bridge is not initialized".to_string()))?
            .next_entity_guid(entity_kind);
        let wlp_logic = dcps_bridge
            .as_ref()
            .and_then(|bridge| bridge.get_participant().ok())
            .and_then(|p| p.wlp_logic());

        drop(dcps_bridge);

        let datawriter = DataWriter::new(
            false,
            guid,
            type_support,
            &topic_arc,
            qos,
            listener,
            mask,
            self_ref,
            wlp_logic,
        )?;

        if let Ok(()) = self.is_enabled() {
            if self.get_qos_arc()?.entity_factory.autoenable_created_entities {
                datawriter.enable()?;
            }
        }

        let writer_ops: Arc<dyn DataWriterInternal<Qos = DataWriterQos>> = datawriter
            .self_ref
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .as_ref()
            .ok_or(DdsError::Error("DataWriter is not properly initialized".to_string()))?
            .clone();
        let weak_writer = Arc::downgrade(&writer_ops);
        {
            let mut writers_by_topic_name = match self.writers_by_topic_name.lock() {
                Ok(writers_guard) => writers_guard,
                Err(e) => return Err(DdsError::Error(e.to_string())),
            };

            let mut writers_by_topic_handle = match self.writers_by_topic_handle.lock() {
                Ok(writers_guard) => writers_guard,
                Err(e) => return Err(DdsError::Error(e.to_string())),
            };

            let topic_name = topic.get_name().to_string();
            let topic_handle = topic.get_instance_handle()?;

            writers_by_topic_name
                .entry(topic_name)
                .or_insert_with(Vec::new)
                .push(weak_writer.clone());
            writers_by_topic_handle
                .entry(topic_handle)
                .or_insert_with(Vec::new)
                .push(weak_writer.clone());
        }

        Ok(datawriter)
    }

    /// Creates a new `DataWriter` using QoS settings from a loaded profile.
    ///
    /// This is a convenience method that retrieves QoS from the profile and delegates
    /// to [`create_datawriter`](Self::create_datawriter).
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to write data to.
    /// * `qos_path` - QoS path in the format `"Library::Profile"` or `"Library::Profile::QosName"`.
    ///   See [`QosProvider`](crate::config::json::QosProvider) for supported path formats.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Errors
    ///
    /// Returns an error if the profile is not found or datawriter creation fails.
    pub fn create_datawriter_with_profile<Foo: 'static + Clone>(
        &self,
        topic: &Topic,
        qos_path: &str,
        listener: Option<Arc<dyn DataWriterListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataWriter<Foo>> {
        let qos = self.get_datawriter_qos_from_profile(qos_path)?;
        self.create_datawriter::<Foo>(topic, qos, listener, mask)
    }

    /// Creates a `DataWriter` from a `<domain_participant_library>` declaration at `path`
    /// (`ParticipantLibrary::Participant::Publisher::Writer`). The topic (name, type, QoS)
    /// it follows and the writer QoS all come from the XML; only the Rust type `Foo` is in code.
    pub fn create_datawriter_from_config<Foo: DdsType + 'static + Clone>(
        &self,
        path: &str,
        listener: Option<Arc<dyn DataWriterListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataWriter<Foo>> {
        let resolved = DomainParticipantFactory::get_instance().resolve_datawriter(path)?;
        let topic = self.get_participant()?.create_topic::<Foo>(
            &resolved.topic.topic_name,
            &resolved.topic.type_name,
            resolved.topic.topic_qos,
            None,
            mask,
        )?;
        self.create_datawriter::<Foo>(&topic, resolved.qos, listener, mask)
    }

    /// Creates a `DataWriter` for `DynamicData` using a `DynamicTypeSupport`.
    ///
    /// This method is used when the data type is not known at compile time.
    /// The `DynamicTypeSupport` is typically created from a `TypeObject`.
    ///
    /// # Arguments
    ///
    /// * `topic` - The topic to write data to.
    /// * `type_support` - The `DynamicTypeSupport` describing the data type.
    /// * `qos` - QoS policies for the DataWriter.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use int2dds::xtypes::{DynamicTypeSupport, DynamicData};
    ///
    /// // Create DynamicTypeSupport from a TypeObject
    /// let type_support = DynamicTypeSupport::from_type_object(type_object)?;
    ///
    /// // Create a DataWriter for DynamicData
    /// let writer = publisher.create_datawriter_dynamic(
    ///     &topic,
    ///     Arc::new(type_support.clone()),
    ///     DataWriterQos::default(),
    ///     None,
    ///     StatusMask::default(),
    /// )?;
    ///
    /// // Create and write data
    /// let mut data = type_support.create_data();
    /// data.set("id", 42i32)?;
    /// data.set("message", "Hello!")?;
    /// writer.write(&data, InstanceHandle::NIL)?;
    /// ```
    pub fn create_datawriter_dynamic(
        &self,
        topic: &Topic,
        type_support: Arc<crate::xtypes::DynamicTypeSupport>,
        qos: impl Into<QosKind<DataWriterQos>>,
        listener: Option<Arc<dyn DataWriterListener<Foo = crate::xtypes::DynamicData>>>,
        mask: StatusMask,
    ) -> DdsResult<DataWriter<crate::xtypes::DynamicData>> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        // Same default-resolution chain as the typed create_datawriter: a caller that
        // wants the QoS profile applied passes DATAWRITER_QOS_DEFAULT. Passing a
        // concrete DataWriterQos still means "use exactly this" via the blanket
        // From<T> for QosKind<T>, so existing callers are unaffected.
        let qos = self.resolve_datawriter_qos(qos.into());

        self.create_datawriter_impl(type_support, topic, qos, listener, mask)
    }

    pub fn delete_datawriter<Foo: 'static + Clone>(
        &self,
        datawriter: DataWriter<Foo>,
    ) -> DdsResult<()> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        let arc_writer: Arc<dyn DataWriterInternal<Qos = DataWriterQos>> =
            Arc::new(datawriter.clone());
        match self.try_delete_datawriter(&arc_writer) {
            Ok(()) => Ok(()),
            Err(e) => {
                let writer_ref: Arc<dyn DataWriterInternal<Qos = DataWriterQos>> = datawriter
                    .self_ref
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .as_ref()
                    .ok_or(DdsError::Error("DataWriter is not properly initialized".to_string()))?
                    .clone();
                let mut orphaned =
                    self.orphaned_writers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                if !orphaned.iter().any(|c| Arc::ptr_eq(c, &writer_ref)) {
                    orphaned.push(writer_ref);
                }
                Err(e)
            }
        }
    }

    fn try_delete_datawriter(
        &self,
        datawriter: &Arc<dyn DataWriterInternal<Qos = DataWriterQos>>,
    ) -> DdsResult<()> {
        // Deleting from inside a listener callback would block on the writer's in-flight-callback
        // drain, which only this thread can release. Refuse instead of deadlocking. The caller must
        // delete from another thread or after the callback returns.
        if crate::utils::notify::in_listener_callback() {
            return Err(DdsError::IllegalOperation);
        }

        self.remove_orphaned_writer(datawriter);
        if datawriter.get_publisher()?.get_instance_handle()? != self.get_instance_handle()? {
            return Err(DdsError::PreconditionNotMet);
        }
        let handle = datawriter.get_instance_handle()?;
        let topic_name = datawriter.get_topic()?.get_name().to_string();
        let topic_handle = datawriter.get_topic()?.get_instance_handle()?;

        {
            let participant = self.get_participant()?;
            let mut bridge_guard = participant.get_dcps_bridge()?;
            match bridge_guard.as_mut() {
                Some(bridge) => bridge
                    .delete_rtps_writer(topic_name.clone(), handle.to_guid().entity_id())
                    .map_err(|e| DdsError::Error(e.message))?,
                None => return Err(DdsError::Error("DCPS Bridge is not initialized".to_string())),
            };
        }

        let mut removed = false;

        {
            if let Ok(mut writers_by_topic_name) = self.writers_by_topic_name.lock() {
                if let Some(weak_writers) = writers_by_topic_name.get_mut(&topic_name) {
                    // Clean up dead references
                    weak_writers.retain(|weak_writer| weak_writer.upgrade().is_some());

                    // Find and remove target writer
                    let original_len = weak_writers.len();
                    weak_writers.retain(|weak_writer| {
                        weak_writer
                            .upgrade()
                            .and_then(|writer| writer.get_instance_handle().ok())
                            .is_some_and(|h| h != handle)
                    });

                    removed = weak_writers.len() < original_len;

                    if weak_writers.is_empty() {
                        writers_by_topic_name.remove(&topic_name);
                    }
                }
            } else {
                return Err(DdsError::Error("Failed to lock writers_by_topic_name".to_string()));
            }
        }

        // If found in first map, also remove from second map
        if removed {
            if let Ok(mut writers_by_topic_handle) = self.writers_by_topic_handle.lock() {
                if let Some(writers) = writers_by_topic_handle.get_mut(&topic_handle) {
                    writers.retain(|weak_writer| {
                        weak_writer
                            .upgrade()
                            .and_then(|writer| writer.get_instance_handle().ok())
                            .is_some_and(|h| h != handle)
                    });

                    if writers.is_empty() {
                        writers_by_topic_handle.remove(&topic_handle);
                    }
                }
            }
            datawriter.delete();
            Ok(())
        } else {
            Err(DdsError::Error("DataWriter not found".to_string()))
        }
    }

    fn remove_orphaned_writer(
        &self,
        target: &Arc<dyn DataWriterInternal<Qos = DataWriterQos>>,
    ) -> bool {
        let mut orphaned_writers = self.orphaned_writers.lock().unwrap();
        if let Some(pos) = orphaned_writers
            .iter()
            .position(|t| t.get_instance_handle().unwrap() == target.get_instance_handle().unwrap())
        {
            orphaned_writers.remove(pos);
            true
        } else {
            false
        }
    }

    pub(crate) fn handle_writer_drop(
        &self,
        topic_name: &str,
        topic_handle: &InstanceHandle,
        writer_handle: &InstanceHandle,
    ) {
        let mut found_writer: Option<Arc<dyn DataWriterInternal<Qos = DataWriterQos>>> = None;

        // First search in writers_by_topic_name
        if let Ok(writers_by_topic_name) = self.writers_by_topic_name.lock() {
            if let Some(weak_writers) = writers_by_topic_name.get(topic_name) {
                for weak_writer in weak_writers.iter() {
                    if let Some(strong_writer) = weak_writer.upgrade() {
                        if let Ok(handle) = strong_writer.get_instance_handle() {
                            if handle == *writer_handle {
                                found_writer = Some(strong_writer);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // If not found, search in writers_by_topic_handle
        if found_writer.is_none() {
            if let Ok(writers_by_topic_handle) = self.writers_by_topic_handle.lock() {
                if let Some(weak_writers) = writers_by_topic_handle.get(topic_handle) {
                    for weak_writer in weak_writers.iter() {
                        if let Some(strong_writer) = weak_writer.upgrade() {
                            if let Ok(handle) = strong_writer.get_instance_handle() {
                                if handle == *writer_handle {
                                    found_writer = Some(strong_writer);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // If found, add to orphaned_writers
        if let Some(writer) = found_writer {
            if let Ok(mut orphaned_writers) = self.orphaned_writers.lock() {
                if !orphaned_writers.iter().any(|c| Arc::ptr_eq(c, &writer)) {
                    orphaned_writers.push(writer);
                }
            }
        }
        // If not found, it was already properly deleted via delete_writer, so do nothing
    }

    pub fn get_data_writers(&self) -> DdsResult<Vec<Arc<dyn DataWriterBase<Qos = DataWriterQos>>>> {
        let mut result = Vec::new();
        let writers_guard =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for weak_writers in writers_guard.values() {
            for weak_writer in weak_writers.iter() {
                if let Some(writer) = weak_writer.upgrade() {
                    result.push(writer as Arc<dyn DataWriterBase<Qos = DataWriterQos>>);
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn get_datawriters_internal(
        &self,
    ) -> DdsResult<Vec<Arc<dyn DataWriterInternal<Qos = DataWriterQos>>>> {
        self.is_deleted()?;
        let mut result = Vec::new();
        let writers_by_topic_name =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for weak_writers in writers_by_topic_name.values() {
            for weak_writer in weak_writers.iter() {
                if let Some(writer) = weak_writer.upgrade() {
                    result.push(writer as Arc<dyn DataWriterInternal<Qos = DataWriterQos>>);
                }
            }
        }
        Ok(result)
    }

    pub fn lookup_datawriter<Foo: 'static + Clone>(
        &self,
        topic_name: &str,
    ) -> DdsResult<DataWriter<Foo>> {
        let _ = self.cleanup_dead_writers();
        self.is_deleted()?;
        {
            let writers_guard =
                self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

            if let Some(weak_writers) = writers_guard.get(topic_name) {
                for weak_writer in weak_writers {
                    if let Some(writer) = weak_writer.upgrade() {
                        if let Some(typed_writer) =
                            writer.as_any().downcast_ref::<DataWriter<Foo>>()
                        {
                            return Ok(typed_writer.clone());
                        }
                    }
                }
            }
        }
        Err(DdsError::Error("DataWriter not found.".to_string()))
    }

    pub fn get_datawriters_of_type<Foo: 'static>(&self) -> DdsResult<Vec<Arc<DataWriter<Foo>>>> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();
        let mut result = Vec::new();

        if let Ok(writers_by_topic_name) = self.writers_by_topic_name.lock() {
            for weak_writers in writers_by_topic_name.values() {
                for weak_writer in weak_writers.iter() {
                    if let Some(writer) = weak_writer.upgrade() {
                        if writer.get_type_id() == target_type_id {
                            // Convert from DataWriterBase to Arc<DataWriter<Foo>>
                            if let Some(typed_writer) =
                                writer.as_any().downcast_ref::<DataWriter<Foo>>()
                            {
                                // Get Arc through self_ref
                                if let Ok(guard) = &typed_writer.self_ref.lock() {
                                    if let Some(arc_writer) = guard.as_ref() {
                                        result.push(arc_writer.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    // Function to find a specific type DataWriter for a specific topic
    pub fn get_datawriter<Foo: 'static>(
        &self,
        topic_name: &str,
    ) -> DdsResult<Option<Arc<DataWriter<Foo>>>> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(writers_by_topic_name) = self.writers_by_topic_name.lock() {
            if let Some(weak_writers) = writers_by_topic_name.get(topic_name) {
                for weak_writer in weak_writers.iter() {
                    if let Some(writer) = weak_writer.upgrade() {
                        if writer.get_type_id() == target_type_id {
                            if let Some(typed_writer) =
                                writer.as_any().downcast_ref::<DataWriter<Foo>>()
                            {
                                if let Ok(guard) = &typed_writer.self_ref.lock() {
                                    if let Some(arc_writer) = guard.as_ref() {
                                        return Ok(Some(arc_writer.clone()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    // Process with closure internally (no lifetime issues)
    pub fn with_datawriter<Foo: 'static, R>(
        &self,
        topic_name: &str,
        f: impl FnOnce(&DataWriter<Foo>) -> R,
    ) -> DdsResult<Option<R>> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(writers_by_topic_name) = self.writers_by_topic_name.lock() {
            if let Some(weak_writers) = writers_by_topic_name.get(topic_name) {
                for weak_writer in weak_writers.iter() {
                    if let Some(writer) = weak_writer.upgrade() {
                        if writer.get_type_id() == target_type_id {
                            if let Some(typed_writer) =
                                writer.as_any().downcast_ref::<DataWriter<Foo>>()
                            {
                                return Ok(Some(f(typed_writer)));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    // Execute closure for all DataWriters of a specific type
    pub fn for_each_datawriter_of_type<Foo: 'static>(
        &self,
        mut f: impl FnMut(&DataWriter<Foo>),
    ) -> DdsResult<()> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(writers_by_topic_name) = self.writers_by_topic_name.lock() {
            for weak_writers in writers_by_topic_name.values() {
                for weak_writer in weak_writers.iter() {
                    if let Some(writer) = weak_writer.upgrade() {
                        if writer.get_type_id() == target_type_id {
                            if let Some(typed_writer) =
                                writer.as_any().downcast_ref::<DataWriter<Foo>>()
                            {
                                f(typed_writer);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // Return DataWriters grouped by type
    #[allow(clippy::type_complexity)]
    pub fn get_writers_by_type(
        &self,
    ) -> DdsResult<HashMap<TypeId, Vec<Box<dyn DataWriterBase<Qos = DataWriterQos>>>>> {
        self.is_deleted()?;
        let mut type_map: HashMap<TypeId, Vec<Box<dyn DataWriterBase<Qos = DataWriterQos>>>> =
            HashMap::new();

        if let Ok(writers_by_topic_name) = self.writers_by_topic_name.lock() {
            for weak_writers in writers_by_topic_name.values() {
                for weak_writer in weak_writers.iter() {
                    if let Some(writer) = weak_writer.upgrade() {
                        let type_id = writer.get_type_id();
                        let type_writers = type_map.entry(type_id).or_default();
                        type_writers.push(writer.clone_boxed());
                    }
                }
            }
        }

        Ok(type_map)
    }

    // TODO
    pub fn suspend_publications(&self) -> DdsResult<()> {
        /*
            This operation notifies the Service that the application is about to make multiple modifications using DataWriter objects belonging to the Publisher.
            This is a hint to the Service to allow it to optimize its performance, for example by holding back the propagation of the modifications and batch them together.
            The Service is not required to use this hint.

            After calling this operation, subsequent modifications must be matched by a call to resume_publications indicating that the modifications have completed.
            If the Publisher is deleted before resume_publications is called, any pending modifications that have not been propagated are discarded.
        */
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    // TODO
    pub fn resume_publications(&self) -> DdsResult<()> {
        /*
            This operation indicates to the Service that the application has completed the multiple modifications initiated by the previous suspend_publications.
            This is a hint that the Service can use, for example, to batch all the modifications made since suspend_publications.

            The call to resume_publications must match a previous call to suspend_publications.
            Otherwise, this operation returns PRECONDITION_NOT_MET error.
        */
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    pub fn begin_coherent_changes(&self) -> DdsResult<()> {
        /*
            This operation requests the application to begin a 'coherent set' of modifications using DataWriter objects attached to the Publisher.
            The 'coherent set' is terminated by calling end_coherent_changes.

            A 'coherent set' is a set of modifications that should be interpreted by the receiving side as a consistent set of modifications.
            That is, the receiver can only access the data after all the modifications in the set have been received.

            Connectivity state changes (connectivity changes) may occur during a modification set.
            For example, the partition set of the Publisher or its Subscriber may change, a late-joining DataReader may appear on the network, or communication errors may occur.
            If such changes cause an entity to not receive the complete modification set, that entity must behave as if it received none of the set.

            This call can be nested. In this case, the set is terminated when the last end_coherent_changes call is made.

            The 'coherent changes' feature allows a publisher application to modify multiple data instance values together, and make those changes appear atomic to readers.
            This is useful, for example, when two data instances represent 'altitude' and 'velocity vector' that change together for the same aircraft.
            Without delivering both values together, readers might misinterpret them as indicating an aircraft on a collision course.
        */
        self.is_deleted()?;
        // Nested calls only deepen the current set; a new set starts at depth 0 -> 1.
        self.coherent_depth.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    pub fn end_coherent_changes(&self) -> DdsResult<()> {
        /*
            This operation terminates the 'coherent set' initiated by begin_coherent_changes.
            If called without a matching begin_coherent_changes call, this operation returns PRECONDITION_NOT_MET error.
        */
        self.is_deleted()?;

        // fetch_update returns the pre-decrement depth; checked_sub refuses to go below zero.
        match self
            .coherent_depth
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |depth| depth.checked_sub(1))
        {
            // Depth was 0: no matching begin_coherent_changes.
            Err(_) => Err(DdsError::PreconditionNotMet),
            // Depth 1 -> 0: outermost end closes the set, writers send their end markers.
            Ok(1) => self.end_writer_coherent_sets(),
            // Depth 2+ -> 1+: nested end, the set stays open.
            Ok(_) => Ok(()),
        }
    }

    // Ask every attached writer to close its open coherent set; returns the first error.
    fn end_writer_coherent_sets(&self) -> DdsResult<()> {
        let writers_by_topic_name =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let mut result = Ok(());
        for weak_writers in writers_by_topic_name.values() {
            for weak_writer in weak_writers.iter() {
                if let Some(writer) = weak_writer.upgrade() {
                    let end_result = writer.end_coherent_set();
                    if result.is_ok() {
                        result = end_result;
                    }
                }
            }
        }
        result
    }

    // True while a coherent set is open (begin called without matching end).
    pub(crate) fn in_coherent_changes(&self) -> bool {
        self.coherent_depth.load(Ordering::Acquire) > 0
    }

    pub fn delete_contained_entities(&self) -> DdsResult<()> {
        /*
            This operation deletes all entities that were created using create operations on the Publisher.
            That is, it deletes all DataWriter objects contained by this Publisher.

            If any of the contained entities is in a state where it cannot be deleted, this operation returns PRECONDITION_NOT_MET error.
            When delete_contained_entities returns successfully, the application is guaranteed that the Publisher no longer contains any DataWriter objects and can delete the Publisher.
        */
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        {
            match self.get_datawriters_internal() {
                Ok(writers) => {
                    for writer in writers {
                        self.try_delete_datawriter(&writer)?;
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
    pub fn set_default_datawriter_qos(
        &self,
        qos: impl Into<QosKind<DataWriterQos>>,
    ) -> DdsResult<()> {
        self.is_deleted()?;
        match qos.into() {
            QosKind::Default => self.reset_default_datawriter_qos(),
            QosKind::Specific(qos) => {
                qos.is_consistent()?;
                match self.default_datawriter_qos.lock() {
                    Ok(mut default_qos) => {
                        *default_qos = Some(qos);
                        Ok(())
                    }
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
            }
        }
    }

    fn reset_default_datawriter_qos(&self) -> DdsResult<()> {
        match self.default_datawriter_qos.lock() {
            Ok(mut default_qos) => {
                *default_qos = None;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn get_default_datawriter_qos(&self) -> DdsResult<DataWriterQos> {
        self.is_deleted()?;
        match self.default_datawriter_qos.lock() {
            Ok(default_datawriter_qos) => Ok(default_datawriter_qos.clone().unwrap_or_default()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    /// Retrieves `DataWriterQos` from a loaded profile.
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    ///
    /// Returns an error if the publisher is deleted or the profile is not found.
    pub fn get_datawriter_qos_from_profile(&self, qos_path: &str) -> DdsResult<DataWriterQos> {
        self.is_deleted()?;
        DomainParticipantFactory::get_instance().get_datawriter_qos_from_profile(qos_path)
    }

    pub fn copy_from_topic_qos(
        &self,
        mut datawriter_qos: DataWriterQos,
        topic_qos: &TopicQos,
    ) -> DdsResult<DataWriterQos> {
        self.is_deleted()?;
        datawriter_qos.durability = topic_qos.durability;
        datawriter_qos.durability_service = topic_qos.durability_service;
        datawriter_qos.deadline = topic_qos.deadline;
        datawriter_qos.latency_budget = topic_qos.latency_budget;
        datawriter_qos.liveliness = topic_qos.liveliness;
        datawriter_qos.reliability = topic_qos.reliability;
        datawriter_qos.destination_order = topic_qos.destination_order;
        datawriter_qos.history = topic_qos.history;
        datawriter_qos.resource_limits = topic_qos.resource_limits;
        datawriter_qos.transport_priority = topic_qos.transport_priority;
        datawriter_qos.lifespan = topic_qos.lifespan;
        datawriter_qos.ownership = topic_qos.ownership;
        datawriter_qos.data_representation = topic_qos.data_representation.clone();
        Ok(datawriter_qos)
    }

    pub fn wait_for_acknowledgments(&self, max_wait: Duration) -> DdsResult<()> {
        /*
            This operation blocks the calling thread until one of the following occurs:
            All data written by reliable DataWriter entities are acknowledged by all matched reliable DataReader entities, or
            The maximum wait time specified by the max_wait parameter elapses,
            whichever happens first.
            A return value of OK means that all written samples have been acknowledged by all matched reliable DataReaders.
            A return value of TIMEOUT means that not all data was acknowledged before the max_wait time elapsed.
        */
        self.is_deleted()?;
        let mut current = max_wait;
        let writers_by_topic_name =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let participant = self.get_participant()?;

        for weak_writers in writers_by_topic_name.values() {
            for weak_writer in weak_writers {
                if let Some(writer) = weak_writer.upgrade() {
                    let begin = participant.get_current_time()?;
                    writer.wait_for_acknowledgments(current)?;
                    let end = participant.get_current_time()?;

                    current = current - (end - begin);
                    if current < Duration::zero() {
                        return Err(DdsError::Timeout);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn get_participant(&self) -> DdsResult<DomainParticipant> {
        self.is_deleted()?;
        if let Some(weak_ref) = self.participant.as_ref() {
            // Attempt to upgrade Weak<T> to Arc<T>
            if let Some(participant_arc) = weak_ref.upgrade() {
                let mut participant = (*participant_arc).clone();
                participant.self_ref = Some(participant_arc);
                return Ok(participant);
            }
        }

        // Case when participant is None or reference has expired
        Err(DdsError::Error("Participant reference is invalid or expired".to_string()))
    }

    /// Upgraded parent handle without the deep clone [`Self::get_participant`] performs.
    ///
    /// Same failure modes and messages -- `AlreadyDeleted` when this publisher is deleted,
    /// `Error` when the parent `Weak` has expired -- but two atomic read-modify-writes instead
    /// of ~52. `get_participant` clones a struct of 25 `Arc` fields to call one method on it,
    /// and `write()` does that on every sample.
    ///
    /// Returns [`ParticipantRef`], not a bare `Arc`: `get_participant` force-sets `self_ref` on
    /// the value it hands back, so that value performs the factory orphan handoff when it is
    /// the last reference standing. `ParticipantRef` reproduces it for two plain atomic loads.
    ///
    /// Only for internal call sites that need `get_current_time`, `get_listener`,
    /// `set_communication_status` or the QoS accessors. The value behind the returned handle
    /// has `self_ref: None`, so it must not reach `create_*`/`delete_*`; use
    /// [`Self::get_participant`] for those.
    pub(crate) fn participant_arc(&self) -> DdsResult<ParticipantRef> {
        self.is_deleted()?;
        self.participant
            .as_ref()
            .and_then(|weak_ref| weak_ref.upgrade())
            .map(ParticipantRef::new)
            .ok_or_else(|| {
                DdsError::Error("Participant reference is invalid or expired".to_string())
            })
    }

    pub(crate) fn has_active_entities(&self) -> DdsResult<bool> {
        self.is_deleted()?;
        {
            match self.writers_by_topic_name.lock() {
                Ok(writers) => {
                    // Check for non-builtin writers
                    for weak_writers in writers.values() {
                        for weak_writer in weak_writers {
                            if let Some(writer) = weak_writer.upgrade() {
                                if !writer.is_builtin() {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                }
                Err(e) => return Err(DdsError::Error(e.to_string())),
            }
        }
        Ok(false)
    }

    pub fn contains_entity(&self, handle: InstanceHandle) -> DdsResult<bool> {
        self.is_deleted()?;
        match self.writers_by_topic_name.lock() {
            Ok(writers_by_topic_name) => {
                for weak_writers in writers_by_topic_name.values() {
                    for weak_writer in weak_writers {
                        if let Some(writer) = weak_writer.upgrade() {
                            if writer.get_instance_handle()? == handle {
                                return Ok(true);
                            }
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => Err(DdsError::Error(e.to_string())), // Return false when lock acquisition fails
        }
    }

    // For Entity
    pub fn set_listener(
        &self,
        listener: Option<Arc<dyn PublisherListener>>,
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
    pub fn get_listener(&self) -> DdsResult<Option<Arc<dyn PublisherListener>>> {
        self.is_deleted()?;
        match self.listener.read() {
            Ok(guard) => Ok(guard.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub(crate) fn disable(&self) -> DdsResult<()> {
        self.set_listener(None, StatusMask::default())?;

        let writers =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for topic_writers in writers.values() {
            for weak_writer in topic_writers {
                if let Some(writer) = weak_writer.upgrade() {
                    writer.disable()?;
                }
            }
        }

        Ok(())
    }

    pub(crate) fn is_enabled(&self) -> DdsResult<()> {
        self.is_deleted()?;
        if self.enabled.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(DdsError::NotEnabled)
        }
    }

    pub(crate) fn has_writers_for_topic(&self, topic: &Topic) -> DdsResult<bool> {
        match self.writers_by_topic_handle.lock() {
            Ok(writers_by_topic_handle) => {
                Ok(writers_by_topic_handle.contains_key(&topic.get_instance_handle()?))
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn cleanup_dead_writers(&self) -> DdsResult<()> {
        let mut writers_by_name =
            self.writers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let mut writers_by_handle =
            self.writers_by_topic_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        // Clean up both data structures
        for weak_writers in writers_by_name.values_mut() {
            weak_writers.retain(|weak_writer| weak_writer.upgrade().is_some());
        }
        writers_by_name.retain(|_, writers| !writers.is_empty());

        for weak_writers in writers_by_handle.values_mut() {
            weak_writers.retain(|weak_writer| weak_writer.upgrade().is_some());
        }
        writers_by_handle.retain(|_, writers| !writers.is_empty());

        Ok(())
    }

    pub(crate) fn delete(&mut self) {
        self.self_ref = None;

        // Wake any WaitSet parked on this publisher's status condition and let it drop it.
        if let Ok(status_condition) = self.status_condition.lock() {
            status_condition.mark_dead();
        }

        self.deleted.store(true, Ordering::SeqCst);
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
mod tests {
    use int2dds_derive::DdsType;

    use super::*;
    use crate::{
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        publication::qos::PublisherQos,
    };

    #[derive(DdsType)]
    pub struct HelloWorld {
        pub index: u32,
        pub message: String,
    }

    use std::sync::mpsc::SyncSender;
    use std::sync::Arc;

    const DRAIN_CALLBACK_SLEEP: std::time::Duration = std::time::Duration::from_millis(400);
    const DRAIN_MIN_BLOCK: std::time::Duration = std::time::Duration::from_millis(250);
    const DRAIN_DEADLINE: std::time::Duration = std::time::Duration::from_secs(20);

    struct MatchSleepWriterListener {
        entered: SyncSender<()>,
    }

    impl DataWriterListener for MatchSleepWriterListener {
        type Foo = HelloWorld;

        fn on_publication_matched(
            &self,
            _writer: &DataWriter<Self::Foo>,
            _status: &crate::infrastructure::status::PublicationMatchedStatus,
        ) {
            let _ = self.entered.try_send(());
            std::thread::sleep(DRAIN_CALLBACK_SLEEP);
        }
    }

    #[test]
    fn deleting_a_writer_waits_for_its_in_flight_publication_matched_callback() {
        use crate::{
            core::time::Duration,
            infrastructure::qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
            subscription::qos::{DataReaderQos, SubscriberQos},
            test_utils::unique_domain_id,
            topic::qos::TopicQos,
        };
        use std::time::Instant;

        let reliable = ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration::from_millis(100),
        };

        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel::<()>(1);

        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();

        // Publisher side: the writer whose PUBLICATION_MATCHED callback we make in-flight.
        let pub_participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let pub_topic = pub_participant
            .create_topic::<HelloWorld>(
                "PubMatchDrainTopic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let publisher = pub_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorld>(
                &pub_topic,
                DataWriterQos { reliability: reliable.clone(), ..Default::default() },
                Some(Arc::new(MatchSleepWriterListener { entered: entered_tx })),
                StatusMask::PUBLICATION_MATCHED,
            )
            .unwrap();

        // Subscriber side: a remote reader whose discovery matches the writer, firing
        // PUBLICATION_MATCHED on the publisher participant's discovery thread.
        let sub_participant = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let sub_topic = sub_participant
            .create_topic::<HelloWorld>(
                "PubMatchDrainTopic",
                "HelloWorld",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();
        let subscriber = sub_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let _reader = subscriber
            .create_datareader::<HelloWorld>(
                &sub_topic,
                DataReaderQos { reliability: reliable, ..Default::default() },
                None,
                StatusMask::default(),
            )
            .unwrap();

        // on_publication_matched has started on the discovery thread and is now sleeping.
        entered_rx.recv_timeout(DRAIN_DEADLINE).expect("on_publication_matched never ran");

        let start = Instant::now();
        publisher.delete_datawriter(writer).unwrap();
        let elapsed = start.elapsed();

        assert!(
            elapsed >= DRAIN_MIN_BLOCK,
            "delete returned in {elapsed:?}, so it did not wait for the in-flight publication-matched callback"
        );

        pub_participant.delete_contained_entities().unwrap();
        factory.delete_participant(pub_participant).unwrap();
        sub_participant.delete_contained_entities().unwrap();
        factory.delete_participant(sub_participant).unwrap();
    }

    #[test]
    fn test_publisher_drop_without_delete() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let publisher = domain_participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let publisher_handle = publisher.get_instance_handle().unwrap();

        drop(publisher);
        let contains_publisher = domain_participant.contains_entity(publisher_handle).unwrap();
        assert!(contains_publisher, "Publisher deleted!")
    }

    #[test]
    fn test_delete_datawriter_removes_rtps_writer() {
        use crate::{
            core::time::Duration,
            infrastructure::{
                qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
                wait_set::WaitSet,
            },
            subscription::qos::{DataReaderQos, SubscriberQos},
            test_utils::unique_domain_id,
        };

        let domain_id = unique_domain_id();
        let factory = DomainParticipantFactory::get_instance();

        // Participant 1: publisher side
        let participant1 = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic1 = participant1
            .create_topic::<HelloWorld>(
                "test_delete_writer",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let reliable_qos = ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: Duration::from_seconds(1),
        };

        let publisher = participant1
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();
        let writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic1,
                DataWriterQos { reliability: reliable_qos.clone(), ..Default::default() },
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Participant 2: subscriber side
        let participant2 = factory
            .create_participant(
                domain_id,
                DomainParticipantQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let topic2 = participant2
            .create_topic::<HelloWorld>(
                "test_delete_writer",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = participant2
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();
        let reader = subscriber
            .create_datareader::<HelloWorld>(
                &topic2,
                DataReaderQos { reliability: reliable_qos, ..Default::default() },
                None,
                StatusMask::default(),
            )
            .unwrap();

        // Wait for publication matched
        let wait_set = WaitSet::new();
        let cond = writer.get_statuscondition().unwrap().clone();
        cond.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        wait_set.attach_condition(cond.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(5)).unwrap();
        let pub_status = writer.get_publication_matched_status().unwrap();
        assert_eq!(pub_status.current_count(), 1);
        wait_set.detach_condition(cond).unwrap();

        // Wait for subscription matched
        let cond = reader.get_statuscondition().unwrap().clone();
        cond.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(cond.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(5)).unwrap();
        let sub_status = reader.get_subscription_matched_status().unwrap();
        assert_eq!(sub_status.current_count(), 1);
        wait_set.detach_condition(cond).unwrap();

        // Delete writer
        publisher.delete_datawriter(writer).unwrap();

        // Wait for subscription matched to drop to 0
        let cond = reader.get_statuscondition().unwrap().clone();
        cond.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(cond.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(5)).unwrap();
        let sub_status = reader.get_subscription_matched_status().unwrap();
        assert_eq!(
            sub_status.current_count(),
            0,
            "Reader should see 0 matched writers after delete_datawriter"
        );
        wait_set.detach_condition(cond).unwrap();

        participant1.delete_contained_entities().unwrap();
        factory.delete_participant(participant1).unwrap();
        participant2.delete_contained_entities().unwrap();
        factory.delete_participant(participant2).unwrap();
    }

    #[test]
    fn test_delete_contained_entities_publisher() {
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

        let publisher = participant
            .create_publisher(PublisherQos::default(), None, StatusMask::default())
            .unwrap();

        let _writer = publisher
            .create_datawriter::<HelloWorld>(
                &topic,
                DataWriterQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        assert!(!publisher.get_data_writers().unwrap().is_empty());

        publisher.delete_contained_entities().unwrap();

        assert!(publisher.get_data_writers().unwrap().is_empty());

        participant.delete_contained_entities().unwrap();
        factory.delete_participant(participant).unwrap();
    }
}
