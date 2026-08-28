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
    FragmentInfo, Reader, ReaderCallbackLease, StatefulReader, StatelessReader, WriterProxy,
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

/// How many in-progress fragmented samples a participant holds before the oldest are evicted.
/// One entry per (writer, reader, sample), so several readers of one topic each take a slot.
const FRAGMENT_BUFFER_LIMIT: usize = 128;

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
    /// Re-asks left before the periodic heartbeat takes over again. Bounded so a writer that
    /// has gone away, with its proxy still matched, cannot be re-asked forever.
    retries_left: u32,
    /// Delay before re-asking when a request got no reply. From QoS, but carried here rather
    /// than re-read on re-arm, because the request itself is rebuilt from scratch each time.
    retry_delay: Duration,
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

/// Arms `request` to fire after `delay`, re-arming itself at `request.retry_delay` for as long
/// as fragments stay missing.
///
/// A named function rather than a self-referencing closure: the callback cannot clone itself, and
/// a non-repeating timer is dropped after it triggers, so re-adding the same `TimerId` from
/// inside the callback lands on the next tick against an empty slot.
///
/// A zero `delay` still goes through the timer, firing on its next pass: the arming site holds
/// the reader's `writer_proxies` lock and `fire()` takes it again, so firing inline would
/// deadlock.
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
            // `next` is moved into the call below, so its Copy field is read out first.
            let retry_delay = next.retry_delay;
            schedule_nackfrag(next, retry_delay);
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

/// Receive buffer assumed for a peer that advertised none and when this participant's own
/// transport reports none either. Low enough that no real socket is smaller.
const RECEIVE_BUFFER_FLOOR_BYTES: usize = 128 * 1024;

/// IPv4 (20) + UDP (8) headers. The kernel charges the receive buffer for these alongside the
/// RTPS message, so a window that counted only RTPS bytes would overshoot the socket.
const UDP_IP_HEADER_BYTES: usize = 28;

/// Everything `create_data_frag_msg` writes except the payload and its alignment padding:
/// RTPS header (20) + INFO_DST (4 + 12) + INFO_TS (4 + 8) + the DATA_FRAG submessage header (4)
/// and its fixed fields (32). `data_frag_datagram_bytes_match_the_builder` pins this down.
const DATA_FRAG_FIXED_BYTES: usize = 20 + 16 + 12 + 4 + 32;

/// A piggybacked HEARTBEAT: submessage header (4) plus its fixed body (28).
const HEARTBEAT_SUBMESSAGE_BYTES: usize = 32;

/// Wire bytes one writer may put toward one remote participant in a single send: two thirds of
/// the receive buffer that participant reports, the last third left for the per-datagram
/// overhead the kernel charges on top of the bytes counted here.
///
/// Resolution order is the peer's advertised value, then our own socket's, then a floor. A
/// value that failed validation at parse arrives as `None`, indistinguishable from absence.
fn receive_window_bytes(advertised: Option<usize>, own: Option<usize>) -> usize {
    advertised.or(own).unwrap_or(RECEIVE_BUFFER_FLOOR_BYTES).saturating_mul(2) / 3
}

/// What one DATA_FRAG datagram costs the peer's receive buffer.
fn data_frag_datagram_bytes(payload_bytes: usize, with_heartbeat: bool) -> usize {
    DATA_FRAG_FIXED_BYTES
        + payload_bytes.next_multiple_of(4)
        + if with_heartbeat { HEARTBEAT_SUBMESSAGE_BYTES } else { 0 }
        + UDP_IP_HEADER_BYTES
}

/// The leading part of `plan` that fits in `window_remaining` wire bytes, with the heartbeat
/// flag moved onto the last entry kept. Returns that prefix and the bytes it costs.
///
/// A datagram is never split: an entry that does not fit whole ends the window, and the rest is
/// dropped rather than deferred -- the reader re-asks for it once the window's heartbeat lands.
/// Room for that heartbeat is reserved against every candidate, since whichever entry ends the
/// window has to carry it.
///
/// One datagram addressed to several locators of one participant is written into that
/// participant's one socket once per locator, so each entry costs `locator_count` times its
/// size. `at_least_one` keeps a spent window from stalling a burst entirely.
fn bound_fragment_plan(
    plan: &[(u32, u32, bool)],
    fragment_size: usize,
    sample_size: usize,
    locator_count: usize,
    window_remaining: usize,
    at_least_one: bool,
    with_heartbeat: bool,
) -> (Vec<(u32, u32, bool)>, usize) {
    let heartbeat_bytes =
        if with_heartbeat { HEARTBEAT_SUBMESSAGE_BYTES * locator_count } else { 0 };

    let mut charged = 0usize;
    let mut kept = 0usize;
    for &(fragment_num, count, _) in plan {
        let offset = (fragment_num.saturating_sub(1) as usize).saturating_mul(fragment_size);
        let payload =
            sample_size.saturating_sub(offset).min((count as usize).saturating_mul(fragment_size));
        let cost = data_frag_datagram_bytes(payload, false).saturating_mul(locator_count);

        if charged + cost + heartbeat_bytes > window_remaining && !(at_least_one && kept == 0) {
            break;
        }
        charged += cost;
        kept += 1;
    }

    let mut bounded: Vec<(u32, u32, bool)> = plan[..kept].to_vec();
    if let Some(last) = bounded.last_mut() {
        last.2 = true;
        charged += heartbeat_bytes;
    }
    (bounded, charged)
}

/// How long a charge keeps counting against a peer's window with nothing heard back from it.
///
/// Longer than one whole reader retry cycle -- `nack_frag_response_delay` then
/// `nack_frag_retry_delay`, 205ms at their defaults -- so a reader whose first NACK_FRAG was lost
/// still gets to release this charge by answering. Expiring any sooner would make the timeout,
/// rather than the answer, the usual way a lost round recovers, and leave this a primary path
/// instead of the safety net it is.
const SEND_CREDIT_BACKSTOP: Duration = Duration::from_millis(250);

/// Wire bytes charged toward one remote participant that have not been shown to have drained.
struct SendCredit {
    spent: usize,
    /// When this accumulation started. Not the last charge: the backstop measures the age of the
    /// oldest byte still counted, so refreshing it on every charge would stop it firing.
    since: Instant,
}

/// Bytes of `spent` that still count against the window at `now`.
///
/// Zero once `backstop` has elapsed. Only the peer's own ACKNACK or NACK_FRAG proves the bytes
/// left its socket, and a peer that has gone silent never sends one -- without this the writer
/// would hold a spent window against it for good.
///
/// Takes `now` rather than reading the clock so the boundary is testable with no elapsed time,
/// the same shape as `should_accept_count`.
fn carried_spend(spent: usize, since: Instant, now: Instant, backstop: Duration) -> usize {
    if now.duration_since(since) >= backstop {
        0
    } else {
        spent
    }
}

/// Wire bytes already spent toward each remote participant.
///
/// Keyed by participant, not by reader: the receive buffer belongs to the participant's socket,
/// and several readers behind one participant share it. A first transmission addressed to
/// `ENTITYID_UNKNOWN` is also one datagram serving all of them.
///
/// The window itself is resolved per call, but the spend is carried in `credit`, which the
/// participant owns. Without that, every send call opened a fresh full window, and the peer's
/// socket -- which drains at the peer's pace, not ours -- saw the sum of them.
struct SendWindows {
    /// This participant's own receive buffer, the fallback for a peer that does not advertise.
    own: Option<usize>,
    /// (window, bytes charged in this call) per destination, resolved on first use. The second
    /// element feeds `at_least_one` only; the window arithmetic reads `credit`.
    state: HashMap<GuidPrefix, (usize, usize)>,
    /// Shared with every other send call in this participant. `None` for a builtin writer, which
    /// spends outside the shared budget so a user writer's burst cannot delay discovery.
    credit: Option<Arc<DashMap<GuidPrefix, SendCredit>>>,
    backstop: Duration,
}

impl SendWindows {
    fn new(
        transport: &dyn TransportPlugin,
        credit: Option<Arc<DashMap<GuidPrefix, SendCredit>>>,
    ) -> Self {
        Self {
            own: transport.advertised_receive_buffer_size(),
            state: HashMap::new(),
            credit,
            backstop: send_credit_backstop(),
        }
    }

    /// The shared budget, if this send is one whose charge can ever be released.
    ///
    /// Only a reliable reader answers, and only its answer releases the charge: a writer skips
    /// the heartbeat for a non-reliable proxy entirely, so a best-effort burst would leave its
    /// charge standing until the backstop and throttle itself to one datagram per backstop
    /// period. Best-effort therefore keeps the per-call accounting this window has always used.
    fn shared(&self, reliable: bool) -> Option<&Arc<DashMap<GuidPrefix, SendCredit>>> {
        if reliable {
            self.credit.as_ref()
        } else {
            None
        }
    }

