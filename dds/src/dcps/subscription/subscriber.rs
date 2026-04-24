//! Subscriber - Container and factory for DataReader objects.
//!
//! A `Subscriber` acts as a container for DataReader objects that receive data from topics.
//! It provides a way to group related data readers together and manage their lifecycle.
//!
//! # Overview
//!
//! Subscribers are created by a `DomainParticipant` and are used to create `DataReader`
//! objects. All data readers created by the same subscriber share the subscriber's QoS
//! policies (unless overridden).
//!
//! # Key Features
//!
//! - **DataReader Factory**: Creates and manages DataReader objects
//! - **QoS Management**: Subscribers have their own QoS policies
//! - **Entity Lookup**: Find data readers by topic name

use std::{
    any::TypeId,
    collections::HashMap,
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
};

use crate::{
    common::instance_handle::InstanceHandle,
    core::error::{DdsError, DdsResult},
    dcps::topic::type_support::TypeSupport,
    domain::{
        domain_participant::DomainParticipant, domain_participant_factory::DomainParticipantFactory,
    },
    infrastructure::{
        domain_entity::DomainEntity,
        entity::{
            impl_check_parent_enabled, impl_dds_entity, impl_dds_entity_impl, BaseEntity,
            EnableChild, Entity, EntityInternal, UpdateStatus,
        },
        qos_kind::QosKind,
        qos_policy::{PresentationQosAccessScopeKind, Qos},
        status::StatusMask,
        status_condition::StatusCondition,
    },
    rtps::{
        common::{entity_kind::EntityKind, guid::Guid},
        entities::reader::Reader,
    },
    topic::{qos::TopicQos, topic_description::TopicDescription},
    DdsType,
};

use super::{
    data_reader::{DataReader, DataReaderBase, DataReaderInternal},
    data_reader_listener::DataReaderListener,
    qos::{DataReaderQos, SubscriberQos},
    sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
    subscriber_listener::SubscriberListener,
};

fn effective_topic_name(topic_description: &dyn TopicDescription) -> DdsResult<String> {
    if let Some(cft) = topic_description
        .as_any()
        .downcast_ref::<crate::topic::content_filtered_topic::ContentFilteredTopic>()
    {
        return Ok(cft.get_related_topic()?.get_name().to_string());
    }
    Ok(topic_description.get_name().to_string())
}

#[derive(Clone)]
pub struct Subscriber {
    // Indicates whether this entity is a built-in entity.
    //
    // Built-in entities are managed internally and have restricted operations:
    // - Cannot be deleted (delete_subscriber)
    // - Cannot modify QoS (set_qos)
    // - Cannot create/delete child DataReaders (create_datareader, delete_datareader)
    //
    // See also: DomainParticipant::get_builtin_subscriber()
    is_builtin: bool,
    guid: Guid,
    qos: Arc<Mutex<SubscriberQos>>,
    listener: Arc<RwLock<Option<Arc<dyn SubscriberListener>>>>,
    mask: Arc<RwLock<StatusMask>>,
    status_condition: Arc<Mutex<StatusCondition<SubscriberQos>>>,
    pub(crate) self_ref: Option<Arc<Subscriber>>,
    pub(crate) enabled: Arc<AtomicBool>,
    deleted: Arc<AtomicBool>,
    #[allow(clippy::type_complexity)]
    readers_by_topic_name:
        Arc<Mutex<HashMap<String, Vec<Weak<dyn DataReaderInternal<Qos = DataReaderQos>>>>>>,
    #[allow(clippy::type_complexity)]
    readers_by_topic_handle:
        Arc<Mutex<HashMap<InstanceHandle, Vec<Weak<dyn DataReaderInternal<Qos = DataReaderQos>>>>>>,
    builtin_readers: Arc<Mutex<Vec<Arc<dyn DataReaderInternal<Qos = DataReaderQos>>>>>,
    orphaned_readers: Arc<Mutex<Vec<Arc<dyn DataReaderInternal<Qos = DataReaderQos>>>>>,
    default_datareader_qos: Arc<Mutex<Option<DataReaderQos>>>,
    participant: Option<Weak<DomainParticipant>>,
}

impl Debug for Subscriber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Subscriber")
            .field("guid", &self.guid)
            .field("qos", &self.qos.lock().unwrap())
            .field(
                "listener",
                &self.listener.read().unwrap().as_ref().map(|_| "Arc<dyn SubscriberListener>"),
            )
            .field("mask", &self.mask.read().unwrap())
            .field("status_condition", &self.status_condition.lock().unwrap())
            .field("self_ref", &self.self_ref.as_ref().map(|_| "Arc<Subscriber>"))
            .field("enabled", &self.enabled.load(std::sync::atomic::Ordering::Acquire))
            .field("deleted", &self.deleted.load(std::sync::atomic::Ordering::Acquire))
            .finish()
    }
}

impl PartialEq for Subscriber {
    fn eq(&self, other: &Self) -> bool {
        self.guid == other.guid
    }
}
impl Eq for Subscriber {}

impl Drop for Subscriber {
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
                    let subscriber_handle = InstanceHandle::from_guid(&self.guid);
                    participant.handle_subscriber_drop(&subscriber_handle);
                }
            }
        }
    }
}

impl_dds_entity!(Subscriber, SubscriberQos);
impl EnableChild for Subscriber {
    fn enable_child_entities(&self) -> DdsResult<()> {
        let readers_by_topic_name =
            self.readers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for weak_readers in readers_by_topic_name.values() {
            for weak_reader in weak_readers.iter() {
                if let Some(reader) = weak_reader.upgrade() {
                    reader.enable()?; // Enable only live ones
                }
            }
        }

        Ok(())
    }

