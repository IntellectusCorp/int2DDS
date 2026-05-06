use dashmap::DashMap;
use log::{debug, error, warn};

use crate::{
    common::{
        builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData,
        instance_handle::InstanceHandle,
    },
    infrastructure::{
        liveliness_monitor::LivelinessMonitor,
        qos_policy::{LivelinessQosPolicy, LivelinessQosPolicyKind},
        status::{LivelinessChangedStatus, StatusKind},
    },
    rtps::{
        builtin::data::participant_message_data::{
            ParticipantMessageData, ParticipantMessageDataKind,
        },
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::{RtpsDuration, RtpsTime},
            types::ChangeKind,
        },
        entities::{
            entity::Entity,
            history::history_cache::HistoryCache,
            participant::Participant,
            reader::{Reader, StatefulReader, WriterProxy},
            writer::{StatefulWriter, Writer},
        },
        logic::{
            common::{impl_participant_accessor, ParticipantAccessor},
            data::builtin_endpoint_pair::BuiltinEndpointPair,
            message_processor::unicast_message_processor::UnicastMessageProcessor,
        },
        messages::{
            header::Header,
            message_creator::MessageCreator,
            message_receiver::MessageReceiver,
            submessage_header::SubmessageHeader,
            submessages::{ack_nack::AckNack, data::Data, heartbeat::Heartbeat},
        },
        task::sending_handler::{MessageType, SendingHandler},
        transport::{Transport, TransportSender},
    },
    utils::timer::{timer_handler::TimerHandler, timer_id::TimerId},
};
use std::{
    collections::HashMap,
    net::{SocketAddr, SocketAddrV4},
    sync::{Arc, Mutex, Weak},
    time::{Duration as StdDuration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum WriterAliveState {
    Alive,
    NotAlive,
}

#[derive(Clone)]
pub(crate) struct WriterInfo {
    qos: LivelinessQosPolicy,
    alive_state: WriterAliveState,
}

impl WriterInfo {
    pub fn new(qos: LivelinessQosPolicy) -> Self {
        Self { qos, alive_state: WriterAliveState::Alive }
    }

    pub fn qos(&self) -> &LivelinessQosPolicy {
        &self.qos
    }

    pub fn alive_state(&self) -> WriterAliveState {
        self.alive_state
    }

    pub fn set_alive(&mut self) {
        self.alive_state = WriterAliveState::Alive;
    }

    pub fn set_not_alive(&mut self) {
        self.alive_state = WriterAliveState::NotAlive;
    }
}

#[derive(Clone)]
pub(crate) struct WlpLogic {
    participant: Weak<Participant>,
    sender: Arc<Mutex<Option<Arc<TransportSender>>>>,
    timer_handler: Arc<Mutex<TimerHandler>>,
    // Writers whose liveliness this participant asserts (local data writers).
    asserting_writers: Arc<DashMap<Guid, WriterInfo>>,
    // Writers whose liveliness this participant monitors and fans out to readers (matched writers).
    monitored_writers: Arc<DashMap<GuidPrefix, HashMap<Guid, WriterInfo>>>,
    min_lease_duration: Arc<Mutex<RtpsDuration>>,
    liveliness_monitor: Arc<Mutex<Option<LivelinessMonitor>>>,
}

// Constructor and lifecycle management
impl WlpLogic {
    pub(crate) fn new(participant: Arc<Participant>, sender: Arc<TransportSender>) -> Self {
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());
        Self {
            participant: Arc::downgrade(&participant),
            sender: Arc::new(Mutex::new(Some(sender))),
            timer_handler,
            asserting_writers: Arc::new(DashMap::new()),
            monitored_writers: Arc::new(DashMap::new()),
            min_lease_duration: Arc::new(Mutex::new(RtpsDuration::ZERO)),
            liveliness_monitor: Arc::new(Mutex::new(None)),
        }
    }

    /// Clear the sender reference to allow Arc cleanup.
    /// This should be called during participant shutdown.
    pub(crate) fn clear_sender(&self) {
        if let Ok(mut guard) = self.sender.lock() {
            *guard = None;
        }
    }

    /// Shutdown liveliness monitor and join its thread
    pub(crate) fn shutdown(&self) {
        if let Ok(mut monitor) = self.liveliness_monitor.lock() {
            if let Some(ref mut m) = *monitor {
                m.shutdown();
            }
            *monitor = None;
        }
    }
}

// Writer management (add/remove local and remote writers)
impl WlpLogic {
    pub(crate) fn register_asserting_writer(
        &self,
        writer_guid: Guid,
        liveliness: LivelinessQosPolicy,
    ) -> RtpsResult<()> {
        log::debug!(
            "[WLP] register_asserting_writer: writer_guid={:?}, liveliness_kind={:?}, lease_duration={:?}",
            writer_guid,
            liveliness.kind,
            liveliness.lease_duration
        );
        let info = WriterInfo::new(liveliness);
        self.asserting_writers.insert(writer_guid, info);

        let participant = self.get_upgraded_participant()?;

        if liveliness.kind == LivelinessQosPolicyKind::Automatic {
            match self.min_lease_duration.lock() {
                Ok(mut min_lease_duration) => {
                    let prev = *min_lease_duration;
                    let new = liveliness.lease_duration;
                    if (prev.is_zero() || prev.is_infinite())
                        && !(new.is_zero() || new.is_infinite())
                    {
                        *min_lease_duration = new.into();

                        self.start_periodic_liveliness(*min_lease_duration)?;
                    } else {
                        *min_lease_duration = std::cmp::min(*min_lease_duration, new.into());

                        if prev != *min_lease_duration {
                            self.update_automatic_lease_duration(*min_lease_duration)?;
                        }
                    }
                }
                Err(e) => return Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
            }
        }
        if let Some(writer) = participant.find_writer_from_entity_id(writer_guid.entity_id()) {
            if let Some(writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                match writer.reader_proxies().lock() {
                    Ok(proxies) => {
                        log::debug!(
                            "[WLP] register_asserting_writer: writer_guid={:?}, reader_proxies count={}",
                            writer_guid,
                            proxies.len()
                        );
                        if proxies.is_empty() {
                            // No readers yet, skip liveliness setup
                            log::warn!(
                                    "[WLP] register_asserting_writer: SKIPPING liveliness registration for writer_guid={:?} - no readers matched yet!",
                                    writer_guid
                                );
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to lock reader proxies in register_asserting_writer: {:?}, continuing anyway", e);
                        // Continue to add writer to WLP even if lock fails
                        // Better to have false positive than miss liveliness
                    }
                }
            } else {
                // StatelessWriter has no WLP
                return Ok(());
            }
        }
        match self.liveliness_monitor.lock() {
            Ok(mut liveliness_monitor) => {
                if liveliness_monitor.is_none() {
                    let participant = participant.clone();
                    let asserting_writers = self.asserting_writers.clone();
                    let monitored_writers = self.monitored_writers.clone();

                    let callback = Arc::new(move |guid: Guid| {
                        Self::update_liveliness(
                            participant.clone(),
                            guid,
                            asserting_writers.clone(),
                            monitored_writers.clone(),
                        )
                    });

                    *liveliness_monitor = Some(LivelinessMonitor::new(callback));
                }

                match liveliness_monitor.as_ref() {
                    Some(liveliness_monitor) => {
                        log::info!(
                                "[WLP] register_asserting_writer: Registering writer_guid={:?} to LivelinessMonitor, lease_duration={:?}",
                                writer_guid, liveliness.lease_duration
                            );
                        liveliness_monitor.track_writer(&writer_guid, liveliness.lease_duration);
                        Ok(())
                    }
                    None => Err(RtpsError::new(
                        RtpsErrorCode::NotInitialized,
                        format!("Liveliness Monitor for Participant: {:?}", participant.guid(),),
                    )),
                }
            }
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }

    pub(crate) fn deregister_asserting_writer(&self, writer_guid: Guid) -> RtpsResult<()> {
        self.asserting_writers.remove(&writer_guid);

        match self.liveliness_monitor.lock() {
            Ok(liveliness_monitor) => match liveliness_monitor.as_ref() {
                Some(liveliness_monitor) => {
                    liveliness_monitor.cancel_writer(writer_guid);

                    let has_automatic = self
                        .asserting_writers
                        .iter()
                        .any(|entry| entry.value().qos.kind == LivelinessQosPolicyKind::Automatic);

                    if !has_automatic {
                        self.stop_periodic_liveliness()?;
                    }

                    Ok(())
                }
                None => Err(RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!(
                        "Liveliness Monitor for Participant: {:?}",
                        self.get_upgraded_participant()?.guid()
                    ),
                )),
            },
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }

    pub(crate) fn register_monitored_writer(
        &self,
        writer_guid: Guid,
        liveliness: LivelinessQosPolicy,
    ) -> RtpsResult<()> {
        let prefix = writer_guid.prefix();
        let writer_info = WriterInfo::new(liveliness);

        if let Some(mut writers) = self.monitored_writers.get_mut(&prefix) {
            writers.insert(writer_guid, writer_info);
        } else {
            let mut new_map = HashMap::new();
            new_map.insert(writer_guid, writer_info);
            self.monitored_writers.insert(prefix, new_map);
        }

        let participant = self.get_upgraded_participant()?;

        Self::notify_readers_remote_writer_transition(
            participant.clone(),
            writer_guid,
            LivelinessTransition::Match,
        );

        match self.liveliness_monitor.lock() {
            Ok(mut liveliness_monitor) => {
                if liveliness_monitor.is_none() {
                    let participant = participant.clone();
                    let asserting_writers = self.asserting_writers.clone();
                    let monitored_writers = self.monitored_writers.clone();

                    let callback = Arc::new(move |guid: Guid| {
                        Self::update_liveliness(
                            participant.clone(),
                            guid,
                            asserting_writers.clone(),
                            monitored_writers.clone(),
                        )
                    });

                    *liveliness_monitor = Some(LivelinessMonitor::new(callback));
                }

                match liveliness_monitor.as_ref() {
                    Some(liveliness_monitor) => {
                        liveliness_monitor.track_writer(&writer_guid, liveliness.lease_duration);
                        Ok(())
                    }
                    None => {
                        log::error!(
                            "Liveliness Monitor not initialized for Participant: {:?}",
                            participant.guid()
                        );
                        Err(RtpsError::new(
                            RtpsErrorCode::NotInitialized,
                            format!("Liveliness Monitor for Participant: {:?}", participant.guid(),),
                        ))
                    }
                }
            }
            Err(e) => {
                log::error!(
                    "Failed to lock liveliness monitor in register_monitored_writer: {:?}",
                    e
                );
                // Return Ok to not fail remote writer addition, retry will happen on next update
                Ok(())
            }
        }
    }

    pub(crate) fn deregister_monitored_writer(&self, writer_guid: Guid) -> RtpsResult<()> {
        // Snapshot prior alive state to pick UnmatchAlive vs UnmatchNotAlive.
        let was_alive = self
            .monitored_writers
            .get(&writer_guid.prefix())
            .and_then(|writers| {
                writers.get(&writer_guid).map(|info| info.alive_state() == WriterAliveState::Alive)
            })
            .unwrap_or(true);

        if let Some(mut hash_map) = self.monitored_writers.get_mut(&writer_guid.prefix()) {
            hash_map.remove(&writer_guid);

            if hash_map.is_empty() {
                drop(hash_map);
                self.monitored_writers.remove(&writer_guid.prefix());
            }
        }

        let participant = self.get_upgraded_participant()?;

        let transition = if was_alive {
            LivelinessTransition::UnmatchAlive
        } else {
            LivelinessTransition::UnmatchNotAlive
        };
        Self::notify_readers_remote_writer_transition(participant.clone(), writer_guid, transition);

        match self.liveliness_monitor.lock() {
            Ok(liveliness_monitor) => match liveliness_monitor.as_ref() {
                Some(liveliness_monitor) => {
                    liveliness_monitor.cancel_writer(writer_guid);
                    Ok(())
                }
                None => Err(RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!("Liveliness Monitor for Participant: {:?}", participant.guid(),),
                )),
            },
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }
}

// Sending liveliness and heartbeat messages
impl WlpLogic {
    // for Automatic/ManualByParticipant
    pub(crate) fn send_participant_message_data(
        &self,
        _start_time: Option<Instant>,
        duration: StdDuration,
        participant_message_data: Arc<ParticipantMessageData>,
    ) -> RtpsResult<()> {
        let logic_start_time = Instant::now();

        debug!("send_participant_message_data called, duration: {:?}", duration);
        if let Err(e) = self.send_liveliness_once(&participant_message_data) {
            error!("Failed to send liveliness: {}", e);
        }
        let start_time = Instant::now();
        if duration > StdDuration::ZERO && duration < StdDuration::MAX {
            self.timer_sleep_and_send_message(
                Some(start_time),
                logic_start_time,
                duration,
                MessageType::P2pData(Some(start_time), duration, participant_message_data),
            )?;
        }

        Ok(())
    }
    // EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER Heartbeat
    pub(crate) fn send_liveliness_heartbeat(
        &self,
        liveliness_flag: bool,
        final_flag: bool,
        writer_guid: Option<Guid>,
        target_guid_prefix: Option<GuidPrefix>, // for P2P initial
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        match writer_guid {
            Some(guid) => {
                let writer =
                    participant.find_writer_from_entity_id(guid.entity_id()).ok_or_else(|| {
                        RtpsError::new(RtpsErrorCode::NotInitialized, "Writer not found")
                    })?;

                if let Some(writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                    self.send_heartbeat_to_reader_proxies(
                        writer,
                        None,
                        guid.entity_id(),
                        target_guid_prefix, // None
                        liveliness_flag,
                        final_flag,
                    )
                } else {
                    Ok(()) // Stateless writers do not send Heartbeat messages carrying liveliness flags.
                }
            }
            None => {
                let writer = participant.builtin_participant_message_writer();
                self.send_heartbeat_to_reader_proxies(
                    &writer,
                    Some(EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER),
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                    target_guid_prefix,
                    liveliness_flag,
                    final_flag,
                )
            }
        }
    }

    fn send_heartbeat_to_reader_proxies(
        &self,
        writer: &StatefulWriter,
        reader_entity_id: Option<EntityId>,
        writer_entity_id: EntityId,
        target_guid_prefix: Option<GuidPrefix>,
        liveliness_flag: bool,
        final_flag: bool,
    ) -> RtpsResult<()> {
        let last_change_sn = writer.last_change_sequence_number();
        let (first_sn, last_sn, heartbeat_count) = match writer.writer_cache().lock() {
            Ok(cache) => (
                cache.get_seq_num_min().unwrap_or(last_change_sn + 1),
                cache.get_seq_num_max().unwrap_or(last_change_sn),
                writer.heartbeat_count(),
            ),
            Err(_) => (SequenceNumber::UNKNOWN, SequenceNumber::UNKNOWN, writer.heartbeat_count()),
        };

        let reader_proxies = writer.reader_proxies();
        let proxies_guard = reader_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to lock reader proxies for WLP: {}", e),
            )
        })?;

        let participant = self.get_upgraded_participant()?;

        let remote_datas = participant.remote_participant_proxy_datas().clone();
        let remote_datas_guard = remote_datas
            .lock()
            .map_err(|e| RtpsError::new(RtpsErrorCode::LockError, e.to_string()))?;

        for reader_proxy in proxies_guard.iter() {
            if !reader_proxy.is_active() {
                continue;
            }

            if let Some(prefix) = target_guid_prefix {
                if reader_proxy.remote_reader_guid().prefix() != prefix {
                    continue;
                }
            }

            let actual_reader_entity_id =
                reader_entity_id.unwrap_or_else(|| reader_proxy.remote_group_entity_id());

            let participant_guid = participant.local_participant_proxy_data().participant_guid();

            let buffer = match MessageCreator::create_heartbeat_message(
                participant_guid.prefix(),
                reader_proxy.remote_reader_guid().prefix(),
                heartbeat_count,
                actual_reader_entity_id,
                writer_entity_id,
                first_sn,
                last_sn,
                final_flag,
                liveliness_flag,
            ) {
                Ok(buf) => buf,
                Err(_) => continue,
            };

            // send via the metatraffic locator
            let remote_prefix = reader_proxy.remote_reader_guid().prefix();
            for remote_data in remote_datas_guard.iter() {
                if remote_data.participant_guid().prefix() == remote_prefix {
                    for locator in remote_data.metatraffic_unicast_locator_list() {
                        if locator.kind() == 1 {
                            let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                                locator.to_ip_v4_addr(),
                                locator.port() as u16,
                            ));
                            if let Ok(guard) = self.sender.lock() {
                                if let Some(sender) = guard.as_ref() {
                                    let _ = sender.send(&socket_addr, &buffer);
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }

        writer.increase_heartbeat_count();
        Ok(())
    }

    // ManualByParticipant
    pub(crate) fn send_liveliness_once(&self, data: &ParticipantMessageData) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let writer = participant.builtin_participant_message_writer();

        let payload = data.to_serialized_data();
        // Check if there are any reader proxies first
        let reader_proxies = writer.reader_proxies();
        let proxies_guard = reader_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to lock reader proxies: {}", e),
            )
        })?;

        debug!("send_liveliness_once called, reader_proxy count: {}", proxies_guard.len());

        // If no reader proxies, don't add to cache and don't send
        if proxies_guard.is_empty() {
            debug!("No reader proxies, skipping");
            return Ok(());
        }

        let cache_change = Arc::new(writer.new_change(
            ChangeKind::Alive,
            payload.to_vec(),
            InstanceHandle::NIL,
            Some(RtpsTime::now()),
        ));

        // Try to add to cache, but don't fail if it doesn't work - heartbeat is more important
        if let Ok(mut cache_guard) = writer.writer_cache().lock() {
            let old_changes = cache_guard.get_changes();
            for old_change in old_changes {
                if let Err(e) = cache_guard.remove_change(old_change) {
                    log::warn!("Failed to remove old change: {}", e);
                }
            }
            if let Err(e) = cache_guard.add_change_builtin(cache_change.clone()) {
                log::warn!("Failed to add liveliness change to cache: {}, continuing to send heartbeat anyway", e);
                // Don't return - continue to send heartbeat even if cache add fails
            }
        } else {
            log::warn!("Failed to lock writer cache in send_liveliness_once, continuing to send heartbeat anyway");
            // Don't return - continue to send heartbeat even if cache lock fails
        }

        let participant = self.get_upgraded_participant()?;

        for reader_proxy in proxies_guard.iter() {
            // Get heartbeat info to include in the same RTPS message as Data
            let heartbeat_info = {
                let cache = writer.writer_cache();
                let cache_guard = match cache.lock() {
                    Ok(guard) => guard,
                    Err(e) => {
                        log::warn!("Failed to lock writer cache for heartbeat info: {}", e);
                        continue; // Skip this reader proxy
                    }
                };
                let wlp_last_change_sn = writer.last_change_sequence_number();
                let first_sn = cache_guard.get_seq_num_min().unwrap_or(wlp_last_change_sn + 1);
                let last_sn = cache_guard.get_seq_num_max().unwrap_or(wlp_last_change_sn);
                let info = Some((writer.heartbeat_count(), first_sn, last_sn, false, false));
                debug!(
                    "[WLP] heartbeat_info: count={}, first={:?}, last={:?}",
                    writer.heartbeat_count(),
                    first_sn,
                    last_sn
                );
                info
            };
            let mut send_buffer = match participant.wire_buffer_pool().lock() {
                Ok(mut pool) => pool.acquire(),
                Err(_) => {
                    log::warn!("Failed to lock wire buffer pool");
                    continue; // Skip this reader proxy
                }
            };
            let result = MessageCreator::create_data_msg(
                &cache_change,
                reader_proxy.remote_reader_guid(),
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                heartbeat_info, // Include heartbeat in the same message
                false,
                None,
                &mut send_buffer,
            );

            if let Err(e) = result {
                log::warn!("Failed to create P2P DATA message: {:?}", e);
                if let Ok(mut pool) = participant.wire_buffer_pool().lock() {
                    pool.release(send_buffer);
                }
                continue; // Skip this reader proxy
            }

            for locator in reader_proxy.unicast_locator_list() {
                if locator.kind() == 1 {
                    //UDPv4
                    let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                        locator.to_ip_v4_addr(),
                        locator.port() as u16,
                    ));

                    if let Ok(guard) = self.sender.lock() {
                        if let Some(sender) = guard.as_ref() {
                            if let Err(e) = sender.send(&socket_addr, &send_buffer) {
                                log::warn!("Failed to send P2P DATA message: {:?}", e);
                            }
                        }
                    }
                }
            }
            if let Ok(mut pool) = participant.wire_buffer_pool().lock() {
                pool.release(send_buffer);
            }
        }
        writer.increase_heartbeat_count();

        Ok(())
    }

    fn send_discovery_message(
        &self,
        buffer: &[u8],
        remote_guid: Guid,
        message_type: &str,
    ) -> RtpsResult<bool> {
        let mut is_sent = false;

        let participant = self.get_upgraded_participant()?;

        match participant.remote_participant_proxy_datas().clone().lock() {
            Ok(remote_participant_datas) => {
                for remote_participant_data in remote_participant_datas.iter() {
                    if remote_participant_data.participant_guid().prefix() == remote_guid.prefix() {
                        for locator in remote_participant_data.metatraffic_unicast_locator_list() {
                            if locator.kind() == 1 {
                                // UDPv4
                                let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                                    locator.to_ip_v4_addr(),
                                    locator.port() as u16,
                                ));
                                if let Ok(guard) = self.sender.lock() {
                                    if let Some(sender) = guard.as_ref() {
                                        let _ = sender.send(&socket_addr, buffer);
                                        debug!(
                                            "[{}] WLP Logic: {} message sent to {}",
                                            message_type, message_type, socket_addr
                                        );
                                        is_sent = true;
                                    }
                                }
                            }
                            // TODO: Add UDPv6 support
                        }
                        break;
                    }
                }
            }
            Err(e) => {
                error!("Failed to lock remote_participant_datas: {:?}", e);
            }
        }

        if !is_sent {
            warn!(
                "[{}] WLP Logic: Failed to find remote participant for GUID: {:?}",
                message_type, remote_guid
            );
        }
        Ok(is_sent)
    }

    fn send_liveliness_acknack_message(
        &self,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        missing_changes: Vec<SequenceNumber>,
        acknack_count: u32,
        bitmap_base: SequenceNumber,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let buffer = MessageCreator::create_acknack_message(
            participant.guid(),
            remote_guid,
            reader_entity_id,
            writer_entity_id,
            missing_changes,
            acknack_count,
            bitmap_base,
            false,
        );

        match buffer {
            Ok(buffer) => {
                self.send_discovery_message(&buffer, remote_guid, "AckNack")?;
            }
            Err(e) => {
                warn!("Failed to create WLP AckNack message: {:?}", e);
            }
        }

        Ok(())
    }

    fn timer_sleep_and_send_message(
        &self,
        start_time: Option<Instant>,
        logic_start_time: Instant,
        duration: StdDuration,
        message: MessageType,
    ) -> RtpsResult<()> {
        let elapsed = logic_start_time.elapsed();
        let remaining_duration = match start_time {
            Some(start) => {
                let time_until_start = start.saturating_duration_since(logic_start_time);
                time_until_start + duration
            }
            None => duration.checked_sub(elapsed).unwrap_or(StdDuration::ZERO),
        };

        let participant = self.get_upgraded_participant()?;
        let participant_weak = self.participant.clone();

        let timer_id = TimerId::WlpP2p { guid_prefix: participant.guid().prefix() };

        let message = Arc::new(message);
        if let Ok(handler) = self.timer_handler.lock() {
            handler.add_timer(
                timer_id,
                remaining_duration,
                false, // one-shot
                {
                    let message = message.clone();
                    move || {
                        if let Some(participant) = participant_weak.upgrade() {
                            if !participant.is_terminated() {
                                let sending_handler =
                                    SendingHandler::get_instance(participant, None, None);
                                sending_handler.push_message_and_wake((*message).clone());
                            }
                        }
                    }
                },
            );
        }

        Ok(())
    }
}

