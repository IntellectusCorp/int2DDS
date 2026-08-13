//! User traffic logic for RTPS protocol.
//!
//! This module handles user-level data exchange including
//! writer/reader message processing and data fragmentation.

use chrono::{DateTime, Utc};
use log::{debug, trace, warn};
// use rand::Rng;
use std::collections::{BTreeSet, HashMap};
use std::num::NonZeroU32;
use std::ops::Add;
use std::time::{Duration, Instant};

use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::count_filter::should_accept_count;
use crate::rtps::common::entity_id::EntityId;
use crate::rtps::common::guid::{Guid, GuidPrefix};
use crate::rtps::common::locator::Locator;
use crate::rtps::common::parameters::ParameterList;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::common::sequence::SequenceNumber;
use crate::rtps::common::types::ChangeKind;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::endpoint::Endpoint;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::history::cache_change::{CacheChange, PresentationInfo};
use crate::rtps::entities::history::history_cache::HistoryCache;
use crate::rtps::entities::history::writer_history::WriterHistoryCache;
use crate::rtps::entities::reader::{
    FragmentInfo, Reader, StatefulReader, StatelessReader, WriterProxy,
};
use crate::rtps::entities::writer::reader_locator::ReaderLocator;
use crate::rtps::entities::writer::reader_proxy::ReaderProxy;
use crate::rtps::entities::writer::{StatefulWriter, StatelessWriter, Writer};
use crate::rtps::logic::common::{
    impl_participant_accessor, impl_unicast_thread_handler, ParticipantAccessor,
    UnicastThreadHandler,
};
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor;
use crate::rtps::messages::header::Header;
use crate::rtps::messages::message_creator::{AckNackRequest, MessageCreator};
use crate::rtps::messages::submessage_header::SubmessageHeader;
use crate::rtps::messages::submessages::ack_nack::AckNack;
use crate::rtps::messages::submessages::data::Data;
use crate::rtps::messages::submessages::data_frag::{DataFrag, FragmentBuffer};
use crate::rtps::messages::submessages::gap::Gap;
use crate::rtps::messages::submessages::heartbeat::Heartbeat;
use crate::rtps::messages::submessages::heartbeat_frag::HeartbeatFrag;
use crate::rtps::messages::submessages::nack_frag::NackFrag;
use crate::rtps::task::sending_handler::{MessageType, SendingHandler};
use crate::rtps::task::user_traffic::user_unicast_listening_task::UserUnicastListeningTask;
use crate::rtps::transport::plugin::{MessageSource, SendTarget, TransportPlugin};
use crate::rtps::{
    entities::participant::Participant, messages::message_receiver::MessageReceiver,
};
use crate::serialize::pl_cdr::InlineQosParameters;
use crate::utils::timer::{timer_handler::TimerHandler, timer_id::TimerId};
use dashmap::DashMap;
use mio::Waker;

use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};

/// Delay before the first NACK_FRAG, so a burst of DATA_FRAG is answered once rather than
/// per fragment.
const NACK_FRAG_SUPPRESSION: Duration = Duration::from_millis(80);

/// Delay before re-asking when the request produced nothing.
///
/// Only silence gets here: every retransmitted fragment carries a heartbeat that re-arms the
/// suppression timer, so a healthy repair never waits this long. Its job is to bound the case
/// where the NACK_FRAG itself, or every fragment answering it, was lost -- which otherwise waits
/// for the writer's periodic heartbeat (2 s by default).
const NACK_FRAG_RETRY: Duration = Duration::from_millis(200);

/// How many in-progress fragmented samples a participant holds before the oldest are evicted.
/// One entry per (writer, reader, sample), so several readers of one topic each take a slot.
const FRAGMENT_BUFFER_LIMIT: usize = 128;

/// How many times a request re-asks before giving the job back to the periodic heartbeat.
///
/// Bounded on purpose. A writer that has gone away can leave an incomplete sample behind whose
/// proxy is still matched; without a budget this would re-ask five times a second forever, which
/// the one-shot timer it replaces never did.
const NACK_FRAG_MAX_RETRIES: u32 = 10;

/// Everything a deferred NACK_FRAG needs to build itself, so it can be re-armed without the
/// caller's stack.
#[derive(Clone)]
struct NackFragRequest {
    writer_proxies: Arc<Mutex<Vec<WriterProxy>>>,
    transport: Arc<dyn TransportPlugin>,
    participant: Arc<Participant>,
    reader_guid: Guid,
    remote_writer_guid: Guid,
    /// The change the fragment numbers belong to; also keys the timer.
    incomplete_sn: SequenceNumber,
    /// Re-asks left before the periodic heartbeat takes over again.
    retries_left: u32,
}

impl NackFragRequest {
    fn timer_id(&self) -> TimerId {
        TimerId::NackFrag {
            reader_entity_id: self.reader_guid.entity_id(),
            remote_writer_guid: self.remote_writer_guid,
            sequence_number: self.incomplete_sn,
        }
    }

    /// Sends the request. Returns whether fragments are still outstanding, i.e. whether a retry
    /// is owed.
    fn fire(&self) -> bool {
        let Ok(mut proxies) = self.writer_proxies.lock() else {
            warn!("Failed to acquire writer_proxies lock");
            return false;
        };
        let Some(proxy) =
            proxies.iter_mut().find(|proxy| proxy.remote_writer_guid() == self.remote_writer_guid)
        else {
            return false;
        };

        // Recomputed rather than carried: fragments may have arrived since this was scheduled,
        // and a retry that re-requests them would pull the whole sample down again.
        let mut missing_fragments = proxy.get_ascending_missing_fn_list(self.incomplete_sn);
        if missing_fragments.is_empty() {
            return false;
        }

        // The incomplete fragmented sample is recovered by NACK_FRAG alone. Bundling an ACKNACK
        // that also nacks this sample makes the writer resend it whole from fragment 1. All
        // missing fragments are requested at once as 256-wide windows, not one window per round.
        let first_nackfrag_count = proxy.nackfrag_count().wrapping_add(1);
        let (messages, consumed_count) = match MessageCreator::create_multiple_nackfrag_msgs(
            self.participant.guid(),
            proxy.remote_writer_guid(),
            self.reader_guid.entity_id(),
            proxy.remote_writer_guid().entity_id(),
            self.incomplete_sn,
            &mut missing_fragments,
            first_nackfrag_count,
        ) {
            Ok(result) => result,
            Err(e) => {
                warn!("[UserLogic] Failed to create NACK_FRAG: {:?}", e);
                return true;
            }
        };

        for _ in 0..consumed_count {
            proxy.increase_nackfrag_count();
        }

        // Same SHM > TCP > UDP priority filter as send_rtps_message_to_locators, with can_handle
        // guarding the local-side reachability.
        let locs: Vec<&Locator> = proxy.unicast_locator_list().iter().collect();
        let try_kind = |is_kind: fn(&Locator) -> bool| -> Option<Vec<&Locator>> {
            let v: Vec<&Locator> = locs
                .iter()
                .copied()
                .filter(|l| is_kind(l) && self.transport.can_handle(l))
                .collect();
            (!v.is_empty()).then_some(v)
        };
        let chosen: Vec<&Locator> = try_kind(Locator::is_shm)
            .or_else(|| try_kind(Locator::is_tcp))
            .or_else(|| try_kind(Locator::is_udp))
            .unwrap_or(locs);

        for buffer in &messages {
            for locator in &chosen {
                match self.transport.send(buffer, &SendTarget::UserData(*locator)) {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                        warn!("[UserLogic] {} locator found but no {} sender available", e, e);
                    }
                    Err(e) => warn!("[UserLogic] Failed to send NACK_FRAG: {:?}", e),
                }
            }
        }

        true
    }
}

/// Arms `request` to fire after `delay`, re-arming itself at [`NACK_FRAG_RETRY`] for as long as
/// fragments stay missing.
///
/// A named function rather than a self-referencing closure: the callback cannot clone itself, and
/// a non-repeating timer is dropped after it triggers, so re-adding the same `TimerId` from
/// inside the callback lands on the next tick against an empty slot.
fn schedule_nackfrag(request: NackFragRequest, delay: Duration) {
    let timer_handler = TimerHandler::get_instance(request.participant.guid().prefix());
    let Ok(handler) = timer_handler.lock() else {
        warn!("Failed to acquire timer handler lock for NACK_FRAG");
        return;
    };
    let timer_id = request.timer_id();
    handler.add_timer(timer_id, delay, false, move || {
        if request.fire() && request.retries_left > 0 {
            let mut next = request.clone();
            next.retries_left -= 1;
            schedule_nackfrag(next, NACK_FRAG_RETRY);
        }
    });
}

/// Maximal runs of consecutive fragment numbers, as `(start, count)`.
///
/// A repair request is sparse, but one DATA_FRAG submessage carries only consecutive
/// fragments, so each run is packed on its own. Numbers outside `1..=total` are dropped.
fn contiguous_fragment_runs(fragments: &BTreeSet<u32>, total: u32) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &fragment_num in fragments.iter().filter(|&&n| n >= 1 && n <= total) {
        match runs.last_mut() {
            Some((start, count)) if *start + *count == fragment_num => *count += 1,
            _ => runs.push((fragment_num, 1)),
        }
    }
    runs
}

/// One entry per DATA_FRAG submessage needed to send `runs`, as `(fragment_num, count,
/// is_last)`, packing up to `frags_per_msg` fragments per submessage.
///
/// `is_last` is `true` for exactly one entry: the final chunk of the final run, i.e. the
/// submessage carrying the highest requested fragment. That is the only one that should
/// piggyback a heartbeat, so the repair burst is advertised only once it is fully sent.
///
/// `frags_per_msg` is `NonZeroU32` so a zero budget cannot make `offset` stall: the loop
/// below would spin forever, while `writer_cache` and `reader_proxies` are both held.
fn fragment_send_plan(runs: &[(u32, u32)], frags_per_msg: NonZeroU32) -> Vec<(u32, u32, bool)> {
    let last_run_index = runs.len().saturating_sub(1);
    let mut plan = Vec::new();
    for (run_index, &(run_start, run_len)) in runs.iter().enumerate() {
        let mut offset = 0;
        while offset < run_len {
            let count = std::cmp::min(frags_per_msg.get(), run_len - offset);
            let is_last = run_index == last_run_index && offset + count == run_len;
            plan.push((run_start + offset, count, is_last));
            offset += count;
        }
    }
    plan
}

/// `exclude` must hold every key this call is about to write -- one DATA_FRAG fans out to one
/// buffer per matched reader -- or an omitted key can become its own eviction victim.
fn select_eviction_victims(
    buffers: &DashMap<(Guid, EntityId, SequenceNumber), FragmentBuffer>,
    max_size: usize,
    exclude: &[(Guid, EntityId, SequenceNumber)],
) -> Vec<(Guid, EntityId, SequenceNumber)> {
    if buffers.len() <= max_size {
        return Vec::new();
    }
    let buffers_to_remove = buffers.len() - max_size;

    let mut incomplete: Vec<_> = buffers
        .iter()
        .filter(|entry| !exclude.contains(entry.key()) && !entry.value().all_fragments_received())
        .map(|entry| (*entry.key(), entry.value().last_updated))
        .collect();
    incomplete.sort_by_key(|(_, last_updated)| *last_updated);

    let mut victims: Vec<_> =
        incomplete.into_iter().take(buffers_to_remove).map(|(key, _)| key).collect();

    if victims.len() < buffers_to_remove {
        let mut complete: Vec<_> = buffers
            .iter()
            .filter(|entry| {
                !exclude.contains(entry.key()) && entry.value().all_fragments_received()
            })
            .map(|entry| (*entry.key(), entry.value().created_at))
            .collect();
        complete.sort_by_key(|(_, created_at)| *created_at);

        let remaining = buffers_to_remove - victims.len();
        victims.extend(complete.into_iter().take(remaining).map(|(key, _)| key));
    }

    victims
}

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct UserLogic {
    participant: Weak<Participant>,
    transport: Arc<dyn TransportPlugin>,
    /// Keyed by the reader the DATA_FRAG was addressed to as well: the writer sends one copy
    /// per reader, and merging those streams completes a buffer only one of them is given.
    fragment_buffers: Arc<DashMap<(Guid, EntityId, SequenceNumber), FragmentBuffer>>,
    unicast_listening_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    unicast_listening_waker: Arc<OnceLock<Arc<Waker>>>,
}

