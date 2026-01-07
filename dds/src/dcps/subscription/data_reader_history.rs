//! DataReader history cache implementation.
//!
//! This module implements the history cache for `DataReader`, managing the storage and
//! retrieval of received samples according to History, ResourceLimits, and Ownership QoS policies.
//!
//! The reader history cache handles:
//! - Per-instance sample storage with view/instance state tracking
//! - KEEP_LAST / KEEP_ALL history policies
//! - Resource limit enforcement (max_samples, max_instances, max_samples_per_instance)
//! - Sample state management (READ/NOT_READ, NEW/NOT_NEW, ALIVE/DISPOSED/NO_WRITERS)
//! - Ownership strength arbitration for exclusive ownership
//! - TimeBasedFilter enforcement

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fmt::Debug,
    sync::{Arc, Mutex, Weak},
};

use dashmap::DashMap;
use log::debug;

use crate::{
    common::instance_handle::InstanceHandle,
    core::{
        error::{DdsError, DdsResult},
        time::Duration,
        types::LENGTH_UNLIMITED,
    },
    infrastructure::{
        history_cache::HistoryCache,
        qos_policy::{
            HistoryQosPolicy, HistoryQosPolicyKind, OwnershipQosPolicyKind, ReliabilityQosPolicy,
            ReliabilityQosPolicyKind, ResourceLimitsQosPolicy,
        },
        status::{SampleRejectedStatus, SampleRejectedStatusKind, StatusInfo, StatusKind},
    },
    rtps::{
        common::{guid::Guid, sequence::SequenceNumber, time::RtpsTime, types::ChangeKind},
        entities::{history::cache_change::CacheChange, participant::Participant},
    },
    subscription::{data_reader::DataReader, sample_info::InstanceStateKind},
};

pub(crate) type ReaderChangeId = (Guid, SequenceNumber);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct OwnershipInfo {
    ownership_strength: i32,
    owner_guid: Guid,
}

impl PartialOrd for OwnershipInfo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OwnershipInfo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .ownership_strength
            .cmp(&self.ownership_strength) // ownership_strength in desc
            .then_with(|| self.owner_guid.cmp(&other.owner_guid)) // owner_guid in asc
    }
}

// #[derive(Debug)]
pub(crate) struct DataReaderHistoryCache<Foo> {
    data_reader: Weak<DataReader<Foo>>,
    max_samples: i32,
    max_instances: i32,
    max_samples_per_instance: i32,
    changes: Vec<Arc<CacheChange>>,
    instance_map: Arc<Mutex<HashMap<InstanceHandle, Vec<Weak<CacheChange>>>>>, // NoKey Reader shall not use this
    can_auto_remove: bool, // auto remove oldest changes when full
    ownership_kind: OwnershipQosPolicyKind,
    owner_candidates: Arc<DashMap<InstanceHandle, BTreeSet<OwnershipInfo>>>, // Track valid writers per instance (includes writers that missed deadline or unregistered, not just strictly alive ones by Liveliness QoS)
    lifespan_timers: Arc<Mutex<HashMap<Guid, String>>>, // writer_guid -> timer_id
    #[allow(clippy::type_complexity)]
    status_callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
}

