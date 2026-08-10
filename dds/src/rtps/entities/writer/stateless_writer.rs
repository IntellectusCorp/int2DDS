#![allow(dead_code)]
#![allow(unused_variables)]

use crate::utils::notify::{callback_handle, notify_user};
use std::{
    fmt::Debug,
    sync::{Arc, Mutex, Weak},
};

use log::{debug, error};

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    core::time::Duration as DcpsDuration,
    infrastructure::{
        qos_policy::{LivelinessQosPolicy, QosPolicyId, ReliabilityQosPolicyKind},
        status::{
            OfferedIncompatibleQosStatus, OfferedIncompatibleTypeStatus, PublicationMatchedStatus,
            QosPolicyCount, StatusInfo, StatusKind,
        },
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::{RtpsDuration, RtpsTime},
            types::{ChangeKind, TopicKind},
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{cache_change::CacheChange, writer_history::WriterHistoryCache},
        },
    },
};

use crate::rtps::entities::participant::Participant;

use super::{reader_locator::ReaderLocator, Writer};

#[allow(dead_code)]
pub(crate) struct StatelessWriter {
    guid: Guid,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    reliability_level: ReliabilityQosPolicyKind,
    topic_kind: TopicKind,
    endpoint_id: EntityId,
    push_mode: bool,
    nack_suppression_duration: RtpsDuration,
    nack_response_delay: RtpsDuration,
    last_change_sequence_number: Arc<Mutex<SequenceNumber>>,
    heartbeat_period: RtpsDuration,
    data_max_size_serialized: i32,
    reader_locators: Arc<Mutex<Vec<ReaderLocator>>>,
    writer_cache: Arc<Mutex<WriterHistoryCache>>, // Subject to change to Rc, Mutex, etc. in the future
    #[allow(clippy::type_complexity)]
    callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
    publication_builtin_topic_data: Arc<Mutex<PublicationBuiltinTopicData>>,
    publication_matched_status: Arc<Mutex<PublicationMatchedStatus>>,
    offered_incompatible_qos_status: Arc<Mutex<OfferedIncompatibleQosStatus>>,
    offered_incompatible_type_status: Arc<Mutex<OfferedIncompatibleTypeStatus>>,
}