// Receiving and handling liveliness messages
impl WlpLogic {
    // Automatic, ManualByParticipant
    pub(crate) fn handle_liveliness_message(
        &self,
        participant_message_data: ParticipantMessageData,
        remote_guid: Guid,
        writer_sn: SequenceNumber,
    ) -> RtpsResult<()> {
        debug!("[WlpLogic] handle_liveliness_message called");

        let participant = self.get_upgraded_participant()?;

        // 1. Find WriterProxy
        let reader = participant.builtin_participant_message_reader();

        // 2. Update WriterProxy
        if let Ok(mut matched_writers) = reader.writer_proxies().lock() {
            if let Some(writer_proxy) =
                matched_writers.iter_mut().find(|proxy| proxy.remote_writer_guid() == remote_guid)
            {
                writer_proxy.mark_change_received(writer_sn, None);

                // 3. Update Liveliness timer
                self.update_remote_participant_liveliness(participant_message_data)?;
            }
        }

        Ok(())
    }
    // ManualByTopic
    pub(crate) fn handle_heartbeat_message_inner(
        &self,
        heartbeat: &Heartbeat,
        remote_guid: Guid,
        final_flag: bool,
        liveliness_flag: bool,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        if heartbeat.writer_id.entity_kind().is_built_in() {
            // Builtin endpoint (Automatic/ManualByParticipant)
            let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
                heartbeat.writer_id,
                participant.clone(),
            )? {
                Some(proxy) => proxy,
                None => {
                    warn!(
                      "[heartbeat] No matching P2P builtin reader found for writer entity ID: {:?}",
                      heartbeat.writer_id
                  );
                    return Ok(());
                }
            };

            let local_reader = &builtin_endpoint_pair.reader();

            if !self.find_or_create_writer_proxy(local_reader, remote_guid, heartbeat.writer_id) {
                error!(
                    "[heartbeat] Failed to create WriterProxy for builtin GUID: {:?}",
                    remote_guid
                );
                return Ok(());
            }

            self.handle_heartbeat_and_send_acknack(
                local_reader,
                remote_guid,
                heartbeat,
                final_flag,
            )?;

            debug!("[WLP] Processing builtin heartbeat from writer: {:?}", remote_guid);
        } else {
            match participant.find_readers_matched_with_remote_writer(remote_guid) {
                Ok(readers) => {
                    if readers.is_empty() {
                        warn!(
                            "[heartbeat] No matching reader found for user-defined writer: {:?}",
                            remote_guid
                        );
                        return Ok(());
                    }
                    if liveliness_flag {
                        self.mark_monitored_writer_alive(remote_guid)?;

                        let stateful_reader = readers
                            .iter()
                            .find_map(|r| r.as_any().downcast_ref::<StatefulReader>());

                        match stateful_reader {
                            Some(reader) => {
                                self.handle_heartbeat_and_send_acknack(
                                    reader,
                                    remote_guid,
                                    heartbeat,
                                    final_flag,
                                )?;
                            }
                            None => {
                                debug!(
                              "[heartbeat] No StatefulReader found for writer {:?}. HEARTBEAT requires stateful communication.",
                              remote_guid
                          );
                                return Ok(());
                            }
                        }
                    }
                }
                Err(e) => {
                    error!(
                        "[heartbeat] Failed to find readers for writer {:?}: {:?}",
                        remote_guid, e
                    );
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    fn handle_heartbeat_and_send_acknack(
        &self,
        local_reader: &StatefulReader,
        remote_guid: Guid,
        heartbeat: &Heartbeat,
        final_flag: bool,
    ) -> RtpsResult<()> {
        if let Ok(mut matched_writers) = local_reader.writer_proxies().lock() {
            if let Some(writer_proxy) =
                matched_writers.iter_mut().find(|proxy| proxy.remote_writer_guid() == remote_guid)
            {
                let missing_changes =
                    writer_proxy.process_heartbeat(heartbeat.first_sn, heartbeat.last_sn);
                let bitmap_base = writer_proxy.expected_sn();

                let requires_response = !final_flag;

                if !missing_changes.is_empty() || requires_response {
                    debug!(
                        "Sending AckNack - missing changes: {:?}, requires response: {}",
                        missing_changes, requires_response
                    );

                    writer_proxy.increase_acknack_count();
                    let acknack_count = writer_proxy.acknack_count();
                    self.send_liveliness_acknack_message(
                        remote_guid,
                        local_reader.guid().entity_id(),
                        heartbeat.writer_id,
                        missing_changes,
                        acknack_count,
                        bitmap_base,
                    )?;
                }
            } else {
                warn!("[heartbeat] Failed to find writer proxy for GUID: {:?}", remote_guid);
            }
        } else {
            error!("[heartbeat] Failed to acquire matched_writers lock");
        }

        Ok(())
    }

    fn find_or_create_writer_proxy(
        &self,
        local_reader: &StatefulReader,
        writer_guid: Guid,
        writer_entity_id: EntityId,
    ) -> bool {
        let existing_writer_proxies = local_reader.writer_proxies();

        match existing_writer_proxies.lock() {
            Ok(writer_proxies) => {
                for writer_proxy in writer_proxies.iter() {
                    if writer_proxy.remote_writer_guid().prefix() == writer_guid.prefix() {
                        debug!(
                            "SEDP Logic: Found existing WriterProxy for GUID: {:?}",
                            writer_guid
                        );
                        return true;
                    }
                }
            }
            Err(e) => {
                error!("SEDP Logic: Failed to acquire writer proxies lock: {}", e);
                return false;
            }
        }

        let new_writer_proxy = WriterProxy::new(
            writer_guid,
            writer_entity_id,
            vec![],
            vec![],
            0,
            PublicationBuiltinTopicData::default(),
            local_reader.get_update_status_callback(),
        );
        local_reader.matched_writer_add(new_writer_proxy);
        true
    }
}

// Liveliness assertion and update
impl WlpLogic {
    // Automatic
    pub(crate) fn start_periodic_liveliness(&self, lease_duration: RtpsDuration) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let data = ParticipantMessageData::new(
            participant.guid().prefix(),
            LivelinessQosPolicyKind::Automatic,
        );

        if !lease_duration.is_infinite() {
            let handler = SendingHandler::get_instance(
                participant.clone(),
                self.sender.lock().ok().and_then(|g| g.clone()),
                None,
            );

            let send_period = lease_duration.to_std_duration() * 2 / 3;
            handler.push_message_and_wake(MessageType::P2pData(None, send_period, Arc::new(data)));
        } else if let Err(e) = self.send_liveliness_once(&data) {
            error!("Failed to send liveliness: {}", e);
        }

        Ok(())
    }

    pub(crate) fn stop_periodic_liveliness(&self) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;
        let handler = SendingHandler::get_instance(
            participant.clone(),
            self.sender.lock().ok().and_then(|g| g.clone()),
            None,
        );

        handler.cancel_p2p_messages();

        Ok(())
    }

    pub(crate) fn update_automatic_lease_duration(
        &self,
        lease_duration: RtpsDuration,
    ) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        let data = ParticipantMessageData::new(
            participant.guid().prefix(),
            LivelinessQosPolicyKind::Automatic,
        );

        let handler = SendingHandler::get_instance(
            participant.clone(),
            self.sender.lock().ok().and_then(|g| g.clone()),
            None,
        );

        handler.cancel_p2p_messages();

        if !lease_duration.is_infinite() {
            let send_period = lease_duration.to_std_duration() * 2 / 3;
            handler.push_message_and_wake(MessageType::P2pData(None, send_period, Arc::new(data)));
        }

        Ok(())
    }

