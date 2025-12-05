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
        status::{LivelinessChangedStatus, LivelinessLostStatus, StatusKind},
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
            reader::{StatefulReader, WriterProxy},
            writer::{StatefulWriter, Writer},
        },
        logic::data::builtin_endpoint_pair::BuiltinEndpointPair,
        messages::{
            message_creator::MessageCreator,
            message_receiver::{MessageReceiver, TypedSubmessage},
            submessages::{ack_nack::AckNack, heartbeat::Heartbeat},
        },
        task::{
            sending_handler::{MessageType, SendingHandler},
            timer_handler::TimerHandler,
        },
        transport::{Transport, TransportSender},
    },
};
use std::{
    collections::HashMap,
    net::{SocketAddr, SocketAddrV4},
    sync::{Arc, Mutex},
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
    participant: Arc<Participant>,
    sender: Arc<Mutex<Option<Arc<TransportSender>>>>,
    timer_handler: Arc<Mutex<TimerHandler>>,
    // Local user-defined writers (GUID -> LivelinessQosPolicy)
    local_writers: Arc<DashMap<Guid, WriterInfo>>,
    // Remote user-defined writers (GUID -> LivelinessQosPolicy)
    remote_participants: Arc<DashMap<GuidPrefix, HashMap<Guid, WriterInfo>>>,
    min_lease_duration: Arc<Mutex<RtpsDuration>>,
    liveliness_monitor: Arc<Mutex<Option<LivelinessMonitor>>>,
}