// Initialization
impl UserLogic {
    pub(crate) fn new(participant: Arc<Participant>, transport: Arc<dyn TransportPlugin>) -> Self {
        Self {
            participant: Arc::downgrade(&participant),
            transport,
            fragment_buffers: Arc::new(DashMap::new()),
            unicast_listening_handle: Arc::new(Mutex::new(None)),
            unicast_listening_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn wake_unicast_listening_thread(&self) {
        if let Some(waker) = self.unicast_listening_waker.get() {
            let _ = waker.wake();
        }
    }

    #[allow(unused_variables)]
    pub(crate) fn start_user_traffic(
        &self,
        domain_id: DomainId,
        user_unicast_source: Option<MessageSource>,
    ) -> RtpsResult<()> {
        if let Some(unicast_source) = user_unicast_source {
            let participant = self.get_upgraded_participant()?;

            let mut user_unicast_listening_task =
                UserUnicastListeningTask::new(participant.clone());
            user_unicast_listening_task.set_shutdown_waker(self.unicast_listening_waker.clone());
            let participant_guid = participant.guid();

            let unicast_handle = thread::Builder::new()
                .name("user_traffic_unicast_listening".to_string())
                .spawn(move || {
                    {
                        use crate::rtps::task::thread_monitor::ThreadMonitor;
                        ThreadMonitor::register_current_thread_name_with_guid_prefix(
                            "user_traffic_unicast_listening",
                            participant_guid.prefix(),
                        );
                    }

                    let _ = user_unicast_listening_task.unicast_listening(unicast_source);
                    {
                        use crate::rtps::task::thread_monitor::ThreadMonitor;
                        ThreadMonitor::remove_map_guard();
                    }
                    debug!("user unicast listening thread finished");
                })
                .expect("Failed to create user unicast listening thread");

            if let Ok(mut handle_guard) = self.unicast_listening_handle.lock() {
                *handle_guard = Some(unicast_handle);
            }
        }

        Ok(())
    }
}

// Writer Message Sending (Local Writer -> Remote Reader)
impl UserLogic {
    pub(crate) fn send_unsent_changes(
        &self,
        writer: &(dyn Writer + Send + Sync),
        cache: &WriterHistoryCache,
    ) -> RtpsResult<()> {
        if let Some(writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
            self.send_unsent_changes_of_stateful_writer(writer, cache)?;
        } else if let Some(writer) = writer.as_any().downcast_ref::<StatelessWriter>() {
            self.send_unsent_changes_of_stateless_writer(writer, cache)?;
        } else {
            return Err(RtpsError::new(RtpsErrorCode::DowncastError, "Failed to downcast writer"));
        }
        Ok(())
    }

    pub(crate) fn send_requested_changes(
        &self,
        writer_entity_id: EntityId,
        remote_reader_guid: Guid,
    ) -> RtpsResult<()> {
        let writer = self.find_stateful_writer(writer_entity_id)?;
        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let participant = self.get_upgraded_participant()?;

        enum RequestedChangeType {
            Gap(SequenceNumber),
            Data(Arc<CacheChange>),
        }

        // Pre-collect and store the information required for wire-level transmission
        // to minimize the duration of `writer_cache` and `reader_proxies`.
        struct SendPlan {
            locators: Vec<Locator>,
            group_id: EntityId,
            reliable: bool,
            requested_change_types: Vec<RequestedChangeType>,
        }

        // LOCK ORDER: acquires `writer_cache` first, then `reader_proxies`.
        let plan = {
            let writer_cache = stateful_writer.writer_cache();
            let cache_guard = writer_cache.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire writer cache lock: {}", e),
                )
            })?;

            let reader_proxies = stateful_writer.reader_proxies();
            let mut reader_proxies_guard = reader_proxies.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire reader_proxies lock: {}", e),
                )
            })?;

            let reader_proxy = reader_proxies_guard
                .iter_mut()
                .find(|rp| rp.remote_reader_guid() == remote_reader_guid)
                .ok_or_else(|| {
                    RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, "ReaderProxy not found")
                })?;

            let mut requested_change_types: Vec<RequestedChangeType> = Vec::new();

            // Answer the changes the ReaderProxy requested via NACK
            for requested_change_sn in reader_proxy.requested_changes().iter() {
                // For volatile readers, sequence numbers <= last_irrelevant_sn should be responded with GAP
                if *requested_change_sn <= reader_proxy.last_irrelevant_sn() {
                    requested_change_types.push(RequestedChangeType::Gap(*requested_change_sn));
                    continue;
                }

                if let Some(a_change) = cache_guard.get_change(*requested_change_sn) {
                    // ACK may have been received in the meantime, so check first
                    if reader_proxy.max_acked_sn() >= *requested_change_sn {
                        continue;
                    }

                    if reader_proxy.is_change_in_gapped_coherent_set(&a_change) {
                        requested_change_types.push(RequestedChangeType::Gap(*requested_change_sn));
                        continue;
                    }

                    // TODO: Filter message according to Reader Proxy's request (time based filter, content filtered topic, etc)
                    requested_change_types.push(RequestedChangeType::Data(a_change));
                } else {
                    requested_change_types.push(RequestedChangeType::Gap(*requested_change_sn));

                    debug!(
                        "[UserLogic] [AckNack] CacheChange not found for sequence number: {}",
                        requested_change_sn
                    );
                }
            }

            reader_proxy.empty_requested_changes();

            SendPlan {
                locators: reader_proxy.unicast_locator_list().to_vec(),
                group_id: reader_proxy.remote_group_entity_id(),
                reliable: reader_proxy.is_reliable(),
                requested_change_types,
            }
        };

        if plan.requested_change_types.is_empty() {
            return Ok(());
        }

        // Serialize and send outside both locks, reusing one buffer.
        let SendPlan { locators, group_id, reliable, requested_change_types } = plan;
        let piggyback = reliable && !stateful_writer.disable_piggyback_heartbeat();

        let mut send_buffer = participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .acquire();
        let mut gap_list: Vec<SequenceNumber> = Vec::new();

        for change_type in requested_change_types {
            let a_change = match change_type {
                RequestedChangeType::Gap(sn) => {
                    gap_list.push(sn);
                    continue;
                }
                RequestedChangeType::Data(a_change) => a_change,
            };

            if a_change.is_fragmented() {
                let requested_change_sn = a_change.sequence_number();
                debug!("[UserLogic] [RequestedChanges] Fragmented change: {}", requested_change_sn);

                let timestamp = Utc::now();

                // Read here, not once per call: most requests carry no fragmented change.
                let max_message_size = crate::common::env::get_max_message_size();
                let total_fragments = a_change.total_fragments();
                let frags_per_msg: NonZeroU32 =
                    a_change.fragments_per_submessage(max_message_size).into();

                // A single contiguous run: the ACKNACK path always resends everything.
                for (fragment_num, count, is_last) in
                    fragment_send_plan(&[(1, total_fragments)], frags_per_msg)
                {
                    let Some(fragment_data) =
                        a_change.get_fragment_range_data(fragment_num, count as u16)
                    else {
                        continue;
                    };

                    // Piggyback one heartbeat on the final fragment so the sample is advertised
                    // only after the whole burst is on the wire.
                    let heartbeat_info = (piggyback && is_last).then(|| {
                        (
                            stateful_writer.heartbeat_count(),
                            requested_change_sn,
                            requested_change_sn,
                            false, // final_flag
                            false, // liveliness_flag = false for retransmission
                        )
                    });

                    if MessageCreator::create_data_frag_msg(
                        &a_change,
                        remote_reader_guid,
                        group_id,
                        writer.endpoint_id(),
                        fragment_num,
                        count as u16,
                        a_change.fragment_size() as u16,
                        a_change.data_value().len() as u32,
                        fragment_data,
                        heartbeat_info,
                        timestamp,
                        &mut send_buffer,
                    )
                    .is_ok()
                    {
                        match self.send_rtps_message_to_locators(locators.iter(), &send_buffer) {
                            Ok(_) => {
                                if heartbeat_info.is_some() {
                                    stateful_writer.increase_heartbeat_count();
                                }
                            }
                            Err(e) => {
                                warn!("Failed to send DATA_FRAG for requested change: {:?}", e)
                            }
                        }
                    }
                }
            } else if MessageCreator::create_data_msg(
                &a_change,
                remote_reader_guid,
                group_id,
                writer.endpoint_id(),
                None, // No heartbeat
                true, // Use inline QoS (default)
                None, // No content filter for retransmission (TODO: consider adding filter)
                &mut send_buffer,
            )
            .is_ok()
            {
                if let Err(e) = self.send_rtps_message_to_locators(locators.iter(), &send_buffer) {
                    warn!("Failed to send DATA for requested change: {:?}", e);
                }
            }
        }

        participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .release(send_buffer);

        if !gap_list.is_empty() {
            let buffer_list = MessageCreator::create_multiple_gap_msgs(
                writer.guid(),
                remote_reader_guid,
                group_id,
                writer.endpoint_id(),
                &mut gap_list,
            )
            .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

            for buf in buffer_list {
                if let Err(e) = self.send_rtps_message_to_locators(locators.iter(), buf.as_slice())
                {
                    warn!("Failed to send GAP: {:?}", e);
                }
            }
        }

        Ok(())
    }

    pub(crate) fn send_requested_fragments(
        &self,
        writer_entity_id: EntityId,
        remote_reader_guid: Guid,
    ) -> RtpsResult<()> {
        let writer = self.find_stateful_writer(writer_entity_id)?;
        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // Read before taking any lock below: bounds how many fragments ride in one DATA_FRAG.
        let max_message_size = crate::common::env::get_max_message_size();

        // LOCK ORDER: acquires `writer_cache` first, then `reader_proxies`.
        let writer_cache = stateful_writer.writer_cache();
        let history_cache_guard = writer_cache.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire history cache lock")
        })?;

        let reader_proxies = stateful_writer.reader_proxies();
        let mut reader_proxies_guard = reader_proxies.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire reader_proxies lock")
        })?;

        let reader_proxy = reader_proxies_guard
            .iter_mut()
            .find(|proxy| proxy.remote_reader_guid() == remote_reader_guid)
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::InvalidEntityKind, "Reader proxy not found")
            })?;

        let requested_fragments_by_sn = reader_proxy.take_requested_fragments();

        if requested_fragments_by_sn.is_empty() {
            return Ok(());
        }

        let last_sn = history_cache_guard.get_seq_num_max().ok_or_else(|| {
            RtpsError::new(
                RtpsErrorCode::DataNotSet,
                "Writer cache should not be empty while sending DATA_FRAG",
            )
        })?;

        let piggyback =
            reader_proxy.is_reliable() && !stateful_writer.disable_piggyback_heartbeat();

        let timestamp = Utc::now();
        let participant = self.get_upgraded_participant()?;

        let mut send_buffer = participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .acquire();

        for (writer_sn, requested_fragments) in requested_fragments_by_sn {
            // ACK may have been received in the meantime, so check first
            if reader_proxy.max_acked_sn() >= writer_sn {
                continue;
            }

            let Some(change) = history_cache_guard.get_change(writer_sn) else {
                debug!(
                    "[UserLogic] [NackFrag] Change removed before resend SN={}",
                    writer_sn.to_i64()
                );
                continue;
            };

            if !change.is_fragmented() {
                continue;
            }

            let total_frags = change.total_fragments();
            let runs = contiguous_fragment_runs(&requested_fragments, total_frags);
            let frags_per_msg: NonZeroU32 =
                change.fragments_per_submessage(max_message_size).into();

            for (fragment_num, count, is_last) in fragment_send_plan(&runs, frags_per_msg) {
                // One heartbeat per burst, on the submessage carrying the highest requested fragment.
                let heartbeat_info = (piggyback && is_last)
                    .then(|| (stateful_writer.heartbeat_count(), writer_sn, last_sn, false, false));

                if self.send_data_frag_to_reader_proxy(
                    &change,
                    reader_proxy,
                    writer_entity_id,
                    fragment_num,
                    count as u16,
                    heartbeat_info,
                    timestamp,
                    &mut send_buffer,
                ) && heartbeat_info.is_some()
                {
                    stateful_writer.increase_heartbeat_count();
                }
            }
        }

        participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .release(send_buffer);

        Ok(())
    }

    fn send_unsent_changes_of_stateful_writer(
        &self,
        writer: &StatefulWriter,
        history_cache: &WriterHistoryCache,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        enum UnsentChangeType {
            Gap(SequenceNumber, SequenceNumber),
            Data(SequenceNumber),
        }

        // Pre-collect and store the information required for wire-level transmission
        // to minimize the duration of `reader_proxies`.
        struct SendPlan {
            locators: Vec<Locator>,
            group_id: EntityId,
            reliable: bool,
            piggyback: bool,
            content_filter:
                Option<crate::rtps::builtin::data::content_filtered_topic::ContentFilterInfo>,
            unsent_change_types: Vec<UnsentChangeType>,
        }

        let first_sn = history_cache.get_seq_num_min();
        let last_sn = history_cache.get_seq_num_max();

        // Read once per call: bounds how many fragments ride in one DATA_FRAG.
        let max_message_size = crate::common::env::get_max_message_size();

        let mut send_buffer = participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .acquire();

        // Plan every reader in ONE pass under a single lock.
        //
        // Batching across readers needs every plan in hand before anything goes out,
        // so the plans are collected first. Nothing is sent while the lock is held -
        // the property the pre-collect exists for - and the per-reader re-lock plus
        // linear `find` (O(N^2) in fan-out) disappears with it.
        let plans: Vec<(Guid, SendPlan)> = {
            let reader_proxies_lock = writer.reader_proxies();
            let mut reader_proxies = reader_proxies_lock.lock().map_err(|_| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    "[Data] Failed to acquire reader proxies lock",
                )
            })?;

            let mut plans: Vec<(Guid, SendPlan)> = Vec::with_capacity(reader_proxies.len());
            for reader_proxy in reader_proxies.iter_mut() {
                let reader_guid = reader_proxy.remote_reader_guid();
                let reliable = reader_proxy.is_reliable();
                let piggyback = !writer.disable_piggyback_heartbeat();
                let mut unsent_change_types: Vec<UnsentChangeType> = Vec::new();

                loop {
                    let a_change_seq_num = reader_proxy.next_unsent_change(history_cache);
                    if a_change_seq_num == SequenceNumber::UNKNOWN {
                        break;
                    }

                    // RTPS 2.5 - 8.4.9.1.4 GAP the hole below the next change (reliable only).
                    if reader_proxy.highest_sent_change_sn() != SequenceNumber::UNKNOWN
                        && a_change_seq_num > reader_proxy.highest_sent_change_sn() + 1
                        && reliable
                    {
                        unsent_change_types.push(UnsentChangeType::Gap(
                            reader_proxy.highest_sent_change_sn() + 1,
                            SequenceNumber::from_i64(a_change_seq_num.to_i64() - 1),
                        ));
                    }

                    if let Some(a_change) = history_cache.get_change(a_change_seq_num) {
                        // A change in a coherent set whose first sequence number was GAPped can never
                        // complete on this reader; answer with GAP so no DATA references a gapped set start.
                        if reader_proxy.is_change_in_gapped_coherent_set(&a_change) {
                            if reliable {
                                unsent_change_types.push(UnsentChangeType::Gap(
                                    a_change_seq_num,
                                    a_change_seq_num,
                                ));
                            }
                            reader_proxy.extend_last_irrelevant_sn(a_change_seq_num);
                            reader_proxy.set_highest_sent_change_sn(a_change_seq_num);
                            continue;
                        }

                        unsent_change_types.push(UnsentChangeType::Data(a_change_seq_num));
                    } else {
                        warn!(
                            "[Data] Failed to find change in history cache for seq_num: {}",
                            a_change_seq_num
                        );
                    }

                    reader_proxy.set_highest_sent_change_sn(a_change_seq_num);
                }

                plans.push((
                    reader_guid,
                    SendPlan {
                        locators: reader_proxy.unicast_locator_list().to_vec(),
                        group_id: reader_proxy.remote_group_entity_id(),
                        reliable,
                        piggyback,
                        content_filter: reader_proxy.generate_content_filter_info(),
                        unsent_change_types,
                    },
                ));
            }
            plans
        };

        // Readers whose DATA reached the wire.
        let mut readers_with_sent_data: Vec<Guid> = Vec::new();

        //---------------------------------------------------------------------
        // Batched path: one datagram per destination participant.
        //
        // INFO_DST addresses a participant and reader id UNKNOWN reaches every matched
        // reader behind it, so one DATA (or DATA_FRAG burst) plus one piggyback
        // HEARTBEAT serves the whole participant. On this topology that turns 12,015
        // sends per publish cycle into 4,705; the hub topic writer goes from 123
        // datagrams to 45.
        //
        // Only single-change plans are batched. GAPs and multi-change catch-up keep
        // the original per-reader path below, so that message shape is untouched.
        //---------------------------------------------------------------------
        let mut deferred: Vec<(Guid, SendPlan)> = Vec::with_capacity(plans.len());
        // (destination prefix, sequence number, locators) -> readers
        let mut groups: Vec<((GuidPrefix, SequenceNumber, Vec<Locator>), Vec<(Guid, SendPlan)>)> =
            Vec::new();

        for (reader_guid, plan) in plans {
            let single_data = match plan.unsent_change_types.as_slice() {
                [UnsentChangeType::Data(sn)] => Some(*sn),
                _ => None,
            };

            match single_data {
                Some(sn) => {
                    let key = (reader_guid.prefix(), sn, plan.locators.clone());
                    match groups.iter_mut().find(|(k, _)| *k == key) {
                        Some((_, members)) => members.push((reader_guid, plan)),
                        None => groups.push((key, vec![(reader_guid, plan)])),
                    }
                }
                None => deferred.push((reader_guid, plan)),
            }
        }

        for ((dst_prefix, sn, locators), members) in groups {
            // A lone reader gains nothing from the batched builder; send it on the
            // original path so single-reader traffic keeps the exact same bytes.
            if members.len() < 2 {
                deferred.extend(members);
                continue;
            }

            let Some(a_change) = history_cache.get_change(sn) else {
                deferred.extend(members);
                continue;
            };
            let (Some(first), Some(last)) = (first_sn, last_sn) else {
                return Err(RtpsError::new(
                    RtpsErrorCode::DataNotSet,
                    "Writer cache should not be empty while sending DATA",
                ));
            };

            // One heartbeat count for the whole batch. Each reader still sees a
            // non-decreasing series, which is all HEARTBEAT.count requires.
            let heartbeat_count = writer.heartbeat_count();

            if a_change.is_fragmented() {
                // The fragment burst goes out once per participant: reader id UNKNOWN
                // reaches every matched reader behind dst_prefix.
                let is_piggyback_wanted =
                    members.iter().any(|(_, plan)| plan.reliable && plan.piggyback);
                let timestamp = Utc::now();
                let mut is_any_fragment_sent = false;

                debug!(
                    "[Data] Batched DATA_FRAG sn={} to {} readers behind one participant",
                    sn.to_i64(),
                    members.len()
                );

                // Pack fragments per datagram: one per datagram costs the receiver a
                // socket-buffer charge each, which is where small fragments lose data.
                let total_fragments = a_change.total_fragments();
                let frags_per_msg: NonZeroU32 =
                    a_change.fragments_per_submessage(max_message_size).into();

                for (fragment_num, count, is_last) in
                    fragment_send_plan(&[(1, total_fragments)], frags_per_msg)
                {
                    let Some(fragment_data) =
                        a_change.get_fragment_range_data(fragment_num, count as u16)
                    else {
                        continue;
                    };

                    // Piggyback one heartbeat on the final fragment so the sample is
                    // advertised only after the whole burst is on the wire.
                    let heartbeat_info = (is_piggyback_wanted && is_last)
                        .then(|| (heartbeat_count, first, last, false, false));

                    if MessageCreator::create_data_frag_msg(
                        &a_change,
                        Guid::new(dst_prefix, EntityId::UNKNOWN),
                        EntityId::UNKNOWN,
                        writer.endpoint_id(),
                        fragment_num,
                        count as u16,
                        a_change.fragment_size() as u16,
                        a_change.data_value().len() as u32,
                        fragment_data,
                        heartbeat_info,
                        timestamp,
                        &mut send_buffer,
                    )
                    .is_ok()
                    {
                        if self.send_rtps_message_to_locators(locators.iter(), &send_buffer).is_ok()
                        {
                            is_any_fragment_sent = true;
                            if heartbeat_info.is_some() {
                                writer.increase_heartbeat_count();
                            }
                        }
                    }
                }

                if is_any_fragment_sent {
                    for (reader_guid, plan) in &members {
                        if plan.piggyback {
                            readers_with_sent_data.push(*reader_guid);
                        }
                    }
                }
                continue;
            }

            let is_piggyback_wanted =
                members.iter().any(|(_, plan)| plan.reliable && plan.piggyback);
            let heartbeat_info =
                is_piggyback_wanted.then(|| (heartbeat_count, first, last, false, false));

            debug!(
                "[Data] Batched DATA sn={} to {} readers behind one participant",
                sn.to_i64(),
                members.len()
            );

            if let Err(e) = MessageCreator::create_data_msg(
                &a_change,
                Guid::new(dst_prefix, EntityId::UNKNOWN),
                EntityId::UNKNOWN,
                writer.endpoint_id(),
                heartbeat_info,
                true, // Use inline QoS (default)
                None,
                &mut send_buffer,
            ) {
                warn!("[Data] Failed to build batched DATA: {:?}", e);
                deferred.extend(members);
                continue;
            }

            if self.send_rtps_message_to_locators(locators.iter(), &send_buffer).is_ok() {
                if heartbeat_info.is_some() {
                    writer.increase_heartbeat_count();
                }
                for (reader_guid, plan) in &members {
                    if plan.piggyback {
                        readers_with_sent_data.push(*reader_guid);
                    }
                }
            }
        }

        //---------------------------------------------------------------------
        // Original per-reader path for everything the batcher left behind.
        //---------------------------------------------------------------------
        for (reader_guid, plan) in deferred {
            if plan.unsent_change_types.is_empty() {
                continue;
            }

            // Serialize and send each message outside reader_proxies.
            let SendPlan {
                locators,
                group_id,
                reliable,
                piggyback,
                content_filter,
                unsent_change_types,
            } = plan;
            let mut data_sent = false;
            for change_type in unsent_change_types {
                match change_type {
                    UnsentChangeType::Gap(start, end) => {
                        match MessageCreator::create_gap_msg_consecutive(
                            participant.guid(),
                            reader_guid,
                            reader_guid.entity_id(),
                            writer.endpoint_id(),
                            start,
                            end,
                        ) {
                            Ok(buffer) => {
                                if let Err(e) =
                                    self.send_rtps_message_to_locators(locators.iter(), &buffer[..])
                                {
                                    warn!("[Data] Failed to send GAP: {:?}", e);
                                }
                            }
                            Err(e) => warn!("[Data] Failed to build GAP: {:?}", e),
                        }
                    }
                    UnsentChangeType::Data(a_change_seq_num) => {
                        let Some(a_change) = history_cache.get_change(a_change_seq_num) else {
                            warn!(
                                "[Data] Failed to find change in history cache for seq_num: {}",
                                a_change_seq_num
                            );
                            continue;
                        };

                        let (first_sn, last_sn) = match (first_sn, last_sn) {
                            (Some(first), Some(last)) => (first, last),
                            _ => {
                                return Err(RtpsError::new(
                                    RtpsErrorCode::DataNotSet,
                                    "Writer cache should not be empty while sending DATA",
                                ))
                            }
                        };

                        if a_change.is_fragmented() {
                            let timestamp = Utc::now();
                            let total_fragments = a_change.total_fragments();
                            let frags_per_msg: NonZeroU32 =
                                a_change.fragments_per_submessage(max_message_size).into();

                            for (fragment_num, count, is_last) in
                                fragment_send_plan(&[(1, total_fragments)], frags_per_msg)
                            {
                                let Some(fragment_data) =
                                    a_change.get_fragment_range_data(fragment_num, count as u16)
                                else {
                                    continue;
                                };

                                // Piggyback one heartbeat on the final fragment so the sample is
                                // advertised only after the whole burst is on the wire.
                                let heartbeat_info = if reliable && piggyback && is_last {
                                    Some((
                                        writer.heartbeat_count(),
                                        first_sn,
                                        last_sn,
                                        false,
                                        false,
                                    ))
                                } else {
                                    None
                                };

                                if MessageCreator::create_data_frag_msg(
                                    &a_change,
                                    reader_guid,
                                    group_id,
                                    writer.endpoint_id(),
                                    fragment_num,
                                    count as u16,
                                    a_change.fragment_size() as u16,
                                    a_change.data_value().len() as u32,
                                    fragment_data,
                                    heartbeat_info,
                                    timestamp,
                                    &mut send_buffer,
                                )
                                .is_ok()
                                {
                                    if self
                                        .send_rtps_message_to_locators(
                                            locators.iter(),
                                            &send_buffer,
                                        )
                                        .is_ok()
                                    {
                                        data_sent = true;
                                        if heartbeat_info.is_some() {
                                            writer.increase_heartbeat_count();
                                        }
                                    }
                                }
                            }
                        } else {
                            let heartbeat_info = if reliable && piggyback {
                                Some((writer.heartbeat_count(), first_sn, last_sn, false, false))
                            } else {
                                None
                            };

                            MessageCreator::create_data_msg(
                                &a_change,
                                reader_guid,
                                group_id,
                                writer.endpoint_id(),
                                heartbeat_info,
                                true, // Use inline QoS (default)
                                content_filter.clone(),
                                &mut send_buffer,
                            )
                            .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

                            if self
                                .send_rtps_message_to_locators(locators.iter(), &send_buffer)
                                .is_ok()
                            {
                                data_sent = true;
                                if piggyback {
                                    writer.increase_heartbeat_count();
                                }
                            }
                        }
                    }
                }
            }

            if data_sent && piggyback {
                readers_with_sent_data.push(reader_guid);
            }
        }

        participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .release(send_buffer);

        if !readers_with_sent_data.is_empty() {
            let reader_proxies_lock = writer.reader_proxies();
            let mut reader_proxies = reader_proxies_lock.lock().map_err(|_| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    "[Data] Failed to acquire reader proxies lock",
                )
            })?;

            for reader_proxy in reader_proxies.iter_mut() {
                if !reader_proxy.is_first_hb_sent()
                    && readers_with_sent_data.contains(&reader_proxy.remote_reader_guid())
                {
                    reader_proxy.set_first_hb_sent();
                }
            }
        }

        // Periodic heartbeat timer resuming when new changes are sent
        if !writer.heartbeat_timer_running() {
            writer.register_periodic_heartbeat_timer();
        }

        Ok(())
    }

    fn send_unsent_changes_of_stateless_writer(
        &self,
        writer: &StatelessWriter,
        cache: &WriterHistoryCache,
    ) -> RtpsResult<()> {
        let reader_tasks: Vec<(ReaderLocator, Vec<Arc<CacheChange>>)> = {
            let reader_locators = writer.reader_locator();
            let reader_locators_guard = reader_locators.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire reader_locators lock: {}", e),
                )
            })?;

            let mut tasks = Vec::new();
            for reader_locator in reader_locators_guard.iter() {
                let mut changes_to_send = Vec::new();
                let mut current_sn = reader_locator.highest_sent_change_sn();

                // Collect cache changes not yet sent to the Remote Reader (Arc clone occurs)
                while let Some(change) = cache.next_change_after(current_sn) {
                    current_sn = change.sequence_number();
                    changes_to_send.push(change);
                }

                if !changes_to_send.is_empty() {
                    tasks.push((reader_locator.clone(), changes_to_send));
                }
            }
            tasks
        };

        if reader_tasks.is_empty() {
            return Ok(()); // Nothing to send
        }

        let participant = self.get_upgraded_participant()?;

        // Read once per call: bounds how many fragments ride in one DATA_FRAG.
        let max_message_size = crate::common::env::get_max_message_size();

        for (reader_locator, changes) in reader_tasks.iter() {
            for change in changes.iter() {
                // Create DATA or DATA_FRAG message
                if change.is_fragmented() {
                    let timestamp = Utc::now();

                    // Reuse a single send buffer across every fragment of this change.
                    let mut send_buffer = participant
                        .wire_buffer_pool()
                        .lock()
                        .map_err(|_| {
                            RtpsError::new(
                                RtpsErrorCode::LockError,
                                "Failed to lock wire buffer pool",
                            )
                        })?
                        .acquire();

                    // Send each fragment as DATA_FRAG submessage immediately
                    let total_fragments = change.total_fragments();
                    let frags_per_msg: NonZeroU32 =
                        change.fragments_per_submessage(max_message_size).into();

                    // A stateless writer's reader locators are not reliability-tracked, so
                    // there is no heartbeat to piggyback and the plan's `is_last` is unused.
                    for (fragment_num, count, _is_last) in
                        fragment_send_plan(&[(1, total_fragments)], frags_per_msg)
                    {
                        let Some(fragment_data) =
                            change.get_fragment_range_data(fragment_num, count as u16)
                        else {
                            continue;
                        };

                        let result = MessageCreator::create_data_frag_msg(
                            change,
                            Guid::new(reader_locator.guid_prefix(), EntityId::PARTICIPANT),
                            reader_locator.remote_entity_id(),
                            writer.endpoint_id(),
                            fragment_num,
                            count as u16,
                            change.fragment_size() as u16,
                            change.data_value().len() as u32,
                            fragment_data,
                            None,
                            timestamp,
                            &mut send_buffer,
                        );

                        if result.is_ok() {
                            // Send fragmented message immediately
                            if let Err(e) = self.send_rtps_message_to_locators(
                                &[reader_locator.locator()],
                                &send_buffer,
                            ) {
                                warn!("Failed to send DATA_FRAG message: {:?}", e);
                            }
                        }
                    }

                    participant
                        .wire_buffer_pool()
                        .lock()
                        .map_err(|_| {
                            RtpsError::new(
                                RtpsErrorCode::LockError,
                                "Failed to lock wire buffer pool",
                            )
                        })?
                        .release(send_buffer);
                } else {
                    // Send as regular DATA message
                    let mut send_buffer = participant
                        .wire_buffer_pool()
                        .lock()
                        .map_err(|_| {
                            RtpsError::new(
                                RtpsErrorCode::LockError,
                                "Failed to lock wire buffer pool",
                            )
                        })?
                        .acquire();
                    MessageCreator::create_data_msg(
                        change,
                        Guid::new(reader_locator.guid_prefix(), EntityId::PARTICIPANT),
                        reader_locator.remote_entity_id(),
                        writer.endpoint_id(),
                        None, // No heartbeat
                        true, // Use inline QoS (default)
                        None, // No content filter for stateless writer
                        &mut send_buffer,
                    )
                    .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

                    if let Err(e) = self
                        .send_rtps_message_to_locators(&[reader_locator.locator()], &send_buffer)
                    {
                        warn!("Failed to send DATA message: {:?}", e);
                        // Continue sending other messages instead of aborting
                    }
                    participant
                        .wire_buffer_pool()
                        .lock()
                        .map_err(|_| {
                            RtpsError::new(
                                RtpsErrorCode::LockError,
                                "Failed to lock wire buffer pool",
                            )
                        })?
                        .release(send_buffer);
                }
            }
        }

        {
            let reader_locators = writer.reader_locator();
            let mut reader_locators_guard = reader_locators.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire reader_locators lock for update: {}", e),
                )
            })?;

            for (task_reader, changes) in reader_tasks.iter() {
                if let Some(last_change) = changes.last() {
                    let last_sn = last_change.sequence_number();

                    // Find and update matching reader_locator
                    if let Some(reader_locator) = reader_locators_guard.iter_mut().find(|rl| {
                        rl.guid_prefix() == task_reader.guid_prefix()
                            && rl.remote_entity_id() == task_reader.remote_entity_id()
                            && rl.locator() == task_reader.locator()
                    }) {
                        reader_locator.set_highest_sent_change_sn(last_sn);
                    }
                }
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn send_data_frag_to_reader_proxy(
        &self,
        change: &CacheChange,
        reader_proxy: &ReaderProxy,
        writer_id: EntityId,
        fragment_num: u32,
        fragments_in_submessage: u16,
        heartbeat_info: Option<(u32, SequenceNumber, SequenceNumber, bool, bool)>,
        timestamp: DateTime<Utc>,
        send_buffer: &mut Vec<u8>,
    ) -> bool {
        if let Some(fragment_data) =
            change.get_fragment_range_data(fragment_num, fragments_in_submessage)
        {
            let result = MessageCreator::create_data_frag_msg(
                change,
                reader_proxy.remote_reader_guid(),
                reader_proxy.remote_group_entity_id(),
                writer_id,
                fragment_num,
                fragments_in_submessage,
                change.fragment_size() as u16,
                change.data_value().len() as u32,
                fragment_data,
                heartbeat_info,
                timestamp,
                send_buffer,
            );

            if result.is_ok() {
                return self
                    .send_rtps_message_to_locators(
                        reader_proxy.unicast_locator_list(),
                        send_buffer.as_slice(),
                    )
                    .is_ok();
            }
        }
        false
    }

    // Sending heartbeat message to all matched reader proxies of the given writer
    pub(crate) fn send_heartbeat_to_anonymous_matched_readers(
        &self,
        entity_id: EntityId,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let found_writer = participant
            .find_writer_from_entity_id(entity_id)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::RtpsEntityNotFound, None))?;

        let writer = found_writer
            .as_any()
            .downcast_ref::<StatefulWriter>()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::DowncastError, "Not a stateful writer"))?;

        // If no samples are available or all readers have acknowledged up to latest sequence number, stop heartbeat
        if writer.stop_heartbeat_if_acked_by_all()? {
            return Ok(());
        }

        let writer_cache_lock = writer.writer_cache();
        let history_cache = match writer_cache_lock.lock() {
            Ok(cache) => cache,
            Err(e) => {
                return Err(RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire writer cache lock for heartbeat: {:?}", e),
                ));
            }
        };

        let reader_proxies_lock = writer.reader_proxies();
        let reader_proxies = reader_proxies_lock.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire reader_proxies lock: {}", e),
            )
        })?;

        // Group by participant: participant_guid -> all locators
        // This ensures only one heartbeat is sent per participant
        let mut participant_locators: HashMap<GuidPrefix, Vec<Locator>> = HashMap::new();

        for reader_proxy in reader_proxies.iter() {
            if !reader_proxy.is_reliable() {
                continue;
            }
            let participant_guid_prefix = reader_proxy.remote_reader_guid().prefix();
            participant_locators
                .entry(participant_guid_prefix)
                .or_insert_with(|| reader_proxy.unicast_locator_list().to_vec());
        }

        // Send heartbeat once per participant
        for (target_participant_prefix, locators) in participant_locators.iter() {
            let highest_sn = history_cache.highest_sn();
            let buffer = MessageCreator::create_heartbeat_message(
                writer.guid().prefix(),
                *target_participant_prefix,
                writer.heartbeat_count(),
                EntityId::UNKNOWN, // This ensures all readers in the participant receive the heartbeat
                writer.endpoint_id(),
                history_cache.get_seq_num_min().unwrap_or(highest_sn + 1),
                history_cache.get_seq_num_max().unwrap_or(highest_sn),
                false,
                false,
            );

            if let Ok(buf) = buffer {
                self.send_rtps_message_to_locators(locators, &buf)?;
            }
        }

        if !participant_locators.is_empty() {
            writer.increase_heartbeat_count();
        }

        Ok(())
    }

    pub(crate) fn send_heartbeat_to_a_reader_proxy(
        &self,
        writer_entity_id: EntityId,
        remote_reader_guid: Guid,
        is_preemptive: bool,
    ) -> RtpsResult<()> {
        let writer = self.find_stateful_writer(writer_entity_id)?;
        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // LOCK ORDER: acquires `writer_cache` first, then `reader_proxies`.
        let writer_cache_lock = stateful_writer.writer_cache();
        let history_cache = writer_cache_lock.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire writer cache lock for heartbeat: {:?}", e),
            )
        })?;

        let reader_proxies_lock = stateful_writer.reader_proxies();
        let mut reader_proxies = reader_proxies_lock.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire reader_proxies lock: {}", e),
            )
        })?;

        let reader_proxy = reader_proxies
            .iter_mut()
            .find(|proxy| proxy.remote_reader_guid() == remote_reader_guid)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        if !reader_proxy.is_reliable() {
            trace!("Remote reader is not reliable, skipping heartbeat.");
            return Ok(());
        }

        if is_preemptive && reader_proxy.is_first_hb_sent() {
            trace!("Remote reader should already know about my writer's status by now, skipping preemptive heartbeat.");
            return Ok(());
        }

        self.send_heartbeat_to_a_reader_proxy_inner(
            stateful_writer,
            reader_proxy,
            is_preemptive,
            &history_cache,
        )?;

        Ok(())
    }

    fn send_heartbeat_to_a_reader_proxy_inner(
        &self,
        writer: &StatefulWriter,
        reader_proxy: &mut ReaderProxy,
        should_send_gap: bool,
        history_cache: &WriterHistoryCache,
    ) -> RtpsResult<()> {
        if !reader_proxy.is_reliable() {
            trace!("Remote reader is not reliable, skipping heartbeat.");
            return Ok(());
        }

        let highest_sn = history_cache.highest_sn();

        let buffer = MessageCreator::create_heartbeat_message(
            writer.guid().prefix(),
            reader_proxy.remote_reader_guid().prefix(),
            writer.heartbeat_count(),
            reader_proxy.remote_group_entity_id(),
            writer.endpoint_id(),
            history_cache.get_seq_num_min().unwrap_or(highest_sn + 1),
            history_cache.get_seq_num_max().unwrap_or(highest_sn),
            false,
            false,
        );

        if let Ok(buf) = buffer {
            self.send_rtps_message_to_locators(reader_proxy.unicast_locator_list(), &buf)?;
            writer.increase_heartbeat_count();
            if !reader_proxy.is_first_hb_sent() {
                reader_proxy.set_first_hb_sent();
            }
        } else {
            return Err(RtpsError::new(
                RtpsErrorCode::Io,
                "Failed to create heartbeat message for reader proxy",
            ));
        }

        if should_send_gap && !history_cache.is_empty() {
            let min_sn: SequenceNumber = history_cache
                .get_seq_num_min()
                .ok_or_else(|| RtpsError::new(RtpsErrorCode::DataNotSet, None))?;

            // For volatile readers, send GAP for irrelevant sequence numbers
            let last_irrelevant = reader_proxy.last_irrelevant_sn();
            if min_sn <= last_irrelevant {
                self.send_gap_for_range(
                    writer.guid(),
                    reader_proxy,
                    writer.endpoint_id(),
                    min_sn,
                    last_irrelevant,
                )?;
            }
        }

        Ok(())
    }

    fn send_gap_for_range(
        &self,
        local_guid: Guid,
        reader_proxy: &ReaderProxy,
        writer_entity_id: EntityId,
        gap_start: SequenceNumber,
        gap_end: SequenceNumber,
    ) -> RtpsResult<()> {
        let buffer = MessageCreator::create_gap_msg_consecutive(
            local_guid,
            reader_proxy.remote_reader_guid(),
            reader_proxy.remote_reader_guid().entity_id(),
            writer_entity_id,
            gap_start,
            gap_end,
        )
        .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

        self.send_rtps_message_to_locators(reader_proxy.unicast_locator_list(), buffer.as_slice())?;

        Ok(())
    }
}