    // ManualByParticipant
    pub(crate) fn assert_participant_liveliness(&self) -> RtpsResult<()> {
        self.update_local_participant_liveliness()?;

        let participant = self.get_upgraded_participant()?;

        let data = ParticipantMessageData::new(
            participant.guid().prefix(),
            LivelinessQosPolicyKind::ManualByParticipant,
        );

        participant.increase_manual_liveliness_count()?;

        self.send_liveliness_once(&data)
    }
    // ManualByTopic
    pub(crate) fn assert_writer_liveliness(&self, writer_guid: Guid) -> RtpsResult<()> {
        self.renew_asserting_writer(&writer_guid)?;

        self.send_liveliness_heartbeat(true, true, Some(writer_guid), None)
    }

    fn update_liveliness(
        participant: Arc<Participant>,
        guid: Guid,
        asserting_writers: Arc<DashMap<Guid, WriterInfo>>,
        monitored_writers: Arc<DashMap<GuidPrefix, HashMap<Guid, WriterInfo>>>,
    ) -> bool {
        log::info!("[WLP] update_liveliness called: guid={:?}", guid);

        // Local
        if participant.find_writer_from_entity_id(guid.entity_id()).is_some() {
            Self::mark_asserting_writer_lost(participant, guid, asserting_writers);
            return false;
        }

        // Remote
        log::info!(
            "[WLP] update_liveliness: No local writer found for guid={:?}, treating as REMOTE",
            guid
        );
        Self::mark_monitored_writer_lost(participant, guid, monitored_writers);
        true
    }