impl WlpLogic {
    pub(crate) fn new(participant: Arc<Participant>, sender: Arc<TransportSender>) -> Self {
        let timer_handler = TimerHandler::get_instance(participant.clone());
        Self {
            participant,
            sender: Arc::new(Mutex::new(Some(sender))),
            timer_handler,
            local_writers: Arc::new(DashMap::new()),
            remote_participants: Arc::new(DashMap::new()),
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

    pub(crate) fn add_local_writer(
        &self,
        writer_guid: Guid,
        liveliness: LivelinessQosPolicy,
    ) -> RtpsResult<()> {
        log::debug!(
            "[WLP] add_local_writer: writer_guid={:?}, liveliness_kind={:?}, lease_duration={:?}",
            writer_guid,
            liveliness.kind,
            liveliness.lease_duration
        );
        let info = WriterInfo::new(liveliness);
        self.local_writers.insert(writer_guid, info);

        Self::update_local_liveliness(
            self.participant.clone(),
            writer_guid,
            self.local_writers.clone(),
            true,
        );

        if liveliness.kind == LivelinessQosPolicyKind::Automatic {
            match self.min_lease_duration.lock() {
                Ok(mut min_lease_duration) => {
                    let prev = *min_lease_duration;
                    let new = liveliness.lease_duration;
                    if (prev.is_zero() || prev.is_infinite())
                        && !(new.is_zero() || new.is_infinite())
                    {
                        *min_lease_duration = new.into();

                        self.start_periodic_liveliness(*min_lease_duration);
                        Ok(())
                    } else {
                        *min_lease_duration = std::cmp::min(*min_lease_duration, new.into());

                        if prev != *min_lease_duration {
                            self.update_automatic_lease_duration(*min_lease_duration);
                        }
                        Ok(())
                    }
                }
                Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
            }
        } else {
            if let Some(writer) =
                self.participant.find_writer_from_entity_id(writer_guid.entity_id())
            {
                if let Some(writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                    match writer.reader_proxies().lock() {
                        Ok(proxies) => {
                            log::debug!(
                                "[WLP] add_local_writer: writer_guid={:?}, reader_proxies count={}",
                                writer_guid,
                                proxies.len()
                            );
                            if proxies.is_empty() {
                                // No readers yet, skip liveliness setup
                                log::warn!(
                                    "[WLP] add_local_writer: SKIPPING liveliness registration for writer_guid={:?} - no readers matched yet!",
                                    writer_guid
                                );
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to lock reader proxies in add_local_writer: {:?}, continuing anyway", e);
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
                        let participant = self.participant.clone();
                        let local_writers = self.local_writers.clone();
                        let remote_participants = self.remote_participants.clone();

                        let callback = Arc::new(move |guid: Guid| {
                            Self::update_liveliness(
                                participant.clone(),
                                guid,
                                local_writers.clone(),
                                remote_participants.clone(),
                            )
                        });

                        *liveliness_monitor = Some(LivelinessMonitor::new(callback));
                    }

                    match liveliness_monitor.as_ref() {
                        Some(liveliness_monitor) => {
                            log::info!(
                                "[WLP] add_local_writer: Registering writer_guid={:?} to LivelinessMonitor, lease_duration={:?}",
                                writer_guid, liveliness.lease_duration
                            );
                            liveliness_monitor
                                .track_writer(&writer_guid, liveliness.lease_duration);
                            Ok(())
                        }
                        None => Err(RtpsError::new(
                            RtpsErrorCode::NotInitialized,
                            format!(
                                "Liveliness Monitor for Participant: {:?}",
                                self.participant.guid(),
                            ),
                        )),
                    }
                }
                Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
            }
        }
    }

    pub(crate) fn remove_local_writer(&self, writer_guid: Guid) -> RtpsResult<()> {
        self.local_writers.remove(&writer_guid);

        match self.liveliness_monitor.lock() {
            Ok(liveliness_monitor) => match liveliness_monitor.as_ref() {
                Some(liveliness_monitor) => {
                    liveliness_monitor.cancel_writer(writer_guid);

                    let has_automatic = self
                        .local_writers
                        .iter()
                        .any(|entry| entry.value().qos.kind == LivelinessQosPolicyKind::Automatic);

                    if !has_automatic {
                        self.stop_periodic_liveliness();
                    }

                    Ok(())
                }
                None => Err(RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!("Liveliness Monitor for Participant: {:?}", self.participant.guid(),),
                )),
            },
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }

    pub(crate) fn add_remote_writer(
        &self,
        writer_guid: Guid,
        liveliness: LivelinessQosPolicy,
    ) -> RtpsResult<()> {
        let prefix = writer_guid.prefix();
        let writer_info = WriterInfo::new(liveliness);

        if let Some(mut writers) = self.remote_participants.get_mut(&prefix) {
            writers.insert(writer_guid, writer_info);
        } else {
            let mut new_map = HashMap::new();
            new_map.insert(writer_guid, writer_info);
            self.remote_participants.insert(prefix, new_map);
        }

        Self::update_remote_liveliness(
            self.participant.clone(),
            writer_guid,
            self.remote_participants.clone(),
            true,
        );

        match self.liveliness_monitor.lock() {
            Ok(mut liveliness_monitor) => {
                if liveliness_monitor.is_none() {
                    let participant = self.participant.clone();
                    let local_writers = self.local_writers.clone();
                    let remote_participants = self.remote_participants.clone();

                    let callback = Arc::new(move |guid: Guid| {
                        Self::update_liveliness(
                            participant.clone(),
                            guid,
                            local_writers.clone(),
                            remote_participants.clone(),
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
                            self.participant.guid()
                        );
                        Err(RtpsError::new(
                            RtpsErrorCode::NotInitialized,
                            format!(
                                "Liveliness Monitor for Participant: {:?}",
                                self.participant.guid(),
                            ),
                        ))
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to lock liveliness monitor in add_remote_writer: {:?}", e);
                // Return Ok to not fail remote writer addition, retry will happen on next update
                Ok(())
            }
        }
    }

    pub(crate) fn remove_remote_writer(&self, writer_guid: Guid) -> RtpsResult<()> {
        if let Some(mut hash_map) = self.remote_participants.get_mut(&writer_guid.prefix()) {
            hash_map.remove(&writer_guid);

            if hash_map.is_empty() {
                drop(hash_map);
                self.remote_participants.remove(&writer_guid.prefix());
            }
        }

        Self::update_remote_liveliness(
            self.participant.clone(),
            writer_guid,
            self.remote_participants.clone(),
            false,
        );

        match self.liveliness_monitor.lock() {
            Ok(liveliness_monitor) => match liveliness_monitor.as_ref() {
                Some(liveliness_monitor) => {
                    liveliness_monitor.cancel_writer(writer_guid);
                    Ok(())
                }
                None => Err(RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    format!("Liveliness Monitor for Participant: {:?}", self.participant.guid(),),
                )),
            },
            Err(e) => Err(RtpsError::new(RtpsErrorCode::LockError, e.to_string())),
        }
    }

    // for Automatic/ManualByParticipant
    pub(crate) fn send_participant_message_data(
        &self,
        _start_time: Option<Instant>,
        duration: StdDuration,
        participant_message_data: Arc<ParticipantMessageData>,
    ) {
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
                MessageType::P2p(Some(start_time), duration, participant_message_data),
            );
        }
    }
    // EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER Heartbeat
    pub(crate) fn send_liveliness_heartbeat(
        &self,
        liveliness_flag: bool,
        final_flag: bool,
        writer_guid: Option<Guid>,
        target_guid_prefix: Option<GuidPrefix>, // for P2P initial
    ) -> RtpsResult<()> {
        if let Some(writer_guid) = writer_guid {
            if let Some(writer) =
                self.participant.find_writer_from_entity_id(writer_guid.entity_id())
            {
                if let Some(writer) = writer.as_any().downcast_ref::<StatefulWriter>() {
                    let (first_sn, last_sn, heartbeat_count) = {
                        match writer.writer_cache().lock() {
                            Ok(writer_cache) => {
                                let info = (
                                    writer_cache.get_seq_num_min(),
                                    writer_cache.get_seq_num_max(),
                                    writer.heartbeat_count(),
                                );
                                debug!("send_liveliness_heartbeat called: count={}, first={:?}, last={:?}, cache_size={}, liveliness_flag={}",
                        info.2, info.0, info.1, writer_cache.get_changes().len(), liveliness_flag);
                                info
                            }
                            Err(e) => {
                                log::warn!("Failed to lock writer cache for WLP heartbeat: {}, using UNKNOWN sequence numbers", e);
                                // Use UNKNOWN sequence numbers to still send heartbeat
                                (
                                    SequenceNumber::UNKNOWN,
                                    SequenceNumber::UNKNOWN,
                                    writer.heartbeat_count(),
                                )
                            }
                        }
                    };

                    let reader_proxies = writer.reader_proxies();
                    let proxies_guard = reader_proxies.lock().map_err(|e| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock reader proxies for WLP: {}", e),
                        )
                    })?;
                    for reader_proxy in proxies_guard.iter() {
                        if !reader_proxy.is_active() {
                            log::warn!(
                                "[WLP] Skipping inactive reader proxy: {:?}",
                                reader_proxy.remote_reader_guid()
                            );
                            continue;
                        }
                        log::debug!(
                            "[WLP] Sending heartbeat to active reader proxy: {:?}",
                            reader_proxy.remote_reader_guid()
                        );

                        let participant_guid = {
                            let local_participant_data =
                                self.participant.local_participant_proxy_data();
                            local_participant_data.participant_guid()
                        };

                        let buffer = match MessageCreator::create_heartbeat_message(
                            participant_guid,
                            reader_proxy.remote_reader_guid(),
                            heartbeat_count,
                            reader_proxy.remote_group_entity_id(),
                            writer_guid.entity_id(),
                            first_sn,
                            last_sn,
                            final_flag,
                            liveliness_flag,
                        ) {
                            Ok(buf) => buf,
                            Err(e) => {
                                log::warn!("Failed to create WLP heartbeat message: {:?}", e);
                                continue; // Skip this reader proxy
                            }
                        };

                        // WLP messages must use metatraffic locators, not user traffic locators
                        let remote_guid_prefix = reader_proxy.remote_reader_guid().prefix();
                        match self.participant.remote_participant_proxy_datas().clone().lock() {
                            Ok(remote_participant_datas) => {
                                for remote_participant_data in remote_participant_datas.iter() {
                                    if remote_participant_data.participant_guid().prefix()
                                        == remote_guid_prefix
                                    {
                                        for locator in remote_participant_data
                                            .metatraffic_unicast_locator_list()
                                        {
                                            if locator.kind() == 1 {
                                                //UDPv4
                                                let socket_addr =
                                                    SocketAddr::V4(SocketAddrV4::new(
                                                        locator.to_ip_v4_addr(),
                                                        locator.port() as u16,
                                                    ));

                                                if let Ok(guard) = self.sender.lock() {
                                                    if let Some(sender) = guard.as_ref() {
                                                        if let Err(e) =
                                                            sender.send(&socket_addr, &buffer)
                                                        {
                                                            log::warn!(
                                                                "Failed to send WLP heartbeat: {:?}",
                                                                e
                                                            );
                                                        } else {
                                                            debug!("[WLP] Sent liveliness heartbeat to metatraffic port: {}", socket_addr);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        break;
                                    }
                                }
                            }
                            Err(e) => {
                                log::warn!(
                                    "Failed to lock remote_participant_datas for WLP heartbeat: {}",
                                    e
                                );
                                continue; // Skip this reader proxy, continue with others
                            }
                        }
                    }

                    writer.increase_heartbeat_count();
                    Ok(())
                } else {
                    // Nothing to process
                    Ok(())
                    // Err(RtpsError::new(
                    //     RtpsErrorCode::InvalidEntityKind,
                    //     "User writer is not stateful",
                    // ))
                }
            } else {
                Err(RtpsError::new(
                    RtpsErrorCode::NotInitialized,
                    "Writer not Initialized for Guid: {:?}",
                ))
            }
        } else {
            let writer = self.participant.builtin_participant_message_writer();

            let (first_sn, last_sn, heartbeat_count) = {
                match writer.writer_cache().lock() {
                    Ok(writer_cache) => {
                        let info = (
                            writer_cache.get_seq_num_min(),
                            writer_cache.get_seq_num_max(),
                            writer.heartbeat_count(),
                        );
                        debug!("send_liveliness_heartbeat called: count={}, first={:?}, last={:?}, cache_size={}, liveliness_flag={}",
                        info.2, info.0, info.1, writer_cache.get_changes().len(), liveliness_flag);
                        info
                    }
                    Err(e) => {
                        log::warn!("Failed to lock writer cache for P2P WLP heartbeat: {}, using UNKNOWN sequence numbers", e);
                        // Use UNKNOWN sequence numbers to still send heartbeat
                        (SequenceNumber::UNKNOWN, SequenceNumber::UNKNOWN, writer.heartbeat_count())
                    }
                }
            };

            let reader_proxies = writer.reader_proxies();
            let proxies_guard = reader_proxies.lock().map_err(|e| {
                RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to lock reader proxies for P2P: {}", e),
                )
            })?;

            for reader_proxy in proxies_guard.iter() {
                if !reader_proxy.is_active() {
                    continue;
                }

                if let Some(prefix) = target_guid_prefix {
                    if reader_proxy.remote_reader_guid().prefix() != prefix {
                        continue;
                    }
                }

                let participant_guid = {
                    let local_participant_data = self.participant.local_participant_proxy_data();
                    local_participant_data.participant_guid()
                };
                let buffer = match MessageCreator::create_heartbeat_message(
                    participant_guid,
                    reader_proxy.remote_reader_guid(),
                    heartbeat_count,
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                    first_sn,
                    last_sn,
                    final_flag,
                    liveliness_flag,
                ) {
                    Ok(buf) => buf,
                    Err(e) => {
                        log::warn!("Failed to create P2P heartbeat message: {:?}", e);
                        continue; // Skip this reader proxy
                    }
                };

                // WLP messages must use metatraffic locators, not user traffic locators
                let remote_guid_prefix = reader_proxy.remote_reader_guid().prefix();
                match self.participant.remote_participant_proxy_datas().clone().lock() {
                    Ok(remote_participant_datas) => {
                        for remote_participant_data in remote_participant_datas.iter() {
                            if remote_participant_data.participant_guid().prefix()
                                == remote_guid_prefix
                            {
                                for locator in
                                    remote_participant_data.metatraffic_unicast_locator_list()
                                {
                                    if locator.kind() == 1 {
                                        //UDPv4
                                        let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                                            locator.to_ip_v4_addr(),
                                            locator.port() as u16,
                                        ));

                                        if let Ok(guard) = self.sender.lock() {
                                            if let Some(sender) = guard.as_ref() {
                                                if let Err(e) = sender.send(&socket_addr, &buffer) {
                                                    log::warn!(
                                                        "Failed to send P2P heartbeat: {:?}",
                                                        e
                                                    );
                                                } else {
                                                    debug!("[WLP] Sent P2P liveliness heartbeat to metatraffic port: {}", socket_addr);
                                                }
                                            }
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        log::warn!(
                            "Failed to lock remote_participant_datas for P2P heartbeat: {}",
                            e
                        );
                        continue; // Skip this reader proxy, continue with others
                    }
                }
            }

            writer.increase_heartbeat_count();
            Ok(())
        }
    }
    // Automatic
    pub(crate) fn start_periodic_liveliness(&self, lease_duration: RtpsDuration) {
        let data = ParticipantMessageData::new(
            self.participant.guid().prefix(),
            LivelinessQosPolicyKind::Automatic,
        );

        if !lease_duration.is_infinite() {
            let handler = SendingHandler::get_instance(
                self.participant.clone(),
                self.sender.lock().ok().and_then(|g| g.clone()),
                None,
            );

            let send_period = lease_duration.to_std_duration() * 2 / 3;
            handler.push_message_and_wake(MessageType::P2p(None, send_period, Arc::new(data)));
        } else if let Err(e) = self.send_liveliness_once(&data) {
            error!("Failed to send liveliness: {}", e);
        }
    }

    pub(crate) fn stop_periodic_liveliness(&self) {
        let handler = SendingHandler::get_instance(
            self.participant.clone(),
            self.sender.lock().ok().and_then(|g| g.clone()),
            None,
        );

        handler.cancel_p2p_messages();
    }

    pub(crate) fn update_automatic_lease_duration(&self, lease_duration: RtpsDuration) {
        let data = ParticipantMessageData::new(
            self.participant.guid().prefix(),
            LivelinessQosPolicyKind::Automatic,
        );

        let handler = SendingHandler::get_instance(
            self.participant.clone(),
            self.sender.lock().ok().and_then(|g| g.clone()),
            None,
        );

        handler.cancel_p2p_messages();

        if !lease_duration.is_infinite() {
            let send_period = lease_duration.to_std_duration() * 2 / 3;
            handler.push_message_and_wake(MessageType::P2p(None, send_period, Arc::new(data)));
        }
    }
    // ManualByParticipant
    pub(crate) fn send_liveliness_once(&self, data: &ParticipantMessageData) -> RtpsResult<()> {
        let writer = self.participant.builtin_participant_message_writer();

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
            Arc::from(payload.to_vec()),
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
            if let Err(e) = cache_guard.add_change(cache_change.clone()) {
                log::warn!("Failed to add liveliness change to cache: {}, continuing to send heartbeat anyway", e);
                // Don't return - continue to send heartbeat even if cache add fails
            }
        } else {
            log::warn!("Failed to lock writer cache in send_liveliness_once, continuing to send heartbeat anyway");
            // Don't return - continue to send heartbeat even if cache lock fails
        }

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
                let info = Some((
                    writer.heartbeat_count(),
                    cache_guard.get_seq_num_min(),
                    cache_guard.get_seq_num_max(),
                    false,
                    false,
                ));
                debug!(
                    "[WLP] heartbeat_info: count={}, first={:?}, last={:?}",
                    writer.heartbeat_count(),
                    cache_guard.get_seq_num_min(),
                    cache_guard.get_seq_num_max()
                );
                info
            };

            let buffer = match MessageCreator::create_data_msg(
                cache_change.clone(),
                reader_proxy.remote_reader_guid(),
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                heartbeat_info, // Include heartbeat in the same message
                false,
                None,
            ) {
                Ok(buf) => buf,
                Err(e) => {
                    log::warn!("Failed to create P2P DATA message: {:?}", e);
                    continue; // Skip this reader proxy
                }
            };

            for locator in reader_proxy.unicast_locator_list() {
                if locator.kind() == 1 {
                    //UDPv4
                    let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                        locator.to_ip_v4_addr(),
                        locator.port() as u16,
                    ));

                    if let Ok(guard) = self.sender.lock() {
                        if let Some(sender) = guard.as_ref() {
                            if let Err(e) = sender.send(&socket_addr, &buffer) {
                                log::warn!("Failed to send P2P DATA message: {:?}", e);
                            }
                        }
                    }
                }
            }
        }
        writer.increase_heartbeat_count();

        Ok(())
    }
    // ManualByParticipant
    pub(crate) fn assert_participant_liveliness(&self) -> RtpsResult<()> {
        self.update_local_participant_liveliness();

        let data = ParticipantMessageData::new(
            self.participant.guid().prefix(),
            LivelinessQosPolicyKind::ManualByParticipant,
        );

        self.participant.increase_manual_liveliness_count()?;

        self.send_liveliness_once(&data)
    }
    // ManualByTopic
    pub(crate) fn assert_writer_liveliness(&self, writer_guid: Guid) -> RtpsResult<()> {
        self.update_local_writer_liveliness(&writer_guid);

        self.send_liveliness_heartbeat(true, true, Some(writer_guid), None)
    }

    fn send_discovery_message(&self, buffer: &[u8], remote_guid: Guid, message_type: &str) -> bool {
        let mut is_sent = false;

        match self.participant.remote_participant_proxy_datas().clone().lock() {
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
        is_sent
    }

    pub(crate) fn handle_rtps_message(&self, message_receiver: MessageReceiver) {
        debug!("[WlpLogic] handle_rtps_message called");
        if message_receiver.has_dst_submessage() {
            let local_guid_prefix = self.participant.guid().prefix();
            if !message_receiver.is_dst_me(local_guid_prefix) {
                debug!("WLP Logic: INFO_DST is not me");
                return;
            }
        }

        let rtps_header = *message_receiver.rtps_message_header().unwrap();

        let submessages = message_receiver.parse_submessages();
        for submessage in submessages {
            match submessage {
                TypedSubmessage::Heartbeat(header, heartbeat) => {
                    let final_flag = header.final_flag().unwrap_or(false);
                    let liveliness_flag = header.liveliness_flag().unwrap_or(false);
                    if heartbeat.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
                        debug!("[WLP Logic] P2P HEARTBEAT matched - final_flag: {}, liveliness_flag: {}", final_flag, liveliness_flag);

                        let _ = self.handle_heartbeat_message(
                            heartbeat,
                            Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
                            final_flag,
                            liveliness_flag,
                        );
                    } else if liveliness_flag {
                        debug!("[WLP Logic] User HEARTBEAT matched - final_flag: {}, liveliness_flag: {}", final_flag, liveliness_flag);

                        let _ = self.handle_heartbeat_message(
                            heartbeat,
                            Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
                            final_flag,
                            liveliness_flag,
                        );
                    }
                }
                TypedSubmessage::AckNack(_header, acknack) => {
                    if acknack.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
                        let _ = self.handle_acknack_message(
                            acknack.clone(),
                            Guid::new(rtps_header.guid_prefix(), acknack.writer_id),
                        );
                        debug!("[WlpLogic] handle_acknack_message should be called");
                    }
                }
                TypedSubmessage::Data(_header, data) => {
                    if data.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
                        let payload =
                            message_receiver.payload_from_data(data.reader_id, data.writer_id);

                        match payload {
                            Some(payload) => {
                                match ParticipantMessageData::from_serialized_data(payload) {
                                    Ok(pmd) => {
                                        self.handle_liveliness_message(
                                            pmd,
                                            Guid::new(
                                                rtps_header.guid_prefix(),
                                                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                                            ),
                                            data.writer_sn,
                                        );
                                    }
                                    Err(e) => {
                                        error!("WLP Logic: failed to deserialize data: {}", e);
                                    }
                                }
                            }
                            None => {
                                debug!("WLP Logic: No payload found");
                            }
                        }
                    }
                }
                _ => {
                    // debug!("Wlp Logic: Unsupported submessage type: {:?}", submessage);
                }
            }
        }
    }

    // Automatic, ManualByParticipant
    pub(crate) fn handle_liveliness_message(
        &self,
        participant_message_data: ParticipantMessageData,
        remote_guid: Guid,
        writer_sn: SequenceNumber,
    ) {
        debug!("[WlpLogic] handle_liveliness_message called");
        // 1. Find WriterProxy
        let reader = self.participant.builtin_participant_message_reader();

        // 2. Update WriterProxy
        if let Ok(mut matched_writers) = reader.writer_proxies().lock() {
            if let Some(writer_proxy) =
                matched_writers.iter_mut().find(|proxy| proxy.remote_writer_guid() == remote_guid)
            {
                writer_proxy.mark_change_received(writer_sn, None);

                // 3. Update Liveliness timer
                self.update_remote_participant_liveliness(participant_message_data);
            }
        }
    }
    // ManualByTopic
    pub(crate) fn handle_heartbeat_message(
        &self,
        heartbeat: &Heartbeat,
        remote_guid: Guid,
        final_flag: bool,
        liveliness_flag: bool,
    ) -> RtpsResult<()> {
        if heartbeat.writer_id.entity_kind().is_built_in() {
            // Builtin endpoint (Automatic/ManualByParticipant)
            let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
                heartbeat.writer_id,
                self.participant.clone(),
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
            );

            debug!("[WLP] Processing builtin heartbeat from writer: {:?}", remote_guid);
        } else {
            match self.participant.find_readers_matched_with_remote_writer(remote_guid) {
                Ok(readers) => {
                    if readers.is_empty() {
                        warn!(
                            "[heartbeat] No matching reader found for user-defined writer: {:?}",
                            remote_guid
                        );
                        return Ok(());
                    }
                    if liveliness_flag {
                        self.update_remote_writer_liveliness(remote_guid);

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
                                );
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
    ) {
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
                        heartbeat.reader_id,
                        heartbeat.writer_id,
                        missing_changes,
                        acknack_count,
                        bitmap_base,
                    );
                }
            } else {
                warn!("[heartbeat] Failed to find writer proxy for GUID: {:?}", remote_guid);
            }
        } else {
            error!("[heartbeat] Failed to acquire matched_writers lock");
        }
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

    fn send_liveliness_acknack_message(
        &self,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        missing_changes: Vec<SequenceNumber>,
        acknack_count: i32,
        bitmap_base: SequenceNumber,
    ) {
        let buffer = MessageCreator::create_acknack_message(
            self.participant.guid(),
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
                self.send_discovery_message(&buffer, remote_guid, "AckNack");
            }
            Err(e) => {
                warn!("Failed to create WLP AckNack message: {:?}", e);
            }
        }
    }

