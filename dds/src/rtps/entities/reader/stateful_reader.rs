#![allow(dead_code)]
#![allow(unused_variables)]

use crate::utils::notify::{callback_handle, notify_user};
use std::{
    any::Any,
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
    infrastructure::{
        history_cache::HistoryCache as dcps_history_cache,
        qos_policy::{QosPolicyId, ReaderReliabilityExtensionQosPolicy, ReliabilityQosPolicyKind},
        status::{
            QosPolicyCount, RequestedIncompatibleQosStatus, RequestedIncompatibleTypeStatus,
            StatusInfo, StatusKind, SubscriptionMatchedStatus,
        },
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
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

use super::{writer_proxy::WriterProxy, Reader};

pub(crate) struct StatefulReader {
    guid: Guid,
    topic_kind: TopicKind,
    reliability_level: ReliabilityQosPolicyKind,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    endpoint_id: EntityId,
    expects_inline_qos: bool,
    reader_reliability_extension: ReaderReliabilityExtensionQosPolicy,
    reader_cache: Arc<Mutex<ReaderHistoryCache>>,
    matched_writers: Arc<Mutex<Vec<WriterProxy>>>,
    #[allow(clippy::type_complexity)]
    change_callback: Arc<Mutex<Option<Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>>>>,
    #[allow(clippy::type_complexity)]
    status_callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
    subscription_builtin_topic_data: Arc<Mutex<SubscriptionBuiltinTopicData>>,
    subscription_matched_status: Arc<Mutex<SubscriptionMatchedStatus>>,
    requested_incompatible_qos_status: Arc<Mutex<RequestedIncompatibleQosStatus>>,
    requested_incompatible_type_status: Arc<Mutex<RequestedIncompatibleTypeStatus>>,
}

impl StatefulReader {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
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
        let reader_reliability_extension =
            *subscription_builtin_topic_data.reader_reliability_extension();

        Self {
            guid,
            topic_kind,
            reliability_level,
            unicast_locator_list,
            multicast_locator_list,
            endpoint_id,
            expects_inline_qos,
            reader_reliability_extension,
            reader_cache: Arc::new(Mutex::new(ReaderHistoryCache::new(endpoint_id, None))),
            matched_writers: Arc::new(Mutex::new(Vec::new())),
            change_callback: Arc::new(Mutex::new(change_callback)),
            status_callback: Arc::new(Mutex::new(status_callback)),
            subscription_builtin_topic_data: Arc::new(Mutex::new(subscription_builtin_topic_data)),
            subscription_matched_status: Arc::new(Mutex::new(SubscriptionMatchedStatus::default())),
            requested_incompatible_qos_status: Arc::new(Mutex::new(
                RequestedIncompatibleQosStatus::default(),
            )),
            requested_incompatible_type_status: Arc::new(Mutex::new(
                RequestedIncompatibleTypeStatus::default(),
            )),
        }
    }

    pub(crate) fn preemptive_acknack_delay(&self) -> RtpsDuration {
        RtpsDuration::from(self.reader_reliability_extension.preemptive_acknack_delay)
    }

    /// Add `a_writer_proxy` unless a proxy for the same remote writer is already
    /// present. Returns whether it was added.
    ///
    /// The check happens under the same lock as the insert: SEDP matching can be
    /// driven concurrently by the local-creation path and the SEDP receive path,
    /// and a plain `matched_writer_is_matched` guard at the call site leaves a
    /// window where both callers pass it and push a duplicate proxy.
    pub(crate) fn matched_writer_add(&self, a_writer_proxy: WriterProxy) -> bool {
        match self.matched_writers.lock() {
            Ok(mut matched_writers) => {
                let remote_guid = a_writer_proxy.remote_writer_guid();
                if matched_writers.iter().any(|proxy| proxy.remote_writer_guid() == remote_guid) {
                    return false;
                }
                matched_writers.push(a_writer_proxy);
                true
            }
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
                false
            }
        }
    }

    pub(crate) fn matched_writer_remove(&self, a_writer_proxy: WriterProxy) {
        match self.matched_writers.lock() {
            Ok(mut matched_writers) => {
                matched_writers.retain(|proxy| proxy != &a_writer_proxy);
            }
            Err(e) => {
                error!("Failed to acquire matched_writers lock: {}", e);
            }
        }
    }

    pub(crate) fn matched_writer_lookup(&self, a_writer_guid: Guid) -> Option<WriterProxy> {
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

    pub(crate) fn writer_proxies(&self) -> Arc<Mutex<Vec<WriterProxy>>> {
        Arc::clone(&self.matched_writers)
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

    pub(crate) fn update_requested_incompatible_type_status(&self) {
        match self.requested_incompatible_type_status.lock() {
            Ok(mut requested_incompatible_type_status) => {
                requested_incompatible_type_status.total_count += 1;
                requested_incompatible_type_status.total_count_change += 1;

                self.update_status(
                    StatusKind::REQUESTED_INCOMPATIBLE_TYPE,
                    Some(Arc::new(requested_incompatible_type_status.clone())),
                );

                requested_incompatible_type_status.total_count_change = 0;
            }
            Err(e) => {
                log::error!("Failed to lock requested_incompatible_type_status: {:?}", e);
            }
        }
    }
}

impl Entity for StatefulReader {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        // Lift the callback out before calling it: the listener it reaches may
        // re-enter this entity, and an unwind through the call would poison the slot.
        if let Some(callback) = callback_handle(&self.status_callback) {
            notify_user("stateful_reader", || callback(status, info.clone()));
        }
    }

    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        self.status_callback.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace(f);
    }

    fn set_update_change(&self, f: Arc<dyn Fn(Arc<CacheChange>) + Send + Sync>) {
        self.change_callback.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace(f);
    }

    fn get_update_status_callback(
        &self,
    ) -> Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>
    {
        self.status_callback.clone()
    }
}

