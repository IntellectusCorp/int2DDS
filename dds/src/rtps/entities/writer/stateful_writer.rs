#![allow(dead_code)]
#![allow(unused_variables)]

use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
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
        qos_policy::{
            LivelinessQosPolicy, QosPolicyId, ReliabilityQosPolicyKind,
            WriterReliabilityExtensionQosPolicy,
        },
        status::{
            OfferedIncompatibleQosStatus, PublicationMatchedStatus, QosPolicyCount, StatusInfo,
            StatusKind,
        },
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::{RtpsDuration, RtpsTime},
            types::{ChangeKind, SerializedData, TopicKind},
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{
                cache_change::CacheChange, history_cache::HistoryCache as _,
                writer_history::WriterHistoryCache,
            },
        },
        task::sending_handler::{MessageType, SendingHandler},
    },
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
};

use super::{reader_proxy::ReaderProxy, Writer};

#[allow(dead_code)]
pub(crate) struct StatefulWriter {
    guid: Guid,
    topic_kind: TopicKind,
    reliability_level: ReliabilityQosPolicyKind,
    unicast_locator_list: Vec<Locator>,
    multicast_locator_list: Vec<Locator>,
    endpoint_id: EntityId,
    last_change_sequence_number: Arc<Mutex<SequenceNumber>>,
    periodic_heartbeat_timer_id: TimerId,
    data_max_size_serialized: i32,
    matched_readers: Arc<Mutex<Vec<ReaderProxy>>>,
    writer_cache: Arc<Mutex<WriterHistoryCache>>,
    heartbeat_count: Arc<Mutex<u32>>,
    #[allow(clippy::type_complexity)]
    callback:
        Arc<Mutex<Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>>>,
    publication_builtin_topic_data: Arc<Mutex<PublicationBuiltinTopicData>>,
    heartbeat_timer_running: Arc<AtomicBool>,
    publication_matched_status: Arc<Mutex<PublicationMatchedStatus>>,
    offered_incompatible_qos_status: Arc<Mutex<OfferedIncompatibleQosStatus>>,
    writer_reliability_extension: WriterReliabilityExtensionQosPolicy,
}