    pub(crate) fn handle_acknack_message(
        &self,
        acknack: AckNack,
        remote_guid: Guid,
    ) -> RtpsResult<()> {
        let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
            acknack.writer_id,
            self.participant.clone(),
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
            let all_changes = writer_cache_guard.get_changes();
            for seq_num in missing_sequence_numbers {
                if let Some(change) =
                    all_changes.iter().find(|c| c.sequence_number() == seq_num).cloned()
                {
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

                Some((
                    writer.heartbeat_count(),
                    cache_guard.get_seq_num_min(),
                    cache_guard.get_seq_num_max(),
                    false,
                    false,
                ))
            };

            for reader_proxy in proxies_guard.iter() {
                // Get heartbeat info to include in the same RTPS message as Data
                let buffer = match MessageCreator::create_data_msg(
                    change.clone(),
                    reader_proxy.remote_reader_guid(),
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
                    EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER,
                    heartbeat_info, // Include heartbeat in the same message
                    false,
                    None,
                ) {
                    Ok(buf) => buf,
                    Err(e) => {
                        warn!("[WLP] Failed to create DATA message for reader_proxy: {:?}", e);
                        continue; // Skip this reader_proxy and process next reader_proxy
                    }
                };

                for locator in reader_proxy.unicast_locator_list() {
                    if locator.kind() == 1 {
                        //UDPv4
                        let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                            locator.to_ip_v4_addr(),
                            locator.port() as u16,
                        ));

                        if let Ok(guard) = self.sender.lock() {
                            if let Some(sender) = guard.as_ref() {
                                if let Err(e) = sender.send(&socket_addr, &buffer) {
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
            }
            writer.increase_heartbeat_count();
        }

        Ok(())
    }

    fn update_liveliness(
        participant: Arc<Participant>,
        guid: Guid,
        local_writers: Arc<DashMap<Guid, WriterInfo>>,
        remote_participants: Arc<DashMap<GuidPrefix, HashMap<Guid, WriterInfo>>>,
    ) -> bool {
        log::info!("[WLP] update_liveliness called: guid={:?}", guid);

        // Local
        if participant.find_writer_from_entity_id(guid.entity_id()).is_some() {
            Self::update_local_liveliness(participant, guid, local_writers, false);
            return false;
        }

        // Remote
        log::info!(
            "[WLP] update_liveliness: No local writer found for guid={:?}, treating as REMOTE",
            guid
        );
        Self::update_remote_liveliness(participant, guid, remote_participants, false);
        true
    }

    fn update_local_liveliness(
        participant: Arc<Participant>,
        guid: Guid,
        local_writers: Arc<DashMap<Guid, WriterInfo>>,
        is_add: bool,
    ) {
        if let Some(writer) = participant.find_writer_from_entity_id(guid.entity_id()) {
            if !is_add {
                log::info!("[WLP] update_liveliness: Found LOCAL writer for guid={:?}", guid);
                writer.update_status(
                    StatusKind::LIVELINESS_LOST,
                    Some(Arc::new(LivelinessLostStatus { total_count: 0, total_count_change: 1 })),
                );
                log::warn!(
                    "[WLP] update_liveliness: Returning early for LOCAL writer guid={:?} - readers will NOT be notified!",
                    guid
                );
            }
        }

        if let Ok(readers) = participant.find_readers_matched_with_local_writer(&guid) {
            log::info!(
                "[WLP] update_local_liveliness: Found {} readers matched with writer {:?}",
                readers.len(),
                guid
            );
            for reader in readers {
                if is_add {
                    reader.update_status(
                        StatusKind::LIVELINESS_CHANGED,
                        Some(Arc::new(LivelinessChangedStatus {
                            alive_count: 0,
                            not_alive_count: 0,
                            alive_count_change: 1,
                            not_alive_count_change: 0,
                            last_publication_handle: InstanceHandle::from_guid(&guid),
                        })),
                    );
                } else {
                    reader.update_status(
                        StatusKind::LIVELINESS_CHANGED,
                        Some(Arc::new(LivelinessChangedStatus {
                            alive_count: 0,
                            not_alive_count: 0,
                            alive_count_change: -1,
                            not_alive_count_change: 1,
                            last_publication_handle: InstanceHandle::from_guid(&guid),
                        })),
                    );
                }
            }

            if !is_add {
                if let Some(mut writer_info) = local_writers.get_mut(&guid) {
                    writer_info.set_not_alive();
                    debug!(
                        "[WLP] Writer {:?} set to NOT_ALIVE (participant kept for recovery)",
                        guid
                    );
                }
            }
        }
    }

    fn update_remote_liveliness(
        participant: Arc<Participant>,
        guid: Guid,
        remote_participants: Arc<DashMap<GuidPrefix, HashMap<Guid, WriterInfo>>>,
        is_add: bool,
    ) {
        log::debug!("[WLP] update_remote_liveliness: guid={:?}, is_add={}", guid, is_add);

        if let Ok(readers) = participant.find_readers_matched_with_remote_writer(guid) {
            log::info!(
                "[WLP] update_remote_liveliness: Found {} readers matched with writer {:?}",
                readers.len(),
                guid
            );
            for reader in readers {
                if is_add {
                    reader.update_status(
                        StatusKind::LIVELINESS_CHANGED,
                        Some(Arc::new(LivelinessChangedStatus {
                            alive_count: 0,
                            not_alive_count: 0,
                            alive_count_change: 1,
                            not_alive_count_change: 0,
                            last_publication_handle: InstanceHandle::from_guid(&guid),
                        })),
                    );
                } else {
                    reader.update_status(
                        StatusKind::LIVELINESS_CHANGED,
                        Some(Arc::new(LivelinessChangedStatus {
                            alive_count: 0,
                            not_alive_count: 0,
                            alive_count_change: -1,
                            not_alive_count_change: 1,
                            last_publication_handle: InstanceHandle::from_guid(&guid),
                        })),
                    );
                }
            }

            if !is_add {
                let participant_prefix = guid.prefix();

                if let Some(mut remote_writers) = remote_participants.get_mut(&participant_prefix) {
                    if let Some(writer_info) = remote_writers.get_mut(&guid) {
                        writer_info.set_not_alive();
                        debug!(
                            "[WLP] Writer {:?} set to NOT_ALIVE (participant kept for recovery)",
                            guid
                        );
                    }
                }
            }
        }
    }

    pub(crate) fn update_local_writer_liveliness(&self, writer_guid: &Guid) {
        if let Some(mut info) = self.local_writers.get_mut(&writer_guid) {
            let was_not_alive = info.alive_state() == WriterAliveState::NotAlive;
            info.set_alive();

            // NOT_ALIVE -> ALIVE
            if was_not_alive {
                if let Ok(readers) =
                    self.participant.find_readers_matched_with_local_writer(writer_guid)
                {
                    for reader in readers {
                        reader.update_status(
                            StatusKind::LIVELINESS_CHANGED,
                            Some(Arc::new(LivelinessChangedStatus {
                                alive_count: 0,
                                not_alive_count: 0,
                                alive_count_change: 1,
                                not_alive_count_change: -1,
                                last_publication_handle: InstanceHandle::from_guid(&writer_guid),
                            })),
                        );
                    }
                }
            }

            // LivelinessMonitor Timer Update (re-track if removed after LOST)
            if let Ok(monitor) = self.liveliness_monitor.lock() {
                if let Some(monitor) = monitor.as_ref() {
                    monitor.update_writer(&writer_guid);
                }
            }
        }
    }

    // Participant level liveliness renewal (all MANUAL_BY_PARTICIPANT writers)
    pub(crate) fn update_local_participant_liveliness(&self) {
        let guids: Vec<Guid> = self
            .local_writers
            .iter()
            .filter(|entry| entry.value().qos.kind == LivelinessQosPolicyKind::ManualByParticipant)
            .map(|entry| *entry.key())
            .collect();

        for guid in guids {
            self.update_local_writer_liveliness(&guid);
        }
    }

    pub(crate) fn update_remote_participant_liveliness(
        &self,
        participant_message_data: ParticipantMessageData,
    ) {
        let message_kind = participant_message_data.kind();

        if let Ok(monitor) = self.liveliness_monitor.lock() {
            if let Some(monitor) = monitor.as_ref() {
                if let Some(mut remote_writers) = self
                    .remote_participants
                    .get_mut(&participant_message_data.participant_guid_prefix())
                {
                    for (guid, info) in remote_writers.iter_mut() {
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
                            let was_not_alive = info.alive_state() == WriterAliveState::NotAlive;
                            let lease_duration = info.qos().lease_duration;

                            info.set_alive();

                            if was_not_alive {
                                if let Ok(readers) =
                                    self.participant.find_readers_matched_with_remote_writer(*guid)
                                {
                                    for reader in readers {
                                        reader.update_status(
                                            StatusKind::LIVELINESS_CHANGED,
                                            Some(Arc::new(LivelinessChangedStatus {
                                                alive_count: 0,
                                                not_alive_count: 0,
                                                alive_count_change: 1,
                                                not_alive_count_change: -1,
                                                last_publication_handle: InstanceHandle::from_guid(
                                                    guid,
                                                ),
                                            })),
                                        );
                                    }
                                }
                            }

                            // Re-track writer (restores is_alive = true in monitor)
                            monitor.track_writer(guid, lease_duration);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn update_remote_writer_liveliness(&self, writer_guid: Guid) {
        if let Some(remote_writers) = self.remote_participants.get(&writer_guid.prefix()) {
            if let Some(info) = remote_writers.get(&writer_guid) {
                let qos_kind = info.qos().kind;
                drop(remote_writers);

                match qos_kind {
                    LivelinessQosPolicyKind::Automatic => {}
                    LivelinessQosPolicyKind::ManualByParticipant => {
                        self.update_remote_participant_liveliness(ParticipantMessageData::new(
                            writer_guid.prefix(),
                            qos_kind,
                        ));
                    }
                    LivelinessQosPolicyKind::ManualByTopic => {
                        if let Some(mut remote_writers) =
                            self.remote_participants.get_mut(&writer_guid.prefix())
                        {
                            if let Some(info) = remote_writers.get_mut(&writer_guid) {
                                let was_not_alive =
                                    info.alive_state() == WriterAliveState::NotAlive;
                                info.set_alive();

                                let lease_duration = info.qos().lease_duration;

                                drop(remote_writers);

                                // NOT_ALIVE -> ALIVE
                                if was_not_alive {
                                    if let Ok(readers) = self
                                        .participant
                                        .find_readers_matched_with_remote_writer(writer_guid)
                                    {
                                        for reader in readers {
                                            reader.update_status(
                                                StatusKind::LIVELINESS_CHANGED,
                                                Some(Arc::new(LivelinessChangedStatus {
                                                    alive_count: 0,
                                                    not_alive_count: 0,
                                                    alive_count_change: 1,
                                                    not_alive_count_change: -1,
                                                    last_publication_handle:
                                                        InstanceHandle::from_guid(&writer_guid),
                                                })),
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
                    }
                }
            }
        }
    }

    fn timer_sleep_and_send_message(
        &self,
        start_time: Option<Instant>,
        logic_start_time: Instant,
        duration: StdDuration,
        message: MessageType,
    ) {
        let elapsed = logic_start_time.elapsed();
        let remaining_duration = match start_time {
            Some(start) => {
                let time_until_start = start.saturating_duration_since(logic_start_time);
                time_until_start + duration
            }
            None => duration.checked_sub(elapsed).unwrap_or(StdDuration::ZERO),
        };

        let timer_id = format!(
            "wlp_p2p_{:?}_{}",
            self.participant.guid().prefix(),
            logic_start_time.elapsed().as_nanos(),
        );

        let participant = self.participant.clone();
        let message = Arc::new(message);
        if let Ok(handler) = self.timer_handler.lock() {
            handler.add_timer(
                timer_id,
                remaining_duration,
                false, // one-shot
                {
                    let participant = participant.clone();
                    let message = message.clone();
                    move || {
                        let sending_handler =
                            SendingHandler::get_instance(participant.clone(), None, None);
                        sending_handler.push_message_and_wake((*message).clone());
                    }
                },
            );
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