impl StatelessWriter {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn new(
        guid: Guid,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        reliability_level: ReliabilityQosPolicyKind,
        topic_kind: TopicKind,
        endpoint_id: EntityId,
        push_mode: bool,
        heartbeat_period: RtpsDuration,
        data_max_size_serialized: i32,
        callback: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
        participant: Weak<Participant>,
    ) -> Self {
        // in:attribute_values
        // 8.4.7.1.1 & 8.4.7.1.2 & 8.4.7.2.1

        let nack_response_delay = RtpsDuration::new(0, 200 * 1000 * 1000); // 200 milliseconds
        let nack_suppression_duration = RtpsDuration::new(0, 0);

        Self {
            guid,
            unicast_locator_list,
            multicast_locator_list,
            reliability_level,
            topic_kind,
            endpoint_id,
            push_mode,
            nack_response_delay,
            nack_suppression_duration,
            last_change_sequence_number: Arc::new(Mutex::new(SequenceNumber::new(0, 0))), // Assuming SequenceNumber has a new method
            heartbeat_period,
            data_max_size_serialized,
            reader_locators: Arc::new(Mutex::new(Vec::new())),
            writer_cache: Arc::new(Mutex::new(WriterHistoryCache::new(participant, endpoint_id))),
            callback: Arc::new(Mutex::new(callback)),
            publication_builtin_topic_data: Arc::new(Mutex::new(publication_builtin_topic_data)),
            publication_matched_status: Arc::new(Mutex::new(PublicationMatchedStatus::default())),
            offered_incompatible_qos_status: Arc::new(Mutex::new(
                OfferedIncompatibleQosStatus::default(),
            )),
            offered_incompatible_type_status: Arc::new(Mutex::new(
                OfferedIncompatibleTypeStatus::default(),
            )),
        }
    }
    pub(crate) fn reader_locator_add(&self, a_locator: ReaderLocator) {
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                reader_locators.push(a_locator);
            }
            Err(e) => {
                error!("Failed to lock reader_locators: {}", e);
            }
        }
    }
    pub(crate) fn reader_locator_remove(&self, a_locator: ReaderLocator) {
        // void
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                reader_locators.retain(|x| x != &a_locator);
            }
            Err(e) => {
                error!("Failed to lock reader_locators: {}", e);
            }
        }
    }
    pub(crate) fn unsent_changes_reset(&self) {
        // void
        match self.reader_locators.lock() {
            Ok(mut reader_locators) => {
                for reader_locator in reader_locators.iter_mut() {
                    reader_locator.set_highest_sent_change_sn(SequenceNumber::new(0, 0));
                }
            }
            Err(e) => {
                error!("Failed to lock reader_locators: {}", e);
            }
        }
    }
    pub(crate) fn reader_locator(&self) -> Arc<Mutex<Vec<ReaderLocator>>> {
        self.reader_locators.clone()
    }

    pub(crate) fn publication_builtin_topic_data(&self) -> RtpsResult<PublicationBuiltinTopicData> {
        Ok(self
            .publication_builtin_topic_data
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?
            .clone())
    }

    pub(crate) fn update_publication_matched_status(
        &self,
        number: i32,
        last_subscription_handle: InstanceHandle,
    ) {
        match self.publication_matched_status.lock() {
            Ok(mut publication_matched_status) => {
                debug!(
                    "Current count in publication matched status before change: {:?}",
                    publication_matched_status.current_count
                );
                if number > 0 {
                    publication_matched_status.total_count += number;
                    publication_matched_status.total_count_change += number;
                }
                publication_matched_status.last_subscription_handle = last_subscription_handle;
                publication_matched_status.current_count += number;
                publication_matched_status.current_count_change = number;
                debug!(
                    "Current count in publication matched status after change: {:?}",
                    publication_matched_status.current_count
                );
                self.update_status(
                    StatusKind::PUBLICATION_MATCHED,
                    Some(Arc::new(*publication_matched_status)),
                );
                publication_matched_status.total_count_change = 0;
                publication_matched_status.current_count_change = 0;
            }
            Err(e) => {
                log::error!("Failed to lock publication_matched_status: {:?}", e);
            }
        }
    }

    pub(crate) fn update_offered_incompatible_qos_status(&self, policy_id: QosPolicyId) {
        match self.offered_incompatible_qos_status.lock() {
            Ok(mut offered_incompatible_qos_status) => {
                offered_incompatible_qos_status.total_count += 1;
                offered_incompatible_qos_status.total_count_change += 1;
                offered_incompatible_qos_status.last_policy_id = policy_id;

                let mut not_found = true;
                for policy in offered_incompatible_qos_status.policies.iter_mut() {
                    if policy.policy_id == policy_id {
                        policy.count += 1;
                        not_found = false;
                        break;
                    }
                }
                if not_found {
                    offered_incompatible_qos_status
                        .policies
                        .push(QosPolicyCount { policy_id, count: 1 });
                }

                self.update_status(
                    StatusKind::OFFERED_INCOMPATIBLE_QOS,
                    Some(Arc::new(offered_incompatible_qos_status.clone())),
                );

                offered_incompatible_qos_status.total_count_change = 0;
            }
            Err(e) => {
                log::error!("Failed to lock offered_incompatible_qos_status: {:?}", e);
            }
        }
    }

    pub(crate) fn update_offered_incompatible_type_status(&self) {
        match self.offered_incompatible_type_status.lock() {
            Ok(mut offered_incompatible_type_status) => {
                offered_incompatible_type_status.total_count += 1;
                offered_incompatible_type_status.total_count_change += 1;

                self.update_status(
                    StatusKind::OFFERED_INCOMPATIBLE_TYPE,
                    Some(Arc::new(offered_incompatible_type_status.clone())),
                );

                offered_incompatible_type_status.total_count_change = 0;
            }
            Err(e) => {
                log::error!("Failed to lock offered_incompatible_type_status: {:?}", e);
            }
        }
    }

    pub(crate) fn matched_reader_lookup(&self, a_reader_guid: Guid) -> Option<ReaderLocator> {
        match self.reader_locators.lock() {
            Ok(matched_readers) => matched_readers
                .iter()
                .find(|locator| locator.remote_reader_guid() == a_reader_guid)
                .cloned(),
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                None
            }
        }
    }
}

impl Debug for StatelessWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StatelessWriter: {}", self.guid)
    }
}

impl Writer for StatelessWriter {
    fn writer_cache(&self) -> Arc<Mutex<WriterHistoryCache>> {
        Arc::clone(&self.writer_cache)
    }