impl StatefulWriter {
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::type_complexity)]
    pub(crate) fn new(
        guid: Guid,
        unicast_locator_list: Vec<Locator>,
        multicast_locator_list: Vec<Locator>,
        reliability_level: ReliabilityQosPolicyKind,
        topic_kind: TopicKind,
        endpoint_id: EntityId,
        data_max_size_serialized: i32,
        callback: Option<Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>>,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
        participant_guid: Guid,
    ) -> Self {
        let writer_reliability_extension =
            *publication_builtin_topic_data.writer_reliability_extension();

        Self {
            guid,
            unicast_locator_list,
            multicast_locator_list,
            reliability_level,
            topic_kind,
            endpoint_id,
            last_change_sequence_number: Arc::new(Mutex::new(SequenceNumber::new(0, 0))),
            periodic_heartbeat_timer_id: TimerId::PeriodicHeartbeat { entity_id: guid.entity_id() },
            data_max_size_serialized,
            matched_readers: Arc::new(Mutex::new(Vec::new())),
            writer_cache: Arc::new(Mutex::new(WriterHistoryCache::new(
                participant_guid,
                endpoint_id,
            ))),
            heartbeat_count: Arc::new(Mutex::new(1)),
            callback: Arc::new(Mutex::new(callback)),
            publication_builtin_topic_data: Arc::new(Mutex::new(publication_builtin_topic_data)),
            heartbeat_timer_running: Arc::new(AtomicBool::new(false)),
            publication_matched_status: Arc::new(Mutex::new(PublicationMatchedStatus::default())),
            offered_incompatible_qos_status: Arc::new(Mutex::new(
                OfferedIncompatibleQosStatus::default(),
            )),
            writer_reliability_extension,
        }
    }

    pub(crate) fn reader_proxies(&self) -> Arc<Mutex<Vec<ReaderProxy>>> {
        self.matched_readers.clone()
    }

    pub(crate) fn initial_heartbeat_delay(&self) -> RtpsDuration {
        RtpsDuration::from(self.writer_reliability_extension.initial_heartbeat_delay)
    }

    pub(crate) fn periodic_heartbeat_timer_id(&self) -> TimerId {
        self.periodic_heartbeat_timer_id
    }

    pub(crate) fn heartbeat_timer_running(&self) -> bool {
        self.heartbeat_timer_running.load(Ordering::Acquire)
    }

    pub(crate) fn disable_piggyback_heartbeat(&self) -> bool {
        self.writer_reliability_extension.disable_piggyback_heartbeat
    }

    pub(crate) fn publication_builtin_topic_data(&self) -> RtpsResult<PublicationBuiltinTopicData> {
        Ok(self
            .publication_builtin_topic_data
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?
            .clone())
    }

    pub(crate) fn matched_reader_add(&self, a_reader_proxy: ReaderProxy) {
        match self.matched_readers.lock() {
            Ok(mut matched_readers) => {
                matched_readers.push(a_reader_proxy);
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
            }
        }
    }

    pub(crate) fn matched_reader_remove(&self, a_reader_proxy: ReaderProxy) {
        match self.matched_readers.lock() {
            Ok(mut matched_readers) => {
                matched_readers.retain(|x| x != &a_reader_proxy);
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
            }
        }
    }

    pub(crate) fn matched_reader_lookup(&self, a_reader_guid: Guid) -> Option<ReaderProxy> {
        match self.matched_readers.lock() {
            Ok(matched_readers) => matched_readers
                .iter()
                .find(|proxy| proxy.remote_reader_guid() == a_reader_guid)
                .cloned(),
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                None
            }
        }
    }

    /// Register periodic heartbeat timer after a delay (one-shot delay, then periodic)
    pub(crate) fn register_periodic_heartbeat_timer_after_delay(
        &self,
        delay: RtpsDuration,
    ) -> RtpsResult<()> {
        let guid_prefix = self.guid.prefix();
        let guid = self.guid;
        let heartbeat_period = self.heartbeat_period().to_std_duration();
        let timer_id = self.periodic_heartbeat_timer_id();
        let heartbeat_timer_running = Arc::clone(&self.heartbeat_timer_running);
        let delay_timer_id = TimerId::PeriodicHeartbeatDelay { entity_id: self.guid.entity_id() };

        // Clone Arcs for the callback to check is_acked_by_all
        let matched_readers = Arc::clone(&self.matched_readers);
        let writer_cache = Arc::clone(&self.writer_cache);

        if let Ok(handler) = TimerHandler::get_instance(guid_prefix).lock() {
            handler.add_timer(
                delay_timer_id,
                delay.to_std_duration(),
                false, // one-shot
                move || {
                    // Do not trigger periodic heartbeat timer if all readers have acked the all changes
                    if Self::is_acked_by_all_impl(&writer_cache, &matched_readers) {
                        debug!("All readers have acknowledged, not registering periodic heartbeat timer.");
                        return;
                    }

                    Self::register_periodic_heartbeat_timer_impl(
                        guid_prefix,
                        guid,
                        heartbeat_period,
                        timer_id,
                        heartbeat_timer_running.clone(),
                    );
                },
            );
        } else {
            log::error!("Failed to acquire timer handler lock for heartbeat timer");
        }

        Ok(())
    }

    /// Start heartbeat timer (wrapper for instance method)
    pub(crate) fn register_periodic_heartbeat_timer(&self) {
        Self::register_periodic_heartbeat_timer_impl(
            self.guid.prefix(),
            self.guid,
            self.heartbeat_period().to_std_duration(),
            self.periodic_heartbeat_timer_id(),
            Arc::clone(&self.heartbeat_timer_running),
        );
    }

    /// Internal static method to register periodic heartbeat timer
    fn register_periodic_heartbeat_timer_impl(
        guid_prefix: GuidPrefix,
        guid: Guid,
        heartbeat_period: std::time::Duration,
        timer_id: TimerId,
        heartbeat_timer_running: Arc<AtomicBool>,
    ) {
        // CAS check - if already running, skip
        if heartbeat_timer_running
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            debug!("Tried to run a new heartbeat timer but there is already one");
            return;
        }

        if let Ok(handler) = TimerHandler::get_instance(guid_prefix).lock() {
            debug!("Adding a new heartbeat timer");
            handler.add_timer(
                timer_id,
                heartbeat_period,
                true, // repeating timer
                move || {
                    Self::send_heartbeat_to_readers(guid);
                },
            );
        } else {
            log::error!("Failed to acquire timer handler lock for heartbeat timer");
        }
    }

    pub(crate) fn compare_and_set_heartbeat_timer_running(
        &self,
        expected: bool,
        new: bool,
    ) -> RtpsResult<bool> {
        Ok(self
            .heartbeat_timer_running
            .compare_exchange(expected, new, Ordering::Acquire, Ordering::Relaxed)
            .is_ok())
    }

    fn send_heartbeat_to_readers(writer_guid: Guid) {
        if let Some(handler) = SendingHandler::get_instance_by_participant_guid(Guid::new(
            writer_guid.prefix(),
            EntityId::PARTICIPANT,
        )) {
            handler.push_message_and_wake(MessageType::UserHeartbeatToAll(writer_guid.entity_id()));
        }
    }

    pub(crate) fn increase_heartbeat_count(&self) {
        match self.heartbeat_count.lock() {
            Ok(mut heartbeat_count) => {
                *heartbeat_count = heartbeat_count.wrapping_add(1);
            }
            Err(e) => {
                error!("Failed to acquire heartbeat_count lock: {}", e);
            }
        }
    }

    pub(crate) fn heartbeat_count(&self) -> u32 {
        match self.heartbeat_count.lock() {
            Ok(heartbeat_count) => *heartbeat_count,
            Err(e) => {
                error!("Failed to acquire heartbeat_count lock: {}", e);
                0
            }
        }
    }

    /// Check if all readers have acked the latest change
    /// This ensures all samples written are acknowledged since ack status is cumulative
    pub(crate) fn is_acked_by_all(&self) -> RtpsResult<bool> {
        Ok(Self::is_acked_by_all_impl(&self.writer_cache, &self.matched_readers))
    }

    /// Stop heartbeat timer if all readers have acknowledged the latest change.
    /// Returns true if heartbeat was stopped, false otherwise.
    pub(crate) fn stop_heartbeat_if_acked_by_all(&self) -> RtpsResult<bool> {
        if !self.heartbeat_timer_running() || !self.is_acked_by_all()? {
            return Ok(false);
        }

        debug!(
            "All readers have acknowledged up to the latest sequence number, stopping heartbeat."
        );
        self.compare_and_set_heartbeat_timer_running(true, false)?;

        if let Ok(locked_timer_handler) = TimerHandler::get_instance(self.guid().prefix()).lock() {
            locked_timer_handler.remove_timer(self.periodic_heartbeat_timer_id());
        }

        Ok(true)
    }

    /// Check if all readers have acked a specific change
    pub(crate) fn is_change_acked_by_all(&self, a_change_seq_num: SequenceNumber) -> bool {
        Self::is_change_acked_by_all_impl(&self.matched_readers, a_change_seq_num)
    }

    fn is_acked_by_all_impl(
        writer_cache: &Arc<Mutex<WriterHistoryCache>>,
        matched_readers: &Arc<Mutex<Vec<ReaderProxy>>>,
    ) -> bool {
        let cache = match writer_cache.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };

        let latest_sn = match cache.get_seq_num_max() {
            Some(sn) => sn,
            None => return true,
        };

        drop(cache);

        Self::is_change_acked_by_all_impl(matched_readers, latest_sn)
    }

    fn is_change_acked_by_all_impl(
        matched_readers: &Arc<Mutex<Vec<ReaderProxy>>>,
        seq_num: SequenceNumber,
    ) -> bool {
        match matched_readers.lock() {
            Ok(readers) => readers
                .iter()
                .filter(|p| p.subscription_builtin_topic_data().is_reliable())
                .all(|p| p.max_acked_sn() >= seq_num),
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                false
            }
        }
    }

    pub(crate) fn update_reader_acked_changes(
        &self,
        reader_guid: Guid,
        base_seq_num: SequenceNumber,
    ) {
        match self.matched_readers.lock() {
            Ok(mut matched_readers) => {
                for reader_proxy in matched_readers.iter_mut() {
                    if reader_proxy.remote_reader_guid() == reader_guid {
                        reader_proxy.acked_changes_set(base_seq_num);
                        debug!(
                            "[StatefulWriter] Updated reader {:?} acked_changes_set to: {:?}",
                            reader_guid, base_seq_num
                        );
                        break;
                    }
                }
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
            }
        }
    }

    pub(crate) fn get_min_acked_sequence_number(&self) -> SequenceNumber {
        match self.matched_readers.lock() {
            Ok(readers) => {
                if readers.is_empty() {
                    return SequenceNumber::new(0, 0);
                }

                let mut min_acked = SequenceNumber::new(0, 0);
                for reader_proxy in readers.iter() {
                    let highest_acked = reader_proxy.max_acked_sn();
                    if min_acked == SequenceNumber::new(0, 0) || highest_acked < min_acked {
                        min_acked = highest_acked;
                    }
                }
                min_acked
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                SequenceNumber::new(0, 0)
            }
        }
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
}