    /// Bytes still allowed toward `dst`, and whether nothing has gone out to it yet in this send.
    ///
    /// The window is resolved before `credit` is touched: `remote_receive_buffer_size` locks the
    /// participant's proxy list, and `credit` has to stay a leaf.
    fn remaining(
        &mut self,
        participant: &Participant,
        dst: GuidPrefix,
        reliable: bool,
    ) -> (usize, bool) {
        let own = self.own;
        let (window, charged) = *self.state.entry(dst).or_insert_with(|| {
            (receive_window_bytes(participant.remote_receive_buffer_size(dst), own), 0)
        });
        let carried = match self.shared(reliable) {
            Some(credit) => credit
                .get(&dst)
                .map(|c| carried_spend(c.spent, c.since, Instant::now(), self.backstop))
                .unwrap_or(0),
            None => charged,
        };
        (window.saturating_sub(carried), charged == 0)
    }

    fn charge(&mut self, dst: GuidPrefix, bytes: usize, reliable: bool) {
        if let Some((_, charged)) = self.state.get_mut(&dst) {
            *charged += bytes;
        }
        let Some(credit) = self.shared(reliable) else {
            return;
        };
        let now = Instant::now();
        let backstop = self.backstop;
        // `entry` and not get-then-insert: two application threads can be in
        // `send_unsent_changes_of_stateful_writer` for different writers at once, and a split
        // read-modify-write would let both charge against the same starting value.
        credit
            .entry(dst)
            .and_modify(|c| {
                let carried = carried_spend(c.spent, c.since, now, backstop);
                if carried == 0 {
                    c.since = now;
                }
                c.spent = carried.saturating_add(bytes);
            })
            .or_insert(SendCredit { spent: bytes, since: now });
    }
}