    fn mark_asserting_writer_lost(
        participant: Arc<Participant>,
        guid: Guid,
        asserting_writers: Arc<DashMap<Guid, WriterInfo>>,
    ) {
        if let Some(writer) = participant.find_writer_from_entity_id(guid.entity_id()) {
            log::info!("[WLP] mark_asserting_writer_lost: Found LOCAL writer for guid={:?}", guid);
            writer.update_status(StatusKind::LIVELINESS_LOST, None);
            log::warn!(
                "[WLP] mark_asserting_writer_lost: Returning early for LOCAL writer guid={:?} - readers will NOT be notified!",
                guid
            );
        }

        if let Ok(readers) = participant.find_readers_matched_with_local_writer(&guid) {
            log::info!(
                "[WLP] mark_asserting_writer_lost: Found {} readers matched with writer {:?}",
                readers.len(),
                guid
            );
            for reader in readers {
                notify_reader_liveliness_changed(&reader, &guid, LivelinessTransition::Lost);
            }

            if let Some(mut writer_info) = asserting_writers.get_mut(&guid) {
                writer_info.set_not_alive();
                debug!("[WLP] Writer {:?} set to NOT_ALIVE (participant kept for recovery)", guid);
            }
        }
    }

    // ALIVE -> NOT_ALIVE for a remote writer; entry retained for reassert.
    fn mark_monitored_writer_lost(
        participant: Arc<Participant>,
        guid: Guid,
        remote_participants: Arc<DashMap<GuidPrefix, HashMap<Guid, WriterInfo>>>,
    ) {
        log::debug!("[WLP] mark_monitored_writer_lost: guid={:?}", guid);

        if let Ok(readers) = participant.find_readers_matched_with_remote_writer(guid) {
            for reader in readers {
                notify_reader_liveliness_changed(&reader, &guid, LivelinessTransition::Lost);
            }

            if let Some(mut remote_writers) = remote_participants.get_mut(&guid.prefix()) {
                if let Some(writer_info) = remote_writers.get_mut(&guid) {
                    writer_info.set_not_alive();
                }
            }
        }
    }