    fn new_change(
        &self,
        kind: ChangeKind,
        data: Vec<u8>,
        // inline_qos: ParameterList,
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
    ) -> CacheChange {
        let last_change_sequence_number = match self.last_change_sequence_number.lock() {
            Ok(mut last_change_sequence_number) => {
                *last_change_sequence_number += 1;
                *last_change_sequence_number
            }
            Err(e) => {
                error!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        };

        if data.len() > self.data_max_size_serialized as usize {
            // Create fragmented cache change for large payload
            CacheChange::create_fragmented(
                kind,
                self.guid,
                handle,
                last_change_sequence_number,
                &data,
                source_timestamp,
                self.data_max_size_serialized as usize,
            )
        } else {
            // Create regular cache change for small payload
            CacheChange::new(
                kind,
                self.guid,
                handle,
                last_change_sequence_number,
                data,
                source_timestamp,
            )
        }
    }

    fn new_change_with_rpc_callback(
        &self,
        kind: ChangeKind,
        handle: InstanceHandle,
        source_timestamp: Option<RtpsTime>,
        data_fn: Box<dyn FnOnce(Guid, SequenceNumber) -> Vec<u8> + '_>,
    ) -> CacheChange {
        let last_change_sequence_number = match self.last_change_sequence_number.lock() {
            Ok(mut last_change_sequence_number) => {
                *last_change_sequence_number += 1;
                *last_change_sequence_number
            }
            Err(e) => {
                error!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        };

        let data = data_fn(self.guid, last_change_sequence_number);

        if data.len() > self.data_max_size_serialized as usize {
            CacheChange::create_fragmented(
                kind,
                self.guid,
                handle,
                last_change_sequence_number,
                &data,
                source_timestamp,
                self.data_max_size_serialized as usize,
            )
        } else {
            CacheChange::new(
                kind,
                self.guid,
                handle,
                last_change_sequence_number,
                data,
                source_timestamp,
            )
        }
    }

    fn data_max_size_serialized(&self) -> i32 {
        self.data_max_size_serialized
    }

    fn heartbeat_period(&self) -> RtpsDuration {
        self.heartbeat_period
    }

    fn last_change_sequence_number(&self) -> SequenceNumber {
        match self.last_change_sequence_number.lock() {
            Ok(last_change_sequence_number) => *last_change_sequence_number,
            Err(e) => {
                error!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        }
    }

    fn nack_response_delay(&self) -> RtpsDuration {
        self.nack_response_delay
    }

    fn nack_suppression_duration(&self) -> RtpsDuration {
        self.nack_suppression_duration
    }

    fn allocate_sequence_number(&self) -> SequenceNumber {
        match self.last_change_sequence_number.lock() {
            Ok(mut seq) => {
                *seq += 1;
                *seq
            }
            Err(e) => {
                log::error!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        }
    }

    fn push_mode(&self) -> bool {
        self.push_mode
    }

    fn wait_for_all_acked(&self, max_wait: DcpsDuration) -> bool {
        true // This cannot be false because we don't use stateless writer for reliable communication, it never waits for acknowledgments.
    }

    fn matched_reader_is_matched(&self, reader_guid: Guid) -> bool {
        match self.reader_locators.lock() {
            Ok(matched_readers) => {
                matched_readers.iter().any(|locators| locators.remote_reader_guid() == reader_guid)
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                false
            }
        }
    }

    fn matched_readers_guids(&self) -> Vec<Guid> {
        match self.reader_locators.lock() {
            Ok(matched_readers) => {
                matched_readers.iter().map(|locator| locator.remote_reader_guid()).collect()
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                Vec::new()
            }
        }
    }

    fn liveliness(&self) -> RtpsResult<LivelinessQosPolicy> {
        Ok(*self.publication_builtin_topic_data()?.liveliness())
    }

    fn assert_liveliness(&self) -> bool {
        true
    }

    //Any
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn get_publication_builtin_topic_data(
        &self,
    ) -> RtpsResult<Option<PublicationBuiltinTopicData>> {
        Ok(Some(self.publication_builtin_topic_data()?))
    }

    fn set_publication_builtin_topic_data(
        &self,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        *self
            .publication_builtin_topic_data
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))? =
            publication_builtin_topic_data;
        Ok(())
    }

    // For Dcps DataWriter::get_matched_subscription_data
    fn get_matched_subscription_data(
        &self,
        reader_guid: Guid,
    ) -> RtpsResult<SubscriptionBuiltinTopicData> {
        if let Some(reader) = self.matched_reader_lookup(reader_guid) {
            Ok(reader.subscription_builtin_topic_data().clone())
        } else {
            Err(RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, ""))
        }
    }

    fn remove_matched_reader_and_update_status(&self, reader_guid: Guid) -> RtpsResult<bool> {
        let mut locators = self
            .reader_locators
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;

        // One reader can register multiple ReaderLocators (per NIC); drop them all.
        let len_before = locators.len();
        locators.retain(|locator| {
            locator.guid_prefix() != reader_guid.prefix()
                || locator.remote_entity_id() != reader_guid.entity_id()
        });

        if locators.len() == len_before {
            debug!("Reader locator with guid {} not found in matched readers", reader_guid);
            return Ok(false);
        }

        drop(locators);
        self.update_publication_matched_status(-1, InstanceHandle::from_guid(&reader_guid));

        debug!("Removed reader locator with guid {} from matched readers", reader_guid);
        Ok(true)
    }
}

impl Endpoint for StatelessWriter {
    fn endpoint_id(&self) -> EntityId {
        self.endpoint_id
    }

    fn multicast_locator_list(&self) -> Vec<Locator> {
        self.multicast_locator_list.clone()
    }

    fn reliability_level(&self) -> ReliabilityQosPolicyKind {
        self.reliability_level
    }

    fn topic_kind(&self) -> TopicKind {
        self.topic_kind
    }

    fn unicast_locator_list(&self) -> Vec<Locator> {
        self.unicast_locator_list.clone()
    }
}

impl Entity for StatelessWriter {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        // Lift the callback out before calling it: the listener it reaches may
        // re-enter this entity, and an unwind through the call would poison the slot.
        if let Some(callback) = callback_handle(&self.callback) {
            notify_user("stateless_writer", || callback(status, info.clone()));
        }
    }

    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        self.callback.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace(f);
    }
}