/// The backstop from `INT2DDS_SEND_CREDIT_BACKSTOP_MS`, or `SEND_CREDIT_BACKSTOP`.
fn send_credit_backstop() -> Duration {
    crate::common::env::get_send_credit_backstop_ms_override()
        .map(|ms| Duration::from_millis(u64::from(ms)))
        .unwrap_or(SEND_CREDIT_BACKSTOP)
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
    /// Wire bytes charged toward each remote participant, shared by every send call so that
    /// consecutive and concurrent sends draw on one budget per peer instead of each opening a
    /// fresh window. Released when that peer answers, aged out by `SEND_CREDIT_BACKSTOP`.
    send_credit: Arc<DashMap<GuidPrefix, SendCredit>>,
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
            send_credit: Arc::new(DashMap::new()),
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
        let mut windows =
            SendWindows::new(self.transport.as_ref(), self.shared_send_credit(stateful_writer));
        let dst_prefix = remote_reader_guid.prefix();

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
                let plan = fragment_send_plan(&[(1, total_fragments)], frags_per_msg);

                // Bound the resend by what this participant's receive buffer still allows.
                let (remaining, untouched) = windows.remaining(&participant, dst_prefix, reliable);
                let (plan, charged) = bound_fragment_plan(
                    &plan,
                    a_change.fragment_size() as usize,
                    a_change.data_value().len(),
                    self.locators_to_send_to(locators.iter()).len(),
                    remaining,
                    untouched,
                    piggyback,
                );
                windows.charge(dst_prefix, charged, reliable);

                for (fragment_num, count, is_last) in plan {
                    let Some(fragment_data) =
                        a_change.get_fragment_range_data(fragment_num, count as u16)
                    else {
                        continue;
                    };

                    // The heartbeat rides the last datagram of the window, not only the last of
                    // the sample: without it the reader has no trigger to ask for the rest.
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

        // Kept apart from `piggyback`: a reliable reader with piggyback disabled still answers
        // off the periodic heartbeat, so its charge is still releasable.
        let reliable = reader_proxy.is_reliable();
        let piggyback = reliable && !stateful_writer.disable_piggyback_heartbeat();
        // Same for every change below, and one datagram is written once per locator.
        let locator_count = self.locators_to_send_to(reader_proxy.unicast_locator_list()).len();
        let dst_prefix = remote_reader_guid.prefix();

        let timestamp = Utc::now();
        let participant = self.get_upgraded_participant()?;
        let mut windows =
            SendWindows::new(self.transport.as_ref(), self.shared_send_credit(stateful_writer));

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

            let plan = fragment_send_plan(&runs, frags_per_msg);

            // A request larger than one window is served as far as the window reaches; the rest
            // is dropped, and the reader re-asks once this window's heartbeat arrives.
            let (remaining, untouched) = windows.remaining(&participant, dst_prefix, reliable);
            let (plan, charged) = bound_fragment_plan(
                &plan,
                change.fragment_size() as usize,
                change.data_value().len(),
                locator_count,
                remaining,
                untouched,
                piggyback,
            );
            windows.charge(dst_prefix, charged, reliable);

            for (fragment_num, count, is_last) in plan {
                // The heartbeat rides the last datagram of the window, so the reader always has
                // a trigger for the next NACK_FRAG.
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

        // Shared by the batched and the per-reader path below, so two readers behind one
        // participant draw on that participant's one receive buffer rather than one each.
        let mut windows =
            SendWindows::new(self.transport.as_ref(), self.shared_send_credit(writer));

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
                let plan = fragment_send_plan(&[(1, total_fragments)], frags_per_msg);

                // Bound the burst by this participant's receive buffer; the fragments beyond
                // the window are dropped, and the reader asks for them off the heartbeat below.
                let is_reliable_batch = members.iter().any(|(_, plan)| plan.reliable);
                let (remaining, untouched) =
                    windows.remaining(&participant, dst_prefix, is_reliable_batch);
                let (plan, charged) = bound_fragment_plan(
                    &plan,
                    a_change.fragment_size() as usize,
                    a_change.data_value().len(),
                    self.locators_to_send_to(locators.iter()).len(),
                    remaining,
                    untouched,
                    is_piggyback_wanted,
                );
                windows.charge(dst_prefix, charged, is_reliable_batch);

                for (fragment_num, count, is_last) in plan {
                    let Some(fragment_data) =
                        a_change.get_fragment_range_data(fragment_num, count as u16)
                    else {
                        continue;
                    };

                    // The heartbeat rides the last datagram of the window, not only the last of
                    // the sample: without it the reader has no trigger to ask for the rest.
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
                            let plan = fragment_send_plan(&[(1, total_fragments)], frags_per_msg);

                            // Bound the burst by this participant's receive buffer; what is left
                            // over is dropped and re-requested off the window's heartbeat.
                            let (remaining, untouched) =
                                windows.remaining(&participant, reader_guid.prefix(), reliable);
                            let (plan, charged) = bound_fragment_plan(
                                &plan,
                                a_change.fragment_size() as usize,
                                a_change.data_value().len(),
                                self.locators_to_send_to(locators.iter()).len(),
                                remaining,
                                untouched,
                                reliable && piggyback,
                            );
                            windows.charge(reader_guid.prefix(), charged, reliable);

                            for (fragment_num, count, is_last) in plan {
                                let Some(fragment_data) =
                                    a_change.get_fragment_range_data(fragment_num, count as u16)
                                else {
                                    continue;
                                };

                                // The heartbeat rides the last datagram of the window, not only
                                // the last of the sample, or the reader has no trigger to re-ask.
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
                    // No receive window either: with no heartbeat and no repair path, a bounded
                    // burst would drop the remainder with nothing able to ask for it back.
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
            // A reader that is already caught up needs no heartbeat. Without this one
            // unresponsive reader keeps the writer heartbeating every matched participant.
            if !reader_proxy.unacked_changes(&history_cache) {
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

    /// The budget `stateful_writer`'s sends draw on, or `None` for a builtin writer.
    ///
    /// SPDP already sends outside the window entirely. Keeping the rest of discovery out of the
    /// shared budget too means a user writer's burst cannot leave a fragmented SEDP or
    /// TypeLookup change waiting on it.
    fn shared_send_credit(
        &self,
        stateful_writer: &StatefulWriter,
    ) -> Option<Arc<DashMap<GuidPrefix, SendCredit>>> {
        (!stateful_writer.guid().entity_id().entity_kind.is_built_in())
            .then(|| self.send_credit.clone())
    }

    /// Drop what is charged against `dst`, because `dst` just answered.
    ///
    /// An ACKNACK or NACK_FRAG can only be built after the peer read the heartbeat the window
    /// ended with, so the datagrams charged for that window have left its socket.
    ///
    /// The counter is per destination and shared, so this hands back one window in total, not one
    /// per writer -- a NACK_FRAG is loss evidence as much as drain evidence, and re-arming a full
    /// burst per writer on loss is what this whole budget exists to stop.
    fn release_send_credit(&self, dst: GuidPrefix) {
        self.send_credit.remove(&dst);
    }

    /// Drop the budget held for a participant that is gone.
    ///
    /// Single key and not `retain`: `retain` takes every shard's write lock, and peer loss must
    /// not stall sends toward everyone else.
    pub(crate) fn forget_send_credit_for_participant(&self, prefix: GuidPrefix) {
        self.send_credit.remove(&prefix);
    }

    /// Drop every reassembly buffer addressed to `reader_id`. Called when that reader is
    /// removed: it can never complete these, and `get_matched_readers` will not find it again,
    /// so nothing but this would ever reclaim them.
    pub(crate) fn forget_fragment_buffers_for_reader(&self, reader_id: EntityId) {
        self.fragment_buffers.retain(|key, _| key.1 != reader_id);
    }

    /// Drop every reassembly buffer waiting on `writer_guid`. Called when that writer is
    /// unmatched: it will send no further DATA_FRAG, so nothing but this would ever reclaim them.
    pub(crate) fn forget_fragment_buffers_for_writer(&self, writer_guid: Guid) {
        self.fragment_buffers.retain(|key, _| key.0 != writer_guid);
    }

    /// The locators one message is actually written to: the highest-priority kind reachable on
    /// both sides (SHM > TCP > UDP), or the untouched list when none of them matches.
    fn locators_to_send_to<'a, T>(&self, locators: T) -> Vec<&'a Locator>
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
        pick(Locator::is_shm)
            .or_else(|| pick(Locator::is_tcp))
            .or_else(|| pick(Locator::is_udp))
            .unwrap_or(locators)
    }

    /// Send `buffer` via the highest-priority transport reachable on both
    /// sides (SHM > TCP > UDP); a peer advertising multiple transports gets
    /// a single copy. Associated fn so `&self`-less closures (e.g. the
    /// NACK_FRAG timer) can route through the same path.
    fn send_rtps_message_to_locators<'a, T>(&self, locators: T, buffer: &[u8]) -> RtpsResult<()>
    where
        T: IntoIterator<Item = &'a Locator>,
    {
        let locators = self.locators_to_send_to(locators);

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
    ) -> RtpsResult<Vec<ReaderCallbackLease>> {
        let participant = self.get_upgraded_participant()?;
        let mut matched_readers: Vec<ReaderCallbackLease> = Vec::new();

        if reader_entity_id != EntityId::UNKNOWN {
            let reader = participant
                .find_reader_callback_lease_from_entity_id(reader_entity_id)
                .ok_or_else(|| {
                    RtpsError::new(
                        RtpsErrorCode::RtpsEntityNotFound,
                        "No reader found for user data".to_string(),
                    )
                })?;

            // Intra-participant directed sample whose local writer was already destroyed while
            // this just-sent sample was still in flight has no reliable retransmit (so no
            // duplicate is possible); deliver the already-accepted sample instead of dropping it.
            // The matched path and cross-participant samples are unchanged.
            let should_deliver = reader.matched_writer_is_matched(remote_writer_guid)
                || (remote_writer_guid.prefix() == participant.guid().prefix()
                    && participant
                        .find_writer_from_entity_id(remote_writer_guid.entity_id())
                        .is_none());

            if should_deliver {
                matched_readers.push(reader);
            }
        } else {
            matched_readers.extend(
                participant
                    .find_reader_callback_leases_matched_with_remote_writer(remote_writer_guid)?,
            );
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

    /// Undo this reader's fragment arrival record for one sample. Used wherever
    /// `handle_datafrag_message` bails between `mark_frag_received` reporting a sample whole and
    /// actually delivering it, so the ledger does not outlive the buffer that would have done so.
    fn forget_reader_frag_ledger(
        reader: &Arc<dyn Reader + Send + Sync>,
        remote_writer_guid: Guid,
        seq_num: SequenceNumber,
    ) {
        let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() else {
            return;
        };
        let writer_proxies = stateful_reader.writer_proxies();
        let Ok(mut matched_writers) = writer_proxies.lock() else {
            return;
        };
        if let Some(writer_proxy) = matched_writers
            .iter_mut()
            .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
        {
            writer_proxy.forget_fragments(seq_num);
        }
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

        let matched_readers = self.get_matched_readers(remote_writer_guid, data.reader_id)?;

        if matched_readers.is_empty() {
            debug!(
                "[DATA] No matched readers found for remote writer: {}, skipping data handling.",
                remote_writer_guid
            );
            return Ok(());
        }

        // Detach from the receive arena before retaining. A `Bytes` slice keeps its
        // whole arena chunk resident, so copy once here and let every matched reader
        // share that right-sized copy.
        let detached_payload = bytes::Bytes::copy_from_slice(&data.serialized_data());

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
            // Every matched reader shares the one detached copy made above, so
            // fanning out costs a refcount bump per reader, not a payload copy.
            change.set_shared_payload(detached_payload.clone());

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
                        retries_left: stateful_reader.nack_frag_max_retries(),
                        retry_delay: stateful_reader.nack_frag_retry_delay().to_std_duration(),
                    };

                    // Re-armed on every accepted heartbeat, which also cancels the retry the
                    // previous round left pending.
                    if let Ok(handler) =
                        TimerHandler::get_instance(participant.guid().prefix()).lock()
                    {
                        handler.remove_timer(request.timer_id());
                    }
                    // Zero by default: a windowed send carries one heartbeat per window, so this
                    // runs once per round and has no burst left to debounce.
                    let response_delay =
                        stateful_reader.nack_frag_response_delay().to_std_duration();
                    schedule_nackfrag(request, response_delay);
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
        // Before the lookups, not after: an unmatched writer, a missing reader proxy, a stale
        // count and the preemptive branch all return early below, and every one of them would
        // otherwise leave the charge standing until the backstop. The peer answered, which is all
        // this needs to know, and its prefix is already in hand.
        self.release_send_credit(rtps_header.guid_prefix());

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

            // Same knob as the NACK_FRAG path, zero by default. `send_requested_changes` drains
            // the requested set on entry, so a repeated trigger costs an empty call, not a
            // duplicate resend.
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
        let matched_readers = self.get_matched_readers(remote_writer_guid, data_frag.reader_id)?;

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
                // Nothing to stamp the change with. The buffer is already gone, so retract the
                // ledger too and move on to the next reader rather than aborting the datagram --
                // an interop peer that never sends INFO_TS would otherwise strand every reader
                // this loop has not reached yet.
                debug!(
                    "[UserLogic] No source timestamp for assembled DataFrag sn={:?} reader={}; \
                     retracting its ledger instead of delivering",
                    data_frag.writer_sn,
                    reader.guid()
                );
                Self::forget_reader_frag_ledger(reader, remote_writer_guid, data_frag.writer_sn);
                continue;
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

            // Fragments were written straight into one buffer, so this is already the
            // assembled sample and carries no receive-arena chunk with it.
            let payload = buffer.into_bytes();

            let mut assembled_change = match reader.reader_cache().lock() {
                Ok(mut cache) => cache.acquire_change(),
                Err(_) => {
                    // The buffer is already gone; retract the ledger so this reader re-asks
                    // instead of acknowledging a sample it never actually received.
                    debug!(
                        "[UserLogic] Poisoned reader cache for reader={} sn={:?}; retracting its \
                         fragment ledger",
                        reader.guid(),
                        data_frag.writer_sn
                    );
                    Self::forget_reader_frag_ledger(
                        reader,
                        remote_writer_guid,
                        data_frag.writer_sn,
                    );
                    continue;
                }
            };
            assembled_change.reset(
                ChangeKind::Alive,
                remote_writer_guid,
                InstanceHandle::NIL,
                data_frag.writer_sn,
                assembled_timestamp,
            );
            assembled_change.set_shared_payload(payload);
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
        // Same reasoning as in `handle_acknack_message`: released before the early returns, and
        // before `handle_rtps_message` can drop the rest of this datagram's submessages on one
        // handler error.
        self.release_send_credit(rtps_header.guid_prefix());

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

        // Zero by default: the repair this answers is bounded by the peer's receive window, and
        // one heartbeat per window means one request per round, so there is nothing to merge.
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

    //-------------------------------------------------------------------------------------
    // Receive-buffer flow control: the window a writer may fill toward one participant.
    //-------------------------------------------------------------------------------------

    /// The deployment shape: 1344-byte fragments, ten per datagram.
    const WINDOW_TEST_FRAG_SIZE: usize = 1344;
    const WINDOW_TEST_FRAGS_PER_MSG: u32 = 10;
    /// `getsockopt(SO_RCVBUF)` on the deployment host, already doubled by the kernel.
    const WINDOW_TEST_ADVERTISED: usize = 425_984;

    #[test]
    fn the_window_resolves_to_two_thirds_of_the_peers_advertised_buffer() {
        assert_eq!(receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), Some(1_000_000)), 283_989);
    }

    #[test]
    fn the_window_falls_back_to_our_own_socket_when_the_peer_does_not_advertise() {
        assert_eq!(receive_window_bytes(None, Some(WINDOW_TEST_ADVERTISED)), 283_989);
    }

    #[test]
    fn the_window_falls_back_to_the_floor_when_neither_side_reports_one() {
        assert_eq!(receive_window_bytes(None, None), 128 * 1024 * 2 / 3);
        assert_eq!(receive_window_bytes(None, None), 87_381);
    }

    #[test]
    fn a_huge_advertised_buffer_does_not_overflow_the_window() {
        assert_eq!(receive_window_bytes(Some(usize::MAX), None), usize::MAX / 3);
    }

    /// The window is charged in wire bytes, so `data_frag_datagram_bytes` has to agree with what
    /// the builder actually emits. Any drift in the message shape shows up here first.
    #[test]
    fn data_frag_datagram_bytes_match_the_builder() {
        let sample_size = 3 * WINDOW_TEST_FRAG_SIZE + 7; // a ragged final fragment
        let change = CacheChange::create_fragmented(
            ChangeKind::Alive,
            Guid::new([0xC0; 12], EntityId::new([1, 0, 0], EntityKind::USER_DEFINED_WRITER_NO_KEY)),
            InstanceHandle::NIL,
            SequenceNumber::new(0, 1),
            &vec![0xA5; sample_size],
            None,
            WINDOW_TEST_FRAG_SIZE,
            WINDOW_TEST_FRAG_SIZE,
        );

        let mut buffer = Vec::new();
        for (fragment_num, count) in [(1u32, 2u16), (4, 1)] {
            for with_heartbeat in [false, true] {
                let heartbeat = with_heartbeat.then(|| {
                    (1u32, SequenceNumber::new(0, 1), SequenceNumber::new(0, 1), false, false)
                });
                MessageCreator::create_data_frag_msg(
                    &change,
                    Guid::new([0xD0; 12], EntityId::UNKNOWN),
                    EntityId::UNKNOWN,
                    change.writer_guid().entity_id(),
                    fragment_num,
                    count,
                    WINDOW_TEST_FRAG_SIZE as u16,
                    sample_size as u32,
                    change.get_fragment_range_data(fragment_num, count).unwrap(),
                    heartbeat,
                    Utc::now(),
                    &mut buffer,
                )
                .unwrap();

                let payload = change.get_fragment_range_data(fragment_num, count).unwrap().len();
                assert_eq!(
                    data_frag_datagram_bytes(payload, with_heartbeat),
                    buffer.len() + UDP_IP_HEADER_BYTES,
                    "modelled size disagrees with the builder for {count} fragments at \
                     {fragment_num}, heartbeat={with_heartbeat}"
                );
            }
        }
    }

    /// The plan for a whole sample at the deployment shape, unbounded.
    fn whole_sample_plan(sample_size: usize) -> Vec<(u32, u32, bool)> {
        let total = sample_size.div_ceil(WINDOW_TEST_FRAG_SIZE) as u32;
        fragment_send_plan(&[(1, total)], fpm(WINDOW_TEST_FRAGS_PER_MSG))
    }

    #[test]
    fn the_window_stops_before_it_would_be_exceeded_and_never_splits_a_datagram() {
        let sample_size = 1024 * 1024;
        let plan = whole_sample_plan(sample_size);
        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);

        let (bounded, charged) =
            bound_fragment_plan(&plan, WINDOW_TEST_FRAG_SIZE, sample_size, 1, window, true, true);

        assert!(bounded.len() < plan.len(), "a 1 MiB sample must not fit in one window");
        assert!(charged <= window, "charged {charged} bytes into a {window}-byte window");

        // Adding back the datagram that was refused must overshoot: the window stops as late as
        // it can, not early.
        let (refused_num, refused_count, _) = plan[bounded.len()];
        let refused_offset = (refused_num as usize - 1) * WINDOW_TEST_FRAG_SIZE;
        let refused_payload =
            (sample_size - refused_offset).min(refused_count as usize * WINDOW_TEST_FRAG_SIZE);
        assert!(
            charged + data_frag_datagram_bytes(refused_payload, false) > window,
            "one more datagram would still have fit, so the window stopped early"
        );

        // Never split: what survives is a prefix of the plan with the same fragment counts.
        for (kept, original) in bounded.iter().zip(plan.iter()) {
            assert_eq!((kept.0, kept.1), (original.0, original.1), "a datagram was resized");
        }
    }

    #[test]
    fn exactly_one_heartbeat_rides_the_last_datagram_of_every_window() {
        let sample_size = 1024 * 1024;
        let plan = whole_sample_plan(sample_size);
        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);

        let mut sent = 0usize;
        let mut rounds = 0usize;
        while sent < plan.len() {
            let (bounded, charged) = bound_fragment_plan(
                &plan[sent..],
                WINDOW_TEST_FRAG_SIZE,
                sample_size,
                1,
                window,
                true,
                true,
            );
            assert!(!bounded.is_empty(), "a window that carries nothing cannot make progress");
            assert!(charged <= window, "round {rounds} charged {charged} into {window}");

            let heartbeats = bounded.iter().filter(|&&(_, _, is_last)| is_last).count();
            assert_eq!(heartbeats, 1, "round {rounds} carried {heartbeats} heartbeats, not one");
            assert!(
                bounded.last().unwrap().2,
                "round {rounds} put the heartbeat somewhere other than the last datagram"
            );

            sent += bounded.len();
            rounds += 1;
        }
        assert!(
            rounds > 1,
            "a 1 MiB sample at this window must take several rounds, took {rounds}"
        );
    }

    #[test]
    fn a_sample_shorter_than_the_window_goes_out_whole_with_one_heartbeat() {
        let sample_size = 20 * WINDOW_TEST_FRAG_SIZE;
        let plan = whole_sample_plan(sample_size);
        let (bounded, _) = bound_fragment_plan(
            &plan,
            WINDOW_TEST_FRAG_SIZE,
            sample_size,
            1,
            receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None),
            true,
            true,
        );
        assert_eq!(bounded.len(), plan.len(), "a small sample must not be truncated");
        assert_eq!(bounded.iter().filter(|&&(_, _, is_last)| is_last).count(), 1);
    }

    #[test]
    fn fanning_one_datagram_out_to_four_locators_costs_the_window_four_times() {
        let sample_size = 1024 * 1024;
        let plan = whole_sample_plan(sample_size);
        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);

        let one =
            bound_fragment_plan(&plan, WINDOW_TEST_FRAG_SIZE, sample_size, 1, window, true, true);
        let four =
            bound_fragment_plan(&plan, WINDOW_TEST_FRAG_SIZE, sample_size, 4, window, true, true);

        assert_eq!(
            four.0.len(),
            one.0.len() / 4,
            "the peer's one socket sees every locator write, so four NICs quarter the window"
        );
        assert!(four.1 <= window);
    }

    #[test]
    fn a_spent_window_lets_one_datagram_through_only_when_nothing_has_gone_out_yet() {
        let sample_size = 1024 * 1024;
        let plan = whole_sample_plan(sample_size);

        let (forced, _) =
            bound_fragment_plan(&plan, WINDOW_TEST_FRAG_SIZE, sample_size, 1, 0, true, true);
        assert_eq!(forced.len(), 1, "a burst must always make progress on its first datagram");

        let (nothing, charged) =
            bound_fragment_plan(&plan, WINDOW_TEST_FRAG_SIZE, sample_size, 1, 0, false, true);
        assert!(nothing.is_empty(), "a window already spent must send nothing more");
        assert_eq!(charged, 0);
    }

    //-------------------------------------------------------------------------------------
    // The same window, measured on the datagrams the send path actually hands the transport.
    //-------------------------------------------------------------------------------------

    /// One datagram as the peer's socket would see it.
    #[derive(Clone, Debug)]
    struct SentDatagram {
        locator: Locator,
        /// RTPS bytes plus the UDP/IP headers the kernel charges alongside them.
        wire_bytes: usize,
        first_fragment: u32,
        fragment_count: u16,
        carries_heartbeat: bool,
    }

    /// A transport that sends nowhere and records what it was asked to send.
    #[derive(Default)]
    struct DatagramRecorder {
        sent: Mutex<Vec<SentDatagram>>,
    }

    impl DatagramRecorder {
        fn take(&self) -> Vec<SentDatagram> {
            std::mem::take(&mut self.sent.lock().unwrap())
        }

        /// Walks the submessages of one datagram, reading the DATA_FRAG range out of the first
        /// one and noting whether a HEARTBEAT rides along.
        fn record(&self, data: &[u8], locator: Locator) {
            let mut offset = 20; // RTPS header
            let mut datagram = SentDatagram {
                locator,
                wire_bytes: data.len() + UDP_IP_HEADER_BYTES,
                first_fragment: 0,
                fragment_count: 0,
                carries_heartbeat: false,
            };
            while offset + 4 <= data.len() {
                let id = data[offset];
                let body_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;
                let body = offset + 4;
                if id == SubmessageId::HEARTBEAT.as_u8() {
                    datagram.carries_heartbeat = true;
                } else if id == SubmessageId::DATA_FRAG.as_u8() && body + 26 <= data.len() {
                    datagram.first_fragment =
                        u32::from_le_bytes(data[body + 20..body + 24].try_into().unwrap());
                    datagram.fragment_count =
                        u16::from_le_bytes([data[body + 24], data[body + 25]]);
                }
                if body_len == 0 {
                    break; // the last submessage runs to the end of the datagram
                }
                offset = body + body_len;
            }
            self.sent.lock().unwrap().push(datagram);
        }
    }

    impl TransportPlugin for DatagramRecorder {
        fn send(&self, data: &[u8], target: &SendTarget) -> std::io::Result<()> {
            if let SendTarget::UserData(locator) = target {
                self.record(data, (*locator).clone());
            }
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

    /// A writer matched to `reader_count` readers behind one remote participant, each reachable
    /// on `locator_count` locators, with that participant advertising `advertised` bytes of
    /// receive buffer (`None` = it did not advertise).
    #[allow(clippy::type_complexity)]
    fn windowed_writer(
        reader_count: usize,
        locator_count: usize,
        advertised: Option<usize>,
    ) -> (Arc<Participant>, UserLogic, Arc<DatagramRecorder>, Arc<StatefulWriter>, GuidPrefix) {
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let recorder = Arc::new(DatagramRecorder::default());
        let transport: Arc<dyn TransportPlugin> = recorder.clone();
        let user_logic = UserLogic::new(participant.clone(), transport);
        participant.set_user_logic(Arc::new(Some(user_logic.clone())));

        let remote_prefix: GuidPrefix = [0xD0; 12];
        let mut proxy_data = SPDPDiscoveredParticipantData::new(
            0,
            remote_prefix,
            crate::rtps::builtin::data::builtin_endpoint_set::BuiltinEndpointSet::new(),
        );
        proxy_data.set_receive_buffer_size(advertised);
        participant.add_remote_participant_proxy_data(proxy_data);

        let writer_entity_id =
            EntityId::new([0x10, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY);
        // `Weak::new()` for the cache: `add_change` would otherwise pump the send path itself,
        // and these tests drive it explicitly one round at a time.
        let writer = Arc::new(StatefulWriter::new(
            Guid::new(participant.guid().prefix(), writer_entity_id),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            writer_entity_id,
            -1,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        ));

        let locators: Vec<Locator> = (0..locator_count)
            .map(|i| Locator::from_ip(Ipv4Addr::new(127, 0, 0, (i + 1) as u8), 7411))
            .collect();
        for r in 0..reader_count {
            let reader_entity_id =
                EntityId::new([0x20 + r as u8, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY);
            writer.matched_reader_add(ReaderProxy::new(
                Guid::new(remote_prefix, reader_entity_id),
                EntityId::UNKNOWN,
                locators.clone(),
                Vec::new(),
                SequenceNumber::new(0, 0),
                SequenceNumber::new(0, 0),
                false,
                true,
                SubscriptionBuiltinTopicData::builtin_reliable(),
                SequenceNumber::new(0, 0),
            ));
        }

        participant.add_writer("window_test_topic", writer.clone()).unwrap();
        (participant, user_logic, recorder, writer, remote_prefix)
    }

    /// Puts one fragmented sample of `sample_size` bytes into `writer`'s cache and returns its
    /// sequence number and total fragment count.
    fn stage_fragmented_sample(
        writer: &Arc<StatefulWriter>,
        sample_size: usize,
    ) -> (SequenceNumber, u32) {
        let sn = SequenceNumber::new(0, 1);
        let change = Arc::new(CacheChange::create_fragmented(
            ChangeKind::Alive,
            writer.guid(),
            InstanceHandle::NIL,
            sn,
            &vec![0xA5; sample_size],
            None,
            WINDOW_TEST_FRAGS_PER_MSG as usize * WINDOW_TEST_FRAG_SIZE,
            WINDOW_TEST_FRAG_SIZE,
        ));
        let total = change.total_fragments();
        writer.writer_cache().lock().unwrap().add_change(change, writer.as_ref()).unwrap();
        (sn, total)
    }

    /// Drives one first transmission and returns what reached the transport.
    fn first_transmission(
        user_logic: &UserLogic,
        writer: &Arc<StatefulWriter>,
        recorder: &DatagramRecorder,
    ) -> Vec<SentDatagram> {
        let cache_lock = writer.writer_cache();
        let cache = cache_lock.lock().unwrap();
        user_logic.send_unsent_changes(writer.as_ref(), &cache).unwrap();
        drop(cache);
        recorder.take()
    }

    fn assert_one_window(round: &[SentDatagram], window: usize, label: &str) {
        let charged: usize = round.iter().map(|d| d.wire_bytes).sum();
        assert!(!round.is_empty(), "{label}: nothing was sent, so the burst cannot progress");
        assert!(charged <= window, "{label}: charged {charged} bytes into a {window}-byte window");

        let heartbeats = round.iter().filter(|d| d.carries_heartbeat).count();
        assert_eq!(heartbeats, 1, "{label}: {heartbeats} heartbeats in one window, expected one");
        assert!(
            round.last().unwrap().carries_heartbeat,
            "{label}: the heartbeat did not ride the last datagram of the window"
        );
    }

    /// A datagram is never split, so the fragments one locator receives in a round tile a
    /// contiguous span: each datagram picks up exactly where the previous one stopped.
    ///
    /// Stated this way rather than as a cap on `fragment_count`, because how many fragments ride
    /// in one DATA_FRAG comes from `INT2DDS_MAX_MESSAGE_SIZE`, which other tests in this binary
    /// set and unset around themselves.
    fn assert_datagrams_tile(round: &[SentDatagram], label: &str) {
        let mut next_expected: HashMap<Locator, u32> = HashMap::new();
        for datagram in round {
            assert!(datagram.fragment_count >= 1, "{label}: an empty DATA_FRAG went out");
            let expected = next_expected.entry(datagram.locator.clone()).or_insert(1);
            assert_eq!(
                datagram.first_fragment, *expected,
                "{label}: a datagram started at {} where {expected} was owed, so the burst \
                 overlapped or left a hole",
                datagram.first_fragment
            );
            *expected += datagram.fragment_count as u32;
        }
    }

    #[test]
    fn a_first_transmission_is_bounded_by_the_peers_advertised_buffer() {
        let (_participant, user_logic, recorder, writer, _) =
            windowed_writer(1, 1, Some(WINDOW_TEST_ADVERTISED));
        let (_sn, total) = stage_fragmented_sample(&writer, 1024 * 1024);

        let round = first_transmission(&user_logic, &writer, &recorder);
        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);
        assert_one_window(&round, window, "first transmission");

        let sent_fragments: u32 = round.iter().map(|d| d.fragment_count as u32).sum();
        assert!(
            sent_fragments < total,
            "the whole {total}-fragment sample went out in one go, so nothing bounded it"
        );
        assert_datagrams_tile(&round, "first transmission");
    }

    #[test]
    fn four_locators_of_one_participant_each_charge_that_participants_window() {
        let (_participant, user_logic, recorder, writer, _) =
            windowed_writer(1, 4, Some(WINDOW_TEST_ADVERTISED));
        stage_fragmented_sample(&writer, 1024 * 1024);
        let round = first_transmission(&user_logic, &writer, &recorder);

        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);
        let charged: usize = round.iter().map(|d| d.wire_bytes).sum();
        assert!(charged <= window, "four locators charged {charged} into a {window}-byte window");

        // Every locator gets the identical burst, so the window buys a quarter of the fragments
        // it would at one locator -- which is right, because the peer's one socket is written to
        // four times per datagram.
        let mut per_locator: HashMap<Locator, Vec<u32>> = HashMap::new();
        for datagram in &round {
            per_locator.entry(datagram.locator.clone()).or_default().push(datagram.first_fragment);
        }
        assert_eq!(per_locator.len(), 4, "each locator must be written to separately");
        let bursts: Vec<Vec<u32>> = per_locator.into_values().collect();
        for burst in &bursts {
            assert_eq!(burst, &bursts[0], "the four locators received different bursts");
        }

        // Four identical bursts inside one window means each burst is a quarter of it.
        let per_datagram = round[0].wire_bytes;
        assert!(
            bursts[0].len() * 4 * per_datagram <= window,
            "one burst of {} datagrams went four times into a {window}-byte window, so the \
             locator writes were not each charged",
            bursts[0].len()
        );
        assert_datagrams_tile(&round, "four locators");
    }

    #[test]
    fn two_readers_behind_one_participant_share_one_window() {
        let (_participant, user_logic, recorder, writer, remote_prefix) =
            windowed_writer(1, 1, Some(WINDOW_TEST_ADVERTISED));

        // A second reader on the same participant but a different locator, so the batcher cannot
        // fold the two into one datagram and each gets its own burst. Counting per reader would
        // let the two bursts total two windows into the one socket they share.
        writer.matched_reader_add(ReaderProxy::new(
            Guid::new(
                remote_prefix,
                EntityId::new([0x30, 0, 0], EntityKind::USER_DEFINED_READER_NO_KEY),
            ),
            EntityId::UNKNOWN,
            vec![Locator::from_ip(Ipv4Addr::new(127, 0, 0, 9), 7411)],
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            SubscriptionBuiltinTopicData::builtin_reliable(),
            SequenceNumber::new(0, 0),
        ));
        // Sized in bytes, at about three fifths of a window, so one burst fits comfortably and
        // two do not however many fragments the ambient datagram budget packs into one
        // DATA_FRAG. The second reader has to come up short, which per-reader accounting
        // would hide.
        stage_fragmented_sample(&writer, 170_000);

        let round = first_transmission(&user_logic, &writer, &recorder);
        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);

        let mut per_locator: HashMap<Locator, usize> = HashMap::new();
        for datagram in &round {
            *per_locator.entry(datagram.locator.clone()).or_default() += 1;
        }
        assert_eq!(per_locator.len(), 2, "the two readers must have taken separate bursts");

        let charged: usize = round.iter().map(|d| d.wire_bytes).sum();
        assert!(
            charged <= window,
            "two readers behind one participant charged {charged} bytes into their shared \
             {window}-byte window"
        );
        // The second reader draws on what the first left, so its burst is the shorter one.
        let mut counts: Vec<usize> = per_locator.into_values().collect();
        counts.sort_unstable();
        assert!(counts[0] < counts[1], "both readers got a full window instead of sharing one");
    }

    #[test]
    fn a_multi_window_sample_completes_one_bounded_round_at_a_time() {
        let (_participant, user_logic, recorder, writer, remote_prefix) =
            windowed_writer(1, 1, Some(WINDOW_TEST_ADVERTISED));
        let sample_size = 1024 * 1024;
        let (sn, total) = stage_fragmented_sample(&writer, sample_size);
        let window = receive_window_bytes(Some(WINDOW_TEST_ADVERTISED), None);

        let mut delivered: BTreeSet<u32> = BTreeSet::new();
        let record = |round: &[SentDatagram], delivered: &mut BTreeSet<u32>, label: &str| {
            assert_one_window(round, window, label);
            for datagram in round {
                delivered.extend(
                    datagram.first_fragment
                        ..datagram.first_fragment + datagram.fragment_count as u32,
                );
            }
        };

        record(&first_transmission(&user_logic, &writer, &recorder), &mut delivered, "round 0");

        // Each further round is the reader asking, off the window's heartbeat, for everything it
        // is still missing -- which for a windowed send is the whole untransmitted tail.
        let reader_guid = {
            let proxies = writer.reader_proxies();
            let guard = proxies.lock().unwrap();
            guard[0].remote_reader_guid()
        };
        let mut rounds = 1;
        while delivered.len() < total as usize {
            assert!(rounds < 20, "a 1 MiB sample should not need {rounds} rounds");
            let missing: Vec<u32> = (1..=total).filter(|n| !delivered.contains(n)).collect();
            {
                let proxies = writer.reader_proxies();
                let mut guard = proxies.lock().unwrap();
                guard[0].requested_fragments_add(sn, 1, total, missing);
            }
            user_logic.send_requested_fragments(writer.guid().entity_id(), reader_guid).unwrap();
            record(&recorder.take(), &mut delivered, &format!("round {rounds}"));
            rounds += 1;
        }

        assert!(rounds > 1, "the sample fit in one round, so no window was in force");
        assert_eq!(delivered.len(), total as usize, "some fragments were never sent");
        assert_eq!(*delivered.first().unwrap(), 1);
        assert_eq!(*delivered.last().unwrap(), total);
        assert_eq!(remote_prefix, [0xD0; 12]);
    }

    #[test]
    fn a_peer_that_does_not_advertise_falls_back_without_stalling() {
        let (_participant, user_logic, recorder, writer, _) = windowed_writer(1, 1, None);
        stage_fragmented_sample(&writer, 1024 * 1024);

        let round = first_transmission(&user_logic, &writer, &recorder);
        // No peer value and a transport that reports none either: the floor applies.
        assert_one_window(&round, receive_window_bytes(None, None), "non-advertising peer");
    }

    // Fragment reassembly: drives `handle_datafrag_message` directly against two readers on
    // one participant, so the defects below are deterministic instead of loss-dependent.

    use crate::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData;
    use crate::common::builtin::topic::subscription_builtin_topic_data::SubscriptionBuiltinTopicData;
    use crate::infrastructure::qos_policy::ReliabilityQosPolicyKind;
    use crate::rtps::builtin::data::spdp_discovered_participant_data::SPDPDiscoveredParticipantData;
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
        // Wire it back onto the participant, as production startup does, so teardown paths
        // that look it up via `user_logic_if_set` can reach the same `fragment_buffers`.
        participant.set_user_logic(Arc::new(Some(user_logic.clone())));

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

    /// Like `feed_fragment`, but the datagram carries no INFO_TS -- the interop case Finding 1
    /// is about. Returns the result instead of asserting success, since bailing without
    /// delivering is exactly the behavior under test.
    #[allow(clippy::too_many_arguments)]
    fn feed_fragment_without_timestamp(
        user_logic: &mut UserLogic,
        participant_prefix: GuidPrefix,
        writer_guid: Guid,
        sn: SequenceNumber,
        reader_id: EntityId,
        fragment_starting_num: u32,
        fragments_in_submessage: u16,
        payload: Vec<u8>,
    ) -> RtpsResult<()> {
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
        let message_receiver = MessageReceiver::new(participant_prefix, &from_addr);

        user_logic.handle_datafrag_message(&rtps_header, &data_frag, &message_receiver)
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

    // --- Fragment reassembly: a bail between completion and delivery must not leave the ---
    // --- ledger claiming the sample arrived                                             ---

    #[test]
    fn a_reader_that_bails_on_a_missing_timestamp_does_not_end_up_with_a_complete_ledger() {
        let (participant, mut user_logic, reader_a, _reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();

        // No INFO_TS anywhere in this datagram, directed so reader A is the only one it
        // completes for.
        let result = feed_fragment_without_timestamp(
            &mut user_logic,
            prefix,
            writer_guid,
            sn,
            reader_a_id,
            1,
            4,
            whole_sample(),
        );

        assert!(result.is_ok(), "a missing timestamp must not abort the datagram: {result:?}");
        assert!(
            held_sample(&reader_a, writer_guid, sn).is_none(),
            "delivery must not have happened with no timestamp to stamp the change with"
        );
        assert!(
            !ledger_reads_complete(&reader_a, writer_guid, sn),
            "a bailed delivery must not leave the ledger claiming the sample was received"
        );
        assert_eq!(
            ledger_missing(&reader_a, writer_guid, sn),
            vec![1, 2, 3, 4],
            "the reader must ask for the whole sample again"
        );
    }

    #[test]
    fn a_reader_that_bails_on_a_poisoned_cache_does_not_end_up_with_a_complete_ledger() {
        let (participant, mut user_logic, reader_a, _reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();

        // Poison reader A's cache mutex, the same way a panicking listener would.
        let cache = reader_a.reader_cache();
        let _ = std::thread::spawn(move || {
            let _guard = cache.lock().unwrap();
            panic!("deliberate poison for test");
        })
        .join();

        feed_fragment(&mut user_logic, prefix, writer_guid, sn, reader_a_id, 1, 4, whole_sample());

        assert!(
            !ledger_reads_complete(&reader_a, writer_guid, sn),
            "a bailed delivery must not leave the ledger claiming the sample was received"
        );
        assert_eq!(
            ledger_missing(&reader_a, writer_guid, sn),
            vec![1, 2, 3, 4],
            "the reader must ask for the whole sample again"
        );
    }

    // --- Fragment reassembly: buffers must not outlive the reader or writer they wait on ---

    #[test]
    fn deleting_a_reader_frees_only_its_own_half_assembled_fragment_buffer() {
        let (participant, mut user_logic, reader_a, reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();
        let reader_b_id = reader_b.guid().entity_id();

        // Directed repairs half-assemble one sample per reader, each under its own key.
        for reader_id in [reader_a_id, reader_b_id] {
            feed_fragment(
                &mut user_logic,
                prefix,
                writer_guid,
                sn,
                reader_id,
                1,
                2,
                vec![1, 1, 1, 1, 2, 2, 2, 2],
            );
        }
        assert!(user_logic.fragment_buffers.contains_key(&(writer_guid, reader_a_id, sn)));
        assert!(user_logic.fragment_buffers.contains_key(&(writer_guid, reader_b_id, sn)));

        participant
            .remove_reader("frag_test_topic".to_string(), reader_a_id)
            .expect("remove_reader");

        assert!(
            !user_logic.fragment_buffers.contains_key(&(writer_guid, reader_a_id, sn)),
            "reader A's buffer must be freed once the reader that owned it is deleted"
        );
        assert!(
            user_logic.fragment_buffers.contains_key(&(writer_guid, reader_b_id, sn)),
            "reader B's own buffer must survive: the purge must be scoped to the deleted reader"
        );
    }

    #[test]
    fn unmatching_a_remote_writer_frees_the_reassembly_buffers_waiting_on_it() {
        let (participant, mut user_logic, reader_a, reader_b, writer_guid, sn) =
            two_readers_matched_to_one_fragmented_writer();
        let prefix = participant.guid().prefix();
        let reader_a_id = reader_a.guid().entity_id();
        let reader_b_id = reader_b.guid().entity_id();

        for reader_id in [reader_a_id, reader_b_id] {
            feed_fragment(
                &mut user_logic,
                prefix,
                writer_guid,
                sn,
                reader_id,
                1,
                2,
                vec![1, 1, 1, 1, 2, 2, 2, 2],
            );
        }
        assert!(user_logic.fragment_buffers.contains_key(&(writer_guid, reader_a_id, sn)));
        assert!(user_logic.fragment_buffers.contains_key(&(writer_guid, reader_b_id, sn)));

        participant
            .cleanup_resources_for_remote_writer(writer_guid, "frag_test_topic")
            .expect("cleanup_resources_for_remote_writer");

        assert!(
            user_logic.fragment_buffers.is_empty(),
            "every buffer waiting on the unmatched writer must be freed, for both readers"
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

    // NACK_FRAG QoS wiring: drives `handle_heartbeat_message` for real, so the arming site and
    // `schedule_nackfrag`'s self re-arm both run, observed through send() timestamps.

    use crate::core::time::Duration as DcpsDuration;
    use crate::infrastructure::qos_policy::ReaderReliabilityExtensionQosPolicy;
    use crate::subscription::qos::{DataReaderQos, SubscriberQos};
    use crate::topic::qos::TopicQos;
    use const_default::ConstDefault;

    /// Records when each send happens, so a test can observe NACK_FRAG re-fire timing without a
    /// real socket.
    struct RecordingTransport {
        sends: Arc<Mutex<Vec<Instant>>>,
    }

    impl TransportPlugin for RecordingTransport {
        fn send(&self, _data: &[u8], _target: &SendTarget) -> std::io::Result<()> {
            self.sends.lock().expect("sends lock").push(Instant::now());
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

    /// One `StatefulReader` matched to one synthetic writer with a real unicast locator, so
    /// `NackFragRequest::fire` reaches `RecordingTransport` instead of finding no locator to
    /// send to. `qos` becomes the reader's NACK_FRAG timing.
    #[allow(clippy::type_complexity)]
    fn reader_with_nack_frag_qos(
        qos: ReaderReliabilityExtensionQosPolicy,
    ) -> (
        Arc<Participant>,
        UserLogic,
        Arc<StatefulReader>,
        Guid,
        SequenceNumber,
        Arc<Mutex<Vec<Instant>>>,
    ) {
        let sends = Arc::new(Mutex::new(Vec::new()));
        let transport: Arc<dyn TransportPlugin> =
            Arc::new(RecordingTransport { sends: sends.clone() });
        let participant = Arc::new(Participant::new(0, 0, Vec::new(), Vec::new(), Vec::new()));
        let user_logic = UserLogic::new(participant.clone(), transport);
        participant.set_user_logic(Arc::new(Some(user_logic.clone())));

        let writer_guid = Guid::new(
            [0xD0; 12],
            EntityId::new([0x01, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY),
        );
        let sn = SequenceNumber::new(0, 1);
        let entity_id = EntityId::new([0xE0, 0x00, 0x00], EntityKind::BUILT_IN_READER_NO_KEY);
        let guid = Guid::new(participant.guid().prefix(), entity_id);

        let datareader_qos =
            DataReaderQos { reader_reliability_extension: qos, ..DataReaderQos::default() };
        let subscription_data = SubscriptionBuiltinTopicData::new(
            &datareader_qos,
            &SubscriberQos::default(),
            &TopicQos::default(),
        );

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
            subscription_data,
            participant.guid(),
        ));
        // A real locator, unlike the other fixtures' empty lists: `fire()` must actually reach
        // the transport for its timing to be observable.
        reader.matched_writer_add(WriterProxy::new(
            writer_guid,
            writer_guid.entity_id(),
            vec![Locator::from_ip_v4_addr_and_port(&Ipv4Addr::LOCALHOST, 17000)],
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
        drop(guard);
        participant.add_reader("nack_frag_qos_test_topic", reader.clone());

        (participant, user_logic, reader, writer_guid, sn, sends)
    }

    /// Feeds 1 of 4 fragments, then a heartbeat covering `sn`, which arms the NACK_FRAG chain
    /// exactly as a real incomplete-sample repair would. The other 3 fragments never arrive, so
    /// the chain runs to its own budget instead of stopping early.
    fn arm_nack_frag_chain(
        user_logic: &mut UserLogic,
        prefix: GuidPrefix,
        writer_guid: Guid,
        reader_id: EntityId,
        sn: SequenceNumber,
    ) {
        feed_fragment(user_logic, prefix, writer_guid, sn, reader_id, 1, 1, vec![1, 1, 1, 1]);
        let heartbeat = Heartbeat::new(reader_id, writer_guid.entity_id(), sn, sn, 1);
        let rtps_header = Header::new(writer_guid.prefix());
        let submessage_header = SubmessageHeader::new(SubmessageId::HEARTBEAT, 0, 0);
        user_logic
            .handle_heartbeat_message(&rtps_header, &submessage_header, &heartbeat)
            .expect("heartbeat handling must not error");
    }

    #[test]
    fn nack_frag_response_delay_from_qos_gates_the_first_repair_request() {
        let qos = ReaderReliabilityExtensionQosPolicy {
            nack_frag_response_delay: DcpsDuration::from_millis(500),
            ..ReaderReliabilityExtensionQosPolicy::DEFAULT
        };
        let (participant, mut user_logic, reader, writer_guid, sn, sends) =
            reader_with_nack_frag_qos(qos);
        let prefix = participant.guid().prefix();
        let reader_id = reader.guid().entity_id();

        arm_nack_frag_chain(&mut user_logic, prefix, writer_guid, reader_id, sn);

        // The old hardcoded response delay was 80ms; a 500ms QoS delay must still be silent
        // well past that.
        thread::sleep(Duration::from_millis(250));
        assert_eq!(
            sends.lock().unwrap().len(),
            0,
            "fired before its QoS-configured response delay"
        );

        let deadline = Instant::now() + Duration::from_secs(1);
        while sends.lock().unwrap().is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            sends.lock().unwrap().len(),
            1,
            "never fired even after its QoS response delay elapsed"
        );

        if let Ok(handler) = TimerHandler::get_instance(prefix).lock() {
            handler.terminate();
        }
    }

    #[test]
    fn nack_frag_retry_interval_and_budget_come_from_the_request_not_a_const() {
        let qos = ReaderReliabilityExtensionQosPolicy {
            nack_frag_response_delay: DcpsDuration::from_millis(20),
            nack_frag_retry_delay: DcpsDuration::from_millis(40),
            nack_frag_max_retries: 6,
            ..ReaderReliabilityExtensionQosPolicy::DEFAULT
        };
        let (participant, mut user_logic, reader, writer_guid, sn, sends) =
            reader_with_nack_frag_qos(qos);
        let prefix = participant.guid().prefix();
        let reader_id = reader.guid().entity_id();

        arm_nack_frag_chain(&mut user_logic, prefix, writer_guid, reader_id, sn);

        // 1 initial request plus 6 retries, 40ms apart, complete by ~260ms. At the old
        // hardcoded 200ms retry this window would see at most 5 -- nowhere near the budget of 7.
        let deadline = Instant::now() + Duration::from_millis(900);
        while sends.lock().unwrap().len() < 7 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            sends.lock().unwrap().len(),
            7,
            "the retry chain did not complete at its QoS-configured interval and budget"
        );

        // The budget must actually stop the chain, not just space it out.
        thread::sleep(Duration::from_millis(300));
        assert_eq!(sends.lock().unwrap().len(), 7, "kept retrying past its QoS-configured budget");

        if let Ok(handler) = TimerHandler::get_instance(prefix).lock() {
            handler.terminate();
        }
    }

    /// A window's heartbeat is the reader's only trigger for the next round, so losing one leaves
    /// the reader with nothing to react to. The self-re-arming retry is what recovers it, and
    /// that is why the retry pair stays non-zero while the response delay goes to zero.
    ///
    /// The lost heartbeat here is the one that never arrives: exactly one is delivered, and the
    /// remaining fragments are withheld until the retry has been observed on the wire. If the
    /// chain did not re-arm there would be no second request and the sample would never be asked
    /// for again.
    #[test]
    fn a_lost_window_heartbeat_is_recovered_by_the_retry_chain() {
        let qos = ReaderReliabilityExtensionQosPolicy {
            // Default response delay, i.e. zero: the first request answers the one heartbeat that
            // did arrive. A short retry so the test does not sit through the 200ms default.
            // `DEFAULT` and not `default()`, which reads process-global env that sibling tests
            // in this binary set and clear around themselves.
            nack_frag_retry_delay: DcpsDuration::from_millis(40),
            nack_frag_max_retries: 3,
            ..ReaderReliabilityExtensionQosPolicy::DEFAULT
        };
        let (participant, mut user_logic, reader, writer_guid, sn, sends) =
            reader_with_nack_frag_qos(qos);
        let prefix = participant.guid().prefix();
        let reader_id = reader.guid().entity_id();

        let armed_at = Instant::now();
        arm_nack_frag_chain(&mut user_logic, prefix, writer_guid, reader_id, sn);

        // The window's own heartbeat is answered at once. 30ms is well inside the 80ms the
        // response delay used to cost and well outside the timer hop this actually takes.
        let deadline = armed_at + Duration::from_millis(30);
        while sends.lock().unwrap().is_empty() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            sends.lock().unwrap().len(),
            1,
            "the request for the window that did arrive was still waiting out a response delay"
        );

        // From here no heartbeat ever arrives again: this is the lost one. Only the retry chain
        // can produce another request.
        let deadline = Instant::now() + Duration::from_millis(500);
        while sends.lock().unwrap().len() < 2 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            sends.lock().unwrap().len() >= 2,
            "no request followed the lost heartbeat, so the reader was stranded for good"
        );

        // The writer answers that retry with the rest of the sample, which now completes without
        // any further heartbeat.
        feed_fragment(&mut user_logic, prefix, writer_guid, sn, reader_id, 2, 1, vec![2, 2, 2, 2]);
        feed_fragment(&mut user_logic, prefix, writer_guid, sn, reader_id, 3, 1, vec![3, 3, 3, 3]);
        feed_fragment(&mut user_logic, prefix, writer_guid, sn, reader_id, 4, 1, vec![4, 4, 4, 4]);

        assert_eq!(
            held_sample(&reader, writer_guid, sn),
            Some(whole_sample()),
            "the sample did not complete after the retry recovered the lost heartbeat"
        );

        if let Ok(handler) = TimerHandler::get_instance(prefix).lock() {
            handler.terminate();
        }
    }

    //-------------------------------------------------------------------------------------
    // The window carried across send calls, so a peer's one receive buffer is not committed
    // once per call.
    //-------------------------------------------------------------------------------------

    #[test]
    fn a_charge_still_counts_just_before_the_backstop() {
        let now = Instant::now();
        let since = now - (SEND_CREDIT_BACKSTOP - Duration::from_millis(1));
        assert_eq!(carried_spend(4096, since, now, SEND_CREDIT_BACKSTOP), 4096);
    }

    #[test]
    fn a_charge_stops_counting_once_the_backstop_has_passed() {
        let now = Instant::now();
        let since = now - SEND_CREDIT_BACKSTOP;
        assert_eq!(
            carried_spend(4096, since, now, SEND_CREDIT_BACKSTOP),
            0,
            "a peer that never answers would hold the window shut for good"
        );
    }

    /// A second writer on the same participant, matched to the same remote participant as
    /// `windowed_writer`'s. Both therefore aim at one peer socket, which is the whole point.
    fn second_writer_to_the_same_peer(
        participant: &Arc<Participant>,
        remote_prefix: GuidPrefix,
    ) -> Arc<StatefulWriter> {
        second_writer_with_reliability(participant, remote_prefix, true)
    }

    fn second_writer_with_reliability(
        participant: &Arc<Participant>,
        remote_prefix: GuidPrefix,
        reliable: bool,
    ) -> Arc<StatefulWriter> {
        let writer_entity_id =
            EntityId::new([0x11, 0x00, 0x00], EntityKind::USER_DEFINED_WRITER_NO_KEY);
        let writer = Arc::new(StatefulWriter::new(
            Guid::new(participant.guid().prefix(), writer_entity_id),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            writer_entity_id,
            -1,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        ));
        writer.matched_reader_add(ReaderProxy::new(
            Guid::new(
                remote_prefix,
                EntityId::new([0x21, 0x00, 0x00], EntityKind::USER_DEFINED_READER_NO_KEY),
            ),
            EntityId::UNKNOWN,
            vec![Locator::from_ip(Ipv4Addr::new(127, 0, 0, 1), 7411)],
            Vec::new(),
            SequenceNumber::new(0, 0),
            SequenceNumber::new(0, 0),
            false,
            true,
            if reliable {
                SubscriptionBuiltinTopicData::builtin_reliable()
            } else {
                SubscriptionBuiltinTopicData::default()
            },
            SequenceNumber::new(0, 0),
        ));
        participant.add_writer("window_test_topic_b", writer.clone()).unwrap();
        writer
    }

    #[test]
    fn two_writers_aiming_at_one_peer_share_that_peer_s_window() {
        const ADVERTISED: usize = 300_000;
        let window = receive_window_bytes(Some(ADVERTISED), None);
        let (participant, user_logic, recorder, writer_a, remote_prefix) =
            windowed_writer(1, 1, Some(ADVERTISED));
        let writer_b = second_writer_to_the_same_peer(&participant, remote_prefix);

        // Large enough that either writer alone would fill the window.
        stage_fragmented_sample(&writer_a, ADVERTISED);
        stage_fragmented_sample(&writer_b, ADVERTISED);

        let round_a = first_transmission(&user_logic, &writer_a, &recorder);
        let round_b = first_transmission(&user_logic, &writer_b, &recorder);

        assert_one_window(&round_a, window, "writer A");
        assert!(
            !round_b.is_empty(),
            "writer B sent nothing; at_least_one must still let a datagram through"
        );

        let charged: usize =
            round_a.iter().chain(round_b.iter()).map(|d| d.wire_bytes).sum::<usize>();
        let floor: usize = round_b.first().map(|d| d.wire_bytes).unwrap_or(0);
        assert!(
            charged <= window + floor,
            "two writers charged {charged} bytes into one {window}-byte window; the peer's \
             socket sees the sum, so each send call opening a fresh window overruns it"
        );
    }

    #[test]
    fn a_peer_that_answers_gets_its_window_back() {
        const ADVERTISED: usize = 300_000;
        let window = receive_window_bytes(Some(ADVERTISED), None);
        let (participant, user_logic, recorder, writer_a, remote_prefix) =
            windowed_writer(1, 1, Some(ADVERTISED));
        let writer_b = second_writer_to_the_same_peer(&participant, remote_prefix);

        stage_fragmented_sample(&writer_a, ADVERTISED);
        stage_fragmented_sample(&writer_b, ADVERTISED);

        let _ = first_transmission(&user_logic, &writer_a, &recorder);
        user_logic.release_send_credit(remote_prefix);
        let round_b = first_transmission(&user_logic, &writer_b, &recorder);

        assert_one_window(&round_b, window, "writer B after the peer answered");
        let charged: usize = round_b.iter().map(|d| d.wire_bytes).sum();
        assert!(
            charged > window / 2,
            "writer B got only {charged} of {window} bytes back; an answered peer has drained \
             what was charged and the next writer must see a whole window"
        );
    }

    /// A best-effort reader never answers, and the writer never heartbeats at it, so nothing
    /// would ever release a charge made on its behalf. Carrying one would throttle every later
    /// best-effort send to a single datagram until the backstop, turning a stream that used to
    /// deliver every sample into one sample per backstop period -- and best-effort has no repair
    /// path to recover the rest.
    #[test]
    fn a_best_effort_burst_is_not_throttled_by_an_earlier_one() {
        const ADVERTISED: usize = 300_000;
        let window = receive_window_bytes(Some(ADVERTISED), None);
        let (participant, user_logic, recorder, writer_a, remote_prefix) =
            windowed_writer(1, 1, Some(ADVERTISED));
        let writer_b = second_writer_with_reliability(&participant, remote_prefix, false);

        stage_fragmented_sample(&writer_a, ADVERTISED);
        stage_fragmented_sample(&writer_b, ADVERTISED);

        let _ = first_transmission(&user_logic, &writer_a, &recorder);
        let round_b = first_transmission(&user_logic, &writer_b, &recorder);

        let charged: usize = round_b.iter().map(|d| d.wire_bytes).sum();
        assert!(
            charged > window / 2,
            "a best-effort burst got only {charged} of {window} bytes; nothing will ever release \
             the charge that took the rest, so this sample is silently truncated"
        );
    }

    #[test]
    fn a_builtin_writer_does_not_draw_on_the_shared_budget() {
        let (participant, user_logic, _recorder, _writer, remote_prefix) =
            windowed_writer(1, 1, Some(300_000));
        let builtin_entity_id = EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER;
        let builtin = Arc::new(StatefulWriter::new(
            Guid::new(participant.guid().prefix(), builtin_entity_id),
            Vec::new(),
            Vec::new(),
            ReliabilityQosPolicyKind::Reliable,
            TopicKind::NoKey,
            builtin_entity_id,
            -1,
            None,
            PublicationBuiltinTopicData::default(),
            Weak::new(),
        ));

        assert!(
            user_logic.shared_send_credit(&builtin).is_none(),
            "discovery must not queue behind a user writer's burst"
        );

        let mut windows = SendWindows::new(&NullTransport, user_logic.shared_send_credit(&builtin));
        windows.remaining(&participant, remote_prefix, true);
        windows.charge(remote_prefix, 100_000, true);
        assert!(
            user_logic.send_credit.get(&remote_prefix).is_none(),
            "a builtin writer's send left a charge on the shared budget"
        );
    }
}