impl<Foo: 'static + Clone + Debug> HistoryCache for DataReaderHistoryCache<Foo> {
    type CacheChangeInputType = Arc<Mutex<CacheChange>>;

    fn get_changes(&self) -> &Vec<Arc<CacheChange>> {
        &self.changes
    }

    fn get_changes_mut(&mut self) -> &mut Vec<Arc<CacheChange>> {
        &mut self.changes
    }

    fn get_instance_map(
        &self,
    ) -> Arc<Mutex<HashMap<InstanceHandle, Vec<std::sync::Weak<CacheChange>>>>> {
        self.instance_map.clone()
    }

    fn lifespan_timers(&self) -> Arc<Mutex<HashMap<Guid, String>>> {
        self.lifespan_timers.clone()
    }

    fn get_rtps_participant(&self) -> DdsResult<Arc<Participant>> {
        let data_reader = self
            .data_reader
            .upgrade()
            .ok_or_else(|| DdsError::Error("DataReader has been dropped".to_string()))?;
        let participant = data_reader.get_subscriber()?.get_participant()?;
        let rtps_participant = participant.get_rtps_participant()?;
        Ok(Arc::new(rtps_participant))
    }

    fn lifespan_timer(
        &self,
        writer_guid: Guid,
        lifespan_duration: Duration,
        timer_id_prefix: &str,
    ) -> DdsResult<()> {
        use log::debug;

        let data_reader_weak = if let Some(data_reader) = self.data_reader.upgrade() {
            Arc::downgrade(&data_reader)
        } else {
            return Err(DdsError::Error("DataReader has been dropped".to_string()));
        };

        let lifespan_duration_clone = lifespan_duration;
        let writer_guid_clone = writer_guid;

        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
            if let Some(data_reader) = data_reader_weak.upgrade() {
                if let Ok(cache_arc) = data_reader.get_datareader_cache() {
                    if let Ok(mut cache_guard) = cache_arc.lock() {
                        if let Err(e) =
                            cache_guard.lifespan_expired(writer_guid_clone, lifespan_duration_clone)
                        {
                            debug!(
                                "[DataReaderHistoryCache] Failed to check lifespan samples: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        });

        self.lifespan_timer_with_callback(writer_guid, lifespan_duration, timer_id_prefix, callback)
    }

    fn get_max_samples(&self) -> i32 {
        self.max_samples
    }

    fn get_max_instances(&self) -> i32 {
        self.max_instances
    }

    fn get_max_samples_per_instance(&self) -> i32 {
        self.max_samples_per_instance
    }

    /// Adds the given CacheChange to the history vector and map,
    /// and returns any CacheChange that was removed during space allocation before adding.
    fn add_change_with_cleanup(
        &mut self,
        a_change: Arc<Mutex<CacheChange>>,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        let data_reader = self
            .data_reader
            .upgrade()
            .ok_or(DdsError::Error("DataReader has been dropped".to_string()))?;

        let mut cache_change_guard = a_change.lock().map_err(|e| {
            DdsError::Error(format!(
                "Failed to acquire CacheChange lock while adding change: {}",
                e
            ))
        })?;

        // Set instance handle to the original data delivered from RTPS layer if key to be computed
        // This key computing not to be handled in RTPS layer because it does not know about DataReader's type support
        let instance_handle = data_reader.fallback_instance_handle(&cache_change_guard)?;
        cache_change_guard.set_instance_handle(instance_handle);

        self.add_to_owner_candidate_if_new(
            cache_change_guard.instance_handle(),
            cache_change_guard.ownership_strength().unwrap_or(0),
            cache_change_guard.writer_guid(),
        )?;

        cache_change_guard.set_reception_timestamp(RtpsTime::now());

        // This CacheChange is now immutable, we can safely create a new Arc
        // RTPS history and DataReader history now have separate Arc<CacheChange> instances with identical data
        // after both RTPS and DCPS layers made their respective modifications to reach the same final state.
        let immutable_change = Arc::new(cache_change_guard.clone());
        let writer_guid = immutable_change.writer_guid();

        drop(cache_change_guard);

        // Check ownership
        if immutable_change.instance_handle().is_nil() {
            if !self.is_writer_owner_of_instance(
                immutable_change.writer_guid(),
                immutable_change.instance_handle(),
            )? {
                debug!(
                    "Rejecting change, writer {:?} is not owner of instance (which is the whole non keyed topic)",
                    immutable_change.writer_guid()
                );
                return Err(DdsError::IllegalOperation);
            }
        }
        // Check ownership & update instance state
        else {
            self.update_instance_state(&immutable_change)?;
        }

        // Check lifespan qos
        let lifespan_duration =
            immutable_change.lifespan_duration().and_then(|lifespan_duration| {
                if !lifespan_duration.is_infinite() {
                    Some(lifespan_duration)
                } else {
                    None
                }
            });

        // Create timer only if it does not exist
        if let Some(duration) = lifespan_duration {
            let timers = self.lifespan_timers();
            let timers_guard = timers.lock().map_err(|e| DdsError::Error(e.to_string()))?;
            let timer_exists = timers_guard.contains_key(&writer_guid);
            drop(timers_guard);

            if !timer_exists {
                if let Err(e) = self.lifespan_timer(writer_guid, duration, "lifespan_timer_reader")
                {
                    debug!("[DataReaderHistoryCache] Failed to ensure lifespan timer: {:?}", e);
                }
            }
        }
        // end lifespan

        // Since original data's instance handle is already renewed, can drop the lock now
        let removed_change = self.ensure_capacity(immutable_change.instance_handle())?;

        self.add_change_to_instance_map(immutable_change.clone())?;
        if lifespan_duration.is_some() {
            self.insert_change_sorted(immutable_change.clone());
        } else {
            self.changes.push(immutable_change.clone());
        }

        Ok(removed_change)
    }

    /// Removes the given CacheChange from the history vector and map.
    fn remove_change(&mut self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        self.changes.retain(|c| {
            !(c.sequence_number() == a_change.sequence_number()
                && c.writer_guid() == a_change.writer_guid())
        });
        self.remove_change_from_instance_map(&a_change)?;
        Ok(())
    }

    /// Ensures capacity before adding a new CacheChange to the history.
    /// Returns Ok(None) if space is already available, or Ok(Some(CacheChange)) if space was secured after removing an existing CacheChange.
    /// Returns Err(DdsError::OutOfResources) if space cannot be secured, in which case the Sample is rejected.
    fn ensure_capacity(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Option<Arc<CacheChange>>> {
        // NO_KEY
        if instance_handle.is_nil() && self.is_max_samples_per_instance_exceeded(instance_handle)? {
            if !self.can_auto_remove {
                self.on_sample_rejected(SampleRejectedStatus {
                    total_count: 0,
                    total_count_change: 1,
                    last_reason: SampleRejectedStatusKind::RejectedBySamplesLimit,
                    last_instance_handle: instance_handle,
                });
                return Err(DdsError::OutOfResources);
            }
            let removed = self.try_remove_oldest_change_of_all()?;
            return Ok(Some(removed));
        }

        // WITH_KEY
        if !instance_handle.is_nil() {
            if self.is_max_samples_per_instance_exceeded(instance_handle)? {
                if !self.can_auto_remove {
                    self.on_sample_rejected(SampleRejectedStatus {
                        total_count: 0,
                        total_count_change: 1,
                        last_reason: SampleRejectedStatusKind::RejectedBySamplesPerInstanceLimit,
                        last_instance_handle: instance_handle,
                    });
                    return Err(DdsError::OutOfResources);
                }
                let removed = self.try_remove_oldest_change_of_instance(instance_handle)?;
                return Ok(Some(removed));
            } else if self.is_max_instances_exceeded(instance_handle)?
                && !self.remove_unused_instance()?
            {
                self.on_sample_rejected(SampleRejectedStatus {
                    total_count: 0,
                    total_count_change: 1,
                    last_reason: SampleRejectedStatusKind::RejectedByInstancesLimit,
                    last_instance_handle: instance_handle,
                });
                return Err(DdsError::OutOfResources);
            } else if self.is_max_samples_exceeded() {
                if !self.can_auto_remove {
                    self.on_sample_rejected(SampleRejectedStatus {
                        total_count: 0,
                        total_count_change: 1,
                        last_reason: SampleRejectedStatusKind::RejectedBySamplesLimit,
                        last_instance_handle: instance_handle,
                    });
                    return Err(DdsError::OutOfResources);
                }

                let removed = self.try_remove_oldest_change_of_all()?;
                return Ok(Some(removed));
            }
        }

        Ok(None)
    }

    /// Removes the oldest change from all instances.
    fn try_remove_oldest_change_of_all(&mut self) -> DdsResult<Arc<CacheChange>> {
        let oldest = self.changes.iter().min_by_key(|c| c.source_timestamp()).map(Arc::clone);

        match oldest {
            Some(change) => {
                self.remove_change(change.clone())?;
                Ok(change)
            }
            None => Err(DdsError::Error(
                "There is no change to remove, this should never happen".to_string(),
            )),
        }
    }

    /// Removes the oldest change from the specified instance.
    fn try_remove_oldest_change_of_instance(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<Arc<CacheChange>> {
        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        let oldest = instance_map.get(&instance_handle).and_then(|changes_vec| {
            changes_vec
                .iter()
                .filter_map(|weak| weak.upgrade())
                .min_by_key(|c| c.source_timestamp())
        });

        drop(instance_map);

        match oldest {
            Some(change) => {
                self.remove_change(Arc::clone(&change))?;
                Ok(change)
            }
            None => Err(DdsError::Error(
                "There is no change to remove in this instance, this should never happen"
                    .to_string(),
            )),
        }
    }
}

impl<Foo: 'static + Clone + Debug> DataReaderHistoryCache<Foo> {
    pub(crate) fn new(
        data_reader: Weak<DataReader<Foo>>,
        reliability_qos: ReliabilityQosPolicy,
        history_qos: HistoryQosPolicy,
        resource_limits_qos: ResourceLimitsQosPolicy,
        has_key: bool,
        ownership_kind: OwnershipQosPolicyKind,
    ) -> Self {
        #[inline]
        fn cap(v: i32) -> i32 {
            if v == LENGTH_UNLIMITED {
                i32::MAX
            } else {
                v
            }
        }

        // 2.2.3.18 HISTORY
        let max_samples_per_instance = match history_qos.kind {
            // For KEEP_LAST, depth determines the maximum number of data samples to keep per instance, discarding previous values.
            HistoryQosPolicyKind::KeepLast(depth) => depth,
            // For KEEP_ALL, available resources are limited by the RESOURCE_LIMITS QoS.
            HistoryQosPolicyKind::KeepAll => cap(resource_limits_qos.max_samples_per_instance),
        };

        let max_instances = if has_key { cap(resource_limits_qos.max_instances) } else { 1 };
        let max_samples = cap(resource_limits_qos.max_samples);

        let max_samples = i32::min(
            max_samples,
            max_instances.saturating_mul(max_samples_per_instance), // Prevent overflow
        );

        debug!(
            "Creating data reader's history cache with max_samples_per_instance: {:?}",
            max_samples_per_instance
        );
        debug!("Creating data reader's history cache with max_instances: {:?}", max_instances);
        debug!("Creating data reader's history cache with max_samples: {:?}", max_samples);

        Self {
            data_reader,
            max_samples,
            max_instances,
            max_samples_per_instance,
            changes: Vec::new(),
            instance_map: Arc::new(Mutex::new(HashMap::new())),
            can_auto_remove: history_qos.kind != HistoryQosPolicyKind::KeepAll
                || reliability_qos.kind == ReliabilityQosPolicyKind::BestEffort, // 2.2.3.18 - 3. If keep_all && resource limits reached, then the behavior will depend on the RELIABILITY QoS.
            owner_candidates: Arc::new(DashMap::new()),
            ownership_kind,
            lifespan_timers: Arc::new(Mutex::new(HashMap::new())),
            status_callback: Arc::new(Mutex::new(None)),
        }
    }

    /// Must be called immediately after DataReaderHistoryCache creation.
    pub(crate) fn set_datareader(&mut self, data_reader: Weak<DataReader<Foo>>) {
        self.data_reader = data_reader;
    }

    /// Returns the owner Guid of the instance corresponding to the instance handle.
    fn get_owner_of_instance(&self, instance_handle: InstanceHandle) -> Option<Guid> {
        self.owner_candidates.get(&instance_handle)?.first().map(|owner_info| owner_info.owner_guid)
    }

    fn update_instance_state(&self, cache_change: &CacheChange) -> DdsResult<()> {
        if cache_change.instance_handle().is_nil() {
            return Ok(());
        }

        let data_reader = self
            .data_reader
            .upgrade()
            .ok_or(DdsError::Error("DataReader has been dropped".to_string()))?;

        let change_kind = cache_change.kind();

        match change_kind {
            ChangeKind::Alive
            | ChangeKind::AliveFiltered
            | ChangeKind::NotAliveDisposed
            | ChangeKind::NotAliveDisposedUnregistered => {
                if !self.is_writer_owner_of_instance(
                    cache_change.writer_guid(),
                    cache_change.instance_handle(),
                )? {
                    debug!(
                        "Rejecting change, writer {:?} is not owner of instance: {:?}",
                        cache_change.writer_guid(),
                        cache_change.instance_handle()
                    );
                    return Err(DdsError::IllegalOperation);
                }

                let new_state = match change_kind {
                    ChangeKind::Alive | ChangeKind::AliveFiltered => {
                        InstanceStateKind::ALIVE_INSTANCE_STATE
                    }
                    _ => InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE,
                };

                data_reader.update_instance_state(
                    cache_change.instance_handle(),
                    new_state,
                    Some(cache_change),
                )?;
            }
            _ => {}
        }

        if matches!(
            change_kind,
            ChangeKind::NotAliveUnregistered | ChangeKind::NotAliveDisposedUnregistered
        ) {
            self.remove_writer_from_owner_candidates(cache_change.writer_guid())?;
        }

        Ok(())
    }

    /// Removes Ownership information, ownership changes as a result.
    pub(crate) fn remove_writer_from_owner_candidates(
        &self,
        remote_writer_guid: Guid,
    ) -> DdsResult<()> {
        debug!("Removing writer {:?} from owner candidates", remote_writer_guid);
        for mut entry in self.owner_candidates.iter_mut() {
            let writers = entry.value_mut();
            writers.retain(|owner_info| owner_info.owner_guid != remote_writer_guid);

            // Update instance state if no writers left
            if writers.is_empty() {
                debug!("No writers left for instance {:?}", *entry.key());
                self.data_reader
                    .upgrade()
                    .ok_or(DdsError::Error("DataReader has been dropped".to_string()))?
                    .update_instance_state(
                        *entry.key(),
                        InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE,
                        None,
                    )?;
            } else {
                debug!("Now the owner is {:?}", writers.first());
            }
        }

        Ok(())
    }

    /// Revoke the current owner of the instance.
    pub(crate) fn revoke_current_owner_from_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        if self.ownership_kind == OwnershipQosPolicyKind::Shared {
            return Ok(()); // This does not apply for Shared ownership
        }

        let current_owner = self.get_owner_of_instance(instance_handle);
        if let Some(owner_guid) = current_owner {
            self.remove_writer_from_owner_candidates(owner_guid)?;
        }

        Ok(())
    }

    /// Checks if the Writer that sent the CacheChange is the owner of the instance.
    pub(crate) fn is_writer_owner_of_instance(
        &self,
        writer_guid: Guid,
        instance_handle: InstanceHandle,
    ) -> DdsResult<bool> {
        if self.ownership_kind == OwnershipQosPolicyKind::Shared {
            return Ok(true);
        }

        let owner = self.get_owner_of_instance(instance_handle);

        if let Some(owner_guid) = owner {
            Ok(owner_guid == writer_guid)
        } else {
            Ok(false)
        }
    }

    /// Adds a new owner candidate to the owner candidate list for the specified instance.
    pub(crate) fn add_to_owner_candidate_if_new(
        &self,
        instance_handle: InstanceHandle,
        ownership_strength: i32,
        owner_guid: Guid,
    ) -> DdsResult<()> {
        let mut entry = self.owner_candidates.entry(instance_handle).or_default();
        if !entry.iter().any(|info| info.owner_guid == owner_guid) {
            debug!(
                "Adding new owner candidate {:?} with strength {} for instance {:?}",
                owner_guid, ownership_strength, instance_handle
            );
            entry.insert(OwnershipInfo { ownership_strength, owner_guid });
        }
        Ok(())
    }

    /// Removes unused instances from the instance map.
    fn remove_unused_instance(&mut self) -> DdsResult<bool> {
        let mut key_to_remove: Option<InstanceHandle> = None;
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;

        for (instance_handle, changes) in instance_map.iter() {
            if changes.is_empty() && self.check_if_no_writers(*instance_handle)? {
                key_to_remove = Some(*instance_handle);
                break;
            }
        }

        if let Some(key) = key_to_remove {
            instance_map.remove(&key);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // Checks if there is no writer writing to this instance.
    fn check_if_no_writers(&self, instance_handle: InstanceHandle) -> DdsResult<bool> {
        Ok(self.get_owner_of_instance(instance_handle).is_none())
    }

    /// Adds CacheChange to the instance map.
    fn add_change_to_instance_map(&self, a_change: Arc<CacheChange>) -> DdsResult<()> {
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        instance_map
            .entry(a_change.instance_handle())
            .or_insert_with(Vec::new)
            .push(Arc::downgrade(&a_change));
        Ok(())
    }

    /// Removes CacheChange from the instance map.
    fn remove_change_from_instance_map(&self, a_change: &Arc<CacheChange>) -> DdsResult<()> {
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(changes) = instance_map.get_mut(&a_change.instance_handle()) {
            changes.retain(|weak_change| {
                if let Some(strong_change) = weak_change.upgrade() {
                    !Arc::ptr_eq(&strong_change, a_change)
                } else {
                    false
                }
            });
        }
        Ok(())
    }

    /// Removes all samples of the specified instance.
    pub(crate) fn remove_all_changes_of_instance(
        &mut self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<()> {
        let mut instance_map =
            self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        if let Some(changes) = instance_map.get_mut(&instance_handle) {
            changes.clear();
        }

        let changes = &mut self.changes;
        changes.retain(|change| change.instance_handle() != instance_handle);

        Ok(())
    }

    /// Retrieves CacheChange using the SequenceNumber and the Guid of the Writer that sent it.
    pub(crate) fn get_change(
        &self,
        seq_num: SequenceNumber,
        writer_guid: Guid,
    ) -> Option<Arc<CacheChange>> {
        let change = self.changes.iter().find(|change| {
            change.sequence_number() == seq_num && change.writer_guid() == writer_guid
        });
        change.cloned()
    }

    /// Retrieves all change identifiers (writer GUID and sequence number) of the specified instance.
    pub(crate) fn get_change_id_set_of_instance(
        &self,
        instance_handle: InstanceHandle,
    ) -> DdsResult<HashSet<ReaderChangeId>> {
        let instance_map = self.instance_map.lock().map_err(|e| DdsError::Error(e.to_string()))?;
        let changes = if let Some(weak_changes) = instance_map.get(&instance_handle) {
            weak_changes
                .iter()
                .filter_map(|weak| weak.upgrade())
                .map(|change| (change.writer_guid(), change.sequence_number()))
                .collect::<HashSet<ReaderChangeId>>()
        } else {
            HashSet::new()
        };

        Ok(changes)
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        match self.status_callback.lock() {
            Ok(mut callback) => {
                callback.replace(f);
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }

    fn on_sample_rejected(&self, info: SampleRejectedStatus) {
        match self.status_callback.lock() {
            Ok(callback) => {
                if let Some(callback) = callback.as_ref() {
                    callback(StatusKind::SAMPLE_REJECTED, Some(Arc::new(info)));
                }
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::topic::type_support::DdsType;
    use crate::infrastructure::qos_policy::{OwnershipQosPolicy, ReliabilityQosPolicyKind};
    use crate::{
        core::time::Duration,
        domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
        infrastructure::status::StatusMask,
        rtps::common::{
            entity_id::EntityId, entity_kind::EntityKind, guid::Guid, sequence::SequenceNumber,
            types::ChangeKind,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    };

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds")]
    struct HelloWorldType {
        index: u32,
        message: String,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct ShapeType {
        #[dds(key)]
        color: String,
        x: i32,
        y: i32,
        shapesize: i32,
        additional_payload_size: Vec<u8>,
    }

    /// Helper function to create a test CacheChange
    fn create_change_with_key(seq: i64, handle: InstanceHandle) -> Arc<Mutex<CacheChange>> {
        Arc::new(Mutex::new(CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY)),
            handle,
            SequenceNumber::from_i64(seq),
            Arc::from(vec![
                0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0, 20,
                0, 0, 0, 0, 0, 0, 0,
            ]),
            Some(RtpsTime::now()),
        )))
    }

    /// Helper function to create a test CacheChange
    fn create_change_no_key(seq: i64, handle: InstanceHandle) -> Arc<Mutex<CacheChange>> {
        Arc::new(Mutex::new(CacheChange::new(
            ChangeKind::Alive,
            Guid::new([1; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY)),
            handle,
            SequenceNumber::from_i64(seq),
            Arc::from(vec![
                0, 1, 0, 0, 92, 0, 0, 0, 12, 0, 0, 0, 72, 101, 108, 108, 111, 32, 119, 111, 114,
                108, 100, 0,
            ]),
            Some(RtpsTime::now()),
        )))
    }

    fn create_with_key_datareader(data_reader_qos: DataReaderQos) -> DataReader<ShapeType> {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<ShapeType>(
                "TestTopic",
                "ShapeType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let reader = subscriber
            .create_datareader::<ShapeType>(&topic, data_reader_qos, None, StatusMask::default())
            .unwrap();

        reader
    }

    fn create_no_key_datareader(data_reader_qos: DataReaderQos) -> DataReader<HelloWorldType> {
        let domain_participant_factory = DomainParticipantFactory::get_instance();
        let domain_participant = domain_participant_factory
            .create_participant(0, DomainParticipantQos::default(), None, StatusMask::default())
            .unwrap();

        let topic = domain_participant
            .create_topic::<HelloWorldType>(
                "TestTopic",
                "HelloWorldType",
                TopicQos::default(),
                None,
                StatusMask::default(),
            )
            .unwrap();

        let subscriber = domain_participant
            .create_subscriber(SubscriberQos::default(), None, StatusMask::default())
            .unwrap();

        let reader = subscriber
            .create_datareader::<HelloWorldType>(
                &topic,
                data_reader_qos,
                None,
                StatusMask::default(),
            )
            .unwrap();

        reader
    }

    mod history_qos {
        use super::*;

        #[test]
        fn test_history_cache_creation() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(10) },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 50,
                    max_instances: 5,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 50);
            assert_eq!(datareader_cache.get_max_instances(), 5);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 10);
        }

        #[test]
        fn test_history_cache_keep_last() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(5) },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 20,
                    max_instances: 4,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 20);
            assert_eq!(datareader_cache.get_max_instances(), 4);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 5);
        }

        #[test]
        fn test_history_cache_keep_last_with_unlimited_resource_limits() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(5) },
                ..Default::default()
            };
            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 2147483647);
            assert_eq!(datareader_cache.get_max_instances(), 2147483647);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 5);
        }

        #[test]
        fn test_history_cache_keep_all() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 20,
                    max_instances: 4,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 20);
            assert_eq!(datareader_cache.get_max_instances(), 4);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 10);
        }
    }

    mod resource_limits_qos {
        use super::*;

        #[test]
        fn test_history_cache_no_key_instance_limits() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 20,
                    max_instances: 4,
                    max_samples_per_instance: 10,
                },
                ..Default::default()
            };
            let data_reader = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let datareader_cache = datareader_cache.lock().unwrap();

            assert_eq!(datareader_cache.get_max_samples(), 10);
            assert_eq!(datareader_cache.get_max_instances(), 1);
            assert_eq!(datareader_cache.get_max_samples_per_instance(), 10);
        }

        #[test]
        fn test_history_cache_add_change_success_no_key_max_samples_exceeded() {
            // can_auto_remove = true
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(3) },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3,
                    max_instances: 1,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            }; // now max_samples is 3

            let data_reader = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // Add 3 changes for instance 1
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(1, InstanceHandle::NIL))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(2, InstanceHandle::NIL))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(3, InstanceHandle::NIL))
                .is_ok());

            // Add 1 change for instance 2
            let result = datareader_cache
                .add_change_with_cleanup(create_change_no_key(4, InstanceHandle::NIL));
            assert!(result.unwrap().unwrap().sequence_number().to_i64() == 1);

            let changes = datareader_cache.get_changes();
            assert!(changes.len() == 3);
            assert!(changes.get(0).unwrap().sequence_number().to_i64() == 2);
            assert!(changes.get(1).unwrap().sequence_number().to_i64() == 3);
            assert!(changes.get(2).unwrap().sequence_number().to_i64() == 4);
        }

        #[test]
        fn test_history_cache_add_change_fail_no_key_max_samples_exceeded() {
            // KeepAll + Reliable = can_auto_remove = false
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3,
                    max_instances: 1,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            }; // now max_samples is 3

            let data_reader = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // Add 3 changes for instance 1
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(1, InstanceHandle::NIL))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(2, InstanceHandle::NIL))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_no_key(3, InstanceHandle::NIL))
                .is_ok());

            // Add 1 change for instance 2
            let result = datareader_cache
                .add_change_with_cleanup(create_change_no_key(4, InstanceHandle::NIL));
            assert!(matches!(result, Err(DdsError::OutOfResources)));
        }

        #[test]
        fn test_history_cache_add_change_success_with_key_max_samples_per_instance_exceeded() {
            // KeepLast + Reliable = can_auto_remove = true
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(2) },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 10,
                    max_instances: 10,
                    max_samples_per_instance: 2,
                },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            assert!(datareader_cache.can_auto_remove);

            let instance_a = InstanceHandle::new([1; 16]);

            // Add 2 to instance A (max_samples_per_instance reached)
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a))
                .is_ok());

            // 3rd addition should succeed with auto_remove
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_a))
                .is_ok());

            let changes = datareader_cache.get_changes();
            assert_eq!(changes.len(), 2);
            // The oldest change (seq=1) should be removed, leaving only seq=2,3
            assert_eq!(changes.get(0).unwrap().sequence_number().to_i64(), 2);
            assert_eq!(changes.get(1).unwrap().sequence_number().to_i64(), 3);
        }

        #[test]
        fn test_history_cache_add_change_fail_with_key_max_samples_per_instance_exceeded() {
            // KeepAll + Reliable = can_auto_remove = false
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 10,
                    max_instances: 10,
                    max_samples_per_instance: 2,
                },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);

            // Add 2 to instance A
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a))
                .is_ok());

            // 3rd addition should fail because auto_remove is disabled
            let result =
                datareader_cache.add_change_with_cleanup(create_change_with_key(3, instance_a));
            assert!(matches!(result, Err(DdsError::OutOfResources)));
        }

        #[test]
        fn test_history_cache_add_change_success_with_key_max_instances_exceeded() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(3) },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 10,
                    max_instances: 2, // Maximum 2 instances
                    max_samples_per_instance: 5,
                },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);
            let instance_c = InstanceHandle::new([3; 16]);

            let writer_1 = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            // Add instance A, B (max_instances reached)
            let change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_a, // different instance
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                Some(RtpsTime::now()),
            );

            let change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_b, // different instance
                SequenceNumber::from_i64(2),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                Some(RtpsTime::now()),
            );

            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .is_ok());

            let change_c = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_c, // different instance
                SequenceNumber::from_i64(3),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                Some(RtpsTime::now()),
            );

            // Currently adding instance C should result in an error
            let result =
                datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_c.clone())));
            assert!(matches!(result, Err(DdsError::OutOfResources)));

            // Make instance A enter NOT_ALIVE_NO_WRITERS state
            let change_a_unregister = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_1.clone(),
                instance_a,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                Some(RtpsTime::now()),
            );

            // Writer A unregisters instance A
            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a_unregister.clone())))
                .is_ok());

            // Data reader took all the samples from instance A
            assert!(datareader_cache.remove_change(Arc::new(change_a)).is_ok());
            assert!(datareader_cache.remove_change(Arc::new(change_a_unregister)).is_ok());

            // Now adding instance C should succeed via remove_unused_instance
            assert!(datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_c)))
                .is_ok());

            let changes = datareader_cache.get_changes();
            assert_eq!(changes.len(), 2);
        }

        #[test]
        fn test_history_cache_add_change_success_with_key_max_samples_exceeded() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepLast(3) },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3, // Maximum 3 samples total
                    max_instances: 2,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            // Add 3 samples total (max_samples reached)
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_b))
                .is_ok());

            // 4th addition should succeed via try_remove_oldest_change_of_all
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(4, instance_b))
                .is_ok());

            let changes = datareader_cache.get_changes();
            assert_eq!(changes.len(), 3);
            // The oldest change (seq=1) should be removed, leaving only seq=2,3,4
            assert_eq!(changes.get(0).unwrap().sequence_number().to_i64(), 2);
            assert_eq!(changes.get(1).unwrap().sequence_number().to_i64(), 3);
            assert_eq!(changes.get(2).unwrap().sequence_number().to_i64(), 4);
        }

        #[test]
        fn test_history_cache_add_change_fail_with_key_max_samples_exceeded() {
            let reader_qos = DataReaderQos {
                history: HistoryQosPolicy { kind: HistoryQosPolicyKind::KeepAll },
                reliability: ReliabilityQosPolicy {
                    kind: ReliabilityQosPolicyKind::Reliable,
                    max_blocking_time: Duration::from_millis(100),
                },
                resource_limits: ResourceLimitsQosPolicy {
                    max_samples: 3, // Maximum 3 samples total
                    max_instances: 2,
                    max_samples_per_instance: 3,
                },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            // Add 3 samples total
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(1, instance_a))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(2, instance_a))
                .is_ok());
            assert!(datareader_cache
                .add_change_with_cleanup(create_change_with_key(3, instance_b))
                .is_ok());

            // 4th addition should fail because auto_remove is disabled
            let result =
                datareader_cache.add_change_with_cleanup(create_change_with_key(4, instance_b));
            assert!(matches!(result, Err(DdsError::OutOfResources)));
        }
    }

    mod ownership_qos {
        use super::*;

        #[test]
        fn test_history_cache_owner_set_upon_add_change_with_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_a = InstanceHandle::new([1; 16]);
            let instance_b = InstanceHandle::new([2; 16]);

            // This writer will publish both instances
            let writer_1 = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_a, // different instance
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_1.clone(),
                instance_b, // different instance
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change_a.set_ownership_strength(Some(0));
            change_b.set_ownership_strength(Some(0));

            // Add owner candidates
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // Check ownership
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();

            assert!(is_owner_a);
            assert!(is_owner_b);
        }

        #[test]
        fn test_history_cache_owner_set_upon_add_change_no_key() {}

        #[test]
        fn test_history_cache_everyone_is_owner_when_shared() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Shared },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_handle = InstanceHandle::new([1; 16]);

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change_a.set_ownership_strength(Some(0));
            change_b.set_ownership_strength(Some(0));

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // Check ownership
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();

            assert!(is_owner_a);
            assert!(is_owner_b);
        }

        #[test]
        fn test_history_cache_owner_change_when_stronger_candidate_added_with_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let instance_handle = InstanceHandle::new([1; 16]);

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                Some(RtpsTime::now()),
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                Some(RtpsTime::now()),
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // A is no longer the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
        }

        #[test]
        fn test_history_cache_owner_change_when_stronger_candidate_added_no_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let instance_handle = InstanceHandle::NIL;

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![1, 2, 3, 4]),
                Some(RtpsTime::now()),
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![1, 2, 3, 4]),
                Some(RtpsTime::now()),
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // A is no longer the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
        }

        #[test]
        fn test_history_cache_weaker_candidate_fail_to_add_change_with_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let instance_handle = InstanceHandle::NIL;

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![1, 2, 3]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![1, 2, 3]),
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A cannot add change again
            let mut change_a_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );
            change_a_2.set_ownership_strength(Some(20));

            let res = datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_a_2)));
            assert!(matches!(res, Err(DdsError::IllegalOperation)));
        }

        #[test]
        fn test_history_cache_weaker_candidate_fail_to_add_change_no_key() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_no_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let instance_handle = InstanceHandle::NIL;

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                InstanceHandle::NIL,
                SequenceNumber::from_i64(1),
                Arc::from(vec![1, 2, 3]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(), // different writer
                InstanceHandle::NIL,
                SequenceNumber::from_i64(1),
                Arc::from(vec![1, 2, 3]),
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength

            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // Now b is the owner
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A cannot add change again
            let mut change_a_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(), // different writer
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );
            change_a_2.set_ownership_strength(Some(20));

            let res = datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_a_2)));
            assert!(matches!(res, Err(DdsError::IllegalOperation)));
        }

        #[test]
        fn test_history_cache_disposed_when_owner() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change.clone()))).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Owner disposes the instance
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![]),
                None,
            );

            change_disposed.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_disposed.clone())))
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);
        }

        #[test]
        fn test_history_cache_not_disposed_when_not_owner() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // B is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A cannot add change again
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![]),
                None,
            );
            change_disposed.set_ownership_strength(Some(20));

            // Non owner cannot dispose the instance
            let res =
                datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_disposed)));
            assert!(matches!(res, Err(DdsError::IllegalOperation)));

            // Verify instance state is still ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);
        }

        #[test]
        fn test_history_cache_alive_again_when_owner_writes() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change.clone()))).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Owner disposes the instance
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );

            change_disposed.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_disposed.clone())))
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            let mut change_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(3),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change_2.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_2.clone())))
                .unwrap();

            // Verify instance state is ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);
        }

        #[test]
        fn test_history_cache_still_not_alive_when_non_owner_writes() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change.clone()))).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Owner disposes the instance
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );

            change_disposed.set_ownership_strength(Some(20));
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_disposed.clone())))
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            // This is the weaker writer
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change_2.set_ownership_strength(Some(10));
            let res =
                datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_2.clone())));
            assert!(res.is_err());

            // Verify instance state is still NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);
        }

        #[test]
        fn test_history_cache_anyone_can_dispose_and_reclaim_when_shared() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Shared },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            // NO KEY
            let writer_guid = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change = CacheChange::new(
                ChangeKind::Alive,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change.set_ownership_strength(Some(20));
            datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change.clone()))).unwrap();

            let is_owner = datareader_cache
                .is_writer_owner_of_instance(change.writer_guid(), change.instance_handle())
                .unwrap();
            assert!(is_owner);

            // Disposes the instance with lesser strength
            let mut change_disposed = CacheChange::new(
                ChangeKind::NotAliveDisposed,
                writer_guid.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );

            change_disposed.set_ownership_strength(Some(10));
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_disposed.clone())))
                .unwrap();

            // Verify instance state is NOT_ALIVE_DISPOSED
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_DISPOSED_INSTANCE_STATE);

            // This is the weaker writer
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let mut change_2 = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            change_2.set_ownership_strength(Some(5));
            let res =
                datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_2.clone())));
            assert!(res.is_ok());

            // Verify instance state is ALIVE now
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);
        }

        #[test]
        fn test_history_cache_ownership_changes_when_owner_unregister() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // B is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer B now unregister
            let mut change_unregistered = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );
            change_unregistered.set_ownership_strength(Some(30));

            let res =
                datareader_cache.add_change_with_cleanup(Arc::new(Mutex::new(change_unregistered)));
            assert!(res.is_ok());

            // Now writer A is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(is_owner_a);

            // Verify instance state is still ALIVE
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::ALIVE_INSTANCE_STATE);
        }

        #[test]
        fn test_history_cache_no_writers_when_everyone_unregister() {
            let reader_qos = DataReaderQos {
                ownership: OwnershipQosPolicy { kind: OwnershipQosPolicyKind::Exclusive },
                ..Default::default()
            };

            let data_reader = create_with_key_datareader(reader_qos);
            let datareader_cache = data_reader.get_datareader_cache().unwrap();
            let mut datareader_cache = datareader_cache.lock().unwrap();

            let writer_a = Guid::new(
                [1; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );
            let writer_b = Guid::new(
                [2; 12],
                EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_WITH_KEY),
            );

            let instance_handle = InstanceHandle::new([1; 16]);

            let mut change_a = CacheChange::new(
                ChangeKind::Alive,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            let mut change_b = CacheChange::new(
                ChangeKind::Alive,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(1),
                Arc::from(vec![
                    0, 1, 0, 0, 5, 0, 0, 0, 66, 76, 85, 69, 0, 0, 0, 0, 160, 0, 0, 0, 3, 0, 0, 0,
                    20, 0, 0, 0, 0, 0, 0, 0,
                ]),
                None,
            );

            // Before writer b writes, writer a is the owner
            change_a.set_ownership_strength(Some(20)); // Weaker strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_a.clone())))
                .unwrap();

            change_b.set_ownership_strength(Some(30)); // Stronger strength
            datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_b.clone())))
                .unwrap();

            // B is the owner
            let is_owner_a = datareader_cache
                .is_writer_owner_of_instance(change_a.writer_guid(), change_a.instance_handle())
                .unwrap();
            assert!(!is_owner_a);
            let is_owner_b = datareader_cache
                .is_writer_owner_of_instance(change_b.writer_guid(), change_b.instance_handle())
                .unwrap();
            assert!(is_owner_b);

            // Writer A, B all unregister
            let mut change_unregistered_a = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_a.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );

            let mut change_unregistered_b = CacheChange::new(
                ChangeKind::NotAliveUnregistered,
                writer_b.clone(),
                instance_handle,
                SequenceNumber::from_i64(2),
                Arc::from(vec![]),
                None,
            );
            change_unregistered_a.set_ownership_strength(Some(20));
            change_unregistered_b.set_ownership_strength(Some(30));

            let res = datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_unregistered_a.clone())));
            let res_b = datareader_cache
                .add_change_with_cleanup(Arc::new(Mutex::new(change_unregistered_b.clone())));

            assert!(res.is_ok());
            assert!(res_b.is_ok());

            // Now take all samples
            datareader_cache.remove_change(Arc::new(change_a)).unwrap();
            datareader_cache.remove_change(Arc::new(change_b)).unwrap();
            datareader_cache.remove_change(Arc::new(change_unregistered_a)).unwrap();
            datareader_cache.remove_change(Arc::new(change_unregistered_b)).unwrap();

            // Verify instance state is NOT_ALIVE_NO_WRITERS
            let instance_info = data_reader.get_instance_infos().unwrap();
            let info = instance_info.get(&instance_handle).unwrap();
            assert_eq!(info.instance_state, InstanceStateKind::NOT_ALIVE_NO_WRITERS_INSTANCE_STATE);
        }
    }
}