/// A reply the sending task has been asked to make, before it is worked out.
///
/// A heartbeat schedules one of these on a `heartbeat_response_delay` timer rather than
/// replying inline, so the ones that come due together can leave together.
#[derive(Clone, Copy)]
pub(crate) struct PendingAckNack {
    pub(crate) reader_id: EntityId,
    pub(crate) remote_writer_guid: Guid,
    pub(crate) final_flag: bool,
    pub(crate) is_preemptive: bool,
}

// Reader ACKNACK Sending (Local Reader -> Remote Writer)
impl UserLogic {
    /// Send the queued ACKNACKs, one message per remote participant.
    ///
    /// The sending task drains its whole queue at once, so replies that came due in the
    /// same window arrive here together. INFO_DST names a participant, so all the
    /// replies addressed to one participant leave in a single datagram - the return path
    /// of the batched DATA message that triggered them.
    pub(crate) fn send_acknacks(&self, pending: &[PendingAckNack]) -> RtpsResult<()> {
        // (destination prefix, locators) -> replies sharing one message
        let mut groups: Vec<((GuidPrefix, Vec<Locator>), Vec<AckNackRequest>)> = Vec::new();

        for reply in pending {
            match self.prepare_acknack(reply) {
                // Preparing a reply bumps the proxy's acknack count, so a prepared reply
                // has to go out - it is grouped, never dropped.
                Ok(Some((request, locators))) => {
                    let key = (reply.remote_writer_guid.prefix(), locators);
                    match groups.iter_mut().find(|(existing, _)| *existing == key) {
                        Some((_, requests)) => requests.push(request),
                        None => groups.push((key, vec![request])),
                    }
                }
                Ok(None) => {}
                // One writer going away must not silence the replies owed to the others.
                Err(e) => {
                    debug!("[AckNack] Skipping reply to {}: {:?}", reply.remote_writer_guid, e)
                }
            }
        }

        if groups.is_empty() {
            return Ok(());
        }

        let participant = self.get_upgraded_participant()?;

        for ((dst_prefix, locators), requests) in groups {
            let buffer =
                MessageCreator::create_acknack_msg_multi(participant.guid(), dst_prefix, &requests)
                    .map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::SerializationError,
                            format!("Failed to create ACKNACK message: {}", e),
                        )
                    })?;

            self.send_rtps_message_to_locators(locators.iter(), &buffer).map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::SerializationError,
                    format!("Failed to send ACKNACK message: {}", e),
                )
            })?;
        }

        Ok(())
    }

    /// Work out the reply owed to one writer, or `None` if none is owed.
    ///
    /// Kept apart from the send so several replies can share a datagram. It bumps the
    /// proxy's acknack count, so whatever it returns must reach the wire.
    fn prepare_acknack(
        &self,
        reply: &PendingAckNack,
    ) -> RtpsResult<Option<(AckNackRequest, Vec<Locator>)>> {
        let participant = self.get_upgraded_participant()?;

        let reader = participant
            .find_reader_from_entity_id(reply.reader_id)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::RtpsEntityNotFound, None))?;
        let stateful_reader = reader
            .as_any()
            .downcast_ref::<StatefulReader>()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::DowncastError, "Not a StatefulReader"))?;
        let writer_proxies = stateful_reader.writer_proxies();
        let mut writer_proxies_guard = writer_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire writer_proxies lock: {}", e),
            )
        })?;
        let writer_proxy = writer_proxies_guard
            .iter_mut()
            .find(|wp| wp.remote_writer_guid() == reply.remote_writer_guid)
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, "WriterProxy not found")
            })?;

        let (missing_changes, bitmap_base) = if reply.is_preemptive {
            // A preemptive ACKNACK only asks a writer that has told this reader nothing
            // yet to start talking; once it has, there is nothing to send.
            if writer_proxy.expected_sn() != SequenceNumber::UNKNOWN {
                return Ok(None);
            }
            (Vec::new(), SequenceNumber::from_i64(0))
        } else {
            let bitmap_base = writer_proxy.calculate_bitmap_base();
            let last_sn = writer_proxy.changes_from_writer_max();
            (writer_proxy.missing_changes_for_heartbeat(bitmap_base, last_sn), bitmap_base)
        };

        if missing_changes.is_empty() && reply.final_flag && !reply.is_preemptive {
            debug!("Skip ACKNACK because no missing changes, final flag set, not preemptive");
            return Ok(None);
        }

        writer_proxy.increase_acknack_count();

        Ok(Some((
            AckNackRequest {
                reader_entity_id: stateful_reader.guid().entity_id(),
                writer_entity_id: writer_proxy.remote_writer_guid().entity_id(),
                missing_changes,
                acknack_count: writer_proxy.acknack_count(),
                bitmap_base,
                is_preemptive: reply.is_preemptive,
            },
            writer_proxy.unicast_locator_list().to_vec(),
        )))
    }

    fn send_acknack_to_writer_proxy_inner(
        &self,
        writer_proxy: &mut WriterProxy,
        stateful_reader: &StatefulReader,
        missing_changes: Vec<SequenceNumber>,
        bitmap_base: SequenceNumber,
        final_flag: bool,
        is_preemptive: bool,
    ) -> RtpsResult<()> {
        if missing_changes.is_empty() && final_flag && !is_preemptive {
            debug!("Skip ACKNACK because no missing changes, final flag set, not preemptive");
            return Ok(());
        }

        writer_proxy.increase_acknack_count();

        let participant = self.get_upgraded_participant()?;

        let buffer = MessageCreator::create_acknack_message(
            participant.guid(),
            writer_proxy.remote_writer_guid(),
            stateful_reader.guid().entity_id(),
            writer_proxy.remote_writer_guid().entity_id(),
            missing_changes,
            writer_proxy.acknack_count(),
            bitmap_base,
            is_preemptive,
        )
        .map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::SerializationError,
                format!("Failed to create ACKNACK message: {}", e),
            )
        })?;

        self.send_rtps_message_to_locators(writer_proxy.unicast_locator_list(), &buffer).map_err(
            |e| {
                RtpsError::new(
                    RtpsErrorCode::SerializationError,
                    format!("Failed to send ACKNACK message: {}", e),
                )
            },
        )?;

        Ok(())
    }
}