    impl_check_parent_enabled!(get_participant);
}
impl UpdateStatus for Subscriber {}
impl DomainEntity for Subscriber {}

impl Subscriber {
    pub(crate) fn new(
        is_builtin: bool,
        qos: SubscriberQos,
        listener: Option<Arc<dyn SubscriberListener>>,
        mask: StatusMask,
        handle: InstanceHandle,
        participant: &Arc<DomainParticipant>,
    ) -> Self {
        let mut subscriber = Self {
            is_builtin,
            guid: handle.to_guid(),
            qos: Arc::new(Mutex::new(qos)),
            listener: Arc::new(RwLock::new(listener)),
            mask: Arc::new(RwLock::new(mask)),
            status_condition: Arc::new(Mutex::new(StatusCondition::new(None))),
            self_ref: None,
            enabled: Arc::new(AtomicBool::new(false)),
            deleted: Arc::new(AtomicBool::new(false)),
            readers_by_topic_name: Arc::new(Mutex::new(HashMap::new())),
            readers_by_topic_handle: Arc::new(Mutex::new(HashMap::new())),
            builtin_readers: Arc::new(Mutex::new(Vec::new())),
            orphaned_readers: Arc::new(Mutex::new(Vec::new())),
            default_datareader_qos: Arc::new(Mutex::new(None)),
            participant: Some(Arc::downgrade(participant)),
        };
        let subscriber_arc = Arc::new(subscriber.clone());
        let weak_ref = Arc::downgrade(&subscriber_arc);
        {
            let mut status_condition = subscriber.status_condition.lock().unwrap();
            *status_condition = StatusCondition::new(Some(weak_ref));
        }
        subscriber.self_ref = Some(subscriber_arc); // Without Arc, the new() function ends and memory is freed. StatusCondition's entity field would return None.
        subscriber
    }

    /// Returns whether this subscriber is a built-in entity.
    pub(crate) fn is_builtin(&self) -> bool {
        self.is_builtin
    }

    /// Creates a new `DataReader` for receiving data of type `Foo` from the specified topic.
    ///
    /// A data reader is the primary interface for receiving data samples from a topic. Once created,
    /// you can use the reader's `read()` or `take()` methods to access data samples published by
    /// data writers on the same topic. You can also use a listener to be notified when data arrives.
    ///
    /// The data reader will be automatically enabled if the participant's QoS policy
    /// `autoenable_created_entities` is set to true (which is the default).
    ///
    /// # Type Parameters
    ///
    /// * `Foo` - The data type to receive. Must implement the `DdsType` trait.
    ///
    /// # Arguments
    ///
    /// * `topic_description` - The topic to subscribe to. Can be a `Topic` or `ContentFilteredTopic`.
    ///   Must belong to the same `DomainParticipant` as this subscriber.
    /// * `qos` - Quality of Service policies for the data reader. Use `DataReaderQos::default()` for defaults.
    /// * `listener` - Optional listener for notifications (e.g., data available, subscription matched). Pass `None` if not needed.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Returns
    ///
    /// Returns `Ok(DataReader<Foo>)` on success, or a `DdsError` if creation fails.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * The subscriber has been deleted
    /// * The topic does not belong to the same `DomainParticipant` as this subscriber
    /// * Type support for the topic is not found
    /// * The QoS policies are inconsistent
    /// * The DCPS bridge is not initialized
    /// * The topic description type is unsupported
    pub fn create_datareader<Foo: DdsType>(
        &self,
        topic_description: &dyn TopicDescription,
        qos: impl Into<QosKind<DataReaderQos>>,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataReader<Foo>> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        // Resolution chain for QosKind::Default: registered default → configured
        // default profile → spec default. QosKind::Specific is used as-is.
        let qos = match qos.into() {
            QosKind::Specific(q) => q,
            QosKind::Default => {
                if let Some(registered) =
                    self.default_datareader_qos.lock().ok().and_then(|g| g.clone())
                {
                    registered
                } else if let Ok(profile_qos) =
                    DomainParticipantFactory::get_instance().get_datareader_qos_from_profile("")
                {
                    profile_qos
                } else {
                    DataReaderQos::default()
                }
            }
        };

        let type_support =
            self.get_participant()?.find_typesupport(topic_description.get_type_name());
        if type_support.is_none() {
            return Err(DdsError::Error("Failed to create DataReader: Topic does not belong to the same DomainParticipant as Subscriber.".to_string()));
        }
        let type_support =
            type_support.ok_or(DdsError::Error("TypeSupport not found for Topic".to_string()))?;

        self.create_datareader_impl(type_support, topic_description, qos, listener, mask)
    }

