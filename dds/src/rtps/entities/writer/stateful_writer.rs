#![allow(dead_code)]
#![allow(unused_variables)]

use crate::utils::notify::{callback_handle, notify_user};
use std::{
    fmt::Debug,
    sync::{
        atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering},
        Arc, Mutex, OnceLock, RwLock, Weak,
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
            OfferedIncompatibleQosStatus, OfferedIncompatibleTypeStatus, PublicationMatchedStatus,
            QosPolicyCount, StatusInfo, StatusKind,
        },
    },
    rtps::{
        common::{
            entity_id::EntityId,
            guid::{GroupDigest, Guid, GuidPrefix},
            locator::Locator,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::{RtpsDuration, RtpsTime},
            types::{ChangeKind, TopicKind},
        },
        entities::{
            endpoint::Endpoint,
            entity::Entity,
            history::{
                cache_change::CacheChange, history_cache::HistoryCache as _,
                writer_history::WriterHistoryCache,
            },
        },
        messages::submessages::{gap, heartbeat},
        task::sending_handler::{MessageType, SendingHandler},
    },
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
};

use crate::rtps::entities::participant::Participant;

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
    // Invoked with the minimum sequence number acked by all reliable readers when it advances
    #[allow(clippy::type_complexity)]
    all_acked_callback: Arc<Mutex<Option<Arc<dyn Fn(SequenceNumber) + Send + Sync>>>>,
    // Mirrors all_acked_callback presence for a lock-free check on the write hot path
    all_acked_callback_set: Arc<AtomicBool>,
    last_acked_notify_sn: Arc<Mutex<SequenceNumber>>,
    publication_builtin_topic_data: Arc<Mutex<PublicationBuiltinTopicData>>,
    heartbeat_timer_running: Arc<AtomicBool>,
    publication_matched_status: Arc<Mutex<PublicationMatchedStatus>>,
    offered_incompatible_qos_status: Arc<Mutex<OfferedIncompatibleQosStatus>>,
    offered_incompatible_type_status: Arc<Mutex<OfferedIncompatibleTypeStatus>>,
    writer_reliability_extension: WriterReliabilityExtensionQosPolicy,
    // Callback-producing accesses currently in flight against this writer. `remove_writer`
    // drains this to zero before returning.
    in_flight_callbacks: AtomicUsize,
    // Shared with the publisher and set once for GROUP-scope writers.
    last_group_seq_num: OnceLock<Arc<AtomicI64>>,
    writer_set: OnceLock<Arc<RwLock<GroupDigest>>>,
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
        participant: Weak<Participant>,
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
            writer_cache: Arc::new(Mutex::new(WriterHistoryCache::new(participant, endpoint_id))),
            heartbeat_count: Arc::new(Mutex::new(1)),
            callback: Arc::new(Mutex::new(callback)),
            all_acked_callback: Arc::new(Mutex::new(None)),
            all_acked_callback_set: Arc::new(AtomicBool::new(false)),
            last_acked_notify_sn: Arc::new(Mutex::new(SequenceNumber::new(0, 0))),
            publication_builtin_topic_data: Arc::new(Mutex::new(publication_builtin_topic_data)),
            heartbeat_timer_running: Arc::new(AtomicBool::new(false)),
            publication_matched_status: Arc::new(Mutex::new(PublicationMatchedStatus::default())),
            offered_incompatible_qos_status: Arc::new(Mutex::new(
                OfferedIncompatibleQosStatus::default(),
            )),
            offered_incompatible_type_status: Arc::new(Mutex::new(
                OfferedIncompatibleTypeStatus::default(),
            )),
            writer_reliability_extension,
            in_flight_callbacks: AtomicUsize::new(0),
            last_group_seq_num: OnceLock::new(),
            writer_set: OnceLock::new(),
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

    // Share the publisher's group sequence number counter; later calls are ignored.
    // Only a GROUP access scope publisher calls this, so it also marks the writer as one.
    pub(crate) fn set_last_group_seq_num(&self, last_group_seq_num: Arc<AtomicI64>) {
        let _ = self.last_group_seq_num.set(last_group_seq_num);
    }

    // Share the publisher's writerSet digest; later calls are ignored.
    pub(crate) fn set_writer_set(&self, writer_set: Arc<RwLock<GroupDigest>>) {
        let _ = self.writer_set.set(writer_set);
    }

    // Heartbeat group info for the range first_sn..=last_sn. None when this writer has no
    // shared group state, its group has issued nothing yet, or the numbers would be invalid.
    pub(crate) fn create_heartbeat_group_info(
        &self,
        history_cache: &WriterHistoryCache,
        first_sn: SequenceNumber,
        last_sn: SequenceNumber,
    ) -> Option<heartbeat::GroupInfo> {
        // Absent on every writer but a GROUP access scope one, which is what gates this.
        let current_gsn =
            SequenceNumber::from_i64(self.last_group_seq_num.get()?.load(Ordering::Acquire));
        let writer_set = *self.writer_set.get()?.read().ok()?;
        if current_gsn.to_i64() <= 0 {
            return None;
        }

        // A reader has to be told that a writer holding nothing holds nothing, so an absent
        // boundary reports the range firstSN and lastSN report: one that ends before it starts.
        let stored_range =
            history_cache.get_change(first_sn).zip(history_cache.get_change(last_sn));
        let (first_gsn, last_gsn) = match stored_range {
            Some((first_change, last_change)) => (
                first_change.presentation_info().group_seq_num?,
                last_change.presentation_info().group_seq_num?,
            ),
            None => (SequenceNumber::INIT, SequenceNumber::ZERO),
        };

        let group_info = heartbeat::GroupInfo {
            current_gsn,
            first_gsn,
            last_gsn,
            writer_set,
            secure_writer_set: GroupDigest::EMPTY,
        };
        group_info.validate().ok()?;

        Some(group_info)
    }

    // Group sequence numbers a Gap over gap_start..=gap_end declares unavailable. The range
    // stops before the next sample this writer still holds, which it is about to send.
    // None when this writer has no shared group state or the range would be invalid.
    pub(crate) fn create_gap_group_info(
        &self,
        history_cache: &WriterHistoryCache,
        gap_start: SequenceNumber,
        gap_end: SequenceNumber,
    ) -> Option<gap::GroupInfo> {
        // Absent on every writer but a GROUP access scope one, which is what gates this.
        if self.last_group_seq_num.get().is_none() {
            return None;
        }

        // The range must never reach a group sequence number this writer still has to send.
        let gap_end_gsn = match history_cache.next_change_after(gap_end) {
            // This one is still to be sent, so the range stops one short of it.
            Some(change_after_gap) => {
                change_after_gap.presentation_info().group_seq_num?.previous()
            }
            // Nothing is left to send, so the last number declared gone is the furthest proof.
            None => {
                let last_gapped_change = history_cache.get_change(gap_end)?;
                last_gapped_change.presentation_info().group_seq_num?
            }
        };

        // A removed sample took its group sequence number with it, so the range collapses onto
        // its end. The reader's ordering rules read only gapEndGSN.
        let gap_start_gsn = history_cache
            .get_change(gap_start)
            .and_then(|first_gapped_change| first_gapped_change.presentation_info().group_seq_num)
            .unwrap_or(gap_end_gsn);

        let group_info = gap::GroupInfo { gap_start_gsn, gap_end_gsn };
        group_info.validate().ok()?;

        Some(group_info)
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

    /// Add `a_reader_proxy` unless a proxy for the same remote reader is already
    /// present. Returns whether it was added.
    ///
    /// The check happens under the same lock as the insert: SEDP matching can be
    /// driven concurrently by the local-creation path and the SEDP receive path,
    /// and a plain `matched_reader_is_matched` guard at the call site leaves a
    /// window where both callers pass it and push a duplicate proxy.
    pub(crate) fn matched_reader_add(&self, a_reader_proxy: ReaderProxy) -> bool {
        match self.matched_readers.lock() {
            Ok(mut matched_readers) => {
                let remote_guid = a_reader_proxy.remote_reader_guid();
                if matched_readers.iter().any(|proxy| proxy.remote_reader_guid() == remote_guid) {
                    return false;
                }
                matched_readers.push(a_reader_proxy);
                true
            }
            Err(e) => {
                error!("Failed to acquire matched_readers lock: {}", e);
                false
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
            Ok(readers) => {
                readers.iter().filter(|p| p.is_reliable()).all(|p| p.max_acked_sn() >= seq_num)
            }
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

    // Ok(Some) is the floor over matched reliable readers; Ok(None) means none are
    // matched. Best-effort readers never send ACKNACK, so they are excluded to avoid
    // pinning the minimum at 0. Err keeps the lock failure distinct from "no readers".
    pub(crate) fn get_min_acked_sequence_number(&self) -> RtpsResult<Option<SequenceNumber>> {
        let readers = self
            .matched_readers
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;
        Ok(readers.iter().filter(|p| p.is_reliable()).map(|p| p.max_acked_sn()).min())
    }

    pub(crate) fn set_all_acked_callback(&self, f: Arc<dyn Fn(SequenceNumber) + Send + Sync>) {
        match self.all_acked_callback.lock() {
            Ok(mut callback) => {
                callback.replace(f);
                self.all_acked_callback_set.store(true, Ordering::Release);
            }
            Err(e) => {
                error!("Failed to lock all_acked_callback: {:?}", e);
            }
        }
    }

    // Non-blocking RTPS transmit queue length for debug logging.
    pub(crate) fn rtps_cache_len(&self) -> String {
        match self.writer_cache.try_lock() {
            Ok(cache) => cache.len().to_string(),
            Err(_) => "busy".to_string(),
        }
    }

    // Invokes the all-acked callback when the minimum acked sequence number advances.
    // Must not be called while holding the matched_readers lock.
    pub(crate) fn process_acked_changes(&self) {
        if !self.all_acked_callback_set.load(Ordering::Acquire) {
            return;
        }

        let callback = match self.all_acked_callback.lock() {
            Ok(callback) => match callback.as_ref() {
                Some(callback) => callback.clone(),
                None => return,
            },
            Err(e) => {
                error!("Failed to lock all_acked_callback: {:?}", e);
                return;
            }
        };

        let min_acked = match self.get_min_acked_sequence_number() {
            Ok(Some(min)) => min,
            Ok(None) => match self.writer_cache.lock() {
                // With no reliable reader to wait on, everything already transmitted is removable:
                // fall back to the highest sequence number that entered the transmit queue. A lock
                // failure stays an error so unacked samples are never purged by mistake.
                Ok(cache) => cache.highest_sn(),
                Err(e) => {
                    error!("Failed to lock writer_cache: {:?}", e);
                    return;
                }
            },
            Err(e) => {
                error!("Failed to read min acked sequence number: {:?}", e);
                return;
            }
        };

        match self.last_acked_notify_sn.lock() {
            Ok(mut last_notified) => {
                if min_acked <= *last_notified {
                    return;
                }
                *last_notified = min_acked;
            }
            Err(e) => {
                error!("Failed to lock last_acked_notify_sn: {:?}", e);
                return;
            }
        }

        callback(min_acked);
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
}

impl Debug for StatefulWriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "StatefulWriter: {}", self.guid)
    }
}

impl Writer for StatefulWriter {
    fn enter_callback(&self) {
        self.in_flight_callbacks.fetch_add(1, Ordering::SeqCst);
    }

    fn exit_callback(&self) {
        self.in_flight_callbacks.fetch_sub(1, Ordering::SeqCst);
    }

    fn in_flight_callbacks(&self) -> usize {
        self.in_flight_callbacks.load(Ordering::SeqCst)
    }

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

        let max_message_size = crate::common::env::get_max_message_size();

        if data.len() > max_message_size {
            // Create fragmented cache change for large payload
            CacheChange::create_fragmented(
                kind,
                self.guid,
                handle,
                last_change_sequence_number,
                &data,
                source_timestamp,
                max_message_size,
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

        let max_message_size = crate::common::env::get_max_message_size();

        if data.len() > max_message_size {
            CacheChange::create_fragmented(
                kind,
                self.guid,
                handle,
                last_change_sequence_number,
                &data,
                source_timestamp,
                max_message_size,
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

    fn allocate_sequence_number(&self) -> SequenceNumber {
        match self.last_change_sequence_number.lock() {
            Ok(mut seq) => {
                *seq += 1;
                *seq
            }
            Err(e) => {
                error!("Failed to acquire last_change_sequence_number lock: {}", e);
                SequenceNumber::UNKNOWN
            }
        }
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
            Ok(reader.subscription_builtin_topic_data().clone())
        } else {
            Err(RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, ""))
        }
    }

    fn remove_matched_reader_and_update_status(&self, reader_guid: Guid) -> RtpsResult<bool> {
        let mut proxies = self
            .matched_readers
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;

        // Find the index of the reader to remove
        let Some(idx) = proxies.iter().position(|proxy| proxy.remote_reader_guid() == reader_guid)
        else {
            debug!("Reader proxy with guid {} not found in matched readers", reader_guid);
            return Ok(false);
        };

        // Remove the reader proxy from the list
        proxies.swap_remove(idx);
        drop(proxies);

        // Update publication matched status
        self.update_publication_matched_status(-1, InstanceHandle::from_guid(&reader_guid));

        debug!("Removed reader proxy with guid {} from matched readers", reader_guid);

        // A reliable reader leaving can advance the ack floor over the remaining readers.
        debug!("[history-strict] trigger=unmatch-single");
        self.process_acked_changes();
        debug!("[history-strict] after unmatch-single rtps_len={}", self.rtps_cache_len());

        Ok(true)
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
        // Lift the callback out before calling it: the listener it reaches may
        // re-enter this entity, and an unwind through the call would poison the slot.
        if let Some(callback) = callback_handle(&self.callback) {
            notify_user("stateful_writer", || callback(status, info.clone()));
        }
    }

    fn set_update_status(
        &self,
        f: Arc<dyn Fn(StatusKind, Option<Arc<dyn StatusInfo>>) + Send + Sync>,
    ) {
        self.callback.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).replace(f);
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
                entity_id::EntityId,
                entity_kind::EntityKind,
                guid::Guid,
                sequence::SequenceNumber,
                types::{ChangeKind, TopicKind},
            },
            entities::{
                history::cache_change::PresentationInfo, writer::reader_proxy::ReaderProxy,
            },
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
            Weak::new(),
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
            Weak::new(),
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

    fn create_group_scope_writer(last_group_seq_num: i64) -> StatefulWriter {
        let entity_id = EntityId::new([0, 0, 1], EntityKind::USER_DEFINED_WRITER_NO_KEY);
        let writer = StatefulWriter::new(
            Guid::new([0; 12], entity_id),
            vec![],
            vec![],
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            entity_id,
            65000,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        );
        writer.set_last_group_seq_num(Arc::new(AtomicI64::new(last_group_seq_num)));
        writer.set_writer_set(Arc::new(RwLock::new(GroupDigest::EMPTY)));

        writer
    }

    // Store a change the way a write under GROUP access scope leaves it.
    fn store_change_with_group_seq_num(writer: &StatefulWriter, seq_num: i64, group_seq_num: i64) {
        let mut change = CacheChange::new(
            ChangeKind::Alive,
            writer.guid(),
            InstanceHandle::NIL,
            SequenceNumber::from_i64(seq_num),
            vec![1],
            None,
        );
        change.set_presentation_info(PresentationInfo {
            group_seq_num: Some(SequenceNumber::from_i64(group_seq_num)),
            ..Default::default()
        });

        let writer_cache = writer.writer_cache();
        let mut history_cache = writer_cache.lock().unwrap();
        history_cache.add_change(Arc::new(change), writer).unwrap();
    }

    fn get_gap_group_info(
        writer: &StatefulWriter,
        gap_start: i64,
        gap_end: i64,
    ) -> Option<gap::GroupInfo> {
        let writer_cache = writer.writer_cache();
        let history_cache = writer_cache.lock().unwrap();
        writer.create_gap_group_info(
            &history_cache,
            SequenceNumber::from_i64(gap_start),
            SequenceNumber::from_i64(gap_end),
        )
    }

    #[test]
    fn gap_group_info_collapses_when_the_gapped_sample_is_gone() {
        let writer = create_group_scope_writer(5);
        store_change_with_group_seq_num(&writer, 2, 3);
        store_change_with_group_seq_num(&writer, 3, 5);

        // Seq 1 left no group sequence number behind, so the range is only its end.
        assert_eq!(
            get_gap_group_info(&writer, 1, 2),
            Some(gap::GroupInfo {
                gap_start_gsn: SequenceNumber::from_i64(4),
                gap_end_gsn: SequenceNumber::from_i64(4),
            })
        );
    }

    fn get_heartbeat_group_info(
        writer: &StatefulWriter,
        first_sn: i64,
        last_sn: i64,
    ) -> Option<heartbeat::GroupInfo> {
        let writer_cache = writer.writer_cache();
        let history_cache = writer_cache.lock().unwrap();
        writer.create_heartbeat_group_info(
            &history_cache,
            SequenceNumber::from_i64(first_sn),
            SequenceNumber::from_i64(last_sn),
        )
    }

    #[test]
    fn heartbeat_group_info_spans_the_group_sequence_numbers_it_holds() {
        let writer = create_group_scope_writer(5);
        store_change_with_group_seq_num(&writer, 1, 1);
        store_change_with_group_seq_num(&writer, 2, 3);

        assert_eq!(
            get_heartbeat_group_info(&writer, 1, 2),
            Some(heartbeat::GroupInfo {
                current_gsn: SequenceNumber::from_i64(5),
                first_gsn: SequenceNumber::from_i64(1),
                last_gsn: SequenceNumber::from_i64(3),
                writer_set: GroupDigest::EMPTY,
                secure_writer_set: GroupDigest::EMPTY,
            })
        );
    }

    #[test]
    fn heartbeat_group_info_reports_an_empty_range_when_nothing_is_held() {
        let writer = create_group_scope_writer(7);

        // An empty history sends firstSN one past lastSN, and the group range says the same:
        // the reader has no group sequence number to wait on from this writer.
        assert_eq!(
            get_heartbeat_group_info(&writer, 1, 0),
            Some(heartbeat::GroupInfo {
                current_gsn: SequenceNumber::from_i64(7),
                first_gsn: SequenceNumber::INIT,
                last_gsn: SequenceNumber::ZERO,
                writer_set: GroupDigest::EMPTY,
                secure_writer_set: GroupDigest::EMPTY,
            })
        );
    }

    #[test]
    fn heartbeat_group_info_is_absent_before_the_group_issues_a_number() {
        let writer = create_group_scope_writer(0);

        assert_eq!(get_heartbeat_group_info(&writer, 1, 0), None);
    }

    // A receiver rejects the whole Heartbeat over a bad group block, taking firstSN, lastSN and
    // count down with it, so the block is dropped here and a plain Heartbeat goes out instead.
    #[test]
    fn heartbeat_group_info_is_dropped_when_the_range_is_invalid() {
        let writer = create_group_scope_writer(9);
        store_change_with_group_seq_num(&writer, 1, 9);
        store_change_with_group_seq_num(&writer, 2, 3);

        assert_eq!(get_heartbeat_group_info(&writer, 1, 2), None);
    }

    #[test]
    fn gap_group_info_is_dropped_when_the_range_is_invalid() {
        let writer = create_group_scope_writer(9);
        store_change_with_group_seq_num(&writer, 1, 9);
        store_change_with_group_seq_num(&writer, 2, 2);
        store_change_with_group_seq_num(&writer, 3, 4);

        assert_eq!(get_gap_group_info(&writer, 1, 2), None);
    }

    // Neither boundary is held any more, so there is no group sequence number to anchor the
    // range on and the writer can prove nothing about what it will not send.
    #[test]
    fn gap_group_info_is_absent_when_neither_boundary_is_still_held() {
        let writer = create_group_scope_writer(5);
        store_change_with_group_seq_num(&writer, 1, 1);

        assert_eq!(get_gap_group_info(&writer, 2, 3), None);
    }
}
