#![allow(dead_code)]
#![allow(unused_variables)]

use log::{debug, error};

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    infrastructure::{
        history_cache::HistoryCache as dcps_history_cache,
        qos_policy::{QosPolicyId, ReliabilityQosPolicyKind},
        status::{
            QosPolicyCount, RequestedIncompatibleQosStatus, StatusInfo, StatusKind,
            SubscriptionMatchedStatus,
        },
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::RtpsDuration,
            types::TopicKind,
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{
                cache_change::CacheChange, history_cache::HistoryCache,
                reader_history::ReaderHistoryCache,
            },
        },
    },
};
use std::{
    any::Any,
    fmt::Debug,
    sync::{Arc, Mutex, Weak},
};

use super::{Reader, WriterLocator};

#[allow(dead_code)]
pub(crate) struct StatelessReader {
    guid: Guid,
    topic_kind: TopicKind,
    reliability_level: ReliabilityQosPolicyKind,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    endpoint_id: EntityId,
    expects_inline_qos: bool,
    heartbeat_response_delay: RtpsDuration,
    heartbeat_suppression_duration: RtpsDuration,
    reader_cache: Arc<Mutex<ReaderHistoryCache>>,
    matched_writers: Arc<Mutex<Vec<WriterLocator>>>,
    change_callback: Arc<Mutex<Option<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>>>>,
    status_callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
    subscription_builtin_topic_data: Arc<Mutex<SubscriptionBuiltinTopicData>>,
    subscription_matched_status: Arc<Mutex<SubscriptionMatchedStatus>>,
    requested_incompatible_qos_status: Arc<Mutex<RequestedIncompatibleQosStatus>>,
}

impl StatelessReader {
    pub(crate) fn new(
        guid: Guid,
        topic_kind: TopicKind,
        reliability_level: ReliabilityQosPolicyKind,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        endpoint_id: EntityId,
        expects_inline_qos: bool,
        change_callback: Option<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>>,
        status_callback: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
        participant_guid: Guid,
    ) -> Self {
        Self {
            guid,
            topic_kind,
            reliability_level,
            unicast_locator_list,
            multicast_locator_list,
            endpoint_id,
            expects_inline_qos,
            heartbeat_response_delay: RtpsDuration::new(0, 500 * 1000 * 1000),
            heartbeat_suppression_duration: RtpsDuration::new(0, 0),
            reader_cache: Arc::new(Mutex::new(ReaderHistoryCache::new(endpoint_id, None))),
            matched_writers: Arc::new(Mutex::new(Vec::new())),
            change_callback: Arc::new(Mutex::new(change_callback)),
            status_callback: Arc::new(Mutex::new(status_callback)),
            subscription_builtin_topic_data: Arc::new(Mutex::new(subscription_builtin_topic_data)),
            subscription_matched_status: Arc::new(Mutex::new(SubscriptionMatchedStatus::default())),
            requested_incompatible_qos_status: Arc::new(Mutex::new(
                RequestedIncompatibleQosStatus::default(),
            )),
        }
    }

    pub(crate) fn matched_writer_add(&self, a_writer_proxy: WriterLocator) {
        match self.matched_writers.lock() {
            Ok(mut matched_writers) => {
                matched_writers.push(a_writer_proxy);
            }
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
            }
        }
    }

    pub(crate) fn matched_writer_remove(&self, a_writer_proxy: WriterLocator) {
        match self.matched_writers.lock() {
            Ok(mut matched_writers) => {
                matched_writers.retain(|proxy| proxy != &a_writer_proxy);
            }
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
            }
        }
    }

    pub(crate) fn matched_writer_lookup(&self, a_writer_guid: Guid) -> Option<WriterLocator> {
        match self.matched_writers.lock() {
            Ok(matched_writers) => matched_writers
                .iter()
                .find(|proxy| proxy.remote_writer_guid() == a_writer_guid)
                .cloned(),
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
                None
            }
        }
    }

    pub(crate) fn writer_locators(&self) -> Arc<Mutex<Vec<WriterLocator>>> {
        self.matched_writers.clone()
    }

    pub(crate) fn subscription_builtin_topic_data(
        &self,
    ) -> RtpsResult<SubscriptionBuiltinTopicData> {
        Ok(self
            .subscription_builtin_topic_data
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?
            .clone())
    }

    pub(crate) fn update_subscription_matched_status(
        &self,
        number: i32,
        last_publication_handle: InstanceHandle,
    ) {
        match self.subscription_matched_status.lock() {
            Ok(mut subscription_matched_status) => {
                debug!(
                    "Current count in subscription matched status before change: {:?}",
                    subscription_matched_status.current_count
                );
                if number > 0 {
                    subscription_matched_status.total_count += number;
                    subscription_matched_status.total_count_change += number;
                }
                subscription_matched_status.last_publication_handle = last_publication_handle;
                subscription_matched_status.current_count += number;
                subscription_matched_status.current_count_change = number;
                debug!(
                    "Current count in subscription matched status after change: {:?}",
                    subscription_matched_status.current_count
                );
                self.update_status(
                    StatusKind::SUBSCRIPTION_MATCHED,
                    Some(Arc::new(*subscription_matched_status)),
                );
                subscription_matched_status.total_count_change = 0;
                subscription_matched_status.current_count_change = 0;
            }
            Err(e) => {
                log::error!("Failed to lock subscription_matched_status: {:?}", e);
            }
        }
    }

    pub(crate) fn update_requested_incompatible_qos_status(&self, policy_id: QosPolicyId) {
        match self.requested_incompatible_qos_status.lock() {
            Ok(mut requested_incompatible_qos_status) => {
                requested_incompatible_qos_status.total_count += 1;
                requested_incompatible_qos_status.total_count_change += 1;
                requested_incompatible_qos_status.last_policy_id = policy_id;

                let mut not_found = true;
                for policy in requested_incompatible_qos_status.policies.iter_mut() {
                    if policy.policy_id == policy_id {
                        policy.count += 1;
                        not_found = false;
                        break;
                    }
                }
                if not_found {
                    requested_incompatible_qos_status
                        .policies
                        .push(QosPolicyCount { policy_id, count: 1 });
                }

                self.update_status(
                    StatusKind::REQUESTED_INCOMPATIBLE_QOS,
                    Some(Arc::new(requested_incompatible_qos_status.clone())),
                );

                requested_incompatible_qos_status.total_count_change = 0;
            }
            Err(e) => {
                log::error!("Failed to lock requested_incompatible_qos_status: {:?}", e);
            }
        }
    }

    fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<
            Mutex<
                dyn dcps_history_cache<CacheChangeInputType = Arc<Mutex<CacheChange>>>
                    + Send
                    + Sync,
            >,
        >,
    ) {
        match self.reader_cache.lock() {
            Ok(mut guard) => {
                guard.set_datareader_cache(datareader_cache);
            }
            Err(e) => {
                error!("Failed to acquire reader_cache lock: {}", e);
            }
        }
    }
}