    /// Internal implementation for creating a DataReader with a provided TypeSupport.
    fn create_datareader_impl<Foo: DdsType>(
        &self,
        type_support: Arc<dyn TypeSupport>,
        topic_description: &dyn TopicDescription,
        qos: DataReaderQos,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataReader<Foo>> {
        let _ = self.cleanup_dead_readers();

        qos.is_consistent()?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Subscriber is not properly initialized".to_string()))?;

        let participant = self.get_participant()?;
        let entity_kind = if type_support.is_compute_key_provided() {
            EntityKind::USER_DEFINED_READER_WITH_KEY
        } else {
            EntityKind::USER_DEFINED_READER_NO_KEY
        };
        let dcps_bridge = participant.get_dcps_bridge()?;
        let guid = dcps_bridge
            .as_ref()
            .ok_or(DdsError::Error("DCPS Bridge is not initialized".to_string()))?
            .next_entity_guid(entity_kind);

        drop(dcps_bridge);

        let datareader = DataReader::new(
            false,
            guid,
            type_support,
            topic_description,
            qos,
            listener,
            mask,
            self_ref,
            None, // Non-builtin: RTPS reader created in enable_rtps_entities()
        )?;

        if let Ok(()) = self.is_enabled() {
            if self.get_qos()?.entity_factory.autoenable_created_entities {
                datareader.enable()?;
            }
        }

        let reader_ops: Arc<dyn DataReaderInternal<Qos = DataReaderQos>> = datareader
            .self_ref
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .as_ref()
            .ok_or(DdsError::Error("DataReader is not properly initialized".to_string()))?
            .clone();
        let weak_reader: Weak<dyn DataReaderInternal<Qos = DataReaderQos>> =
            Arc::downgrade(&reader_ops);
        {
            let mut readers_by_topic_name = match self.readers_by_topic_name.lock() {
                Ok(readers_guard) => readers_guard,
                Err(e) => return Err(DdsError::Error(e.to_string())),
            };

            let mut readers_by_topic_handle = match self.readers_by_topic_handle.lock() {
                Ok(readers_guard) => readers_guard,
                Err(e) => return Err(DdsError::Error(e.to_string())),
            };

            let topic_name = effective_topic_name(topic_description)?;
            let topic_handle = topic_description.topic_instance_handle()?;

            readers_by_topic_name
                .entry(topic_name)
                .or_insert_with(Vec::new)
                .push(weak_reader.clone());
            readers_by_topic_handle
                .entry(topic_handle)
                .or_insert_with(Vec::new)
                .push(weak_reader.clone());
        }

        Ok(datareader)
    }

    /// Creates a new `DataReader` using QoS settings from a loaded profile.
    ///
    /// This is a convenience method that retrieves QoS from the profile and delegates
    /// to [`create_datareader`](Self::create_datareader).
    ///
    /// # Arguments
    ///
    /// * `topic_description` - The topic description to read data from.
    /// * `qos_path` - QoS path in the format `"Library::Profile"` or `"Library::Profile::QosName"`.
    ///   See [`QosProvider`](crate::config::json::QosProvider) for supported path formats.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Errors
    ///
    /// Returns an error if the profile is not found or datareader creation fails.
    pub fn create_datareader_with_profile<Foo: DdsType>(
        &self,
        topic_description: &dyn TopicDescription,
        qos_path: &str,
        listener: Option<Arc<dyn DataReaderListener<Foo = Foo>>>,
        mask: StatusMask,
    ) -> DdsResult<DataReader<Foo>> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        let qos = self.get_datareader_qos_from_profile(qos_path)?;
        self.create_datareader::<Foo>(topic_description, qos, listener, mask)
    }

    /// Creates a `DataReader` for `DynamicData` using a `DynamicTypeSupport`.
    ///
    /// This method is used when the data type is not known at compile time.
    /// The `DynamicTypeSupport` is typically created from a `TypeObject` received
    /// during discovery.
    ///
    /// # Arguments
    ///
    /// * `topic_description` - The topic description to read data from.
    /// * `type_support` - The `DynamicTypeSupport` describing the data type.
    /// * `qos` - QoS policies for the DataReader.
    /// * `listener` - Optional listener for status notifications.
    /// * `mask` - Status mask indicating which status changes trigger listener callbacks.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use int2dds::xtypes::{DynamicTypeSupport, DynamicData};
    ///
    /// // Create DynamicTypeSupport from a TypeObject received during discovery
    /// let type_support = DynamicTypeSupport::from_type_object(type_object)?;
    ///
    /// // Create a DataReader for DynamicData
    /// let reader = subscriber.create_datareader_dynamic(
    ///     &topic,
    ///     Arc::new(type_support),
    ///     DataReaderQos::default(),
    ///     None,
    ///     StatusMask::default(),
    /// )?;
    ///
    /// // Read data
    /// let samples = reader.take(10)?;
    /// for sample in samples {
    ///     if let Some(data) = sample.data() {
    ///         let id: i32 = data.get("id")?;
    ///         println!("Received id: {}", id);
    ///     }
    /// }
    /// ```
    pub fn create_datareader_dynamic(
        &self,
        topic_description: &dyn TopicDescription,
        type_support: Arc<crate::xtypes::DynamicTypeSupport>,
        qos: DataReaderQos,
        listener: Option<Arc<dyn DataReaderListener<Foo = crate::xtypes::DynamicData>>>,
        mask: StatusMask,
    ) -> DdsResult<DataReader<crate::xtypes::DynamicData>> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;