    // Fan a Match/Unmatch transition out to readers matched with this writer.
    fn notify_readers_remote_writer_transition(
        participant: Arc<Participant>,
        guid: Guid,
        transition: LivelinessTransition,
    ) {
        if let Ok(readers) = participant.find_readers_matched_with_remote_writer(guid) {
            for reader in readers {
                notify_reader_liveliness_changed(&reader, &guid, transition);
            }
        }
    }

    pub(crate) fn renew_asserting_writer(&self, writer_guid: &Guid) -> RtpsResult<()> {
        if let Some(mut info) = self.asserting_writers.get_mut(writer_guid) {
            let was_not_alive = info.alive_state() == WriterAliveState::NotAlive;
            info.set_alive();

            // NOT_ALIVE -> ALIVE
            if was_not_alive {
                let participant = self.get_upgraded_participant()?;
                if let Ok(readers) = participant.find_readers_matched_with_local_writer(writer_guid)
                {
                    for reader in readers {
                        // Recovery: NOT_ALIVE -> ALIVE.
                        notify_reader_liveliness_changed(
                            &reader,
                            writer_guid,
                            LivelinessTransition::Recovered,
                        );
                    }
                }
            }

            // LivelinessMonitor Timer Update (re-track if removed after LOST)
            if let Ok(monitor) = self.liveliness_monitor.lock() {
                if let Some(monitor) = monitor.as_ref() {
                    monitor.update_writer(writer_guid);
                }
            }
        }

        Ok(())
    }