impl Endpoint for StatefulReader {
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

impl Debug for StatefulReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StatefulReader: {}", self.guid)
    }
}

impl Reader for StatefulReader {
    fn reader_cache(&self) -> Arc<Mutex<ReaderHistoryCache>> {
        // self.reader_cache.clone()
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
        RtpsDuration::from(self.reader_reliability_extension.heartbeat_response_delay)
    }

    fn heartbeat_suppression_duration(&self) -> RtpsDuration {
        RtpsDuration::from(self.reader_reliability_extension.heartbeat_suppression_duration)
    }

    fn nack_frag_response_delay(&self) -> RtpsDuration {
        RtpsDuration::from(self.reader_reliability_extension.nack_frag_response_delay)
    }

    fn nack_frag_retry_delay(&self) -> RtpsDuration {
        RtpsDuration::from(self.reader_reliability_extension.nack_frag_retry_delay)
    }

    fn nack_frag_max_retries(&self) -> u32 {
        self.reader_reliability_extension.nack_frag_max_retries
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
        debug!(
            "on_change: seq_num={}, writer_guid={}, kind={:?}, fragmented={}, total_fragments={}",
            change.sequence_number(),
            change.writer_guid(),
            change.kind(),
            change.is_fragmented(),
            change.total_fragments()
        );

        if let Some(callback) = callback_handle(&self.status_callback) {
            notify_user("stateful_reader", || callback(StatusKind::DATA_AVAILABLE, None));
        }

        if let Some(callback) = callback_handle(&self.change_callback) {
            notify_user("stateful_reader", || callback(change));
        }

        debug!("StatefulReader on_change completed.");
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn set_datareader_cache(
        &mut self,
        datareader_cache: Weak<Mutex<dyn dcps_history_cache + Send + Sync>>,
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

    fn remove_matched_writer_and_update_status(&self, writer_guid: Guid) -> RtpsResult<bool> {
        let mut proxies = self
            .matched_writers
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;

        // Find the index of the writer proxy with the given writer_guid
        let Some(idx) = proxies.iter().position(|proxy| proxy.remote_writer_guid() == writer_guid)
        else {
            debug!("Writer proxy with guid {} not found in matched writers", writer_guid);
            return Ok(false);
        };

        // Remove the writer proxy at the found index
        proxies.swap_remove(idx);
        drop(proxies);

        // Connectivity change: drop any open coherent set from the removed writer.
        if let Ok(mut cache) = self.reader_cache.lock() {
            cache.discard_coherent_pending(writer_guid);
        }

        // Update subscription matched status
        self.update_subscription_matched_status(-1, InstanceHandle::from_guid(&writer_guid));

        debug!("Removed writer proxy with guid {} from matched writers", writer_guid);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::time::Duration as DcpsDuration,
        rtps::common::entity_kind::EntityKind,
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    };
    use const_default::ConstDefault;

    #[test]
    fn nack_frag_accessors_read_through_reliability_extension_qos() {
        let guid =
            Guid::new([9; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_READER_WITH_KEY));
        let datareader_qos = DataReaderQos {
            reader_reliability_extension: ReaderReliabilityExtensionQosPolicy {
                nack_frag_response_delay: DcpsDuration::from_millis(123),
                nack_frag_retry_delay: DcpsDuration::from_millis(456),
                nack_frag_max_retries: 7,
                ..ReaderReliabilityExtensionQosPolicy::DEFAULT
            },
            ..DataReaderQos::default()
        };
        let subscription_data = SubscriptionBuiltinTopicData::new(
            &datareader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

        let reader = StatefulReader::new(
            guid,
            TopicKind::WithKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            guid.entity_id(),
            false,
            None,
            None,
            subscription_data,
            guid,
        );

        assert_eq!(
            reader.nack_frag_response_delay(),
            RtpsDuration::from(DcpsDuration::from_millis(123))
        );
        assert_eq!(
            reader.nack_frag_retry_delay(),
            RtpsDuration::from(DcpsDuration::from_millis(456))
        );
        assert_eq!(reader.nack_frag_max_retries(), 7);
    }
}