// Reader Data Delivery (Received Data -> Local Reader)
impl UserLogic {
    fn deliver_change_to_reader(
        &self,
        change: CacheChange,
        reader: &dyn Reader,
        sequence_number: SequenceNumber,
        remote_guid: Guid,
        fragment_info: Option<FragmentInfo>,
    ) -> RtpsResult<()> {
        // The batch is decided under the matched-writer guard, but delivered after it is
        // released. Delivery ends in the user's `on_data_available`, and a listener calling
        // `get_matched_publications`/`get_matched_publication_data` re-locks this very mutex --
        // `std::sync::Mutex` is not reentrant, so notifying under the guard hangs the receive
        // thread with no timeout anywhere in the stack.
        //
        // Ordering does not depend on the guard. Every RTPS submessage for a participant --
        // DATA, HEARTBEAT, GAP, DATA_FRAG -- is decoded by the one
        // `user_traffic_unicast_listening` thread, so no two of them can race here, and the
        // sequence-number work below (`mark_change_received`, `expected_sn`,
        // `flush_buffered_changes`) all stays inside the guard.
        //
        // What the guard protects the proxy list against is the other threads that reach it:
        // discovery, the sending task, `liveliness_monitor`, the NACK_FRAG timer, and any user
        // thread calling `get_matched_publications`. None of them delivers samples, so releasing
        // before delivery costs no ordering.
        let mut change_to_add: Vec<CacheChange> = Vec::new();

        // Update WriterProxy state - mark as Received if data was received
        if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
            if let Ok(mut matched_writers) = stateful_reader.writer_proxies().lock() {
                let writer_proxy = match matched_writers
                    .iter_mut()
                    .find(|proxy| proxy.remote_writer_guid() == remote_guid)
                {
                    Some(proxy) => proxy,
                    None => {
                        // The writer proxy was concurrently removed (the writer was destroyed
                        // while this already-accepted sample was still being delivered
                        // intra-participant). Deliver it directly instead of dropping it.
                        drop(matched_writers);
                        return self.add_change_to_reader_cache_and_notify(reader, vec![change]);
                    }
                };

                // Mark the corresponding sequence number as Received
                writer_proxy.mark_change_received(sequence_number, fragment_info);

                // Deliver change if sequence number is in order
                if change.sequence_number() == writer_proxy.expected_sn() {
                    debug!(
                        "Delivering in-order change: {}, expected_sn: {}",
                        change.sequence_number(),
                        writer_proxy.expected_sn()
                    );

                    writer_proxy.increment_expected_sn();

                    debug!("After delivering, new expected_sn: {}", writer_proxy.expected_sn());

                    let flushed_changes = writer_proxy.flush_buffered_changes();

                    debug!(
                        "Flushed buffered changes from {:?} to {:?} after delivering in-order change.",
                        flushed_changes.first().map(|c| c.sequence_number()),
                        flushed_changes.last().map(|c| c.sequence_number())
                    );

                    change_to_add.reserve(1 + flushed_changes.len());
                    change_to_add.push(change);
                    change_to_add.extend(flushed_changes);
                }
                // Buffer out-of-order changes
                else if change.sequence_number() > writer_proxy.expected_sn() {
                    debug!(
                        "Buffering out-of-order change: {}, expected_sn: {}",
                        change.sequence_number(),
                        writer_proxy.expected_sn()
                    );
                    writer_proxy.add_buffered_change(change);
                }
            }
        } else if let Some(stateless_reader) = reader.as_any().downcast_ref::<StatelessReader>() {
            if let Ok(mut matched_writers) = stateless_reader.remote_writer_infos().lock() {
                let remote_writer_info = matched_writers
                    .iter_mut()
                    .find(|info| info.remote_writer_guid() == remote_guid)
                    .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

                // Deliver change only when sequence number is equal to or greater than expected_sn
                // 8.4.12.1.2 The Best-Effort reader checks that the sequence number associated with the change is strictly greater than
                // the highest sequence number of all changes received in the past from this RTPS Writer
                if change.sequence_number() >= remote_writer_info.expected_sn() {
                    // Advancing before delivery keeps the compare-and-set atomic under the
                    // guard. Delivery cannot report failure -- the notify helper discards
                    // per-change results and always returns Ok -- so nothing is lost by it.
                    remote_writer_info.set_expected_sn(change.sequence_number().add(1));
                    change_to_add.push(change);
                }
            }
        }

        if !change_to_add.is_empty() {
            self.add_change_to_reader_cache_and_notify(reader, change_to_add)?;
        }

        Ok(())
    }

    pub(crate) fn add_change_to_reader_cache_and_notify(
        &self,
        reader: &dyn Reader,
        changes: Vec<CacheChange>,
    ) -> RtpsResult<()> {
        for change in changes.into_iter() {
            Self::deliver_change(reader, change);
        }

        Ok(())
    }

    // Add a change to the reader cache and notify every change it makes available:
    // none when held (TIME_BASED_FILTER) or buffered, several when a coherent set closes.
    fn deliver_change(reader: &dyn Reader, change: CacheChange) {
        let reader_cache = reader.reader_cache();
        let mut res: Option<RtpsResult<Vec<Arc<CacheChange>>>> = None;
        if let Ok(mut cache_guard) = reader_cache.lock() {
            res = Some(cache_guard.add_change(change, true));
        }
        if let Some(Ok(changes)) = res {
            for change in changes {
                reader.on_change(change);
            }
        }
    }
}