    // Participant level liveliness renewal (all MANUAL_BY_PARTICIPANT writers)
    pub(crate) fn update_local_participant_liveliness(&self) -> RtpsResult<()> {
        let guids: Vec<Guid> = self
            .asserting_writers
            .iter()
            .filter(|entry| entry.value().qos.kind == LivelinessQosPolicyKind::ManualByParticipant)
            .map(|entry| *entry.key())
            .collect();

        for guid in guids {
            self.renew_asserting_writer(&guid)?;
        }

        Ok(())
    }

    pub(crate) fn update_remote_participant_liveliness(
        &self,
        participant_message_data: ParticipantMessageData,
    ) -> RtpsResult<()> {
        let message_kind = participant_message_data.kind();

        let mut guids = Vec::new();

        if let Some(remote_writers) =
            self.monitored_writers.get_mut(&participant_message_data.participant_guid_prefix())
        {
            for (guid, info) in remote_writers.iter() {
                let should_update: bool = match message_kind {
                    ParticipantMessageDataKind::AUTOMATIC_LIVELINESS_UPDATE => {
                        info.qos().kind == LivelinessQosPolicyKind::Automatic
                    }
                    ParticipantMessageDataKind::MANUAL_LIVELINESS_UPDATE => {
                        info.qos().kind == LivelinessQosPolicyKind::ManualByParticipant
                    }
                    _ => false,
                };

                if should_update {
                    guids.push(*guid);
                }
            }
        }

        for guid in guids {
            self.mark_monitored_writer_alive(guid)?;
        }

        Ok(())
    }