impl Debug for StatelessReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StatelessReader: {:?}", self.guid)
    }
}

impl Entity for StatelessReader {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        match self.status_callback.lock() {
            Ok(callback) => {
                if let Some(callback) = callback.as_ref() {
                    callback(status, info.clone());
                }
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }

    fn set_update_status(
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

    fn set_update_change(&self, f: Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>) {
        match self.change_callback.lock() {
            Ok(mut callback) => {
                callback.replace(f);
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        }
    }

    fn get_update_status_callback(
        &self,
    ) -> Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>
    {
        self.status_callback.clone()
    }
}

impl Endpoint for StatelessReader {
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

impl Reader for StatelessReader {
    fn reader_cache(&self) -> Arc<Mutex<ReaderHistoryCache>> {
        Arc::clone(&self.reader_cache)
    }

    fn available_changes(&self) -> Vec<Arc<CacheChange>> {
        // Based on Figure 8.7 state diagram
        // out: Vec<CacheChange>
        let cache_guard = match self.reader_cache.lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire reader cache lock: {}", e);
                return vec![];
            }
        };
        cache_guard.get_changes()
    }

    fn expects_inline_qos(&self) -> bool {
        self.expects_inline_qos
    }

    fn heartbeat_response_delay(&self) -> RtpsDuration {
        self.heartbeat_response_delay
    }

    fn heartbeat_suppression_duration(&self) -> RtpsDuration {
        self.heartbeat_suppression_duration
    }

    fn matched_writer_is_matched(&self, writer_guid: Guid) -> bool {
        match self.matched_writers.lock() {
            Ok(matched_writers) => {
                matched_writers.iter().any(|proxy| proxy.remote_writer_guid() == writer_guid)
            }
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
                false
            }
        }
    }

    fn matched_writers_guids(&self) -> Vec<Guid> {
        match self.matched_writers.lock() {
            Ok(matched_writers) => {
                matched_writers.iter().map(|proxy| proxy.remote_writer_guid()).collect()
            }
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
                vec![]
            }
        }
    }

    fn on_change(&self, change: Arc<CacheChange>) {
        log::debug!("change: {:?}", change);

        match self.status_callback.lock() {
            Ok(callback) => {
                if let Some(callback) = callback.as_ref() {
                    callback(StatusKind::DATA_AVAILABLE, None);
                }
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        };

        match self.change_callback.lock() {
            Ok(callback) => {
                if let Some(callback) = callback.as_ref() {
                    callback(change);
                }
            }
            Err(e) => {
                log::error!("Failed to lock callback: {:?}", e);
            }
        };

        log::debug!("StatelessReader on_change completed.");
    }

    fn get_next_sequence_number(&self) -> SequenceNumber {
        let cache_guard = match self.reader_cache.lock() {
            Ok(guard) => guard,
            Err(e) => {
                error!("Failed to acquire reader cache lock: {}", e);
                return SequenceNumber::new(0, 0);
            }
        };
        cache_guard.get_seq_num_max().next()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<
            Mutex<
                dyn dcps_history_cache<CacheChangeInputType = Arc<Mutex<CacheChange>>>
                    + Send
                    + Sync,
            >,
        >,
    ) -> RtpsResult<()> {
        let cache_guard = self.reader_cache.lock();
        match cache_guard {
            Ok(mut guard) => {
                guard.set_datareader_cache(datareader_cache);
                Ok(())
            }
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }

    fn set_subscription_builtin_topic_data(
        &self,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        *self
            .subscription_builtin_topic_data
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))? =
            subscription_builtin_topic_data;
        Ok(())
    }

    fn get_subscription_builtin_topic_data(&self) -> RtpsResult<SubscriptionBuiltinTopicData> {
        self.subscription_builtin_topic_data
            .lock()
            .map(|guard| guard.clone())
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))
    }

    // For Dcps DataReader::get_matched_publication_data
    fn get_matched_publication_data(
        &self,
        writer_guid: Guid,
    ) -> RtpsResult<PublicationBuiltinTopicData> {
        if let Some(writer) = self.matched_writer_lookup(writer_guid) {
            Ok(writer.publication_builtin_topic_data())
        } else {
            Err(RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, ""))
        }
    }
}