// Utilities
impl UserLogic {
    pub(crate) fn on_writer_cache_change_removal(
        &self,
        entity_id: EntityId,
        sequence_number: SequenceNumber,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let writer = participant
            .find_writer_from_entity_id(entity_id)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::RtpsEntityNotFound, None))?;
        let stateful_writer = match writer.as_any().downcast_ref::<StatefulWriter>() {
            Some(writer) => writer,
            None => return Ok(()),
        };
        let reader_proxies = stateful_writer.reader_proxies();
        let mut reader_proxies_guard = reader_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire reader_proxies lock: {}", e),
            )
        })?;
        for reader_proxy in reader_proxies_guard.iter_mut() {
            reader_proxy.remove_cached_sn_on_cache_change_removal(sequence_number);
        }
        Ok(())
    }

    /// Evicts buffers over the cap and returns the keys removed, so the caller can retract the
    /// arrival record the evicted key's reader holds. `exclude` is forwarded unchanged to
    /// `select_eviction_victims`.
    fn cleanup_old_fragment_buffers(
        &self,
        max_size: usize,
        exclude: &[(Guid, EntityId, SequenceNumber)],
    ) -> Vec<(Guid, EntityId, SequenceNumber)> {
        let victims = select_eviction_victims(&self.fragment_buffers, max_size, exclude);

        let mut evicted = Vec::new();
        for key in &victims {
            let Some((_, removed_buffer)) = self.fragment_buffers.remove(key) else {
                continue;
            };
            if removed_buffer.all_fragments_received() {
                debug!(
                    "Cleaned up complete fragment buffer: writer_guid={}, reader={:?}, seq_num={}, age={:.2}s",
                    key.0,
                    key.1,
                    key.2,
                    removed_buffer.created_at.elapsed().as_secs_f64()
                );
            } else {
                debug!(
                    "Evicting incomplete fragment buffer: writer={}, reader={:?}, seq={}, fragments={}/{}, idle={:.2}s, age={:.2}s",
                    key.0,
                    key.1,
                    key.2.to_i64(),
                    removed_buffer.received_count,
                    removed_buffer.total_fragments,
                    removed_buffer.last_updated.elapsed().as_secs_f64(),
                    removed_buffer.created_at.elapsed().as_secs_f64()
                );
            }
            evicted.push(*key);
        }
        evicted
    }

    /// Send `buffer` via the highest-priority transport reachable on both
    /// sides (SHM > TCP > UDP); a peer advertising multiple transports gets
    /// a single copy. Associated fn so `&self`-less closures (e.g. the
    /// NACK_FRAG timer) can route through the same path.
    fn send_rtps_message_to_locators<'a, T>(&self, locators: T, buffer: &[u8]) -> RtpsResult<()>
    where
        T: IntoIterator<Item = &'a Locator>,
    {
        let locators: Vec<&Locator> = locators.into_iter().collect();

        let pick = |is_kind: fn(&Locator) -> bool| -> Option<Vec<&Locator>> {
            let v: Vec<&Locator> = locators
                .iter()
                .copied()
                .filter(|l| is_kind(l) && self.transport.can_handle(l))
                .collect();
            (!v.is_empty()).then_some(v)
        };
        let locators: Vec<&Locator> = pick(Locator::is_shm)
            .or_else(|| pick(Locator::is_tcp))
            .or_else(|| pick(Locator::is_udp))
            .unwrap_or(locators);

        let mut is_sent = false;
        let mut last_error = None;
        for locator in locators {
            match self.transport.send(buffer, &SendTarget::UserData(locator)) {
                Ok(_) => is_sent = true,
                Err(e) if e.kind() == std::io::ErrorKind::Unsupported => {
                    // `io::Error` is `Display`, so the macro formats it lazily; binding a
                    // `String` first allocated even at a level that emits nothing.
                    warn!("[UserLogic] {} locator found but no {} sender available", e, e);
                    continue;
                }
                Err(e) => {
                    warn!("[UserLogic] Failed to send to locator {}: {:?}", locator, e);
                    last_error = Some(e);
                    continue;
                }
            }
        }
        if !is_sent {
            return Err(if let Some(err) = last_error {
                RtpsError::new(RtpsErrorCode::Io, err.to_string())
            } else {
                RtpsError::new(RtpsErrorCode::InvalidEntityKind, "No valid locators found")
            });
        }
        Ok(())
    }

    fn get_matched_readers(
        &self,
        remote_writer_guid: Guid,
        reader_entity_id: EntityId,
    ) -> RtpsResult<Vec<Arc<dyn Reader + Send + Sync>>> {
        let participant = self.get_upgraded_participant()?;
        let mut matched_readers: Vec<Arc<dyn Reader + Send + Sync>> = Vec::new();

        if reader_entity_id != EntityId::UNKNOWN {
            let reader =
                participant.find_reader_from_entity_id(reader_entity_id).ok_or_else(|| {
                    RtpsError::new(
                        RtpsErrorCode::RtpsEntityNotFound,
                        "No reader found for user data".to_string(),
                    )
                })?;
            if reader.matched_writer_is_matched(remote_writer_guid) {
                matched_readers.push(reader.clone());
            } else if remote_writer_guid.prefix() == participant.guid().prefix()
                && participant.find_writer_from_entity_id(remote_writer_guid.entity_id()).is_none()
            {
                // Intra-participant directed sample whose local writer was already destroyed
                // while this just-sent sample was still in flight. With the writer gone there
                // is no reliable retransmit (so no duplicate is possible); deliver the
                // already-accepted sample instead of dropping it. The matched path and
                // cross-participant samples are unchanged.
                matched_readers.push(reader.clone());
            }
        } else {
            matched_readers
                .extend(participant.find_readers_matched_with_remote_writer(remote_writer_guid)?);
        }

        Ok(matched_readers)
    }

    fn find_stateful_writer(
        &self,
        entity_id: EntityId,
    ) -> RtpsResult<Arc<dyn Writer + Send + Sync>> {
        let participant = self.get_upgraded_participant()?;
        let writer = participant
            .find_writer_from_entity_id(entity_id)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::RtpsEntityNotFound, None))?;
        // Type check
        writer
            .as_any()
            .downcast_ref::<StatefulWriter>()
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::DowncastError, "Not a StatefulWriter"))?;
        Ok(writer)
    }

    fn apply_writer_attributes_to_change(
        &self,
        reader: Arc<dyn Reader + Send + Sync>,
        remote_writer_guid: Guid,
        cache_change: &mut CacheChange,
    ) -> RtpsResult<()> {
        let ownership_strength;
        let lifespan_duration;

        if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
            let writer_proxies = stateful_reader.writer_proxies();
            let matched_writers = writer_proxies
                .lock()
                .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;
            // If the writer proxy was concurrently removed (the writer was destroyed while
            // this already-accepted sample is still being delivered intra-participant), keep
            // the change's default attributes instead of dropping the sample.
            let writer_proxy = match matched_writers
                .iter()
                .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
            {
                Some(proxy) => proxy,
                None => return Ok(()),
            };

            // Get attributes from WriterProxy
            ownership_strength = writer_proxy.get_ownership_strength();
            lifespan_duration = writer_proxy.get_lifespan_duration();
        } else if let Some(stateless_reader) = reader.as_any().downcast_ref::<StatelessReader>() {
            let remote_writer_infos = stateless_reader.remote_writer_infos();
            let matched_writers = remote_writer_infos
                .lock()
                .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;
            let remote_writer_info = matched_writers
                .iter()
                .find(|info| info.remote_writer_guid() == remote_writer_guid)
                .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

            // Get attributes from RemoteWriterInfo
            ownership_strength = remote_writer_info.get_ownership_strength();
            lifespan_duration = remote_writer_info.get_lifespan_duration();
        } else {
            return Err(RtpsError::new(RtpsErrorCode::DowncastError, None));
        }

        cache_change.set_ownership_strength(Some(ownership_strength));
        cache_change.set_lifespan_duration(Some(lifespan_duration));

        // According to lifespan qos, reception timestamp is checked when source timestamp has abnormal value
        // Need to verify if it's really necessary and whether checking timestamp for each message affects performance
        // Current state does not set reception timestamp
        // if timestamp.is_some()
        //     && reader.get_lifespan(remote_guid) != LifespanQosPolicy::default()
        // {
        //     change.set_reception_timestamp(RtpsTime::now());
        // }

        Ok(())
    }

    fn apply_inline_qos_to_change(
        &self,
        inline_qos: &ParameterList,
        cache_change: &mut CacheChange,
    ) -> RtpsResult<()> {
        if let Some(key_hash) = inline_qos.get_key_hash() {
            cache_change.set_instance_handle(key_hash);
        }

        if let Some(status_info) = inline_qos.get_status_info() {
            if status_info.disposed() && status_info.unregistered() {
                cache_change.set_kind(ChangeKind::NotAliveDisposedUnregistered);
            } else if status_info.unregistered() {
                cache_change.set_kind(ChangeKind::NotAliveUnregistered);
            } else if status_info.disposed() {
                cache_change.set_kind(ChangeKind::NotAliveDisposed);
            } else if status_info.filtered() {
                cache_change.set_kind(ChangeKind::AliveFiltered);
            }
        }

        // Restore per-sample coherent/group presentation metadata.
        cache_change.set_presentation_info(PresentationInfo {
            coherent_set: inline_qos.get_coherent_set(),
            group_seq_num: inline_qos.get_group_seq_num(),
            group_coherent_set: inline_qos.get_group_coherent_set(),
        });

        Ok(())
    }
}

impl_participant_accessor!(UserLogic);
impl_unicast_thread_handler!(UserLogic);