    pub(crate) fn mark_monitored_writer_alive(&self, writer_guid: Guid) -> RtpsResult<()> {
        if let Some(mut remote_writers) = self.monitored_writers.get_mut(&writer_guid.prefix()) {
            if let Some(info) = remote_writers.get_mut(&writer_guid) {
                let was_not_alive = info.alive_state() == WriterAliveState::NotAlive;
                info.set_alive();

                let lease_duration = info.qos().lease_duration;

                drop(remote_writers);

                // NOT_ALIVE -> ALIVE
                if was_not_alive {
                    let participant = self.get_upgraded_participant()?;
                    if let Ok(readers) =
                        participant.find_readers_matched_with_remote_writer(writer_guid)
                    {
                        for reader in readers {
                            // Recovery: was NOT_ALIVE, now ALIVE
                            notify_reader_liveliness_changed(
                                &reader,
                                &writer_guid,
                                LivelinessTransition::Recovered,
                            );
                        }
                    }
                }

                // LivelinessMonitor Timer Update (re-track if removed after LOST)
                if let Ok(monitor) = self.liveliness_monitor.lock() {
                    if let Some(monitor) = monitor.as_ref() {
                        monitor.track_writer(&writer_guid, lease_duration);
                    }
                }
            }
        }

        Ok(())
    }
}

// Reader-side state transition for a tracked writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LivelinessTransition {
    // None -> ALIVE: first observation.
    Match,
    // ALIVE -> NOT_ALIVE: lease expired.
    Lost,
    // NOT_ALIVE -> ALIVE: reasserted.
    Recovered,
    // ALIVE -> None: unmatch/delete while alive.
    UnmatchAlive,
    // NOT_ALIVE -> None: unmatch/delete while not alive.
    UnmatchNotAlive,
}

impl LivelinessTransition {
    pub(crate) fn deltas(self) -> (i32, i32) {
        match self {
            Self::Match => (1, 0),
            Self::Lost => (-1, 1),
            Self::Recovered => (1, -1),
            Self::UnmatchAlive => (-1, 0),
            Self::UnmatchNotAlive => (0, -1),
        }
    }

    pub(crate) fn from_deltas(alive_change: i32, not_alive_change: i32) -> Option<Self> {
        match (alive_change, not_alive_change) {
            (1, 0) => Some(Self::Match),
            (-1, 1) => Some(Self::Lost),
            (1, -1) => Some(Self::Recovered),
            (-1, 0) => Some(Self::UnmatchAlive),
            (0, -1) => Some(Self::UnmatchNotAlive),
            _ => None,
        }
    }
}

fn notify_reader_liveliness_changed(
    reader: &Arc<dyn Reader + Send + Sync>,
    guid: &Guid,
    transition: LivelinessTransition,
) {
    let (alive_change, not_alive_change) = transition.deltas();
    reader.update_status(
        StatusKind::LIVELINESS_CHANGED,
        Some(Arc::new(LivelinessChangedStatus {
            alive_count: 0,
            not_alive_count: 0,
            alive_count_change: alive_change,
            not_alive_count_change: not_alive_change,
            last_publication_handle: InstanceHandle::from_guid(guid),
        })),
    );
}

impl_participant_accessor!(WlpLogic);

impl UnicastMessageProcessor for WlpLogic {
    fn handle_data_message(
        &mut self,
        rtps_header: &Header,
        _submessage_header: &SubmessageHeader,
        data: &Data,
        _message_receiver: &MessageReceiver,
    ) -> RtpsResult<()> {
        if data.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
            let payload = Some(data.serialized_data());

            match payload {
                Some(payload) => match ParticipantMessageData::from_serialized_data(&payload) {
                    Ok(pmd) => {
                        return self.handle_liveliness_message(
                            pmd,
                            Guid::new(
                                rtps_header.guid_prefix(),
                                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                            ),
                            data.writer_sn,
                        );
                    }
                    Err(e) => {
                        return Err(RtpsError::new(
                            RtpsErrorCode::DeserializationError,
                            format!("WLP Logic: failed to deserialize data: {}", e),
                        ));
                    }
                },
                None => {
                    debug!("WLP Logic: No payload found");
                }
            }
        }