        self.create_datareader_impl(type_support, topic_description, qos, listener, mask)
    }

    pub(crate) fn create_builtin_datareader<Foo: DdsType>(
        &self,
        topic_description: &dyn TopicDescription,
        qos: DataReaderQos,
        rtps_reader: Arc<dyn Reader + Send + Sync>,
    ) -> DdsResult<DataReader<Foo>> {
        if !self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }

        let type_support =
            self.get_participant()?.find_typesupport(topic_description.get_type_name());
        let type_support =
            type_support.ok_or(DdsError::Error("TypeSupport not found".to_string()))?;

        let self_ref = self
            .self_ref
            .as_ref()
            .ok_or(DdsError::Error("Subscriber not initialized".to_string()))?;
        let guid = rtps_reader.guid();

        let datareader = DataReader::new(
            true,
            guid,
            type_support,
            topic_description,
            qos,
            None,
            StatusMask::all(),
            self_ref,
            Some(rtps_reader.clone()), // builtin endpoint(reader)
        )?;

        // Enable if subscriber is enabled and autoenable is set (same as create_datareader)
        if let Ok(()) = self.is_enabled() {
            if self.get_qos()?.entity_factory.autoenable_created_entities {
                datareader.enable()?;
            }
        }

        // Store builtin reader with strong reference (not in readers_by_topic_name/handle)
        let reader_ops: Arc<dyn DataReaderInternal<Qos = DataReaderQos>> = datareader
            .self_ref
            .lock()
            .map_err(|e| DdsError::Error(e.to_string()))?
            .as_ref()
            .ok_or(DdsError::Error("DataReader not initialized".to_string()))?
            .clone();

        self.builtin_readers.lock().map_err(|e| DdsError::Error(e.to_string()))?.push(reader_ops);

        Ok(datareader)
    }

    pub fn delete_datareader<Foo: 'static + Clone + Debug>(
        &self,
        datareader: DataReader<Foo>,
    ) -> DdsResult<()> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        let arc_reader: Arc<dyn DataReaderInternal<Qos = DataReaderQos>> =
            Arc::new(datareader.clone());
        match self.try_delete_datareader(&arc_reader) {
            Ok(()) => Ok(()),
            Err(e) => {
                let reader_ref: Arc<dyn DataReaderInternal<Qos = DataReaderQos>> = datareader
                    .self_ref
                    .lock()
                    .map_err(|e| DdsError::Error(e.to_string()))?
                    .as_ref()
                    .ok_or(DdsError::Error("DataReader is not properly initialized".to_string()))?
                    .clone();
                let mut orphaned =
                    self.orphaned_readers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
                if !orphaned.iter().any(|c| Arc::ptr_eq(c, &reader_ref)) {
                    orphaned.push(reader_ref);
                }
                Err(e)
            }
        }
    }

    fn try_delete_datareader(
        &self,
        datareader: &Arc<dyn DataReaderInternal<Qos = DataReaderQos>>,
    ) -> DdsResult<()> {
        self.remove_orphaned_reader(datareader);
        if datareader.get_subscriber()?.get_instance_handle()? != self.get_instance_handle()? {
            return Err(DdsError::PreconditionNotMet);
        }
        let handle = datareader.get_instance_handle()?;
        let topic_description = datareader.get_topicdescription()?;
        let topic_name = effective_topic_name(topic_description.as_ref())?;
        let topic_handle = topic_description.topic_instance_handle()?;

        {
            let participant = self.get_participant()?;
            let mut bridge_guard = participant.get_dcps_bridge()?;
            match bridge_guard.as_mut() {
                Some(bridge) => bridge
                    .delete_rtps_reader(topic_name.clone(), handle.to_guid().entity_id())
                    .map_err(|e| DdsError::Error(e.message))?,
                None => return Err(DdsError::Error("DCPS Bridge is not initialized".to_string())),
            };
        }

        let mut removed = false;

        {
            if let Ok(mut readers_by_topic_name) = self.readers_by_topic_name.lock() {
                if let Some(weak_readers) = readers_by_topic_name.get_mut(&topic_name) {
                    // Clean up dead references
                    weak_readers.retain(|weak_reader| weak_reader.upgrade().is_some());

                    // Find and remove target reader
                    let original_len = weak_readers.len();
                    weak_readers.retain(|weak_reader| {
                        weak_reader
                            .upgrade()
                            .and_then(|reader| reader.get_instance_handle().ok())
                            .is_some_and(|h| h != handle)
                    });

                    removed = weak_readers.len() < original_len;

                    if weak_readers.is_empty() {
                        readers_by_topic_name.remove(&topic_name);
                    }
                }
            } else {
                return Err(DdsError::Error("Failed to lock readers_by_topic_name".to_string()));
            }
        }

        // If found in first map, also remove from second map
        if removed {
            if let Ok(mut readers_by_topic_handle) = self.readers_by_topic_handle.lock() {
                if let Some(readers) = readers_by_topic_handle.get_mut(&topic_handle) {
                    readers.retain(|weak_reader| {
                        weak_reader
                            .upgrade()
                            .and_then(|reader| reader.get_instance_handle().ok())
                            .is_some_and(|h| h != handle)
                    });

                    if readers.is_empty() {
                        readers_by_topic_handle.remove(&topic_handle);
                    }
                }
            }
            datareader.delete();
            Ok(())
        } else {
            Err(DdsError::Error("DataReader not found".to_string()))
        }
    }

    fn remove_orphaned_reader(
        &self,
        target: &Arc<dyn DataReaderInternal<Qos = DataReaderQos>>,
    ) -> bool {
        let mut orphaned_readers = self.orphaned_readers.lock().unwrap();
        if let Some(pos) = orphaned_readers
            .iter()
            .position(|t| t.get_instance_handle().unwrap() == target.get_instance_handle().unwrap())
        {
            orphaned_readers.remove(pos);
            true
        } else {
            false
        }
    }

    pub(crate) fn handle_reader_drop(
        &self,
        topic_name: &str,
        topic_handle: &InstanceHandle,
        reader_handle: &InstanceHandle,
    ) {
        let mut found_reader: Option<Arc<dyn DataReaderInternal<Qos = DataReaderQos>>> = None;

        // First search in readers_by_topic_name
        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            if let Some(weak_readers) = readers_by_topic_name.get(topic_name) {
                for weak_reader in weak_readers.iter() {
                    if let Some(strong_reader) = weak_reader.upgrade() {
                        if let Ok(handle) = strong_reader.get_instance_handle() {
                            if handle == *reader_handle {
                                found_reader = Some(strong_reader);
                                break;
                            }
                        }
                    }
                }
            }
        }

        // If not found, search in readers_by_topic_handle
        if found_reader.is_none() {
            if let Ok(readers_by_topic_handle) = self.readers_by_topic_handle.lock() {
                if let Some(weak_readers) = readers_by_topic_handle.get(topic_handle) {
                    for weak_reader in weak_readers.iter() {
                        if let Some(strong_reader) = weak_reader.upgrade() {
                            if let Ok(handle) = strong_reader.get_instance_handle() {
                                if handle == *reader_handle {
                                    found_reader = Some(strong_reader);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        // If found, add to orphaned_readers
        if let Some(reader) = found_reader {
            if let Ok(mut orphaned_readers) = self.orphaned_readers.lock() {
                if !orphaned_readers.iter().any(|c| Arc::ptr_eq(c, &reader)) {
                    orphaned_readers.push(reader);
                }
            }
        }
        // If not found, it was already properly deleted via delete_datareader, so do nothing
    }

    pub fn delete_contained_entities(&self) -> DdsResult<()> {
        /*
            This operation deletes all entities that were created using create operations on the Subscriber.
            That is, it deletes all DataReader objects contained by this Subscriber.

            If any of the contained entities is in a state where it cannot be deleted, this operation returns PRECONDITION_NOT_MET error.
            When delete_contained_entities returns successfully, the application is guaranteed that the Subscriber no longer contains any DataReader objects and can delete the Subscriber.
        */
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        {
            match self.get_datareaders_internal() {
                Ok(readers) => {
                    for reader in readers {
                        reader.delete_contained_entities()?;
                        self.try_delete_datareader(&reader)?;
                    }
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }

    // TODO
    pub fn get_datareaders(
        &self,
        _sample_states: &[SampleStateKind],
        _view_states: &[ViewStateKind],
        _instance_states: &[InstanceStateKind],
    ) -> DdsResult<Vec<Box<dyn DataReaderBase<Qos = DataReaderQos> + Send>>> {
        /*
            This operation allows the application to access DataReader objects that contain samples with specified sample_states, view_states, and instance_states.
            If the PRESENTATION QoS policy of the Subscriber to which the DataReader belongs has access_scope set to 'GROUP', this operation must only be called within a begin_access / end_access block.
            Otherwise, this operation returns error code PRECONDITION_NOT_MET.
            Depending on the settings of the PRESENTATION QoS policy (see section 2.2.3.6), the collection of returned DataReader objects can be one of the following:
            If PRESENTATION's access_scope is set to INSTANCE or TOPIC, the returned collection is a "set" where each DataReader is included at most once and the order is undefined.
            If PRESENTATION's access_scope is set to GROUP and ordered_access is set to TRUE, the returned collection is a "list".
            This difference arises because in the second case, samples belonging to different DataReader objects must be accessed in a specific order.
            In this case, the application should process each DataReader in the order they appear in the returned "list" and read or take exactly one sample from each DataReader.
            The pattern that the application should use when accessing data is described in detail in section 2.2.2.5.1 "Access to the data".
        */
        self.is_deleted()?;
        Err(DdsError::Unsupported)
    }

    pub fn get_data_readers(&self) -> DdsResult<Vec<Arc<dyn DataReaderBase<Qos = DataReaderQos>>>> {
        self.is_deleted()?;
        let mut result = Vec::new();
        let readers_by_topic_name =
            self.readers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for weak_readers in readers_by_topic_name.values() {
            for weak_reader in weak_readers.iter() {
                if let Some(reader) = weak_reader.upgrade() {
                    result.push(reader as Arc<dyn DataReaderBase<Qos = DataReaderQos>>);
                }
            }
        }
        Ok(result)
    }

    pub(crate) fn get_datareaders_internal(
        &self,
    ) -> DdsResult<Vec<Arc<dyn DataReaderInternal<Qos = DataReaderQos>>>> {
        self.is_deleted()?;
        let mut result = Vec::new();
        let readers_by_topic_name =
            self.readers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for weak_readers in readers_by_topic_name.values() {
            for weak_reader in weak_readers.iter() {
                if let Some(reader) = weak_reader.upgrade() {
                    result.push(reader as Arc<dyn DataReaderInternal<Qos = DataReaderQos>>);
                }
            }
        }
        Ok(result)
    }

    pub fn lookup_datareader<Foo: 'static + Clone + Debug>(
        &self,
        topic_name: &str,
    ) -> DdsResult<DataReader<Foo>> {
        self.is_deleted()?;

        // For builtin subscriber, search in builtin_readers
        if self.is_builtin {
            let builtin_readers =
                self.builtin_readers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            for reader in builtin_readers.iter() {
                if let Ok(topic) = reader.get_topic() {
                    if topic.get_name() == topic_name {
                        if let Some(typed_reader) =
                            reader.as_any().downcast_ref::<DataReader<Foo>>()
                        {
                            return Ok(typed_reader.clone());
                        }
                    }
                }
            }
            return Err(DdsError::Error("DataReader not found.".to_string()));
        }

        let _ = self.cleanup_dead_readers();
        {
            let readers_by_topic_name =
                self.readers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            if let Some(weak_readers) = readers_by_topic_name.get(topic_name) {
                for weak_reader in weak_readers {
                    if let Some(reader) = weak_reader.upgrade() {
                        if let Some(typed_reader) =
                            reader.as_any().downcast_ref::<DataReader<Foo>>()
                        {
                            return Ok(typed_reader.clone());
                        }
                    }
                }
            }
        }
        Err(DdsError::Error("DataReader not found.".to_string()))
    }

    pub fn get_datareaders_of_type<Foo: 'static>(&self) -> DdsResult<Vec<Arc<DataReader<Foo>>>> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();
        let mut result = Vec::new();

        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            for weak_readers in readers_by_topic_name.values() {
                for weak_reader in weak_readers.iter() {
                    if let Some(reader) = weak_reader.upgrade() {
                        if reader.get_type_id() == target_type_id {
                            // Convert from DataReaderBase to Arc<DataReader<Foo>>
                            if let Some(typed_reader) =
                                reader.as_any().downcast_ref::<DataReader<Foo>>()
                            {
                                // Get Arc through self_ref
                                if let Ok(guard) = &typed_reader.self_ref.lock() {
                                    if let Some(arc_reader) = guard.as_ref() {
                                        result.push(arc_reader.clone());
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

    // Function to find a specific type DataReader for a specific topic
    pub fn get_datareader<Foo: 'static>(
        &self,
        topic_name: &str,
    ) -> DdsResult<Option<Arc<DataReader<Foo>>>> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            if let Some(weak_readers) = readers_by_topic_name.get(topic_name) {
                for weak_reader in weak_readers.iter() {
                    if let Some(reader) = weak_reader.upgrade() {
                        if reader.get_type_id() == target_type_id {
                            if let Some(typed_reader) =
                                reader.as_any().downcast_ref::<DataReader<Foo>>()
                            {
                                if let Ok(guard) = &typed_reader.self_ref.lock() {
                                    if let Some(arc_reader) = guard.as_ref() {
                                        return Ok(Some(arc_reader.clone()));
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
    pub fn with_datareader<Foo: 'static, R>(
        &self,
        topic_name: &str,
        f: impl FnOnce(&DataReader<Foo>) -> R,
    ) -> DdsResult<Option<R>> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            if let Some(weak_readers) = readers_by_topic_name.get(topic_name) {
                for weak_reader in weak_readers.iter() {
                    if let Some(reader) = weak_reader.upgrade() {
                        if reader.get_type_id() == target_type_id {
                            if let Some(typed_reader) =
                                reader.as_any().downcast_ref::<DataReader<Foo>>()
                            {
                                return Ok(Some(f(typed_reader)));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    // Execute closure for all DataReaders of a specific type
    pub fn for_each_datareader_of_type<Foo: 'static>(
        &self,
        mut f: impl FnMut(&DataReader<Foo>),
    ) -> DdsResult<()> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            for (_topic_name, weak_readers) in readers_by_topic_name.iter() {
                for weak_reader in weak_readers.iter() {
                    if let Some(reader) = weak_reader.upgrade() {
                        if reader.get_type_id() == target_type_id {
                            if let Some(typed_reader) =
                                reader.as_any().downcast_ref::<DataReader<Foo>>()
                            {
                                f(typed_reader);
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    // Return DataReaders grouped by type
    #[allow(clippy::type_complexity)]
    pub fn get_readers_by_type(
        &self,
    ) -> DdsResult<HashMap<TypeId, Vec<Box<dyn DataReaderBase<Qos = DataReaderQos>>>>> {
        self.is_deleted()?;
        let mut type_map: HashMap<TypeId, Vec<Box<dyn DataReaderBase<Qos = DataReaderQos>>>> =
            HashMap::new();

        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            for (_topic_name, weak_readers) in readers_by_topic_name.iter() {
                for weak_reader in weak_readers.iter() {
                    if let Some(reader) = weak_reader.upgrade() {
                        let type_id = reader.get_type_id();
                        let type_readers = type_map.entry(type_id).or_default();
                        type_readers.push(reader.clone_boxed());
                    }
                }
            }
        }

        Ok(type_map)
    }

    // Function to notify only for a specific type
    pub fn notify_datareaders_of_type<Foo: 'static>(&self) -> DdsResult<()> {
        self.is_deleted()?;
        let target_type_id = TypeId::of::<Foo>();

        if let Ok(readers_by_topic_name) = self.readers_by_topic_name.lock() {
            for (_topic_name, weak_readers) in readers_by_topic_name.iter() {
                for weak_reader in weak_readers.iter() {
                    if let Some(reader) = weak_reader.upgrade() {
                        if reader.get_type_id() == target_type_id {
                            reader.notify_data_available();
                        }
                    }
                }
            }
        }

        Ok(())
    }

    // TODO
    pub fn begin_access(&self) -> DdsResult<()> {
        /*
            This operation notifies the Service that the application is about to access data samples in one or more DataReader objects attached to the Subscriber.
            The application should only use this operation if the PRESENTATION QoS policy of the Subscriber to which the DataReader belongs has access_scope set to 'GROUP'.
            In such cases, begin_access must be called before invoking sample access operations such as get_datareaders(Subscriber) or read, take, read_w_condition, take_w_condition(DataReader).
            Otherwise, those sample access operations will return PRECONDITION_NOT_MET error.
            After the application completes accessing data samples, it must call end_access.
            If the PRESENTATION QoS policy has access_scope set to a value other than 'GROUP', there is no need to call begin_access/end_access.
            In this case, the calls are not considered errors and have no effect.
            begin_access and end_access calls can be nested. In this case, the application must call end_access as many times as it called begin_access.
            Additional error code that may be returned besides standard errors: PRECONDITION_NOT_MET.
        */
        self.is_deleted()?;
        if self.get_qos()?.presentation.access_scope != PresentationQosAccessScopeKind::Group {
            return Ok(());
        }
        Err(DdsError::Unsupported)
    }

    // TODO
    pub fn end_access(&self) -> DdsResult<()> {
        /*
            This operation indicates that the application has completed accessing data samples from DataReader objects managed by the Subscriber.
            This operation is used to "close" a previously called begin_access.
            After calling end_access, the application should no longer access any Data or SampleInfo elements returned by sample access operations.
            The end_access call must match a previous begin_access call, otherwise this operation returns PRECONDITION_NOT_MET error.
            Additional error code that may be returned besides standard errors: PRECONDITION_NOT_MET.
        */
        self.is_deleted()?;
        if self.get_qos()?.presentation.access_scope != PresentationQosAccessScopeKind::Group {
            return Ok(());
        }
        Err(DdsError::Unsupported)
    }

    pub fn notify_datareaders(&self) -> DdsResult<()> {
        self.is_deleted()?;
        let readers_by_topic_name = match self.readers_by_topic_name.lock() {
            Ok(readers_guard) => readers_guard,
            Err(e) => return Err(DdsError::Error(e.to_string())),
        };

        for readers in readers_by_topic_name.values() {
            for weak_reader in readers.iter() {
                if let Some(reader) = weak_reader.upgrade() {
                    reader.notify_data_available();
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
                return Ok((*participant_arc).clone());
            }
        }

        // Case when participant is None or reference has expired
        Err(DdsError::Error("Participant reference is invalid or expired".to_string()))
    }

    pub fn set_default_datareader_qos(
        &self,
        qos: impl Into<QosKind<DataReaderQos>>,
    ) -> DdsResult<()> {
        if self.is_builtin {
            return Err(DdsError::PreconditionNotMet);
        }
        self.is_deleted()?;
        match qos.into() {
            QosKind::Default => self.reset_default_datareader_qos(),
            QosKind::Specific(qos) => {
                qos.is_consistent()?;
                match self.default_datareader_qos.lock() {
                    Ok(mut default_qos) => {
                        *default_qos = Some(qos);
                        Ok(())
                    }
                    Err(e) => Err(DdsError::Error(e.to_string())),
                }
            }
        }
    }

    fn reset_default_datareader_qos(&self) -> DdsResult<()> {
        match self.default_datareader_qos.lock() {
            Ok(mut default_qos) => {
                *default_qos = None;
                Ok(())
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub fn get_default_datareader_qos(&self) -> DdsResult<DataReaderQos> {
        self.is_deleted()?;
        match self.default_datareader_qos.lock() {
            Ok(default_datareader_qos) => Ok(default_datareader_qos.clone().unwrap_or_default()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    /// Retrieves `DataReaderQos` from a loaded profile.
    ///
    /// # Arguments
    ///
    /// * `qos_path` - QoS path. See [`QosProvider`](crate::config::json::QosProvider) for supported formats.
    ///
    /// # Errors
    ///
    /// Returns an error if the subscriber is deleted or the profile is not found.
    pub fn get_datareader_qos_from_profile(&self, qos_path: &str) -> DdsResult<DataReaderQos> {
        self.is_deleted()?;
        DomainParticipantFactory::get_instance().get_datareader_qos_from_profile(qos_path)
    }

    pub fn copy_from_topic_qos(
        &self,
        mut datareader_qos: DataReaderQos,
        topic_qos: TopicQos,
    ) -> DdsResult<DataReaderQos> {
        self.is_deleted()?;
        datareader_qos.durability = topic_qos.durability;
        datareader_qos.deadline = topic_qos.deadline;
        datareader_qos.latency_budget = topic_qos.latency_budget;
        datareader_qos.liveliness = topic_qos.liveliness;
        datareader_qos.reliability = topic_qos.reliability;
        datareader_qos.destination_order = topic_qos.destination_order;
        datareader_qos.history = topic_qos.history;
        datareader_qos.resource_limits = topic_qos.resource_limits;
        datareader_qos.ownership = topic_qos.ownership;
        datareader_qos.data_representation = topic_qos.data_representation.clone();
        Ok(datareader_qos)
    }

    pub(crate) fn has_active_entities(&self) -> DdsResult<bool> {
        self.is_deleted()?;
        {
            match self.readers_by_topic_name.lock() {
                Ok(readers) => {
                    // Check for non-builtin readers
                    for weak_readers in readers.values() {
                        for weak_reader in weak_readers {
                            if let Some(reader) = weak_reader.upgrade() {
                                if !reader.is_builtin() {
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
        match self.readers_by_topic_name.lock() {
            Ok(guard) => {
                for weak_readers in guard.values() {
                    for weak_reader in weak_readers {
                        if let Some(reader) = weak_reader.upgrade() {
                            if reader.get_instance_handle()? == handle {
                                return Ok(true);
                            }
                        }
                    }
                }
                Ok(false)
            }
            Err(e) => Err(DdsError::Error(e.to_string())), // Return false on lock acquisition failure
        }
    }

    // For Entity
    pub fn set_listener(
        &self,
        listener: Option<Arc<dyn SubscriberListener>>,
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
    pub fn get_listener(&self) -> DdsResult<Option<Arc<dyn SubscriberListener>>> {
        self.is_deleted()?;
        match self.listener.read() {
            Ok(guard) => Ok(guard.clone()),
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    pub(crate) fn disable(&self) -> DdsResult<()> {
        self.set_listener(None, StatusMask::default())?;

        let readers =
            self.readers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for topic_readers in readers.values() {
            for weak_reader in topic_readers {
                if let Some(reader) = weak_reader.upgrade() {
                    reader.disable()?;
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

    pub(crate) fn has_readers_for_topic(&self, topic: &dyn TopicDescription) -> DdsResult<bool> {
        match self.readers_by_topic_handle.lock() {
            Ok(readers_by_topic_handle) => {
                Ok(readers_by_topic_handle.contains_key(&topic.topic_instance_handle()?))
            }
            Err(e) => Err(DdsError::Error(e.to_string())),
        }
    }

    fn cleanup_dead_readers(&self) -> DdsResult<()> {
        let mut readers_by_name =
            self.readers_by_topic_name.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let mut readers_by_handle =
            self.readers_by_topic_handle.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        // Clean up both data structures
        for weak_readers in readers_by_name.values_mut() {
            weak_readers.retain(|weak_reader| weak_reader.upgrade().is_some());
        }
        readers_by_name.retain(|_, readers| !readers.is_empty());

        for weak_readers in readers_by_handle.values_mut() {
            weak_readers.retain(|weak_reader| weak_reader.upgrade().is_some());
        }
        readers_by_handle.retain(|_, readers| !readers.is_empty());

        Ok(())
    }

    pub(crate) fn delete(&mut self) {
        self.self_ref = None;
        self.deleted.store(true, Ordering::SeqCst);
    }

    /// Cleans up builtin entities (datareaders) to break self-reference cycles.
    /// Called during participant deletion.
    pub(crate) fn cleanup_builtin_entities(&mut self) {
        // Delete all builtin datareaders
        if let Ok(mut builtin_readers) = self.builtin_readers.lock() {
            for reader in builtin_readers.drain(..) {
                reader.delete();
            }
        }
        // Break self-reference cycle
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
}

#[cfg(test)]
mod tests {
    use int2dds_derive::DdsType;

    use super::*;
    use crate::{
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        subscription::{
            qos::{DataReaderQos, SubscriberQos},
            sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind},
        },
        topic::qos::TopicQos,
    };

    #[derive(DdsType)]
    pub struct HelloWorld {
        pub index: u32,
        pub message: String,
    }

    #[test]
    fn test_subscriber_drop_without_delete() {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();
        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let subscriber_handle = subscriber.get_instance_handle().unwrap();

        drop(subscriber);
        let contains_subscriber = domain_participant.contains_entity(subscriber_handle).unwrap();
        assert!(contains_subscriber, "Subscriber deleted!")
    }

    #[test]
    fn test_delete_datareader_removes_rtps_reader() {
        use crate::{
            core::time::Duration,
            infrastructure::{
                qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
                wait_set::WaitSet,
            },
            publication::qos::{DataWriterQos, PublisherQos},
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
                "test_delete_reader",
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
                "test_delete_reader",
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
        let mut cond = writer.get_statuscondition().unwrap().clone();
        cond.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        wait_set.attach_condition(cond.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(5)).unwrap();
        let pub_status = writer.get_publication_matched_status().unwrap();
        assert_eq!(pub_status.current_count(), 1);
        wait_set.detach_condition(cond).unwrap();

        // Wait for subscription matched
        let mut cond = reader.get_statuscondition().unwrap().clone();
        cond.set_enabled_statuses(StatusMask::SUBSCRIPTION_MATCHED).unwrap();
        wait_set.attach_condition(cond.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(5)).unwrap();
        let sub_status = reader.get_subscription_matched_status().unwrap();
        assert_eq!(sub_status.current_count(), 1);
        wait_set.detach_condition(cond).unwrap();

        // Delete reader
        subscriber.delete_datareader(reader).unwrap();

        // Wait for publication matched to drop to 0
        let mut cond = writer.get_statuscondition().unwrap().clone();
        cond.set_enabled_statuses(StatusMask::PUBLICATION_MATCHED).unwrap();
        wait_set.attach_condition(cond.clone()).unwrap();
        wait_set.wait(Duration::from_seconds(5)).unwrap();
        let pub_status = writer.get_publication_matched_status().unwrap();
        assert_eq!(
            pub_status.current_count(),
            0,
            "Writer should see 0 matched readers after delete_datareader"
        );
        wait_set.detach_condition(cond).unwrap();

        participant1.delete_contained_entities().unwrap();
        factory.delete_participant(participant1).unwrap();
        participant2.delete_contained_entities().unwrap();
        factory.delete_participant(participant2).unwrap();
    }

    #[test]
    fn test_delete_contained_entities_subscriber() {
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
        assert!(!subscriber.get_data_readers().unwrap().is_empty());

        subscriber.delete_contained_entities().unwrap();

        assert!(subscriber.get_data_readers().unwrap().is_empty());
    }
}