impl UnicastMessageProcessor for UserLogic {
    fn handle_data_message(
        &mut self,
        rtps_header: &Header,
        _submessage_header: &SubmessageHeader,
        data: &Data,
        message_receiver: &MessageReceiver,
    ) -> RtpsResult<()> {
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), data.writer_id);

        let matched_readers: Vec<Arc<dyn Reader + Send + Sync>> =
            self.get_matched_readers(remote_writer_guid, data.reader_id)?;

        if matched_readers.is_empty() {
            debug!(
                "[DATA] No matched readers found for remote writer: {}, skipping data handling.",
                remote_writer_guid
            );
            return Ok(());
        }

        for reader in matched_readers {
            let mut change = match reader.reader_cache().lock() {
                Ok(mut cache) => cache.acquire_change(),
                Err(_) => {
                    return Err(RtpsError::new(
                        RtpsErrorCode::LockError,
                        "Failed to acquire reader cache lock",
                    ));
                }
            };
            change.reset(
                ChangeKind::Alive,
                remote_writer_guid,
                InstanceHandle::NIL,
                data.writer_sn,
                message_receiver.get_source_timestamp(),
            );
            // Zero-copy share of the socket buffer: `data.serialized_data()`
            // returns a `Bytes` slice of the original socket allocation, so
            // each reader gets an Arc refcount bump instead of a payload copy.
            change.set_shared_payload(data.serialized_data());

            self.apply_writer_attributes_to_change(
                reader.clone(),
                remote_writer_guid,
                &mut change,
            )?;

            if let Some(inline_qos) = data.inline_qos() {
                self.apply_inline_qos_to_change(&inline_qos, &mut change)?;
            }

            self.deliver_change_to_reader(
                change,
                reader.as_ref(),
                data.writer_sn,
                remote_writer_guid,
                None,
            )?;
        }

        if let Some(wlp) = self.get_upgraded_participant()?.wlp_logic() {
            wlp.mark_monitored_writer_alive(remote_writer_guid)?;
        }

        Ok(())
    }

    fn handle_heartbeat_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        heartbeat: &Heartbeat,
    ) -> RtpsResult<()> {
        // Temporarily commented out the seemingly unnecessary liveliness heartbeat logic in UserLogic.
        // let participant = self.get_upgraded_participant()?;
        // let is_liveliness_heartbeat = submessage_header.liveliness_flag().unwrap_or(false);

        // if is_liveliness_heartbeat {
        //     if let Some(wlp) = participant.wlp_logic() {
        //         let _ = wlp.handle_heartbeat_message_inner(
        //             heartbeat,
        //             Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
        //             submessage_header.final_flag().unwrap_or(false),
        //             true,
        //         );
        //     }
        // } else {
        let final_flag = submessage_header.final_flag().unwrap_or(false);
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id);

        let participant = self.get_upgraded_participant()?;
        let matched_readers = self.get_matched_readers(remote_writer_guid, heartbeat.reader_id)?;

        if matched_readers.is_empty() {
            debug!(
                "[Heartbeat] No matched readers found for remote writer: {}, skipping data handling.",
                remote_writer_guid
            );
            return Ok(());
        }

        for reader in matched_readers {
            let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() else {
                continue;
            };

            // Collected under the guard, delivered after it: delivery reaches the user's
            // listener, which may re-lock this mutex via `get_matched_publications`.
            let pending_delivery: Vec<CacheChange>;
            let mut acknack_result: RtpsResult<()> = Ok(());

            let writer_proxies = stateful_reader.writer_proxies();
            let mut matched_writers = writer_proxies.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire writer_proxies lock: {}", e),
                )
            })?;

            let writer_proxy = matched_writers
                .iter_mut()
                .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                .ok_or_else(|| {
                    RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, "WriterProxy not found")
                })?;

            let now = Instant::now();
            if !should_accept_count(
                "[UserLogic] [Heartbeat]",
                heartbeat.count,
                writer_proxy.last_heartbeat_count(),
                writer_proxy.last_heartbeat_at(),
                false,
                now,
            ) {
                return Ok(());
            }

            writer_proxy.set_last_heartbeat_count(heartbeat.count);
            writer_proxy.set_last_heartbeat_at(now);

            let (bitmap_base, missing_changes) =
                writer_proxy.process_heartbeat(heartbeat.first_sn, heartbeat.last_sn);

            // This is the first HB for reader
            if writer_proxy.expected_sn() == SequenceNumber::UNKNOWN {
                writer_proxy.set_expected_sn(heartbeat.first_sn);
            }

            // Owed on both branches: a whole buffered sample must reach the cache even while
            // another sample in the range is still short of fragments.
            pending_delivery = writer_proxy.flush_buffered_changes();

            // A short change in the range is repaired by NACK_FRAG in the else branch, keyed on the
            // sample that is actually short. Routing on `last_sn` alone stranded a short earlier
            // sample: it is marked `Received`, so the plain ACKNACK omits it and nothing repairs it.
            if !writer_proxy.has_fragmented_changes(heartbeat.first_sn, heartbeat.last_sn) {
                // Apply heartbeat response delay
                let heartbeat_response_delay = stateful_reader.heartbeat_response_delay();
                let delay_duration = heartbeat_response_delay.to_std_duration();

                if delay_duration.is_zero() {
                    // No delay - send immediately.
                    //
                    // The error is carried, not propagated with `?`. `flush_buffered_changes`
                    // above already removed those changes from the proxy and advanced
                    // `expected_sn`, so returning here would drop `pending_delivery` on the
                    // floor with no way to ever re-request it -- silent loss on a RELIABLE
                    // reader. A failed ACKNACK only costs one ACKNACK; the next heartbeat
                    // retries it.
                    acknack_result = self.send_acknack_to_writer_proxy_inner(
                        writer_proxy,
                        stateful_reader,
                        missing_changes,
                        bitmap_base,
                        final_flag,
                        false,
                    );
                } else {
                    // Schedule delayed ACKNACK via SendingHandler
                    let remote_writer_guid = writer_proxy.remote_writer_guid();
                    let reader_entity_id = stateful_reader.guid().entity_id();
                    let participant_guid = participant.guid();

                    let timer_id = TimerId::Acknack { reader_entity_id, remote_writer_guid };

                    if let Ok(locked_timer_handler) =
                        TimerHandler::get_instance(participant.guid().prefix()).lock()
                    {
                        locked_timer_handler.add_timer(
                            timer_id,
                            delay_duration,
                            false, // one-shot
                            move || {
                                if let Some(sending_handler) =
                                    SendingHandler::get_instance_by_participant_guid(
                                        participant_guid,
                                    )
                                {
                                    sending_handler.push_message_and_wake(
                                        MessageType::UserAcknack(
                                            reader_entity_id,
                                            remote_writer_guid,
                                            final_flag,
                                            false, // is_preemptive
                                        ),
                                    );
                                }
                            },
                        );
                    }
                }
            } else {
                // Case when fragments are not completely received yet - apply suppression delay
                // `incomplete_sn`, not `heartbeat.last_sn`: the writer resolves the request
                // against `writer_sn`, and it also keys the suppression timer.
                if let Some(incomplete_sn) = writer_proxy
                    .first_incomplete_fragmented_sn(heartbeat.first_sn, heartbeat.last_sn)
                {
                    let request = NackFragRequest {
                        writer_proxies: writer_proxies.clone(),
                        transport: self.transport.clone(),
                        participant: participant.clone(),
                        reader_guid: stateful_reader.guid(),
                        remote_writer_guid: writer_proxy.remote_writer_guid(),
                        incomplete_sn,
                        retries_left: NACK_FRAG_MAX_RETRIES,
                    };

                    // Re-armed on every accepted heartbeat, so a burst is answered once after it
                    // settles rather than per fragment.
                    if let Ok(handler) =
                        TimerHandler::get_instance(participant.guid().prefix()).lock()
                    {
                        handler.remove_timer(request.timer_id());
                    }
                    schedule_nackfrag(request, NACK_FRAG_SUPPRESSION);
                }
            }

            // All writer-proxy work for this reader is done; release before notifying so a
            // listener may call back into the reader's matched-writer APIs.
            drop(matched_writers);
            if !pending_delivery.is_empty() {
                self.add_change_to_reader_cache_and_notify(reader.as_ref(), pending_delivery)?;
            }
            // Reported only once the flushed samples are safely in the reader's cache.
            acknack_result?;
        }

        Ok(())
    }

    fn handle_heartbeatfrag_message(
        &mut self,
        rtps_header: &Header,
        heartbeat_frag: &HeartbeatFrag,
    ) -> RtpsResult<()> {
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), heartbeat_frag.writer_id);
        let matched_readers =
            self.get_matched_readers(remote_writer_guid, heartbeat_frag.reader_id)?;

        if matched_readers.is_empty() {
            debug!(
                "[HeartbeatFrag] No matched readers found for remote writer: {:?}, skipping.",
                remote_writer_guid
            );
            return Ok(());
        }

        for reader in matched_readers {
            let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() else {
                continue;
            };

            let writer_proxies = stateful_reader.writer_proxies();
            let mut matched_writers = writer_proxies.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to acquire writer_proxies lock: {}", e),
                )
            })?;

            let Some(writer_proxy) = matched_writers
                .iter_mut()
                .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
            else {
                continue;
            };

            // Drop duplicate or stale announcements
            if writer_proxy
                .last_heartbeat_frag_count()
                .is_some_and(|last| heartbeat_frag.count <= last)
            {
                continue;
            }
            writer_proxy.set_last_heartbeat_frag_count(heartbeat_frag.count);

            // Seed fragment knowledge for this sequence number so the missing
            // set is computable even when every DATA_FRAG of the sample was
            // lost (the announcement carries the last available fragment).
            writer_proxy.mark_frag_received(
                heartbeat_frag.writer_sn,
                heartbeat_frag.last_fragment_num,
                std::iter::empty::<u32>(),
            );

            if !writer_proxy.still_missing_fragments(heartbeat_frag.writer_sn) {
                continue;
            }

            // Respond immediately with NACK_FRAG (zero response delay); the
            // count check above suppresses duplicates per announcement.
            let mut missing_fragments =
                writer_proxy.get_ascending_missing_fn_list(heartbeat_frag.writer_sn);
            if missing_fragments.is_empty() {
                continue;
            }

            let first_nackfrag_count = writer_proxy.nackfrag_count().wrapping_add(1);
            let (messages, consumed_count) = match MessageCreator::create_multiple_nackfrag_msgs(
                stateful_reader.guid(),
                writer_proxy.remote_writer_guid(),
                stateful_reader.guid().entity_id(),
                writer_proxy.remote_writer_guid().entity_id(),
                heartbeat_frag.writer_sn,
                &mut missing_fragments,
                first_nackfrag_count,
            ) {
                Ok(result) => result,
                Err(e) => {
                    warn!("[UserLogic] Failed to create NACK_FRAG for HEARTBEAT_FRAG: {:?}", e);
                    continue;
                }
            };

            for _ in 0..consumed_count {
                writer_proxy.increase_nackfrag_count();
            }

            let locators: Vec<Locator> = writer_proxy.unicast_locator_list().to_vec();
            drop(matched_writers);
            for buffer in &messages {
                self.send_rtps_message_to_locators(locators.iter(), buffer)?;
            }
        }

        Ok(())
    }

    fn handle_acknack_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        acknack: &AckNack,
    ) -> RtpsResult<()> {
        let remote_reader_guid = Guid::new(rtps_header.guid_prefix(), acknack.reader_id);

        let writer = self.find_stateful_writer(acknack.writer_id)?;
        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();
        let reader_proxies = stateful_writer.reader_proxies();
        let mut reader_proxies = reader_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire reader_proxies lock: {}", e),
            )
        })?;

        let reader_proxy = reader_proxies
            .iter_mut()
            .find(|rp| rp.remote_reader_guid() == remote_reader_guid)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        // Preemptive ACKNACK (empty bitmap with seqbase 0 or 1) is an explicit reset signal
        // and bypasses the count/debounce check. Some foreign RTPS stacks use base 1 for this
        // preemptive form, which our own reader also uses for a plain "nothing missing" ack --
        // the FinalFlag is what tells the two apart, and only a plain ack carries it.
        let is_preemptive = acknack.reader_sn_state.num_bits() == 0
            && !submessage_header.final_flag().unwrap_or(false)
            && acknack.reader_sn_state.bitmap_base().to_i64() <= 1;
        let now = Instant::now();

        if !should_accept_count(
            "[UserLogic] [AckNack]",
            acknack.count,
            reader_proxy.last_acknack_count(),
            reader_proxy.last_acknack_at(),
            is_preemptive,
            now,
        ) {
            return Ok(());
        }

        if is_preemptive {
            drop(reader_proxies);
            self.handle_preemptive_acknack_message(rtps_header, acknack)?;
            return Ok(());
        }

        let missing_seq_numbers = acknack.reader_sn_state.extract_numbers();

        // ACK
        // 2.5 - 8.3.8.1.2 All sequence numbers up to the one prior to readerSNState.base are confirmed as received by the reader.
        reader_proxy.acked_changes_set(SequenceNumber::from_i64(
            acknack.reader_sn_state.bitmap_base().to_i64() - 1,
        ));

        debug!(
            "[UserLogic] [AckNack] ACK received up to seq_num={}",
            SequenceNumber::from_i64(acknack.reader_sn_state.bitmap_base().to_i64() - 1)
        );

        reader_proxy.set_last_acknack_count(acknack.count);
        reader_proxy.set_last_acknack_at(now);

        // NACK
        if !missing_seq_numbers.is_empty() {
            debug!(
                "Sending AckNack - missing changes: [{}]",
                missing_seq_numbers.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(", ")
            );

            reader_proxy.requested_changes_set(missing_seq_numbers);

            let participant = self.get_upgraded_participant()?;

            // Apply nack response delay
            let nack_response_delay = stateful_writer.nack_response_delay();
            let delay_duration = nack_response_delay.to_std_duration();

            if delay_duration.is_zero() {
                // No delay - send immediately
                let handler = SendingHandler::get_instance(participant.clone(), None);
                handler.push_message_and_wake(MessageType::UserRequestedChanges(
                    acknack.writer_id,
                    remote_reader_guid,
                ));
            } else {
                // Schedule delayed response via timer
                let writer_entity_id = acknack.writer_id;
                let participant_guid = participant.guid();

                let timer_id = TimerId::NackResponse { writer_entity_id, remote_reader_guid };

                if let Ok(locked_timer_handler) =
                    TimerHandler::get_instance(participant.guid().prefix()).lock()
                {
                    locked_timer_handler.add_timer(
                        timer_id,
                        delay_duration,
                        false, // one-shot
                        move || {
                            if let Some(sending_handler) =
                                SendingHandler::get_instance_by_participant_guid(participant_guid)
                            {
                                sending_handler.push_message_and_wake(
                                    MessageType::UserRequestedChanges(
                                        writer_entity_id,
                                        remote_reader_guid,
                                    ),
                                );
                            }
                        },
                    );
                }
            }
        }

        drop(reader_proxies);

        // May remove all-acked changes from a volatile keep-all writer's history
        debug!("[history-strict] trigger=acknack");
        stateful_writer.process_acked_changes();
        debug!("[history-strict] after acknack rtps_len={}", stateful_writer.rtps_cache_len());

        stateful_writer.stop_heartbeat_if_acked_by_all()?;

        Ok(())
    }

    fn handle_preemptive_acknack_message(
        &mut self,
        rtps_header: &Header,
        acknack: &AckNack,
    ) -> RtpsResult<()> {
        let remote_reader_guid = Guid::new(rtps_header.guid_prefix(), acknack.reader_id);

        let writer = self.find_stateful_writer(acknack.writer_id)?;
        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // LOCK ORDER: acquires `writer_cache` first, then `reader_proxies` (via matched_reader_lookup).
        let writer_cache_lock = stateful_writer.writer_cache();
        let history_cache = writer_cache_lock.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to acquire writer cache lock for heartbeat: {:?}", e),
            )
        })?;

        let mut reader_proxy =
            stateful_writer.matched_reader_lookup(remote_reader_guid).ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, "ReaderProxy not found")
            })?;

        self.send_heartbeat_to_a_reader_proxy_inner(
            stateful_writer,
            &mut reader_proxy,
            true,
            &history_cache,
        )
    }

    fn handle_datafrag_message(
        &mut self,
        rtps_header: &Header,
        data_frag: &DataFrag,
        message_receiver: &MessageReceiver,
    ) -> RtpsResult<()> {
        let source_timestamp = message_receiver.get_source_timestamp();
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), data_frag.writer_id);
        let total_size = data_frag.sample_size;

        // `reader_id` addresses the datagram, not the sample: UNKNOWN is one burst that reached
        // every matched reader, a directed repair only the reader it names.
        let matched_readers: Vec<Arc<dyn Reader + Send + Sync>> =
            self.get_matched_readers(remote_writer_guid, data_frag.reader_id)?;

        if matched_readers.is_empty() {
            debug!(
                "[DATA] No matched readers found for remote writer: {}, skipping data handling.",
                remote_writer_guid
            );
            return Ok(());
        }

        // DashMap is thread-safe, so no explicit lock is needed
        //println!("[DEBUG] FragmentBuffer count: {}, size: {}", self.fragment_buffers.len(), self.fragment_buffers.iter().map(|entry| entry.value().total_size as usize).sum::<usize>());
        // Raised with the per-reader key: the same workload now needs one buffer per reader,
        // and evicting an in-progress one is the very loss this key change removes.
        if self.fragment_buffers.len() > FRAGMENT_BUFFER_LIMIT {
            debug!(
                "[UserLogic] Fragment buffer count exceeded threshold ({}), cleaning up old buffers.",
                self.fragment_buffers.len()
            );
            // This datagram is about to write one key per matched reader below; excluding all
            // of them is what stops the arriving fragment from evicting its own buffer.
            let arriving_keys: Vec<(Guid, EntityId, SequenceNumber)> = matched_readers
                .iter()
                .map(|reader| (remote_writer_guid, reader.guid().entity_id(), data_frag.writer_sn))
                .collect();
            for (evicted_writer_guid, evicted_reader_id, evicted_sn) in
                self.cleanup_old_fragment_buffers(FRAGMENT_BUFFER_LIMIT, &arriving_keys)
            {
                // The bytes are gone, so the ledger must stop claiming them. The key names the
                // one reader that lost them; every other reader's buffer is still whole.
                let Ok(readers) = self.get_matched_readers(evicted_writer_guid, evicted_reader_id)
                else {
                    continue;
                };
                for reader in readers {
                    let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>()
                    else {
                        continue;
                    };
                    let writer_proxies = stateful_reader.writer_proxies();
                    let Ok(mut matched_writers) = writer_proxies.lock() else {
                        continue;
                    };
                    if let Some(writer_proxy) = matched_writers
                        .iter_mut()
                        .find(|proxy| proxy.remote_writer_guid() == evicted_writer_guid)
                    {
                        // A complete ledger means the sample was already delivered and a late
                        // repair merely recreated the buffer. Only a stranded one is retracted.
                        if !writer_proxy.all_fragments_received(evicted_sn) {
                            writer_proxy.forget_fragments(evicted_sn);
                        }
                    }
                }
            }
        }

        // Readers completing on this datagram hold byte-identical chunks, so one shared cache
        // keeps any contiguous fallback to a single materialization.
        let mut assembled_cache: Option<std::sync::Arc<std::sync::OnceLock<bytes::Bytes>>> = None;

        // The key carries the reader, so the fragments go into each matched reader's own
        // buffer. Completion is then single-reader: a buffer belongs to exactly one.
        for reader in &matched_readers {
            let key = (remote_writer_guid, reader.guid().entity_id(), data_frag.writer_sn);

            // Copy fragment data using DashMap entry API
            {
                let mut buffer = self.fragment_buffers.entry(key).or_insert_with(|| {
                    FragmentBuffer::new(data_frag.writer_sn, total_size, data_frag.fragment_size)
                });

                if buffer.source_timestamp.is_none() {
                    // timestamp does not be set in buffer.source_timestmap yet
                    if let Some(ts) = source_timestamp {
                        buffer.source_timestamp = Some(ts); // store source_timestamp from first fragment that equals to INFO_TS
                    }
                }

                // Zero-copy: per-fragment slices are refcount bumps on the socket buffer, so
                // fanning the write out across readers costs slot arrays, not payload copies.
                if let Some(serialized_bytes) = data_frag.serialized_bytes() {
                    let frag_size = data_frag.fragment_size as usize;
                    let total_len = serialized_bytes.len();
                    for i in 0..data_frag.fragments_in_submessage {
                        let fragment_num = data_frag.fragment_starting_num + i as u32;
                        let frag_data_start = i as usize * frag_size;
                        let frag_data_end = std::cmp::min(frag_data_start + frag_size, total_len);
                        buffer.copy_fragment_data(
                            fragment_num,
                            serialized_bytes.slice(frag_data_start..frag_data_end),
                        );
                    }
                }
            } // buffer RefMut is automatically dropped here

            // Update this reader's ChangeFromWriter state from the buffer that just took them.
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                // Query buffer information from DashMap again
                if let Some(buffer_ref) = self.fragment_buffers.get(&key) {
                    let total_fragments = buffer_ref.total_fragments;
                    drop(buffer_ref);

                    let writer_proxies = stateful_reader.writer_proxies();
                    let mut matched_writers = writer_proxies.lock().map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to acquire writer_proxies lock: {}", e),
                        )
                    })?;

                    if let Some(writer_proxy) = matched_writers
                        .iter_mut()
                        .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                    {
                        // Update fragment information with this submessage's range
                        let frag_start = data_frag.fragment_starting_num;
                        let frag_end = frag_start + data_frag.fragments_in_submessage as u32;
                        writer_proxy.mark_frag_received(
                            data_frag.writer_sn,
                            total_fragments,
                            frag_start..frag_end,
                        );
                    }
                }
            }

            // Check if all fragments have been received and process
            let Some(buffer_ref) = self.fragment_buffers.get(&key) else {
                continue;
            };
            if !buffer_ref.all_fragments_received() {
                continue;
            }
            let total_fragments = buffer_ref.total_fragments;
            // complete here; delivery FragmentInfo's set is unused when is_complete
            let received_fragments: std::collections::HashSet<u32> =
                std::collections::HashSet::new();

            // Drop buffer_ref to release DashMap lock
            drop(buffer_ref);

            // Move payload from buffer without cloning
            let Some((_, buffer)) = self.fragment_buffers.remove(&key) else {
                continue;
            };
            // Use timestamp from first fragment, fallback to current message
            let assembled_timestamp = buffer.source_timestamp.or(source_timestamp);
            if assembled_timestamp.is_none() {
                return Err(RtpsError::new(
                    RtpsErrorCode::InvalidSubmessageBody,
                    "No source timestamp available for assembled DataFrag (missing INFO_TS)",
                ));
            }

            let mut ownership_strength = None;
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                let writer_proxies = stateful_reader.writer_proxies();
                let matched_writers = writer_proxies.lock().ok();
                if let Some(guard) = matched_writers {
                    if let Some(writer_proxy) =
                        guard.iter().find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                    {
                        ownership_strength = Some(writer_proxy.get_ownership_strength());
                    }
                }
            }

            // Scatter-gather: keep the chunks instead of assembling a contiguous buffer.
            let chunks = buffer.into_chunks();
            let cached = assembled_cache
                .get_or_insert_with(|| std::sync::Arc::new(std::sync::OnceLock::new()))
                .clone();

            let mut assembled_change = match reader.reader_cache().lock() {
                Ok(mut cache) => cache.acquire_change(),
                Err(_) => continue,
            };
            assembled_change.reset(
                ChangeKind::Alive,
                remote_writer_guid,
                InstanceHandle::NIL,
                data_frag.writer_sn,
                assembled_timestamp,
            );
            assembled_change.set_chained_payload(chunks, cached);
            assembled_change.set_ownership_strength(ownership_strength);

            let _ = self.deliver_change_to_reader(
                assembled_change,
                reader.as_ref(),
                data_frag.writer_sn,
                remote_writer_guid,
                Some(FragmentInfo { total_fragments, received_fragments, is_complete: true }),
            );
        }

        Ok(())
    }

    fn handle_nackfrag_message(
        &mut self,
        rtps_header: &Header,
        nack_frag: &NackFrag,
    ) -> RtpsResult<()> {
        let remote_reader_guid = Guid::new(rtps_header.guid_prefix(), nack_frag.reader_id);
        let writer_id = nack_frag.writer_id;
        let writer_sn = nack_frag.writer_sn;
        let frag_state = &nack_frag.fragment_number_state;

        let writer = self.find_stateful_writer(writer_id)?;
        let stateful_writer = writer.as_any().downcast_ref::<StatefulWriter>().unwrap();

        // LOCK ORDER: acquires `writer_cache` first, then `reader_proxies`.
        let writer_cache = stateful_writer.writer_cache();
        let history_cache_guard = writer_cache.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire history cache lock")
        })?;

        let reader_proxies = stateful_writer.reader_proxies();
        let mut reader_proxies_guard = reader_proxies.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire reader_proxies lock")
        })?;

        let reader_proxy = reader_proxies_guard
            .iter_mut()
            .find(|proxy| proxy.remote_reader_guid() == remote_reader_guid)
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::InvalidEntityKind, "Reader proxy not found")
            })?;

        // Check for duplicate NACK_FRAG. NACK_FRAG has no preemptive form.
        let now = Instant::now();
        if !should_accept_count(
            "[UserLogic] [NackFrag]",
            nack_frag.count,
            reader_proxy.last_nackfrag_count(),
            reader_proxy.last_nackfrag_at(),
            false,
            now,
        ) {
            return Ok(());
        }
        reader_proxy.set_last_nackfrag_count(nack_frag.count);
        reader_proxy.set_last_nackfrag_at(now);

        let change = history_cache_guard.get_change(writer_sn).ok_or_else(|| {
            warn!("NACK_FRAG requested missing change SN={}", writer_sn);
            RtpsError::new(RtpsErrorCode::InvalidSubmessageBody, "Change not found")
        })?;

        if !change.is_fragmented() {
            return Ok(()); // No processing needed for non-fragment case
        }

        reader_proxy.requested_fragments_add(
            writer_sn,
            frag_state.bitmap_base(),
            frag_state.num_bits(),
            frag_state.extract_numbers(),
        );

        let participant = self.get_upgraded_participant()?;

        // Apply nack response delay
        let nack_response_delay = stateful_writer.nack_response_delay();
        let delay_duration = nack_response_delay.to_std_duration();

        if delay_duration.is_zero() {
            // No delay - send immediately
            let handler = SendingHandler::get_instance(participant.clone(), None);
            handler.push_message_and_wake(MessageType::UserRequestedFragments(
                writer_id,
                remote_reader_guid,
            ));
        } else {
            // Schedule delayed response via timer
            let participant_guid = participant.guid();

            let timer_id =
                TimerId::NackFragResponse { writer_entity_id: writer_id, remote_reader_guid };

            if let Ok(locked_timer_handler) =
                TimerHandler::get_instance(participant.guid().prefix()).lock()
            {
                locked_timer_handler.add_timer(
                    timer_id,
                    delay_duration,
                    false, // one-shot
                    move || {
                        if let Some(sending_handler) =
                            SendingHandler::get_instance_by_participant_guid(participant_guid)
                        {
                            sending_handler.push_message_and_wake(
                                MessageType::UserRequestedFragments(writer_id, remote_reader_guid),
                            );
                        }
                    },
                );
            }
        }

        Ok(())
    }

    fn handle_gap_message(&mut self, rtps_header: &Header, gap: &Gap) -> RtpsResult<()> {
        let remote_writer_guid = Guid::new(rtps_header.guid_prefix(), gap.writer_id);
        let matched_readers = self.get_matched_readers(remote_writer_guid, gap.reader_id)?;

        if matched_readers.is_empty() {
            debug!(
                "[Gap] No matched readers found for remote writer: {}, skipping data handling.",
                remote_writer_guid
            );
            return Ok(());
        }

        for reader in matched_readers {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                // Same rule as the DATA and HEARTBEAT paths: the batch is decided under the
                // guard and delivered after it, because delivery ends in the user's listener
                // and that listener may re-lock this mutex.
                let mut pending_delivery: Vec<CacheChange> = Vec::new();

                let writer_proxies_arc = stateful_reader.writer_proxies();
                let mut writer_proxies = writer_proxies_arc.lock().map_err(|_| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        "[GAP] Failed to acquire writer proxies lock",
                    )
                })?;

                let writer_proxy = writer_proxies
                    .iter_mut()
                    .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                    .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

                // Collect irrelevant changes from GAP message
                let capacity =
                    (gap.gap_list.bitmap_base().to_i64() - gap.gap_start.to_i64()).max(0) as usize;
                let mut irrelevant_changes = Vec::with_capacity(capacity);

                for sn in gap.gap_start.to_i64()..gap.gap_list.bitmap_base().to_i64() {
                    irrelevant_changes.push(SequenceNumber::from_i64(sn));
                }

                irrelevant_changes.extend(gap.gap_list.extract_numbers().iter());

                // Flush buffered changes if expected_sn is affected.
                if let Some(last) = irrelevant_changes.last() {
                    if last >= &writer_proxy.expected_sn() {
                        writer_proxy.set_expected_sn(SequenceNumber::from_i64(last.to_i64() + 1));
                        pending_delivery = writer_proxy.flush_buffered_changes();
                    }
                }

                for seq_num in irrelevant_changes {
                    writer_proxy.irrelevant_change_set(seq_num);
                }

                drop(writer_proxies);
                if !pending_delivery.is_empty() {
                    self.add_change_to_reader_cache_and_notify(stateful_reader, pending_delivery)?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn fpm(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    #[test]
    fn runs_split_on_every_gap_and_clamp_to_total() {
        let asked: BTreeSet<u32> = [1, 2, 3, 7, 8, 11, 99].into_iter().collect();
        assert_eq!(contiguous_fragment_runs(&asked, 20), vec![(1, 3), (7, 2), (11, 1)]);
    }

    #[test]
    fn an_empty_or_fully_out_of_range_request_yields_no_runs() {
        assert!(contiguous_fragment_runs(&BTreeSet::new(), 10).is_empty());
        assert!(contiguous_fragment_runs(&[0, 11].into_iter().collect(), 10).is_empty());
    }

    #[test]
    fn fragment_num_equal_to_total_is_included() {
        // The upper bound is inclusive: total itself is a valid, in-range fragment number.
        assert_eq!(contiguous_fragment_runs(&[10].into_iter().collect(), 10), vec![(10, 1)]);
    }

    #[test]
    fn a_number_one_past_total_never_extends_the_run_at_total() {
        // If the upper filter became `n <= total + 1`, 11 would wrongly extend the run at
        // 10 into (10, 2): an over-claimed count, not just a missing fragment.
        assert_eq!(contiguous_fragment_runs(&[10, 11].into_iter().collect(), 10), vec![(10, 1)]);
    }

    #[test]
    fn a_run_ending_exactly_at_total_is_its_own_trailing_run() {
        let asked: BTreeSet<u32> = [1, 2, 3, 5].into_iter().collect();
        assert_eq!(contiguous_fragment_runs(&asked, 5), vec![(1, 3), (5, 1)]);
    }

    #[test]
    fn two_consecutive_fragment_numbers_merge_into_one_run() {
        // Isolates the adjacency check: a set like {1, 3, 5} would not catch
        // `*start + *count == n` regressing to `... - 1 == n`, since none of those pairs
        // are actually adjacent, so packing would silently revert to one fragment each.
        assert_eq!(contiguous_fragment_runs(&[1, 2].into_iter().collect(), 10), vec![(1, 2)]);
    }

    #[test]
    fn a_total_of_zero_yields_no_runs_regardless_of_input() {
        assert!(contiguous_fragment_runs(&BTreeSet::new(), 0).is_empty());
        assert!(contiguous_fragment_runs(&[1, 2, 3].into_iter().collect(), 0).is_empty());
    }

    /// Properties that must hold for any `fragment_send_plan` output, not just the exact
    /// shape of one hand-picked example: every requested fragment is covered exactly once,
    /// no chunk exceeds the datagram budget, and exactly one chunk -- the last one in the
    /// plan -- ends on the highest fragment number across all runs.
    fn assert_plan_invariants(runs: &[(u32, u32)], frags_per_msg: NonZeroU32) {
        let plan = fragment_send_plan(runs, frags_per_msg);

        let expected: Vec<u32> = runs.iter().flat_map(|&(start, len)| start..start + len).collect();
        let mut covered: Vec<u32> = Vec::new();
        for &(start, count, _) in &plan {
            assert!(count >= 1, "a chunk must carry at least one fragment");
            assert!(count <= frags_per_msg.get(), "a chunk must never exceed the datagram budget");
            covered.extend(start..start + count);
        }
        assert_eq!(covered, expected, "chunks must reproduce the runs exactly, in order");

        let last_count = plan.iter().filter(|&&(_, _, is_last)| is_last).count();
        match expected.last() {
            None => assert_eq!(last_count, 0, "an empty plan carries no heartbeat"),
            Some(&highest) => {
                assert_eq!(last_count, 1, "exactly one submessage may carry the heartbeat");
                let &(start, count, is_last) = plan.last().unwrap();
                assert!(is_last, "the marked submessage must be the final entry in the plan");
                assert_eq!(
                    start + count - 1,
                    highest,
                    "the marked submessage must end on the highest fragment"
                );
            }
        }
    }

    #[test]
    fn a_run_shorter_than_one_chunk_is_a_single_submessage() {
        let runs = [(5, 3)];
        assert_eq!(fragment_send_plan(&runs, fpm(10)), vec![(5, 3, true)]);
        assert_plan_invariants(&runs, fpm(10));
    }

    #[test]
    fn a_run_that_is_an_exact_multiple_of_the_chunk_size_splits_evenly() {
        let runs = [(1, 10)];
        assert_eq!(fragment_send_plan(&runs, fpm(5)), vec![(1, 5, false), (6, 5, true)]);
        assert_plan_invariants(&runs, fpm(5));
    }

    #[test]
    fn a_run_one_longer_than_a_multiple_leaves_a_short_final_chunk() {
        let runs = [(1, 11)];
        assert_eq!(
            fragment_send_plan(&runs, fpm(5)),
            vec![(1, 5, false), (6, 5, false), (11, 1, true)]
        );
        assert_plan_invariants(&runs, fpm(5));
    }

    #[test]
    fn only_the_final_chunk_of_the_final_run_is_last_even_when_an_earlier_run_is_longer() {
        // The first run needs two chunks; the shorter, final run needs only one. Only that
        // final run's chunk may carry the heartbeat, never the longer first run's last chunk.
        let runs = [(1, 10), (20, 3)];
        assert_eq!(
            fragment_send_plan(&runs, fpm(5)),
            vec![(1, 5, false), (6, 5, false), (20, 3, true)]
        );
        assert_plan_invariants(&runs, fpm(5));
    }

    #[test]
    fn exactly_one_entry_is_last_and_it_ends_on_the_highest_requested_fragment() {
        let asked: BTreeSet<u32> = [1, 2, 3, 7, 8, 11].into_iter().collect();
        let runs = contiguous_fragment_runs(&asked, 20);
        let plan = fragment_send_plan(&runs, fpm(2));

        let last_entries: Vec<_> = plan.iter().filter(|&&(_, _, is_last)| is_last).collect();
        assert_eq!(last_entries.len(), 1, "exactly one submessage may carry the heartbeat");

        let &(fragment_num, count, _) = last_entries[0];
        assert_eq!(fragment_num + count - 1, 11, "must end on the highest requested fragment");
        assert_plan_invariants(&runs, fpm(2));
    }

    #[test]
    fn an_empty_run_list_yields_an_empty_plan() {
        assert!(fragment_send_plan(&[], fpm(10)).is_empty());
        assert_plan_invariants(&[], fpm(10));
    }

    #[test]
    fn the_production_shape_packs_781_fragments_into_17_submessages() {
        // 781 fragments at 48 per submessage (65000 / 1344): the real repair geometry.
        // Kills an inverted/dropped `min` (a chunk would exceed 48) and a dropped `-1` in
        // `is_last` (no entry would ever end exactly on fragment 781, since 769+13 = 782).
        let runs = [(1, 781)];
        let plan = fragment_send_plan(&runs, fpm(48));
        assert_eq!(plan.len(), 17);
        assert_eq!(plan[0], (1, 48, false));
        assert_eq!(plan[16], (769, 13, true));
        assert_plan_invariants(&runs, fpm(48));
    }

    #[test]
    fn a_chunk_can_land_exactly_on_the_final_fragment() {
        // A geometry where the final chunk's end coincides with total; 781/48 never
        // produces this shape.
        let runs = [(1, 5)];
        assert_eq!(
            fragment_send_plan(&runs, fpm(2)),
            vec![(1, 2, false), (3, 2, false), (5, 1, true)]
        );
        assert_plan_invariants(&runs, fpm(2));
    }

    #[test]
    fn only_the_short_final_fragment_requested_is_the_classic_repair_case() {
        let runs = [(781, 1)];
        assert_eq!(fragment_send_plan(&runs, fpm(48)), vec![(781, 1, true)]);
        assert_plan_invariants(&runs, fpm(48));
    }

    #[test]
    fn the_unpacked_shape_still_emits_exactly_one_heartbeat() {
        // frags_per_msg = 1 reproduces one-fragment-per-datagram; even then, exactly one
        // submessage in the burst must carry the heartbeat.
        let runs = [(1, 5)];
        assert_eq!(
            fragment_send_plan(&runs, fpm(1)),
            vec![(1, 1, false), (2, 1, false), (3, 1, false), (4, 1, false), (5, 1, true)]
        );
        assert_plan_invariants(&runs, fpm(1));
    }

    // Fragment reassembly: drives `handle_datafrag_message` directly against two readers on
    // one participant, so the defects below are deterministic instead of loss-dependent.

    use crate::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData;
    use crate::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData;
    use crate::infrastructure::qos_policy::ReliabilityQosPolicyKind;
    use crate::rtps::common::entity_kind::EntityKind;
    use crate::rtps::common::types::{SubmessagePayload, TopicKind};
    use crate::rtps::messages::submessage_id::SubmessageId;
    use crate::rtps::messages::submessages::info::InfoTimestamp;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    struct NullTransport;

    impl TransportPlugin for NullTransport {
        fn send(&self, _data: &[u8], _target: &SendTarget) -> std::io::Result<()> {
            Ok(())
        }
        fn can_handle(&self, _locator: &Locator) -> bool {
            true
        }
        fn advertised_metatraffic_unicast_locators(&self) -> Vec<Locator> {
            Vec::new()
        }
        fn advertised_default_unicast_locators(&self) -> Vec<Locator> {
            Vec::new()
        }
        fn take_discovery_multicast_source(&self) -> Option<MessageSource> {
            None
        }
        fn take_discovery_unicast_source(&self) -> Option<MessageSource> {
            None
        }
        fn take_user_data_unicast_source(&self) -> Option<MessageSource> {
            None
        }
        fn port(&self) -> u16 {
            0
        }
        fn participant_id(&self) -> u32 {
            0
        }
        fn close(&self) {}
    }

    const FRAG_TEST_FRAGMENT_SIZE: u16 = 4;
    const FRAG_TEST_SAMPLE_SIZE: u32 = 16; // 4 fragments of FRAG_TEST_FRAGMENT_SIZE bytes

    /// Two `StatefulReader`s on one participant, both matched to one synthetic remote writer,
    /// with `expected_sn` primed so the sample under test delivers immediately instead of
    /// buffering forever. Builtin-kind entity ids sidestep needing a DCPS-connected DataReader
    /// cache: fragment fan-out lives entirely at the RTPS layer and does not care which kind
    /// of reader it is.
    fn two_readers_matched_to_one_fragmented_writer(
    ) -> (Arc<Participant>, UserLogic, Arc<StatefulReader>, Arc<StatefulReader>, Guid, SequenceNumber)
    {
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let transport: Arc<dyn TransportPlugin> = Arc::new(NullTransport);
        let user_logic = UserLogic::new(participant.clone(), transport);

        let writer_guid = Guid::new(
            [0xC0; 12],
            EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
        );
        let sn = SequenceNumber::new(0, 1);

        let make_reader = |key: [u8; 3]| -> Arc<StatefulReader> {
            let entity_id = EntityId::new(key, EntityKind::BUILT_IN_READER_NO_KEY);
            let guid = Guid::new(participant.guid().prefix(), entity_id);
            let reader = Arc::new(StatefulReader::new(
                guid,
                TopicKind::NoKey,
                ReliabilityQosPolicyKind::Reliable,
                Vec::new(),
                Vec::new(),
                entity_id,
                false,
                None,
                None,
                SubscriptionBuiltinTopicData::default(),
                participant.guid(),
            ));
            reader.matched_writer_add(WriterProxy::new(
                writer_guid,
                writer_guid.entity_id(),
                Vec::new(),
                Vec::new(),
                0,
                PublicationBuiltinTopicData::default(),
                reader.get_update_status_callback(),
            ));
            // A fresh WriterProxy starts at `expected_sn = UNKNOWN`. Prime it as if a
            // heartbeat had already announced this writer's first sequence number, so the
            // sample under test delivers immediately instead of buffering forever.
            let proxies = reader.writer_proxies();
            let mut guard = proxies.lock().expect("writer proxies lock");
            if let Some(proxy) = guard.iter_mut().find(|p| p.remote_writer_guid() == writer_guid) {
                proxy.set_expected_sn(sn);
            }
            drop(guard);
            participant.add_reader("frag_test_topic", reader.clone());
            reader
        };

        let reader_a = make_reader([0xA0, 0x00, 0x00]);
        let reader_b = make_reader([0xB0, 0x00, 0x00]);

        (participant, user_logic, reader_a, reader_b, writer_guid, sn)
    }

    /// Feeds one DATA_FRAG submessage into `handle_datafrag_message` directly, addressed to
    /// `reader_id`, carrying `fragments_in_submessage` fragments of `FRAG_TEST_FRAGMENT_SIZE`
    /// bytes starting at `fragment_starting_num`.
    #[allow(clippy::too_many_arguments)]
    fn feed_fragment(
        user_logic: &mut UserLogic,
        participant_prefix: GuidPrefix,
        writer_guid: Guid,
        sn: SequenceNumber,
        reader_id: EntityId,
        fragment_starting_num: u32,
        fragments_in_submessage: u16,
        payload: Vec<u8>,
    ) {
        let mut data_frag = DataFrag::new(
            reader_id,
            writer_guid.entity_id(),
            sn,
            fragment_starting_num,
            fragments_in_submessage,
            FRAG_TEST_FRAGMENT_SIZE,
            FRAG_TEST_SAMPLE_SIZE,
        );
        data_frag.add_serialized_data(SubmessagePayload::Owned(bytes::Bytes::from(payload)));

        let rtps_header = Header::new(writer_guid.prefix());
        let from_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 7400);
        let mut message_receiver = MessageReceiver::new(participant_prefix, &from_addr);
        let ts_header = SubmessageHeader::new(SubmessageId::INFO_TS, 0, 0);
        message_receiver.from_timestamp(&ts_header, &InfoTimestamp::new(Utc::now()));

        user_logic
            .handle_datafrag_message(&rtps_header, &data_frag, &message_receiver)
            .expect("handle_datafrag_message must not error");
    }

    /// The payload of the whole four-fragment sample the tests below assemble.
    fn whole_sample() -> Vec<u8> {
        vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4]
    }

    /// The assembled sample a reader holds at `sn`, if any.
    fn held_sample(
        reader: &Arc<StatefulReader>,
        writer_guid: Guid,
        sn: SequenceNumber,
    ) -> Option<Vec<u8>> {
        reader
            .reader_cache()
            .lock()
            .expect("cache lock")
            .get_change(sn, writer_guid)
            .map(|c| c.data_value().to_vec())
    }

    /// Fragment numbers this reader's ledger still calls missing for `sn`. Empty means the
    /// ledger reads complete.
    fn ledger_missing(
        reader: &Arc<StatefulReader>,
        writer_guid: Guid,
        sn: SequenceNumber,
    ) -> Vec<u32> {
        let proxies = reader.writer_proxies();
        let guard = proxies.lock().expect("writer proxies lock");
        guard
            .iter()
            .find(|p| p.remote_writer_guid() == writer_guid)
            .map(|p| p.get_ascending_missing_fn_list(sn))
            .expect("the writer proxy must exist")
    }

    fn ledger_reads_complete(
        reader: &Arc<StatefulReader>,
        writer_guid: Guid,
        sn: SequenceNumber,
    ) -> bool {
        let proxies = reader.writer_proxies();
        let guard = proxies.lock().expect("writer proxies lock");
        guard
            .iter()
            .find(|p| p.remote_writer_guid() == writer_guid)
            .map(|p| p.all_fragments_received(sn))
            .expect("the writer proxy must exist")
    }

    /// Inserts `count` already-complete buffers. Eviction takes incomplete ones first, so
    /// these push the map over the cap without competing to be the victim.
    fn fill_with_complete_buffers(user_logic: &UserLogic, count: u8) {
        let reader_id = EntityId::new([0xF0, 0x00, 0x00], EntityKind::BUILT_IN_READER_NO_KEY);
        let sn = SequenceNumber::new(0, 1);
        for i in 0..count {
            let writer_guid = Guid::new(
                [i; 12],
                EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );
            let mut buffer =
                FragmentBuffer::new(sn, FRAG_TEST_SAMPLE_SIZE, FRAG_TEST_FRAGMENT_SIZE);
            for fragment in 1..=4u32 {
                buffer.copy_fragment_data(
                    fragment,
                    bytes::Bytes::from(vec![0u8; FRAG_TEST_FRAGMENT_SIZE as usize]),
                );
            }
            assert!(buffer.all_fragments_received(), "filler buffers must not be eviction bait");
            user_logic.fragment_buffers.insert((writer_guid, reader_id, sn), buffer);
        }
    }

    #[test]
    fn a_broadcast_burst_then_each_readers_own_repair_completes_for_both() {
        let (participant, mut user_logic, reader_a, reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();
        let reader_b_id = reader_b.guid().entity_id();

        // First transmission: one UNKNOWN burst, which reaches every reader behind dst_prefix.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            EntityId::UNKNOWN,
            1,
            3,
            vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
        );

        // A directed repair for the last fragment, which completes reader A's buffer alone.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_a_id,
            4,
            1,
            vec![4, 4, 4, 4],
        );

        assert_eq!(
            held_sample(&reader_a, writer_guid, sn),
            Some(whole_sample()),
            "reader A's buffer held the broadcast fragments and its own repair completed it"
        );
        assert_eq!(
            held_sample(&reader_b, writer_guid, sn),
            None,
            "reader B was never sent fragment 4, so nothing may have completed for it yet"
        );
        assert_eq!(
            ledger_missing(&reader_b, writer_guid, sn),
            vec![4],
            "reader B's ledger must still ask for exactly the fragment it has not been sent"
        );

        // Reader B's own repair, after A's completion removed A's buffer. One fragment
        // finishes it only if the burst reached B's own key.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_b_id,
            4,
            1,
            vec![4, 4, 4, 4],
        );

        assert_eq!(
            held_sample(&reader_b, writer_guid, sn),
            Some(whole_sample()),
            "reader B's repair carried one fragment; the other three had to already be in its \
             own buffer, put there by the broadcast burst"
        );
        assert!(
            ledger_missing(&reader_b, writer_guid, sn).is_empty(),
            "reader B's ledger must read complete only now that it holds the sample"
        );
    }

    #[test]
    fn a_repeated_directed_repair_after_completion_delivers_no_duplicate() {
        let (participant, mut user_logic, reader_a, reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();
        let reader_b_id = reader_b.guid().entity_id();

        for reader_id in [EntityId::UNKNOWN, reader_a_id, reader_b_id] {
            let (start, count, payload) = if reader_id == EntityId::UNKNOWN {
                (1, 3, vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3])
            } else {
                (4, 1, vec![4, 4, 4, 4])
            };
            feed_fragment(
                &mut user_logic,
                prefix,
                writer_guid,
                sn,
                reader_id,
                start,
                count,
                payload,
            );
        }

        // A retransmit of reader A's own repair, arriving after the sample already completed.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_a_id,
            4,
            1,
            vec![4, 4, 4, 4],
        );

        let count_at_sn = |reader: &Arc<StatefulReader>| {
            reader
                .reader_cache()
                .lock()
                .expect("cache lock")
                .get_changes()
                .iter()
                .filter(|c| c.sequence_number() == sn && c.writer_guid() == writer_guid)
                .count()
        };

        assert_eq!(count_at_sn(&reader_a), 1, "reader A must not receive a duplicate");
        assert_eq!(count_at_sn(&reader_b), 1, "reader B must not receive a duplicate");

        // The retransmit recreated A's buffer at one fragment of four. Nothing completes or
        // removes it, so it is the stale entry the cap picks first.
        assert!(
            user_logic.fragment_buffers.contains_key(&(writer_guid, reader_a_id, sn)),
            "a repair arriving after completion recreates the buffer, and that entry is the \
             precondition the eviction reconciliation has to discriminate"
        );
        assert!(
            ledger_reads_complete(&reader_a, writer_guid, sn),
            "the retransmit must not knock reader A's ledger back off complete"
        );
    }

    #[test]
    fn evicting_a_stale_buffer_keeps_the_delivered_samples_ledger_complete() {
        let (participant, mut user_logic, reader_a, reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();
        let reader_b_id = reader_b.guid().entity_id();

        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            EntityId::UNKNOWN,
            1,
            3,
            vec![1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3],
        );
        for reader_id in [reader_a_id, reader_b_id] {
            feed_fragment(
                &mut user_logic,
                prefix,
                writer_guid,
                sn,
                reader_id,
                4,
                1,
                vec![4, 4, 4, 4],
            );
        }
        // The retransmit that leaves a stale one-fragment buffer behind for reader A.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_a_id,
            4,
            1,
            vec![4, 4, 4, 4],
        );
        assert_eq!(
            user_logic.fragment_buffers.len(),
            1,
            "only the stale entry may remain: both delivered buffers were removed"
        );
        assert!(held_sample(&reader_a, writer_guid, sn).is_some());

        // One over the cap, with the stale entry the only incomplete buffer in the map.
        fill_with_complete_buffers(&user_logic, FRAGMENT_BUFFER_LIMIT as u8);
        assert_eq!(user_logic.fragment_buffers.len(), FRAGMENT_BUFFER_LIMIT + 1);

        // The cap check runs before this datagram's own insert.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn.next(),
            reader_b_id,
            1,
            1,
            vec![9, 9, 9, 9],
        );
        assert!(
            !user_logic.fragment_buffers.contains_key(&(writer_guid, reader_a_id, sn)),
            "the stale entry must be the buffer eviction chose"
        );

        assert!(
            ledger_reads_complete(&reader_a, writer_guid, sn),
            "reader A already holds this sample; retracting its arrival record would drag the \
             ACKNACK base back below a sequence number it has, and reopen a NACK_FRAG for it"
        );
        assert!(
            ledger_missing(&reader_a, writer_guid, sn).is_empty(),
            "a complete ledger must report nothing missing"
        );
    }

    #[test]
    fn evicting_one_readers_buffer_leaves_the_other_readers_ledger_alone() {
        let (participant, mut user_logic, reader_a, reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();
        let reader_b_id = reader_b.guid().entity_id();

        // Both readers take fragment 1 from the same burst.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            EntityId::UNKNOWN,
            1,
            1,
            vec![1, 1, 1, 1],
        );
        // Only reader B takes fragment 2, which also makes its buffer the more recently used,
        // so eviction reaches for reader A's first.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_b_id,
            2,
            1,
            vec![2, 2, 2, 2],
        );

        fill_with_complete_buffers(&user_logic, FRAGMENT_BUFFER_LIMIT as u8 - 1);
        assert_eq!(user_logic.fragment_buffers.len(), FRAGMENT_BUFFER_LIMIT + 1);

        // One more datagram for reader B: the cap check evicts one buffer, reader A's.
        feed_fragment(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_b_id,
            3,
            1,
            vec![3, 3, 3, 3],
        );
        assert!(
            !user_logic.fragment_buffers.contains_key(&(writer_guid, reader_a_id, sn)),
            "reader A's buffer must be the one evicted"
        );
        assert!(
            user_logic.fragment_buffers.contains_key(&(writer_guid, reader_b_id, sn)),
            "reader B's buffer must survive, which is what makes its ledger worth keeping"
        );

        assert_eq!(
            ledger_missing(&reader_a, writer_guid, sn),
            vec![1, 2, 3, 4],
            "reader A lost its bytes, so it must go back to asking for the whole sample"
        );
        assert_eq!(
            ledger_missing(&reader_b, writer_guid, sn),
            vec![4],
            "reader B's buffer still holds fragments 1..=3; retracting its record would make it \
             re-request bytes it has, and the writer resend them into a buffer that already \
             counted them"
        );
    }

    // --- Fragment reassembly: eviction must report exactly what it removed ---

    #[test]
    fn eviction_returns_the_keys_it_removed() {
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let transport: Arc<dyn TransportPlugin> = Arc::new(NullTransport);
        let user_logic = UserLogic::new(participant, transport);

        // More incomplete buffers than the cap, each under its own writer/reader/SN key.
        let reader_id = EntityId::new([0xA0, 0x00, 0x00], EntityKind::BUILT_IN_READER_NO_KEY);
        for i in 0..5u8 {
            let writer_guid = Guid::new(
                [i; 12],
                EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
            );
            let sn = SequenceNumber::new(0, 1);
            let mut buffer =
                FragmentBuffer::new(sn, FRAG_TEST_SAMPLE_SIZE, FRAG_TEST_FRAGMENT_SIZE);
            buffer.copy_fragment_data(
                1,
                bytes::Bytes::from(vec![0u8; FRAG_TEST_FRAGMENT_SIZE as usize]),
            );
            user_logic.fragment_buffers.insert((writer_guid, reader_id, sn), buffer);
        }

        let before: std::collections::HashSet<_> =
            user_logic.fragment_buffers.iter().map(|entry| *entry.key()).collect();
        let evicted = user_logic.cleanup_old_fragment_buffers(2, &[]);
        let after: std::collections::HashSet<_> =
            user_logic.fragment_buffers.iter().map(|entry| *entry.key()).collect();

        assert_eq!(after.len(), 2, "cleanup must leave exactly the cap behind");
        let evicted_set: std::collections::HashSet<_> = evicted.iter().copied().collect();
        assert_eq!(evicted_set.len(), evicted.len(), "no key is reported evicted twice");
        assert_eq!(
            before.difference(&after).copied().collect::<std::collections::HashSet<_>>(),
            evicted_set,
            "the returned keys must be exactly the ones that left the map"
        );
    }

    // --- Fragment reassembly: the cap check must not evict the buffer it was called for ---

    #[test]
    fn the_arriving_key_is_never_its_own_eviction_victim() {
        let buffers: DashMap<(Guid, EntityId, SequenceNumber), FragmentBuffer> = DashMap::new();
        let reader_id = EntityId::new([0xA0, 0x00, 0x00], EntityKind::BUILT_IN_READER_NO_KEY);
        let sn = SequenceNumber::new(0, 1);

        // Insertion order is last_updated order, so keys[0..2] are the two that would
        // normally be picked first -- excluded together, as a multi-reader fan-out writes them.
        let keys: Vec<_> = (0..6u8)
            .map(|i| {
                let writer_guid = Guid::new(
                    [i; 12],
                    EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
                );
                let key = (writer_guid, reader_id, sn);
                let mut buffer =
                    FragmentBuffer::new(sn, FRAG_TEST_SAMPLE_SIZE, FRAG_TEST_FRAGMENT_SIZE);
                buffer.copy_fragment_data(
                    1,
                    bytes::Bytes::from(vec![0u8; FRAG_TEST_FRAGMENT_SIZE as usize]),
                );
                buffers.insert(key, buffer);
                key
            })
            .collect();

        let arriving_keys = [keys[0], keys[1]];
        let victims = select_eviction_victims(&buffers, 3, &arriving_keys);

        for key in &arriving_keys {
            assert!(!victims.contains(key), "no key being written may be its own eviction victim");
        }
        assert_eq!(
            victims,
            vec![keys[2], keys[3], keys[4]],
            "eviction must move on to the next-oldest keys once every arriving key is excluded"
        );
    }

    /// One reader matched to `n` distinct synthetic writers, all reliable and primed past
    /// `expected_sn` so every sample under test can deliver on arrival instead of buffering.
    fn reader_matched_to_n_fragmented_writers(
        n: usize,
    ) -> (Arc<Participant>, UserLogic, Arc<StatefulReader>, Vec<Guid>, SequenceNumber) {
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let transport: Arc<dyn TransportPlugin> = Arc::new(NullTransport);
        let user_logic = UserLogic::new(participant.clone(), transport);

        let reader_entity_id =
            EntityId::new([0xE0, 0x00, 0x00], EntityKind::BUILT_IN_READER_NO_KEY);
        let reader = Arc::new(StatefulReader::new(
            Guid::new(participant.guid().prefix(), reader_entity_id),
            TopicKind::NoKey,
            ReliabilityQosPolicyKind::Reliable,
            Vec::new(),
            Vec::new(),
            reader_entity_id,
            false,
            None,
            None,
            SubscriptionBuiltinTopicData::default(),
            participant.guid(),
        ));

        let sn = SequenceNumber::new(0, 1);
        let writer_guids: Vec<Guid> = (0..n)
            .map(|i| {
                Guid::new(
                    [i as u8; 12],
                    EntityId::new([0x02, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
                )
            })
            .collect();
        for &writer_guid in &writer_guids {
            reader.matched_writer_add(WriterProxy::new(
                writer_guid,
                writer_guid.entity_id(),
                Vec::new(),
                Vec::new(),
                0,
                PublicationBuiltinTopicData::default(),
                reader.get_update_status_callback(),
            ));
            let proxies = reader.writer_proxies();
            let mut guard = proxies.lock().expect("writer proxies lock");
            if let Some(proxy) = guard.iter_mut().find(|p| p.remote_writer_guid() == writer_guid) {
                proxy.set_expected_sn(sn);
            }
        }
        participant.add_reader("frag_cap_test_topic", reader.clone());

        (participant, user_logic, reader, writer_guids, sn)
    }

    #[test]
    fn cap_plus_one_concurrent_reassemblies_all_complete_with_interleaved_arrival() {
        const FRAGMENTS_PER_SAMPLE: u32 = 4; // FRAG_TEST_SAMPLE_SIZE / FRAG_TEST_FRAGMENT_SIZE
        let n = FRAGMENT_BUFFER_LIMIT + 1;
        let (participant, mut user_logic, reader, writer_guids, sn) =
            reader_matched_to_n_fragmented_writers(n);
        let prefix = participant.guid().prefix();
        let payload = whole_sample();
        let fragment_bytes = |lo: u32, hi: u32| -> Vec<u8> {
            let start = (lo - 1) as usize * FRAG_TEST_FRAGMENT_SIZE as usize;
            let end = hi as usize * FRAG_TEST_FRAGMENT_SIZE as usize;
            payload[start..end].to_vec()
        };

        // One fragment per writer per pass, round robin: keeps all N buffers concurrently
        // open past the cap, unlike a sequential per-writer burst.
        for pass in 1..=FRAGMENTS_PER_SAMPLE {
            for &writer_guid in &writer_guids {
                if held_sample(&reader, writer_guid, sn).is_some() {
                    continue;
                }
                feed_fragment(
                    &mut user_logic,
                    prefix,
                    writer_guid,
                    sn,
                    EntityId::UNKNOWN,
                    pass,
                    1,
                    fragment_bytes(pass, pass),
                );
            }
        }

        // Resend each straggler's exact missing span, like a real NACK_FRAG -- not the
        // whole sample, or a self-eviction would be masked by the same call refilling
        // everything regardless of the defect.
        for _ in 0..8 {
            let mut all_delivered = true;
            for &writer_guid in &writer_guids {
                if held_sample(&reader, writer_guid, sn).is_some() {
                    continue;
                }
                all_delivered = false;
                let missing = ledger_missing(&reader, writer_guid, sn);
                let lo = *missing.first().expect("not-yet-delivered writer has something missing");
                let hi = *missing.last().expect("not-yet-delivered writer has something missing");
                feed_fragment(
                    &mut user_logic,
                    prefix,
                    writer_guid,
                    sn,
                    EntityId::UNKNOWN,
                    lo,
                    (hi - lo + 1) as u16,
                    fragment_bytes(lo, hi),
                );
            }
            if all_delivered {
                break;
            }
        }

        let delivered = writer_guids
            .iter()
            .filter(|&&writer_guid| held_sample(&reader, writer_guid, sn).is_some())
            .count();
        assert_eq!(
            delivered, n,
            "every one of the {n} concurrent reassemblies must complete; falling short means \
             the cap check evicted a buffer out from under the fragment that was about to fill it"
        );
    }
}