        Ok(())
    }

    fn handle_heartbeat_message(
        &mut self,
        rtps_header: &Header,
        submessage_header: &SubmessageHeader,
        heartbeat: &Heartbeat,
    ) -> RtpsResult<()> {
        let final_flag = submessage_header.final_flag().unwrap_or(false);
        let liveliness_flag = submessage_header.liveliness_flag().unwrap_or(false);

        if heartbeat.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
            debug!(
                "[WLP Logic] P2P HEARTBEAT matched - final_flag: {}, liveliness_flag: {}",
                final_flag, liveliness_flag
            );

            let _ = self.handle_heartbeat_message_inner(
                heartbeat,
                Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
                final_flag,
                liveliness_flag,
            );
        } else if liveliness_flag {
            debug!(
                "[WLP Logic] User HEARTBEAT matched - final_flag: {}, liveliness_flag: {}",
                final_flag, liveliness_flag
            );

            let _ = self.handle_heartbeat_message_inner(
                heartbeat,
                Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
                final_flag,
                liveliness_flag,
            );
        }

        Ok(())
    }

    fn handle_acknack_message(
        &mut self,
        rtps_header: &Header,
        acknack: &AckNack,
    ) -> RtpsResult<()> {
        if acknack.writer_id != EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
            return Ok(());
        }

        debug!("[WlpLogic] handle_acknack_message should be called");

        let participant = self.get_upgraded_participant()?;
        let remote_guid = Guid::new(rtps_header.guid_prefix(), acknack.writer_id);

        let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
            acknack.writer_id,
            participant.clone(),
        )? {
            Some(proxy) => proxy,
            None => {
                warn!(
                    "[acknack] No matching P2P builtin reader found for reader entity ID: {:?}",
                    acknack.reader_id
                );
                return Ok(());
            }
        };

        let writer = builtin_endpoint_pair.writer();

        let missing_sequence_numbers = acknack.reader_sn_state.extract_numbers();

        debug!(
            "Missing sequence numbers {:?} from remote: {:?}",
            missing_sequence_numbers, remote_guid
        );

        if missing_sequence_numbers.is_empty() {
            return Ok(());
        }

        let missing_changes = {
            let writer_cache = writer.writer_cache();
            let writer_cache_guard = match writer_cache.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("[acknack] Failed to acquire writer cache lock: {}", e);
                    return Ok(());
                }
            };

            let mut missing_changes = Vec::new();
            for seq_num in missing_sequence_numbers {
                if let Some(change) = writer_cache_guard.get_change(seq_num) {
                    missing_changes.push(change);
                }
            }
            missing_changes
        };

        if missing_changes.is_empty() {
            return Ok(());
        }

        debug!(
            "Retransmitting {} missing changes from remote: {:?}",
            missing_changes.len(),
            remote_guid
        );

        let reader_proxies = writer.reader_proxies();
        let proxies_guard = reader_proxies.lock().map_err(|e| {
            RtpsError::new(
                RtpsErrorCode::LockError,
                format!("Failed to lock reader proxies: {}", e),
            )
        })?;

        // If no reader proxies, don't add to cache and don't send
        if proxies_guard.is_empty() {
            debug!("[WLP] No reader proxies, skipping");
            return Ok(());
        }

        for change in missing_changes {
            let heartbeat_info = {
                let writer_cache = writer.writer_cache();
                let cache_guard = match writer_cache.lock() {
                    Ok(guard) => guard,
                    Err(e) => {
                        warn!("[WLP] Failed to acquire writer cache lock for change, skipping this change: {}", e);
                        continue; // Skip this change and process next change
                    }
                };

                let wlp_last_change_sn = writer.last_change_sequence_number();
                Some((
                    writer.heartbeat_count(),
                    cache_guard.get_seq_num_min().unwrap_or(wlp_last_change_sn + 1),
                    cache_guard.get_seq_num_max().unwrap_or(wlp_last_change_sn),
                    false,
                    false,
                ))
            };

            let participant = self.get_upgraded_participant()?;
            for reader_proxy in proxies_guard.iter() {
                // Get heartbeat info to include in the same RTPS message as Data
                let mut send_buffer = match participant.wire_buffer_pool().lock() {
                    Ok(mut pool) => pool.acquire(),
                    Err(_) => {
                        warn!("[WLP] Failed to lock wire buffer pool");
                        continue; // Skip this reader_proxy
                    }
                };
                let result = MessageCreator::create_data_msg(
                    &change,
                    reader_proxy.remote_reader_guid(),
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                    heartbeat_info, // Include heartbeat in the same message
                    false,
                    None,
                    &mut send_buffer,
                );

                if let Err(e) = result {
                    warn!("[WLP] Failed to create DATA message for reader_proxy: {:?}", e);
                    if let Ok(mut pool) = participant.wire_buffer_pool().lock() {
                        pool.release(send_buffer);
                    }
                    continue; // Skip this reader_proxy and process next reader_proxy
                }

                for locator in reader_proxy.unicast_locator_list() {
                    if locator.kind() == 1 {
                        //UDPv4
                        let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                            locator.to_ip_v4_addr(),
                            locator.port() as u16,
                        ));

                        if let Ok(guard) = self.sender.lock() {
                            if let Some(sender) = guard.as_ref() {
                                if let Err(e) = sender.send(&socket_addr, &send_buffer) {
                                    warn!(
                                        "[WLP] Failed to send DATA message to locator {:?}: {:?}",
                                        socket_addr, e
                                    );
                                    // Even if error occurs, continue trying other locators
                                }
                            }
                        }
                    }
                }
                if let Ok(mut pool) = participant.wire_buffer_pool().lock() {
                    pool.release(send_buffer);
                }
            }
            writer.increase_heartbeat_count();
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::LivelinessTransition;

    // Transition matrix for LivelinessChangedStatus deltas.
    #[test]
    fn match_increments_alive_only() {
        assert_eq!(LivelinessTransition::Match.deltas(), (1, 0));
    }

    #[test]
    fn lost_swaps_alive_to_not_alive() {
        assert_eq!(LivelinessTransition::Lost.deltas(), (-1, 1));
    }

    #[test]
    fn recovered_swaps_not_alive_back_to_alive() {
        assert_eq!(LivelinessTransition::Recovered.deltas(), (1, -1));
    }

    // Normal unmatch must NOT bump not_alive_count — see DDS spec
    // LivelinessChangedStatus: not_alive_count tracks failed liveliness only.
    #[test]
    fn unmatch_alive_decrements_alive_only() {
        assert_eq!(LivelinessTransition::UnmatchAlive.deltas(), (-1, 0));
    }

    #[test]
    fn unmatch_not_alive_decrements_not_alive_only() {
        assert_eq!(LivelinessTransition::UnmatchNotAlive.deltas(), (0, -1));
    }
}