impl Debug for StatefulWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StatefulWriter: {:?}", self.guid)
    }
}

impl Writer for StatefulWriter {
    fn writer_cache(&self) -> Arc<Mutex<WriterHistoryCache>> {
        Arc::clone(&self.writer_cache)
    }

    fn new_change(
        &self,
        kind: ChangeKind,
        data: SerializedData,
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
        data_fn: Box<dyn FnOnce(Guid, SequenceNumber) -> SerializedData + '_>,
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
        RtpsDuration::from(self.writer_reliability_extension.heartbeat_period)
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
        RtpsDuration::from(self.writer_reliability_extension.nack_response_delay)
    }

    fn nack_suppression_duration(&self) -> RtpsDuration {
        RtpsDuration::from(self.writer_reliability_extension.nack_suppression_duration)
    }

    fn push_mode(&self) -> bool {
        self.writer_reliability_extension.push_mode
    }

    fn wait_for_all_acked(&self, max_wait: DcpsDuration) -> bool {
        let start_time = std::time::Instant::now();

        loop {
            let last_written_cache_sn = self.last_change_sequence_number();
            let is_acked_by_all = self.is_change_acked_by_all(last_written_cache_sn);

            if is_acked_by_all {
                return true;
            } else {
                let elapsed = start_time.elapsed();
                let elapsed_duration =
                    DcpsDuration::new(elapsed.as_secs() as i32, elapsed.subsec_nanos());

                if elapsed_duration >= max_wait {
                    return false;
                }

                // Avoid busy waiting if max_wait is significant.
                if max_wait > DcpsDuration::from_nanos(1_000_000) {
                    thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        }
    }

    fn matched_reader_is_matched(&self, reader_guid: Guid) -> bool {
        match self.matched_readers.lock() {
            Ok(matched_readers) => {
                matched_readers.iter().any(|proxy| proxy.remote_reader_guid() == reader_guid)
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                false
            }
        }
    }

    fn matched_readers_guids(&self) -> Vec<Guid> {
        match self.reader_proxies().lock() {
            Ok(matched_readers) => {
                matched_readers.iter().map(|proxy| proxy.remote_reader_guid()).collect()
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
        if let Some(handler) = SendingHandler::get_instance_by_participant_guid(Guid::new(
            self.guid().prefix(),
            EntityId::PARTICIPANT,
        )) {
            if let Some(wlp_logic) = handler.wlp_logic() {
                match wlp_logic.assert_writer_liveliness(self.guid) {
                    Ok(()) => return true,
                    Err(_) => return false,
                }
            }
        }
        false
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
            Ok(reader.subscription_builtin_topic_data())
        } else {
            Err(RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, ""))
        }
    }
}

impl Endpoint for StatefulWriter {
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

impl Entity for StatefulWriter {
    fn guid(&self) -> Guid {
        self.guid
    }

    fn update_status(&self, status: StatusKind, info: Option<Arc<dyn StatusInfo>>) {
        match self.callback.lock() {
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
        match self.callback.lock() {
            Ok(mut callback) => {
                callback.replace(f);
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
    use crate::{
        core::time::Duration as DcpsDuration,
        infrastructure::qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        rtps::{
            common::{
                entity_id::EntityId, entity_kind::EntityKind, guid::Guid, sequence::SequenceNumber,
                types::TopicKind,
            },
            entities::writer::reader_proxy::ReaderProxy,
        },
        subscription::qos::{DataReaderQos, SubscriberQos},
        topic::qos::TopicQos,
    };
    use std::thread;

    #[test]
    fn test_wait_for_all_acked_success() {
        let stateful_writer = StatefulWriter::new(
            Guid::new([0; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY)),
            vec![],
            vec![],
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            65000,
            None,
            PublicationBuiltinTopicData::default(),
            Guid::new([0; 12], EntityId::PARTICIPANT),
        );

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let datareader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: DcpsDuration::from_millis(100),
            },
            ..DataReaderQos::default()
        };

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            SubscriptionBuiltinTopicData::new(
                &datareader_qos,
                &SubscriberQos::default(),
                &TopicQos::default(),
            ),
            SequenceNumber::new(0, 0),
        );

        // Reader initially hasn't acked anything
        stateful_writer.matched_reader_add(reader_proxy.clone());

        // Simulate writing a change
        {
            let mut last_sn = stateful_writer.last_change_sequence_number.lock().unwrap();
            *last_sn = SequenceNumber::new(0, 0);
        }

        // Ack the change after 50ms in a separate thread
        let writer_clone = Arc::new(stateful_writer);
        let writer_for_thread = writer_clone.clone();

        let handle = thread::spawn(move || {
            thread::sleep(std::time::Duration::from_millis(50));
            writer_for_thread
                .update_reader_acked_changes(remote_reader_guid, SequenceNumber::new(0, 1));
        });

        // Wait for ack with 200ms timeout
        let result = writer_clone.wait_for_all_acked(DcpsDuration::from_millis(200));

        handle.join().unwrap();

        // Should succeed
        assert!(result);
    }

    #[test]
    fn test_wait_for_all_acked_timeout() {
        let stateful_writer = StatefulWriter::new(
            Guid::new([0; 12], EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY)),
            vec![],
            vec![],
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            65000,
            None,
            PublicationBuiltinTopicData::default(),
            Guid::new([0; 12], EntityId::PARTICIPANT),
        );

        let remote_reader_guid =
            Guid::new([0; 12], EntityId::new([0, 0, 0], EntityKind::USER_DEFINED_READER_WITH_KEY));

        let datareader_qos = DataReaderQos {
            reliability: ReliabilityQosPolicy {
                kind: ReliabilityQosPolicyKind::Reliable,
                max_blocking_time: DcpsDuration::from_millis(100),
            },
            ..DataReaderQos::default()
        };

        let reader_proxy = ReaderProxy::new(
            remote_reader_guid,
            EntityId::UNKNOWN,
            Vec::new(),
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            SubscriptionBuiltinTopicData::new(
                &datareader_qos,
                &SubscriberQos::default(),
                &TopicQos::default(),
            ),
            SequenceNumber::new(0, 0),
        );

        stateful_writer.matched_reader_add(reader_proxy);

        // Simulate writing a change
        {
            let mut last_sn = stateful_writer.last_change_sequence_number.lock().unwrap();
            *last_sn = SequenceNumber::new(0, 1);
        }

        // Don't ack anything, should timeout
        let result = stateful_writer.wait_for_all_acked(DcpsDuration::from_millis(50));

        // Should timeout (return false)
        assert!(!result);
    }
}
