//! User traffic logic for RTPS protocol.
//!
//! This module handles user-level data exchange including
//! writer/reader message processing and data fragmentation.

use chrono::{DateTime, Utc};
use log::{debug, trace, warn};
// use rand::Rng;
use std::collections::HashMap;
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
use crate::rtps::entities::history::cache_change::CacheChange;
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
use crate::rtps::messages::message_creator::MessageCreator;
use crate::rtps::messages::submessage_header::SubmessageHeader;
use crate::rtps::messages::submessages::ack_nack::AckNack;
use crate::rtps::messages::submessages::data::Data;
use crate::rtps::messages::submessages::data_frag::{DataFrag, FragmentBuffer};
use crate::rtps::messages::submessages::gap::Gap;
use crate::rtps::messages::submessages::heartbeat::Heartbeat;
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

#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct UserLogic {
    participant: Weak<Participant>,
    transport: Arc<dyn TransportPlugin>,
    fragment_buffers: Arc<DashMap<(Guid, SequenceNumber), FragmentBuffer>>,
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

        // LOCK ORDER: acquires `writer_cache` first, then `reader_proxies`.
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

        // If requested change is not in HistoryCache or did not pass DDS FILTER, send GAP message
        let mut gap_list: Vec<SequenceNumber> = Vec::new();

        // Send RequestedChanges that ReaderProxy requested via Nack
        for requested_change_sn in reader_proxy.requested_changes().iter() {
            // For volatile readers, sequence numbers <= last_irrelevant_sn should be responded with GAP
            if *requested_change_sn <= reader_proxy.last_irrelevant_sn() {
                gap_list.push(*requested_change_sn);
                continue;
            }

            if let Some(a_change) = cache_guard.get_change(*requested_change_sn) {
                // ACK may have been received in the meantime, so check first
                if reader_proxy.max_acked_sn() >= *requested_change_sn {
                    continue;
                }

                // TODO: Filter message according to Reader Proxy's request (time based filter, content filtered topic, etc)
                // Send DATA message or GAP message depending on filter result

                // In case of fragment, fragment state is checked via last seq number, so
                // use current seq number in previous heartbeat to get ack from reader for retransmitted message
                // In case of data retransmission, decide whether to send heartbeat
                let heartbeat_info = Some((
                    stateful_writer.heartbeat_count(),
                    *requested_change_sn,
                    *requested_change_sn,
                    false, // final_flag
                    false, // liveliness_flag = false for retransmission
                ));

                if a_change.is_fragmented() {
                    debug!(
                        "[UserLogic] [RequestedChanges] Fragmented change: {}",
                        a_change.sequence_number()
                    );

                    let participant = self.get_upgraded_participant()?;
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

                    for fragment_num in 1..=a_change.total_fragments() {
                        if let Some(fragment_data) = a_change.get_fragment_data(fragment_num) {
                            let result = MessageCreator::create_data_frag_msg(
                                &a_change,
                                reader_proxy.remote_reader_guid(),
                                reader_proxy.remote_group_entity_id(),
                                writer.endpoint_id(),
                                fragment_num,
                                1,
                                a_change.fragment_size() as u16,
                                a_change.data_value().len() as u32,
                                fragment_data,
                                heartbeat_info,
                                timestamp,
                                &mut send_buffer,
                            );

                            if result.is_ok() {
                                if let Err(e) = self.send_rtps_message_to_locators(
                                    reader_proxy.unicast_locator_list(),
                                    &send_buffer,
                                ) {
                                    warn!("Failed to send DATA_FRAG for requested change: {:?}", e);
                                }
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
                    // No fragment case - send regular DATA message
                    let participant = self.get_upgraded_participant()?;
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
                    let result = MessageCreator::create_data_msg(
                        &a_change,
                        reader_proxy.remote_reader_guid(),
                        reader_proxy.remote_group_entity_id(),
                        writer.endpoint_id(),
                        None, // No heartbeat
                        true, // Use inline QoS (default)
                        None, // No content filter for retransmission (TODO: consider adding filter)
                        &mut send_buffer,
                    );

                    if result.is_ok() {
                        if let Err(e) = self.send_rtps_message_to_locators(
                            reader_proxy.unicast_locator_list(),
                            &send_buffer,
                        ) {
                            warn!("Failed to send DATA for requested change: {:?}", e);
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
                }
            } else {
                gap_list.push(*requested_change_sn);

                debug!(
                    "[UserLogic] [AckNack] CacheChange not found for sequence number: {}",
                    requested_change_sn
                );
            }
        }

        self.send_gap_for_vec(writer.guid(), reader_proxy, writer.endpoint_id(), &mut gap_list)?;

        // Clear after sending all requested changes to prevent duplicate transmission. Can safely clear since Lock has been acquired.
        reader_proxy.empty_requested_changes();

        Ok(())
    }

    fn send_unsent_changes_of_stateful_writer(
        &self,
        writer: &StatefulWriter,
        history_cache: &WriterHistoryCache,
    ) -> RtpsResult<()> {
        let reader_proxies_lock = writer.reader_proxies();
        let mut reader_proxies = reader_proxies_lock.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "[Data] Failed to acquire reader proxies lock")
        })?;

        let participant = self.get_upgraded_participant()?;
        let mut disconnected_peer: Option<GuidPrefix> = None;

        // Send unsent CacheChanges to matched readers
        for reader_proxy in reader_proxies.iter_mut() {
            loop {
                let a_change_seq_num = reader_proxy.next_unsent_change(history_cache);

                // Send cache changes that have not been sent to this reader until none remain
                if a_change_seq_num == SequenceNumber::UNKNOWN {
                    break;
                }

                // RTPS 2.5 - 8.4.9.1.4 This may happen when a CacheChanges is removed from the Writer cache
                // GAP only sent on reliable communication for efficiency
                if reader_proxy.highest_sent_change_sn() != SequenceNumber::UNKNOWN
                    && a_change_seq_num > reader_proxy.highest_sent_change_sn() + 1
                    && reader_proxy.is_reliable()
                {
                    self.send_gap_for_range(
                        participant.guid(),
                        reader_proxy,
                        writer.endpoint_id(),
                        reader_proxy.highest_sent_change_sn() + 1,
                        SequenceNumber::from_i64(a_change_seq_num.to_i64() - 1),
                    )?;
                }

                // TODO: Filter message according to Reader Proxy's request (time based filter, content filtered topic, etc)

                // Send DATA message or GAP message depending on filter result
                if let Some(a_change) = history_cache.get_change(a_change_seq_num) {
                    let first_sn = history_cache.get_seq_num_min().ok_or_else(|| {
                        RtpsError::new(
                            RtpsErrorCode::DataNotSet,
                            "Writer cache should not be empty while sending DATA",
                        )
                    })?;
                    let last_sn = history_cache.get_seq_num_max().ok_or_else(|| {
                        RtpsError::new(
                            RtpsErrorCode::DataNotSet,
                            "Writer cache should not be empty while sending DATA",
                        )
                    })?;

                    if a_change.is_fragmented() {
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

                        for fragment_num in 1..=a_change.total_fragments() {
                            let mut heartbeat_info = None;

                            if reader_proxy.is_reliable() && !writer.disable_piggyback_heartbeat() {
                                heartbeat_info = Some((
                                    writer.heartbeat_count(),
                                    first_sn,
                                    last_sn,
                                    false,
                                    false,
                                ));
                            }

                            match self.send_data_frag_to_reader_proxy(
                                &a_change,
                                reader_proxy,
                                writer.endpoint_id(),
                                fragment_num,
                                heartbeat_info,
                                timestamp,
                                &mut send_buffer,
                            ) {
                                Ok(true) => {
                                    if !writer.disable_piggyback_heartbeat() {
                                        writer.increase_heartbeat_count();
                                        if !reader_proxy.is_first_hb_sent() {
                                            reader_proxy.set_first_hb_sent();
                                        }
                                    }
                                }
                                Err(e) if e.code == RtpsErrorCode::PeerDisconnected => {
                                    warn!(
                                        "[DataFrag] Peer disconnected for reader {}",
                                        reader_proxy.remote_reader_guid()
                                    );
                                    disconnected_peer =
                                        Some(reader_proxy.remote_reader_guid().prefix());
                                    break;
                                }
                                _ => {}
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
                        // TODO: Fill in inlineQos if ReaderProxy.expects_inline_qos() == true
                        let mut heartbeat_info = None;

                        if reader_proxy.is_reliable() && !writer.disable_piggyback_heartbeat() {
                            heartbeat_info =
                                Some((writer.heartbeat_count(), first_sn, last_sn, false, false));
                        }

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
                            &a_change,
                            reader_proxy.remote_reader_guid(),
                            reader_proxy.remote_group_entity_id(),
                            writer.endpoint_id(),
                            heartbeat_info,
                            true, // Use inline QoS (default)
                            reader_proxy.generate_content_filter_info(),
                            &mut send_buffer,
                        )
                        .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

                        let send_result = self.send_rtps_message_to_locators(
                            reader_proxy.unicast_locator_list(),
                            &send_buffer,
                        );
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
                        match send_result {
                            Ok(()) => {
                                if !writer.disable_piggyback_heartbeat() {
                                    writer.increase_heartbeat_count();
                                    if !reader_proxy.is_first_hb_sent() {
                                        reader_proxy.set_first_hb_sent();
                                    }
                                }
                            }
                            Err(e) if e.code == RtpsErrorCode::PeerDisconnected => {
                                warn!(
                                    "[Data] Peer disconnected for reader {}",
                                    reader_proxy.remote_reader_guid()
                                );
                                disconnected_peer =
                                    Some(reader_proxy.remote_reader_guid().prefix());
                                break;
                            }
                            Err(_) => {}
                        }
                    }
                } else {
                    warn!(
                        "[Data] Failed to find change in history cache for seq_num: {}",
                        a_change_seq_num
                    );
                }

                reader_proxy.set_highest_sent_change_sn(a_change_seq_num);
            }
        }

        drop(reader_proxies);

        if let Some(prefix) = disconnected_peer {
            let guid = Guid::new(prefix, EntityId::PARTICIPANT);
            let _ = participant.unmatch_with_remote_participant(&guid);
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
                    for fragment_num in 1..=change.total_fragments() {
                        if let Some(fragment_data) = change.get_fragment_data(fragment_num) {
                            let result = MessageCreator::create_data_frag_msg(
                                change,
                                Guid::new(reader_locator.guid_prefix(), EntityId::PARTICIPANT),
                                reader_locator.remote_entity_id(),
                                writer.endpoint_id(),
                                fragment_num,
                                1,
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

    /// Returns Ok(true) if sent, Ok(false) if skipped, Err(PeerDisconnected) if peer is gone.
    fn send_data_frag_to_reader_proxy(
        &self,
        change: &CacheChange,
        reader_proxy: &ReaderProxy,
        writer_id: EntityId,
        fragment_num: u32,
        heartbeat_info: Option<(u32, SequenceNumber, SequenceNumber, bool, bool)>,
        timestamp: DateTime<Utc>,
        send_buffer: &mut Vec<u8>,
    ) -> RtpsResult<bool> {
        if let Some(fragment_data) = change.get_fragment_data(fragment_num) {
            let result = MessageCreator::create_data_frag_msg(
                change,
                reader_proxy.remote_reader_guid(),
                reader_proxy.remote_group_entity_id(),
                writer_id,
                fragment_num,
                1,
                change.fragment_size() as u16,
                change.data_value().len() as u32,
                fragment_data,
                heartbeat_info,
                timestamp,
                send_buffer,
            );

            if result.is_ok() {
                return match self.send_rtps_message_to_locators(
                    reader_proxy.unicast_locator_list(),
                    send_buffer.as_slice(),
                ) {
                    Ok(()) => Ok(true),
                    Err(e) if e.code == RtpsErrorCode::PeerDisconnected => Err(e),
                    Err(_) => Ok(false),
                };
            }
        }
        Ok(false)
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

    fn send_gap_for_vec(
        &self,
        local_guid: Guid,
        reader_proxy: &ReaderProxy,
        writer_entity_id: EntityId,
        gap_list: &mut Vec<SequenceNumber>,
    ) -> RtpsResult<()> {
        if gap_list.is_empty() {
            return Ok(());
        }

        let buffer_list = MessageCreator::create_multiple_gap_msgs(
            local_guid,
            reader_proxy.remote_reader_guid(),
            reader_proxy.remote_group_entity_id(),
            writer_entity_id,
            gap_list,
        )
        .map_err(|e| RtpsError::new(RtpsErrorCode::Io, e.to_string()))?;

        for buf in buffer_list {
            if let Err(e) = self
                .send_rtps_message_to_locators(reader_proxy.unicast_locator_list(), buf.as_slice())
            {
                warn!("Failed to send GAP: {:?}", e);
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

// Reader ACKNACK Sending (Local Reader -> Remote Writer)
impl UserLogic {
    pub(crate) fn send_acknack(
        &self,
        reader_id: EntityId,
        remote_writer_guid: Guid,
        final_flag: bool,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let reader = participant
            .find_reader_from_entity_id(reader_id)
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
            .find(|wp| wp.remote_writer_guid() == remote_writer_guid)
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, "WriterProxy not found")
            })?;

        let bitmap_base = writer_proxy.expected_sn();
        let last_sn = writer_proxy.changes_from_writer_max();
        let missing_changes = writer_proxy.missing_changes_for_heartbeat(bitmap_base, last_sn);

        self.send_acknack_to_writer_proxy_inner(
            writer_proxy,
            stateful_reader,
            missing_changes,
            bitmap_base,
            final_flag,
            false,
        )?;

        Ok(())
    }

    pub(crate) fn send_preemptive_acknack(
        &self,
        reader_id: EntityId,
        remote_writer_guid: Guid,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let reader = participant
            .find_reader_from_entity_id(reader_id)
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
            .find(|wp| wp.remote_writer_guid() == remote_writer_guid)
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, "WriterProxy not found")
            })?;

        if writer_proxy.expected_sn() == SequenceNumber::UNKNOWN {
            self.send_acknack_to_writer_proxy_inner(
                writer_proxy,
                stateful_reader,
                vec![],
                SequenceNumber::from_i64(0),
                false,
                true,
            )?;
        }

        Ok(())
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
        // Update WriterProxy state - mark as Received if data was received
        if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
            if let Ok(mut matched_writers) = stateful_reader.writer_proxies().lock() {
                let writer_proxy = matched_writers
                    .iter_mut()
                    .find(|proxy| proxy.remote_writer_guid() == remote_guid)
                    .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

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

                    let mut change_to_add: Vec<CacheChange> =
                        Vec::with_capacity(1 + flushed_changes.len());
                    change_to_add.push(change);
                    change_to_add.extend(flushed_changes);
                    self.add_change_to_reader_cache_and_notify(reader, change_to_add)?;
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
                    let next_sn = change.sequence_number().add(1);
                    self.add_change_to_reader_cache_and_notify(reader, vec![change])?;
                    remote_writer_info.set_expected_sn(next_sn);
                }
            }
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

    // Add a single change to the reader cache and notify the application. TIME_BASED_FILTER is
    // applied inside the reader history cache: a held sample returns None and is delivered later
    // by the cache's own timer, so it is not notified here.
    fn deliver_change(reader: &dyn Reader, change: CacheChange) {
        let reader_cache = reader.reader_cache();
        let mut res: Option<RtpsResult<Option<Arc<CacheChange>>>> = None;
        if let Ok(mut cache_guard) = reader_cache.lock() {
            res = Some(cache_guard.add_change(change, true));
        }
        if let Some(Ok(Some(change))) = res {
            reader.on_change(change);
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

    fn cleanup_old_fragment_buffers(&self, max_size: usize) {
        // DashMap allows direct access without lock
        if self.fragment_buffers.len() <= max_size {
            return;
        }

        // Sort incomplete fragment buffers by created_at and remove oldest ones
        let mut incomplete_buffers: Vec<_> = self
            .fragment_buffers
            .iter()
            .filter(|entry| !entry.value().all_fragments_received())
            .map(|entry| (*entry.key(), entry.value().created_at))
            .collect();

        // Sort in ascending order by creation time (oldest first)
        incomplete_buffers.sort_by_key(|(_, created_at)| *created_at);

        let buffers_to_remove = self.fragment_buffers.len() - max_size;
        let mut removed_count = 0;

        for (key, _) in incomplete_buffers.iter().take(buffers_to_remove) {
            if let Some((_, _removed_buffer)) = self.fragment_buffers.remove(key) {
                // warn!(
                //     "Cleaned up incomplete fragment buffer: writer_guid={:?}, seq_num={:?}, \
                //      received_fragments={}/{}, age={:.2}s",
                //     key.0,
                //     key.1,
                //     removed_buffer.received_fragments.len(),
                //     removed_buffer.total_fragments,
                //     removed_buffer.created_at.elapsed().as_secs_f64()
                // );
                removed_count += 1;
            }
        }

        // If not enough removed yet, also remove completed buffers (remove oldest ones)
        if removed_count < buffers_to_remove {
            let mut complete_buffers: Vec<_> = self
                .fragment_buffers
                .iter()
                .filter(|entry| entry.value().all_fragments_received())
                .map(|entry| (*entry.key(), entry.value().created_at))
                .collect();

            complete_buffers.sort_by_key(|(_, created_at)| *created_at);

            let remaining_to_remove = buffers_to_remove - removed_count;
            for (key, _) in complete_buffers.iter().take(remaining_to_remove) {
                if let Some((_, removed_buffer)) = self.fragment_buffers.remove(key) {
                    debug!(
                        "Cleaned up complete fragment buffer: writer_guid={}, seq_num={}, age={:.2}s",
                        key.0,
                        key.1,
                        removed_buffer.created_at.elapsed().as_secs_f64()
                    );
                    removed_count += 1;
                }
            }
        }

        if removed_count > 0 {
            // warn!(
            //     "Fragment buffer cleanup completed: removed {} buffers, remaining {} buffers",
            //     removed_count,
            //     self.fragment_buffers.len()
            // );
        }
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
                    let kind = e.to_string();
                    warn!("[UserLogic] {} locator found but no {} sender available", kind, kind);
                    continue;
                }
                Err(e) => {
                    warn!("[UserLogic] Failed to send to locator {}: {:?}", locator, e);
                    match e.kind() {
                        std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::ConnectionRefused => {
                            return Err(RtpsError::new(
                                RtpsErrorCode::PeerDisconnected,
                                format!(
                                    "[UserLogic] Peer disconnected at locator {}: {}",
                                    locator, e
                                ),
                            ));
                        }
                        _ => {
                            last_error = Some(e);
                            continue;
                        }
                    }
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
            let writer_proxy = matched_writers
                .iter()
                .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

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

            let missing_changes =
                writer_proxy.process_heartbeat(heartbeat.first_sn, heartbeat.last_sn);

            // Case when ACKNACK sending is required: no fragments or
            // all fragments have been received
            if !writer_proxy.has_fragmented_changes(heartbeat.first_sn, heartbeat.last_sn)
                || writer_proxy.all_fragments_received(heartbeat.last_sn)
            {
                // This is the first HB for reader
                if writer_proxy.expected_sn() == SequenceNumber::UNKNOWN {
                    writer_proxy.set_expected_sn(heartbeat.first_sn);
                }

                // Always flush buffer after updating sequence number
                let change_to_add = writer_proxy.flush_buffered_changes();
                self.add_change_to_reader_cache_and_notify(reader.as_ref(), change_to_add)?;

                // Apply heartbeat response delay
                let heartbeat_response_delay = stateful_reader.heartbeat_response_delay();
                let delay_duration = heartbeat_response_delay.to_std_duration();

                if delay_duration.is_zero() {
                    // No delay - send immediately
                    let bitmap_base = writer_proxy.expected_sn();
                    self.send_acknack_to_writer_proxy_inner(
                        writer_proxy,
                        stateful_reader,
                        missing_changes,
                        bitmap_base,
                        final_flag,
                        false,
                    )?;
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
                let missing_fragments =
                    writer_proxy.calculate_missing_fragments(heartbeat.first_sn, heartbeat.last_sn);

                if missing_fragments.is_some() {
                    let writer_proxies_clone = writer_proxies.clone();
                    let stateful_reader_guid = stateful_reader.guid();
                    let participant = participant.clone();
                    let transport_clone = self.transport.clone();
                    let last_sn = heartbeat.last_sn;
                    let missing_fragments_clone = missing_fragments.clone();
                    let remote_writer_guid = writer_proxy.remote_writer_guid();

                    writer_proxy.increase_nackfrag_count();

                    let timer_id = TimerId::NackFrag {
                        reader_entity_id: stateful_reader.guid().entity_id(),
                        remote_writer_guid,
                        sequence_number: last_sn,
                    };
                    if let Ok(locked_timer_handler) =
                        TimerHandler::get_instance(participant.guid().prefix()).lock()
                    {
                        locked_timer_handler.remove_timer(timer_id);
                        locked_timer_handler.add_timer(
                            timer_id,
                            Duration::from_millis(5),
                            false, // not repeating
                            move || {
                                let mut writer_proxies_guard = match writer_proxies_clone.lock() {
                                    Ok(guard) => guard,
                                    Err(e) => {
                                        warn!("Failed to acquire writer_proxies lock: {}", e);
                                        return;
                                    }
                                };

                                if let Some(current_writer_proxy) = writer_proxies_guard
                                    .iter_mut()
                                    .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                                {
                                    let still_missing =
                                        current_writer_proxy.still_missing_fragments(last_sn);

                                    if still_missing {
                                        // Create and send NACK_FRAG message
                                        current_writer_proxy.increase_acknack_count();

                                        let acknack_info = Some((
                                            current_writer_proxy.acknack_count(),
                                            last_sn,
                                            missing_changes.clone(),
                                        ));

                                            if let Ok(buffer) = MessageCreator::create_nackfrag_msg(
                                                participant.guid(),
                                                current_writer_proxy.remote_writer_guid(),
                                                stateful_reader_guid.entity_id(),
                                                current_writer_proxy
                                                    .remote_writer_guid()
                                                    .entity_id(),
                                                last_sn,
                                                missing_fragments_clone.as_ref().unwrap().clone(),
                                                current_writer_proxy.nackfrag_count(),
                                                acknack_info,
                                            ) {
                                                // Same SHM > TCP > UDP priority filter as
                                                // send_rtps_message_to_locators, with can_handle
                                                // guarding the local-side reachability.
                                                let locs: Vec<&Locator> =
                                                    current_writer_proxy.unicast_locator_list().iter().collect();
                                                let try_kind = |is_kind: fn(&Locator) -> bool| -> Option<Vec<&Locator>> {
                                                    let v: Vec<&Locator> = locs.iter().copied()
                                                        .filter(|l| is_kind(l) && transport_clone.can_handle(l))
                                                        .collect();
                                                    (!v.is_empty()).then_some(v)
                                                };
                                                let chosen: Vec<&Locator> = try_kind(Locator::is_shm)
                                                    .or_else(|| try_kind(Locator::is_tcp))
                                                    .or_else(|| try_kind(Locator::is_udp))
                                                    .unwrap_or(locs);
                                                for locator in chosen {
                                                    match transport_clone
                                                        .send(&buffer, &SendTarget::UserData(locator))
                                                    {
                                                        Ok(_) => {}
                                                        Err(e)
                                                            if e.kind()
                                                                == std::io::ErrorKind::Unsupported =>
                                                        {
                                                            let kind = e.to_string();
                                                            warn!(
                                                                "[UserLogic] {} locator found but no {} sender available",
                                                                kind, kind
                                                            );
                                                        }
                                                        Err(e) => {
                                                            warn!(
                                                                "[UserLogic] Failed to send NACK_FRAG: {:?}",
                                                                e
                                                            );
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                            );
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_acknack_message(
        &mut self,
        rtps_header: &Header,
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

        // Preemptive ACKNACK (seqbase == 0, empty bitmap) is an explicit reset
        // signal and bypasses the count/debounce check.
        let is_preemptive = acknack.reader_sn_state.bitmap_base() == SequenceNumber::from_i64(0)
            && acknack.reader_sn_state.num_bits() == 0;
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

        if acknack.reader_sn_state.bitmap_base() == SequenceNumber::from_i64(0)
            && acknack.reader_sn_state.num_bits() == 0
        {
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

        let key = (remote_writer_guid, data_frag.writer_sn);
        let total_size = data_frag.sample_size;

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
        if self.fragment_buffers.len() > 30 {
            self.cleanup_old_fragment_buffers(30);
        }

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

            // Zero-copy: `serialized_bytes` is the DataFrag payload as `Bytes`
            // (a refcount on the socket buffer). Per-fragment slices below are
            // again refcount bumps, not memcpys.
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

        // For StatefulReader case, update WriterProxy's ChangeFromWriter state for all matched readers
        for reader in &matched_readers {
            if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>() {
                // Query buffer information from DashMap again
                if let Some(buffer_ref) = self.fragment_buffers.get(&key) {
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
                            buffer_ref.total_fragments,
                            frag_start..frag_end,
                        );
                    }
                }
            }
        }

        // Check if all fragments have been received and process
        if let Some(buffer_ref) = self.fragment_buffers.get(&key) {
            if buffer_ref.all_fragments_received() {
                let total_fragments = buffer_ref.total_fragments;
                // complete here; delivery FragmentInfo's set is unused when is_complete
                let received_fragments: std::collections::HashSet<u32> =
                    std::collections::HashSet::new();
                let is_complete = buffer_ref.all_fragments_received();

                // Drop buffer_ref to release DashMap lock
                drop(buffer_ref);

                // Move payload from buffer without cloning
                let removed = self.fragment_buffers.remove(&key);
                if removed.is_none() {
                    return Ok(());
                }
                let (_, buffer) = removed.unwrap();
                // Use timestamp from first fragment, fallback to current message
                let assembled_timestamp = buffer.source_timestamp.or(source_timestamp);
                if assembled_timestamp.is_none() {
                    return Err(RtpsError::new(
                        RtpsErrorCode::InvalidSubmessageBody,
                        "No source timestamp available for assembled DataFrag (missing INFO_TS)",
                    ));
                }

                // Scatter-gather: keep fragment chunks as-is instead of assembling
                // a contiguous buffer. One shared cache per sample so any contiguous
                // fallback materializes at most once across all readers.
                let chunks = buffer.into_chunks();
                let cached = std::sync::Arc::new(std::sync::OnceLock::new());

                for reader in matched_readers.iter() {
                    let mut ownership_strength = None;
                    if let Some(stateful_reader) = reader.as_any().downcast_ref::<StatefulReader>()
                    {
                        let writer_proxies = stateful_reader.writer_proxies();
                        let matched_writers = writer_proxies.lock().ok();
                        if let Some(guard) = matched_writers {
                            if let Some(writer_proxy) = guard
                                .iter()
                                .find(|proxy| proxy.remote_writer_guid() == remote_writer_guid)
                            {
                                ownership_strength = Some(writer_proxy.get_ownership_strength());
                            }
                        }
                    }

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
                    assembled_change.set_chained_payload(chunks.clone(), cached.clone());
                    assembled_change.set_ownership_strength(ownership_strength);

                    let _ = self.deliver_change_to_reader(
                        assembled_change,
                        reader.as_ref(),
                        data_frag.writer_sn,
                        remote_writer_guid,
                        Some(FragmentInfo {
                            total_fragments,
                            received_fragments: received_fragments.clone(),
                            is_complete,
                        }),
                    );
                }

                // Remove completed fragment buffer
                self.fragment_buffers.remove(&key);
            }
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

        let total_frags = change.total_fragments();
        let requested_fragments = frag_state.extract_numbers();
        let heartbeat_count = stateful_writer.heartbeat_count();
        let last_sn = history_cache_guard.get_seq_num_max().ok_or_else(|| {
            RtpsError::new(
                RtpsErrorCode::DataNotSet,
                "Writer cache should not be empty while sending DATA_FRAG",
            )
        })?;
        let heartbeat_info = Some((heartbeat_count, writer_sn, last_sn, false, false));

        let timestamp = Utc::now();
        let participant = self.get_upgraded_participant()?;
        let mut send_buffer = participant
            .wire_buffer_pool()
            .lock()
            .map_err(|_| {
                RtpsError::new(RtpsErrorCode::LockError, "Failed to lock wire buffer pool")
            })?
            .acquire();
        for fragment_num in requested_fragments {
            if fragment_num >= 1 && fragment_num <= total_frags {
                if let Err(e) = self.send_data_frag_to_reader_proxy(
                    &change,
                    reader_proxy,
                    writer_id,
                    fragment_num,
                    heartbeat_info,
                    timestamp,
                    &mut send_buffer,
                ) {
                    if e.code == RtpsErrorCode::PeerDisconnected {
                        warn!(
                            "[NackFrag] Peer disconnected during retransmit for reader {}",
                            reader_proxy.remote_reader_guid()
                        );
                        break;
                    }
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

                        let flushed_changes = writer_proxy.flush_buffered_changes();
                        self.add_change_to_reader_cache_and_notify(
                            stateful_reader,
                            flushed_changes,
                        )?;
                    }
                }

                for seq_num in irrelevant_changes {
                    writer_proxy.irrelevant_change_set(seq_num);
                }
            }
        }

        Ok(())
    }
}
