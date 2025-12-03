//! SEDP (Simple Endpoint Discovery Protocol) logic implementation.
//!
//! This module implements the SEDP protocol for discovering DataReaders and DataWriters.
//! SEDP announces local endpoints and processes endpoint announcements from remote
//! participants to enable reader-writer matching.

use std::{
    net::{SocketAddr, SocketAddrV4},
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration as StdDuration, Instant},
};

use log::{debug, error, warn};
use rand::random_range;
use speedy::{Endianness, Writable};

use crate::{
    common::{
        builtin::topic::{
            publication_builtin_topic_data::PublicationBuiltinTopicData,
            subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
        },
        instance_handle::InstanceHandle,
    },
    infrastructure::qos_policy::{DurabilityQosPolicyKind, QosPolicyId, ReliabilityQosPolicyKind},
    rtps::{
        builtin::{
            builtin_endpoints::BuiltinEndpoints,
            data::{
                content_filtered_topic::ContentFilterProperty,
                discovered_data::{DiscoveredReaderData, DiscoveredWriterData},
                spdp_discovered_participant_data::SPDPDiscoveredParticipantData,
            },
        },
        common::{
            entity_id::EntityId,
            guid::{Guid, GuidPrefix},
            locator::{
                LOCATOR_KIND_TCP_V4, LOCATOR_KIND_TCP_V6, LOCATOR_KIND_UDP_V4, LOCATOR_KIND_UDP_V6,
            },
            parameters::ParameterList,
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
        },
        entities::{
            entity::Entity,
            history::{cache_change::CacheChange, history_cache::HistoryCache},
            participant::Participant,
            qos::{check_qos_compatibility, check_qos_compatibility_with_policy_id},
            reader::{Reader, StatefulReader, StatelessReader, WriterLocator, WriterProxy},
            writer::{
                reader_locator::ReaderLocator, reader_proxy::ReaderProxy, StatefulWriter,
                StatelessWriter, Writer,
            },
        },
        logic::data::builtin_endpoint_pair::BuiltinEndpointPair,
        messages::{
            message_creator::MessageCreator,
            message_receiver::{MessageReceiver, TypedSubmessage},
            sedp_message::SEDPMessage,
            submessages::{ack_nack::AckNack, gap::Gap, heartbeat::Heartbeat},
        },
        task::{
            discovery_traffic::{
                discovery_multicast_listening_task::DiscoveryMulticastListeningTask,
                discovery_unicast_listening_task::DiscoveryUnicastListeningTask,
            },
            sending_handler::{MessageType, SendingHandler},
            timer_handler::TimerHandler,
        },
        transport::{
            tcp::tcp_listener::TcpListener, udp::udp_listener::UdpListener, Transport,
            TransportSender,
        },
    },
    serialize::pl_cdr::InlineQosParameters,
};

enum MatchType {
    ReaderPublication,
    WriterSubscription,
}

enum BuiltinTopicData {
    Publication(PublicationBuiltinTopicData),
    Subscription(SubscriptionBuiltinTopicData),
}
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct SedpLogic {
    participant: Arc<Participant>,
    builtin_endpoints: Arc<BuiltinEndpoints>,
    spdp_message: Option<Arc<Vec<u8>>>,
    sender: Option<Arc<TransportSender>>,
    multicast_listening_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    unicast_listening_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    timer_handler: Arc<Mutex<TimerHandler>>,
}

fn validate_endpoint_compatibility<L>(
    local: &L,
    requested: &SubscriptionBuiltinTopicData,
    offered: &PublicationBuiltinTopicData,
    update_incompatible_qos: impl Fn(&L, QosPolicyId),
    who: &'static str, // for log
) -> RtpsResult<()> {
    // QoS
    if !check_qos_compatibility(requested, offered) {
        if let Some(pid) = check_qos_compatibility_with_policy_id(requested, offered) {
            update_incompatible_qos(local, pid);
        }
        let err = RtpsError::new(RtpsErrorCode::QosIncompatible, format!("[QoS failed :{}]", who));

        return Err(err);
    }

    // Partition
    if !is_partition_compatible(&requested.partition().name, &offered.partition().name) {
        let err = RtpsError::new(
            RtpsErrorCode::PartitionIncompatible,
            format!("[Partition failed :{}]", who),
        );
        return Err(err);
    }

    Ok(())
}

fn is_partition_compatible(requested: &[String], offered: &[String]) -> bool {
    if requested.is_empty() && offered.is_empty() {
        return true;
    }
    let requested = if requested.is_empty() { vec!["".into()] } else { requested.to_vec() };
    let offered = if offered.is_empty() { vec!["".into()] } else { offered.to_vec() };

    for r in &requested {
        for o in &offered {
            if r == o || r == "*" || o == "*" {
                return true;
            }
            if let Some(prefix) = r.strip_suffix('*') {
                if o.starts_with(prefix) {
                    return true;
                }
            }
            if let Some(suffix) = r.strip_prefix('*') {
                if o.ends_with(suffix) {
                    return true;
                }
            }
        }
    }
    false
}

impl SedpLogic {
    pub(crate) fn new(participant: Arc<Participant>, sender: Option<Arc<TransportSender>>) -> Self {
        let builtin_endpoints = participant.builtin_endpoints();
        let timer_handler = TimerHandler::get_instance(participant.clone());
        Self {
            participant,
            builtin_endpoints,
            spdp_message: None,
            sender,
            multicast_listening_handle: Arc::new(Mutex::new(None)),
            unicast_listening_handle: Arc::new(Mutex::new(None)),
            timer_handler,
        }
    }

    pub(crate) fn start_sedp(
        &self,
        discovery_multicast_listener: Option<UdpListener>,
        discovery_unicast_listener: Option<UdpListener>,
        discovery_tcp_listener: Option<TcpListener>,
    ) {
        let mut discovery_multicast_listening_task = DiscoveryMulticastListeningTask::new(
            discovery_multicast_listener,
            self.participant.clone(),
        );

        // multicast listening
        let multicast_handle = thread::Builder::new()
            .name("discovery_traffic_multicast_listening".to_string())
            .spawn(move || {
                // Register thread name for monitoring
                {
                    use crate::rtps::task::thread_monitor::ThreadMonitor;
                    ThreadMonitor::register_current_thread_name(
                        "discovery_traffic_multicast_listening",
                    );
                }

                let _ = discovery_multicast_listening_task.multicast_listening();
                debug!("discovery multicast listening thread finished");
            })
            .expect("Failed to create discovery multicast listening thread");

        // Store multicast handle
        if let Ok(mut handle_guard) = self.multicast_listening_handle.lock() {
            *handle_guard = Some(multicast_handle);
        }

        let mut discovery_unicast_listening_task = DiscoveryUnicastListeningTask::new(
            discovery_unicast_listener,
            discovery_tcp_listener,
            self.participant.clone(),
        );

        // unicast listening
        let unicast_handle = thread::Builder::new()
            .name("discovery_traffic_unicast_listening".to_string())
            .spawn(move || {
                // Register thread name for monitoring
                {
                    use crate::rtps::task::thread_monitor::ThreadMonitor;
                    ThreadMonitor::register_current_thread_name(
                        "discovery_traffic_unicast_listening",
                    );
                }

                let _ = discovery_unicast_listening_task.unicast_listening();
                debug!("discovery unicast listening thread finished");
            })
            .expect("Failed to create discovery unicast listening thread");

        // Store unicast handle
        if let Ok(mut handle_guard) = self.unicast_listening_handle.lock() {
            *handle_guard = Some(unicast_handle);
        }
    }

    pub(crate) fn join_multicast_listening_thread(&self) -> RtpsResult<()> {
        if let Ok(mut handle_guard) = self.multicast_listening_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                handle.join().map_err(|_| RtpsError::new(RtpsErrorCode::ThreadJoinError, None))?;
            }
        }
        Ok(())
    }

    pub(crate) fn join_unicast_listening_thread(&self) -> RtpsResult<()> {
        if let Ok(mut handle_guard) = self.unicast_listening_handle.lock() {
            if let Some(handle) = handle_guard.take() {
                handle.join().map_err(|_| RtpsError::new(RtpsErrorCode::ThreadJoinError, None))?;
            }
        }
        Ok(())
    }

    pub(crate) fn join_all_listening_threads(&self) -> RtpsResult<()> {
        self.join_multicast_listening_thread()?;
        self.join_unicast_listening_thread()?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn get_multicast_listening_handle(&self) -> Arc<Mutex<Option<JoinHandle<()>>>> {
        Arc::clone(&self.multicast_listening_handle)
    }

    #[cfg(test)]
    pub(crate) fn get_unicast_listening_handle(&self) -> Arc<Mutex<Option<JoinHandle<()>>>> {
        Arc::clone(&self.unicast_listening_handle)
    }

    pub(crate) fn send_sedp_message(
        &self,
        duration: StdDuration,
        spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
        data: Option<Arc<Vec<u8>>>,
    ) {
        let handler = SendingHandler::get_instance(self.participant.clone(), None, None);
        handler.push_message(MessageType::SedpSpdp(
            None,
            duration,
            spdp_discovered_participant_data.clone(),
            data,
        ));
        handler.push_message(MessageType::SedpPublication(
            None,
            duration,
            Arc::new(spdp_discovered_participant_data.guid_prefix()),
        ));
        handler.push_message(MessageType::SedpSubscription(
            None,
            duration,
            Arc::new(spdp_discovered_participant_data.guid_prefix()),
        ));
        handler.wake_event_loop();
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
            Some(start_time) => {
                let elapsed = start_time.elapsed();
                duration.checked_sub(elapsed).unwrap_or(StdDuration::from_millis(0))
            }
            None => duration.checked_sub(elapsed).unwrap_or(StdDuration::from_millis(0)),
        };

        // Generate unique timer ID using participant GUID, timestamp and random number
        let timer_id = format!(
            "sedp_send_{:?}_{}_{:?}",
            self.participant.guid().prefix(),
            logic_start_time.elapsed().as_nanos(),
            random_range(0..10000)
        );

        let participant = Arc::new(self.participant.clone());
        let message = Arc::new(message);
        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                remaining_duration,
                false, // one-shot timer
                {
                    let participant = participant.clone();
                    let message = message.clone();
                    move || {
                        let sending_handler =
                            SendingHandler::get_instance((*participant).clone(), None, None);
                        sending_handler.push_message_and_wake((*message).clone());
                    }
                },
            );
        } else {
            error!("Failed to acquire timer handler lock for SEDP message sending");
        }
    }

    // SEDP SPDP message repeated transmission
    #[allow(unused_variables)]
    pub(crate) fn send_sedp_spdp_message(
        &self,
        start_time: Option<Instant>,
        duration: StdDuration,
        spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
        data: Option<Arc<Vec<u8>>>,
    ) -> RtpsResult<()> {
        let logic_start_time = Instant::now();
        match data {
            Some(data) => {
                let mut list: Vec<SPDPDiscoveredParticipantData> = Vec::new();
                match self.participant.remote_participant_proxy_datas().lock() {
                    Ok(remote_participant_datas) => {
                        // Send SPDP message to everyone
                        for remote_participant_data in remote_participant_datas.iter() {
                            if spdp_discovered_participant_data.guid_prefix()
                                != remote_participant_data.guid_prefix()
                            {
                                continue;
                            }
                            list.push(remote_participant_data.clone());
                        }
                    }
                    Err(e) => {
                        return Err(RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock remote_participant_datas: {:?}", e),
                        ));
                    }
                }
                for remote_participant_data in list.iter() {
                    if let Err(e) = self.send_discovery_message(
                        &data,
                        remote_participant_data.participant_guid(),
                        "spdp",
                    ) {
                        warn!("Failed to send SPDP discovery message: {:?}", e);
                    }
                    let start_time = Instant::now();
                    self.timer_sleep_and_send_message(
                        Some(start_time),
                        logic_start_time,
                        duration,
                        MessageType::SedpSpdp(
                            Some(start_time),
                            duration,
                            spdp_discovered_participant_data.clone(),
                            Some(data.clone()),
                        ),
                    );
                }
            }
            None => return Err(RtpsError::new(RtpsErrorCode::DataNotSet, "Data is not set")),
        };
        Ok(())
    }

    // SEDP HEARTBEAT message connection and unsent / reader proxy check unsent
    #[allow(unused_variables)]
    pub(crate) fn send_sedp_heartbeat_message(
        &self,
        start_time: Option<Instant>,
        duration: StdDuration,
        // spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
        guid_prefix: Arc<GuidPrefix>,
        entity_id: EntityId,
    ) -> RtpsResult<bool> {
        let logic_start_time = Instant::now();
        // Get reader and writer corresponding to entity
        let builtin_endpoint_pair =
            BuiltinEndpointPair::reader_writer_from_entity_id(entity_id, self.participant.clone())?;
        let (reader, writer) = match builtin_endpoint_pair {
            Some(builtin_endpoint_pair) => {
                (builtin_endpoint_pair.reader(), builtin_endpoint_pair.writer())
            }
            None => {
                return Err(RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    "Failed to get reader and writer proxy",
                ))
            }
        };

        let reader_entity_id = reader.guid().entity_id();
        let writer_entity_id = writer.guid().entity_id();

        let (first_sn, last_sn, heartbeat_count) = {
            match writer.writer_cache().lock() {
                Ok(writer_cache) => (
                    writer_cache.get_seq_num_min(),
                    writer_cache.get_seq_num_max(),
                    writer.heartbeat_count(),
                ),
                Err(e) => {
                    return Err(RtpsError::new(
                        RtpsErrorCode::WriterCacheNotSet,
                        format!("Failed to get writer cache: {}", e),
                    ));
                }
            }
        };

        let mut is_sent = false;
        let mut retry: bool = false;

        let participant_guid = {
            let local_participant_data = self.participant.local_participant_proxy_data();
            local_participant_data.participant_guid()
        };

        match writer.reader_proxies().lock() {
            Ok(reader_proxies) => {
                for reader_proxy in reader_proxies.iter() {
                    if reader_proxy.remote_reader_guid().prefix() != *guid_prefix {
                        continue;
                    }
                    if !reader_proxy.is_active() {
                        continue;
                    }
                    let buffer = MessageCreator::create_heartbeat_message(
                        participant_guid,
                        Guid::new(reader_proxy.remote_reader_guid().prefix(), EntityId::UNKNOWN),
                        heartbeat_count,
                        reader_entity_id,
                        writer_entity_id,
                        first_sn,
                        last_sn,
                        false,
                        false,
                    );
                    match buffer {
                        Ok(buffer) => {
                            for locator in reader_proxy.unicast_locator_list() {
                                if locator.kind() == 1 {
                                    //UDPv4
                                    let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                                        locator.to_ip_v4_addr(),
                                        locator.port() as u16,
                                    ));
                                    if let Some(ref sender) = self.sender {
                                        if let Err(e) = sender.send(&socket_addr, &buffer) {
                                            warn!("Failed to send SEDP heartbeat: {:?}", e);
                                        } else {
                                            is_sent = true;
                                        }
                                    } else {
                                        debug!("UDP sender not available, skipping SEDP heartbeat");
                                    }
                                };
                            }
                            writer.increase_heartbeat_count();
                            if writer.last_change_sequence_number() != SequenceNumber::ZERO {
                                retry = true;
                            }
                        }
                        Err(e) => {
                            warn!("Failed to create SEDP heartbeat message: {:?}", e);
                            // Continue to next reader proxy instead of failing entirely
                        }
                    }
                }
            }
            Err(e) => {
                return Err(RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to get reader proxies: {}", e),
                ));
            }
        }

        if retry {
            let start_time = Instant::now();
            if entity_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER {
                self.timer_sleep_and_send_message(
                    Some(start_time),
                    logic_start_time,
                    duration,
                    MessageType::SedpPublication(Some(start_time), duration, guid_prefix.clone()),
                );
            } else if entity_id == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER {
                self.timer_sleep_and_send_message(
                    Some(start_time),
                    logic_start_time,
                    duration,
                    MessageType::SedpSubscription(Some(start_time), duration, guid_prefix.clone()),
                );
            } else if entity_id == EntityId::SEDP_BUILTIN_TOPICS_WRITER {
                self.timer_sleep_and_send_message(
                    Some(start_time),
                    logic_start_time,
                    duration,
                    MessageType::SedpTopic(Some(start_time), duration, guid_prefix.clone()),
                );
            } else {
                return Err(RtpsError::new(
                    RtpsErrorCode::InvalidEntityKind,
                    format!("Invalid entity id: {:?}", entity_id),
                ));
            }
        }
        Ok(is_sent)
    }

    pub(crate) fn send_sedp_data_message(
        &self,
        cache_change: Arc<CacheChange>,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
    ) -> RtpsResult<()> {
        let buffer = MessageCreator::create_data_msg(
            cache_change,
            remote_guid,
            reader_entity_id,
            writer_entity_id,
            None, // No heartbeat
            true, // Use inline QoS (default)
            None, // No content filter for SEDP messages
        );

        match buffer {
            Ok(buffer) => self.send_discovery_message(&buffer, remote_guid, "data")?,
            Err(e) => {
                return Err(RtpsError::new(
                    RtpsErrorCode::SerializationError,
                    format!("Failed to create SEDP DATA message: {}", e),
                ));
            }
        };

        Ok(())
    }

    fn send_discovery_message(
        &self,
        buffer: &[u8],
        remote_guid: Guid,
        message_type: &str,
    ) -> RtpsResult<bool> {
        // Get remote participant's locator information and send
        let mut is_sent = false;

        match self.participant.remote_participant_proxy_datas().clone().lock() {
            Ok(remote_participant_datas) => {
                for remote_participant_data in remote_participant_datas.iter() {
                    if remote_participant_data.participant_guid().prefix() == remote_guid.prefix() {
                        for locator in remote_participant_data.metatraffic_unicast_locator_list() {
                            // Handle both UDP and TCP locators
                            match locator.kind() {
                                LOCATOR_KIND_UDP_V4 | LOCATOR_KIND_TCP_V4 => {
                                    // Both UDP and TCP use IPv4 addressing
                                    let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                                        locator.to_ip_v4_addr(),
                                        locator.port() as u16,
                                    ));
                                    if let Some(ref sender) = self.sender {
                                        let _ = sender.send(&socket_addr, buffer);
                                        debug!(
                                            "[{}] SEDP Logic: {} message sent to {} (transport: {})",
                                            message_type,
                                            message_type,
                                            socket_addr,
                                            if locator.kind() == LOCATOR_KIND_TCP_V4 {
                                                "TCP"
                                            } else {
                                                "UDP"
                                            }
                                        );
                                        is_sent = true;
                                    } else {
                                        debug!("UDP sender not available, skipping SEDP message");
                                    }
                                }
                                LOCATOR_KIND_UDP_V6 | LOCATOR_KIND_TCP_V6 => {
                                    // IPv6 support can be added here in the future
                                    warn!(
                                        "[{}] SEDP Logic: IPv6 locator not yet supported (kind: {})",
                                        message_type, locator.kind()
                                    );
                                }
                                _ => {
                                    warn!(
                                        "[{}] SEDP Logic: Unsupported locator kind: {}",
                                        message_type,
                                        locator.kind()
                                    );
                                }
                            }
                        }
                        break; // Found the participant, exit loop
                    }
                }
            }
            Err(e) => {
                return Err(RtpsError::new(
                    RtpsErrorCode::LockError,
                    format!("Failed to lock remote_participant_datas: {:?}", e),
                ));
            }
        }

        if !is_sent {
            return Err(RtpsError::new(
                RtpsErrorCode::RtpsEntityNotFound,
                format!(
                    "[{}] SEDP Logic: Failed to find remote participant for GUID: {:?}",
                    message_type, remote_guid
                ),
            ));
        }
        Ok(is_sent)
    }

    pub(crate) fn handle_rtps_message(
        &mut self,
        message_receiver: MessageReceiver,
    ) -> RtpsResult<()> {
        // If INFO_DST exists, local guid must match
        if message_receiver.has_dst_submessage() {
            let local_guid_prefix = self.participant.guid().prefix();
            if !message_receiver.is_dst_me(local_guid_prefix) {
                return Err(RtpsError::new(
                    RtpsErrorCode::InvalidDestinationGuid,
                    "SEDP Logic: INFO_DST is not me",
                ));
            }
        }

        let rtps_header = *message_receiver.rtps_message_header().unwrap();

        let submessages = message_receiver.parse_submessages();
        for submessage in submessages {
            match submessage {
                TypedSubmessage::Heartbeat(header, heartbeat) => {
                    // Wlp Logic
                    if heartbeat.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
                        debug!("[heartbeat] P2P heartbeat, skipping in sedp_logic");
                        continue;
                    }
                    debug!(
                        "[heartbeat] entity id - reader: {:?}, writer: {:?}",
                        heartbeat.reader_id, heartbeat.writer_id
                    );
                    self.handle_heartbeat_message(
                        heartbeat.clone(),
                        Guid::new(rtps_header.guid_prefix(), heartbeat.writer_id),
                        header.final_flag().unwrap_or(false),
                    )?;
                }

                TypedSubmessage::AckNack(_, acknack) => {
                    // Wlp Logic
                    if acknack.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
                        debug!("[AckNack] P2P acknack, skipping in sedp_logic");
                        continue;
                    }
                    debug!(
                        "[acknack] entity id - reader: {:?}, writer: {:?}",
                        acknack.reader_id, acknack.writer_id
                    );
                    self.handle_acknack_message(
                        acknack.clone(),
                        Guid::new(rtps_header.guid_prefix(), acknack.writer_id),
                    )?;
                }

                TypedSubmessage::Data(header, data) => {
                    // Wlp Logic
                    if data.writer_id == EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_WRITER {
                        debug!("[SedpLogic] [Data] P2P data, skipping in sedp_logic");
                        continue;
                    }

                    debug!(
                        "[data] entity id - reader: {:?}, writer: {:?}",
                        data.reader_id, data.writer_id
                    );

                    let is_big_endian = header
                        .endianness_flag()
                        .is_some_and(|endianness| endianness == Endianness::BigEndian);

                    let inline_qos_params = data.inline_qos();

                    if (data.reader_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_READER
                        || data.reader_id == EntityId::UNKNOWN // opendds uses publication_reader and unknown for reader_id
                            && data.writer_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER)
                        || (data.reader_id == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER
                            && data.writer_id == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER)
                    {
                        let builtin_endpoint_pair =
                            BuiltinEndpointPair::reader_writer_from_entity_id(
                                data.writer_id,
                                self.participant.clone(),
                            )?;

                        // Check if the builtin reader is matched with the remote writer GUID
                        // Marked as received
                        match builtin_endpoint_pair {
                            Some(builtin_endpoint_pair) => {
                                let reader = builtin_endpoint_pair.reader();

                                let writer_guid =
                                    Guid::new(rtps_header.guid_prefix(), data.writer_id);

                                let seq_num = data.writer_sn;

                                // Issue with matched_writer_lookup using clone to find reader_proxies, which prevents preserving the original
                                match reader.writer_proxies().lock() {
                                    Ok(mut matched_writers) => {
                                        if let Some(writer_proxy) = matched_writers
                                            .iter_mut()
                                            .find(|proxy| proxy.remote_writer_guid() == writer_guid)
                                        {
                                            writer_proxy.mark_change_received(seq_num, None);
                                            writer_proxy.increment_expected_sn();
                                        } else {
                                            return Err(RtpsError::new(
                                                RtpsErrorCode::BuiltinEndpointNotFound,
                                                format!(
                                                    "[heartbeat] SPDP Message may have not been received, cause builtin reader not matched with remote guid: {:?}",
                                                    writer_guid
                                                ),
                                            ));
                                        }
                                    }
                                    Err(e) => {
                                        return Err(RtpsError::new(
                                            RtpsErrorCode::LockError,
                                            format!(
                                                "[data] Failed to acquire matched_writers lock: {}",
                                                e
                                            ),
                                        ));
                                    }
                                };
                            }
                            None => {
                                return Err(RtpsError::new(
                                    RtpsErrorCode::BuiltinEndpointNotFound,
                                    "[data] Failed to get builtin endpoint pair",
                                ));
                            }
                        }

                        let payload =
                            message_receiver.payload_from_data(data.reader_id, data.writer_id);

                        match payload {
                            Some(payload) => {
                                //TODO: DATA(r), DATA(w) must match (ex, qos etc)

                                // Process differently according to Reader ID
                                // Check if deserialized rtps reader/writer is matched and if not, ignore adding proxy or reader locator
                                if data.reader_id == EntityId::SEDP_BUILTIN_PUBLICATIONS_READER {
                                    // Remote is sending publication information
                                    match SEDPMessage::<DiscoveredWriterData>::from_serialized_payload(
                                        payload.as_ref(),
                                        is_big_endian,
                                    ) {
                                        Ok(writer_data) => {
                                            debug!(
                                                "SEDP Logic: DiscoveredWriterData: {:?}",
                                                writer_data
                                            );
                                            self.handle_publication_builtin_topic_data(
                                                writer_data.publication_builtin_topic_data,
                                                inline_qos_params,
                                            );
                                        }
                                        Err(e) => {
                                            return Err(RtpsError::new(RtpsErrorCode::DeserializationError, format!("Failed to parse DiscoveredWriterData: {}", e)));
                                        }
                                    }
                                } else if data.reader_id
                                    == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER
                                {
                                    // Remote is sending subscription information
                                    match SEDPMessage::<DiscoveredReaderData>::from_serialized_payload(
                                        payload.as_ref(),
                                        is_big_endian,
                                    ) {
                                        Ok(reader_data) => {
                                            debug!(
                                                "SEDP Logic: DiscoveredReaderData: {:?}",
                                                reader_data
                                            );
                                            self.handle_subscription_builtin_topic_data(
                                                reader_data.subscription_builtin_topic_data,
                                                inline_qos_params,
                                                reader_data.content_filter,
                                            );
                                        }
                                        Err(e) => {
                                            return Err(RtpsError::new(RtpsErrorCode::DeserializationError, format!("Failed to parse DiscoveredReaderData: {}", e)));
                                        }
                                    }
                                }
                            }
                            None => {
                                return Err(RtpsError::new(
                                    RtpsErrorCode::SerializationError,
                                    "SEDP Logic: No payload found: ",
                                ));
                            }
                        }
                    } else if data.reader_id == EntityId::SPDP_BUILTIN_PARTICIPANT_READER
                        || data.writer_id == EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER
                    {
                        let participant_proxy_data = message_receiver
                            .extract_participant_proxy_data(self.participant.domain_id());
                        match participant_proxy_data {
                            Some((participant_proxy_data, inline_qos_params)) => {
                                let mut is_termination_message = false;

                                if let Some(inline_qos_params) = inline_qos_params {
                                    if let Some(status_info) = inline_qos_params.get_status_info() {
                                        if status_info.disposed() && status_info.unregistered() {
                                            // Terminating Participant provides GUID via KeyHash, but sometimes sends DATA message without SerializedData payload
                                            let terminated_participant_guid = inline_qos_params
                                                .get_key_hash()
                                                .unwrap_or_else(|| {
                                                    InstanceHandle::from_guid(
                                                        &participant_proxy_data.participant_guid(),
                                                    )
                                                });

                                            self.participant.unmatch_with_remote_participant(
                                                &terminated_participant_guid.to_guid(),
                                            );

                                            is_termination_message = true;
                                        }
                                    }
                                }

                                if !is_termination_message {
                                    let handler = SendingHandler::get_instance(
                                        self.participant.clone(),
                                        None,
                                        None,
                                    );
                                    handler.push_message_and_wake(
                                        MessageType::OnSpdpMessageArrival(
                                            participant_proxy_data.clone(),
                                        ),
                                    );
                                }
                            }
                            None => {
                                return Err(RtpsError::new(
                                    RtpsErrorCode::DeserializationError,
                                    "Failed to parse RTPS message",
                                ));
                            }
                        }
                    }
                }

                TypedSubmessage::Gap(_, gap) => {
                    let _ = self.handle_gap_message(
                        gap,
                        Guid::new(rtps_header.guid_prefix(), gap.writer_id),
                    );
                }

                _ => {
                    return Err(RtpsError::new(
                        RtpsErrorCode::UnsupportedSubmessageType,
                        format!("SEDP Logic: Unsupported submessage type: {:?}", submessage),
                    ));
                }
            }
        }
        Ok(())
    }

    fn handle_acknack_message(&self, acknack: AckNack, remote_guid: Guid) -> RtpsResult<()> {
        let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
            acknack.writer_id,
            self.participant.clone(),
        )? {
            Some(proxy) => proxy,
            None => {
                return Err(RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    format!(
                    "[acknack] No matching SEDP builtin reader found for reader entity ID: {:?}",
                    acknack.reader_id
                ),
                ));
            }
        };

        let local_writer = builtin_endpoint_pair.writer();

        if !self.find_reader_proxy(&local_writer, remote_guid) {
            return Err(RtpsError::new(
                RtpsErrorCode::RtpsEntityNotFound,
                format!("[acknack] Failed to create ReaderProxy for GUID: {:?}", remote_guid),
            ));
        }

        self.handle_acknack_message_send_data(local_writer, remote_guid, acknack)
    }

    fn handle_acknack_message_send_data(
        &self,
        local_writer: Arc<StatefulWriter>,
        remote_guid: Guid,
        acknack: AckNack,
    ) -> RtpsResult<()> {
        let missing_sequence_numbers = acknack.reader_sn_state.extract_numbers();

        debug!(
            "Missing sequence numbers {:?} from remote: {:?}",
            missing_sequence_numbers, remote_guid
        );
        if missing_sequence_numbers.is_empty() {
            return Ok(());
        }
        let mut missing_changes = Vec::new();
        let writer_cache = local_writer.writer_cache();
        for seq_num in missing_sequence_numbers {
            let all_changes = writer_cache.lock().unwrap().get_changes();
            if let Some(change) =
                all_changes.iter().find(|c| c.sequence_number() == seq_num).cloned()
            {
                missing_changes.push(change);
            }
        }

        if !missing_changes.is_empty() {
            debug!(
                "Retransmitting {} missing changes from remote: {:?}",
                missing_changes.len(),
                remote_guid
            );

            for change in missing_changes {
                self.send_sedp_data_message(
                    change,
                    remote_guid,
                    acknack.reader_id,
                    acknack.writer_id,
                )?;
            }
        }
        Ok(())
    }

    fn handle_heartbeat_message(
        &self,
        heartbeat: Heartbeat,
        remote_guid: Guid,
        final_flag: bool,
    ) -> RtpsResult<()> {
        let builtin_endpoint_pair = match BuiltinEndpointPair::reader_writer_from_entity_id(
            heartbeat.writer_id,
            self.participant.clone(),
        )? {
            Some(proxy) => proxy,
            None => {
                return Err(RtpsError::new(
                    RtpsErrorCode::BuiltinEndpointNotFound,
                    format!(
                    "[heartbeat] No matching SEDP builtin reader found for writer entity ID: {:?}",
                    heartbeat.writer_id
                ),
                ));
            }
        };

        let local_reader = builtin_endpoint_pair.reader();

        if !local_reader.matched_writer_is_matched(remote_guid) {
            debug!("[heartbeat] SPDP Message may have not been received, cause builtin reader not matched with remote guid: {:?}", remote_guid);
            return Ok(());
        }

        self.handle_heartbeat_and_send_acknack(local_reader, remote_guid, heartbeat, final_flag)
    }

    /// Process Heartbeat and send ACKNACK
    fn handle_heartbeat_and_send_acknack(
        &self,
        local_reader: Arc<StatefulReader>,
        remote_guid: Guid,
        heartbeat: Heartbeat,
        final_flag: bool,
    ) -> RtpsResult<()> {
        // Issue with matched_writer_lookup using clone to find reader_proxies, which prevents preserving the original
        // Use matched_writers mutex directly to find and update WriterProxy
        if let Ok(mut matched_writers) = local_reader.writer_proxies().lock() {
            if let Some(writer_proxy) =
                matched_writers.iter_mut().find(|proxy| proxy.remote_writer_guid() == remote_guid)
            {
                let missing_changes =
                    writer_proxy.process_heartbeat(heartbeat.first_sn, heartbeat.last_sn);
                let bitmap_base = writer_proxy.expected_sn();

                // Check Heartbeat's final flag
                let requires_response = !final_flag;

                // Send AckNack if there are missing changes or if response is required
                if !missing_changes.is_empty() || requires_response {
                    debug!(
                        "Sending AckNack - missing changes: {:?}, requires response: {}",
                        missing_changes, requires_response
                    );

                    writer_proxy.increase_acknack_count();
                    let acknack_count = writer_proxy.acknack_count();
                    self.send_sedp_acknack_message(
                        remote_guid,
                        heartbeat.reader_id,
                        heartbeat.writer_id,
                        missing_changes,
                        acknack_count,
                        bitmap_base,
                    )
                } else {
                    Ok(())
                }
            } else {
                Err(RtpsError::new(
                    RtpsErrorCode::RtpsEntityNotFound,
                    format!("[heartbeat] Failed to find writer proxy for GUID: {:?}", remote_guid),
                ))
            }
        } else {
            Err(RtpsError::new(
                RtpsErrorCode::LockError,
                "[heartbeat] Failed to acquire matched_writers lock",
            ))
        }
    }

    fn send_sedp_acknack_message(
        &self,
        remote_guid: Guid,
        reader_entity_id: EntityId,
        writer_entity_id: EntityId,
        missing_changes: Vec<SequenceNumber>,
        acknack_count: i32,
        bitmap_base: SequenceNumber,
    ) -> RtpsResult<()> {
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
                if let Err(e) = self.send_discovery_message(&buffer, remote_guid, "AckNack") {
                    warn!("Failed to send SEDP AckNack message: {:?}", e);
                }
            }
            Err(e) => {
                warn!("Failed to create SEDP AckNack message: {:?}", e);
            }
        }

        Ok(())
    }

    // /// Method to find or create WriterProxy in SEDP Builtin Reader
    // fn find_writer_proxy(
    //     &self,
    //     local_reader: &StatefulReader,
    //     writer_guid: Guid,
    //     writer_entity_id: EntityId,
    // ) -> bool {
    //     let existing_writer_proxies = local_reader.writer_proxies();

    //     match existing_writer_proxies.lock() {
    //         Ok(writer_proxies) => {
    //             // Check if existing WriterProxy exists
    //             for writer_proxy in writer_proxies.iter() {
    //                 if writer_proxy.remote_writer_guid().prefix() == writer_guid.prefix() {
    //                     debug!(
    //                         "SEDP Logic: Found existing WriterProxy for GUID: {:?}",
    //                         writer_guid
    //                     );
    //                     return true;
    //                 }
    //             }
    //         }
    //         Err(e) => {
    //             error!("SEDP Logic: Failed to acquire writer proxies lock: {}", e);
    //             return false;
    //         }
    //     }

    //     // Does not exist, so create new one
    //     let new_writer_proxy = WriterProxy::new(
    //         writer_guid,
    //         writer_entity_id,
    //         vec![],
    //         vec![],
    //         0,
    //         PublicationBuiltinTopicData::default(),
    //         local_reader.get_update_status_callback(),
    //     );
    //     local_reader.matched_writer_add(new_writer_proxy);
    //     true
    // }

    fn find_reader_proxy(&self, local_writer: &StatefulWriter, reader_guid: Guid) -> bool {
        let existing_reader_proxies = local_writer.reader_proxies();

        match existing_reader_proxies.lock() {
            Ok(reader_proxies) => {
                for reader_proxy in reader_proxies.iter() {
                    if reader_proxy.remote_reader_guid().prefix() == reader_guid.prefix() {
                        return true;
                    }
                }
            }
            Err(e) => {
                error!("SEDP Logic: Failed to acquire reader proxies lock: {}", e);
            }
        }
        false
    }

    fn match_endpoint(
        &self,
        match_type: MatchType,
        endpoint: &dyn std::any::Any,
        builtin_topic_data: BuiltinTopicData,
    ) -> RtpsResult<()> {
        match (match_type, builtin_topic_data) {
            (MatchType::ReaderPublication, BuiltinTopicData::Publication(mut publication_data)) => {
                if let Some(stateful_reader) = endpoint.downcast_ref::<StatefulReader>() {
                    self.handle_empty_locator_lists_for_publication(
                        &mut publication_data,
                        &self.participant,
                    );
                    self.handle_stateful_reader_publication(stateful_reader, publication_data)
                } else if let Some(stateless_reader) = endpoint.downcast_ref::<StatelessReader>() {
                    self.handle_empty_locator_lists_for_publication(
                        &mut publication_data,
                        &self.participant,
                    );
                    self.handle_stateless_reader_publication(stateless_reader, publication_data)
                } else {
                    warn!("SEDP Logic: reader is not StatefulReader or StatelessReader");
                    Err(RtpsError::new(RtpsErrorCode::DowncastError, "Unknown reader type"))
                }
            }
            (
                MatchType::WriterSubscription,
                BuiltinTopicData::Subscription(mut subscription_data),
            ) => {
                if let Some(stateful_writer) = endpoint.downcast_ref::<StatefulWriter>() {
                    self.handle_empty_locator_lists(&mut subscription_data, &self.participant);
                    self.handle_stateful_writer_subscription(stateful_writer, subscription_data)
                } else if let Some(stateless_writer) = endpoint.downcast_ref::<StatelessWriter>() {
                    self.handle_empty_locator_lists(&mut subscription_data, &self.participant);
                    self.handle_stateless_writer_subscription(stateless_writer, subscription_data)
                } else {
                    warn!(
                        "SEDP Logic: Unknown writer type, cannot determine subscription handling"
                    );
                    Err(RtpsError::new(RtpsErrorCode::DowncastError, "Unknown writer type"))
                }
            }
            _ => Err(RtpsError::new(
                RtpsErrorCode::DowncastError,
                "Mismatched direction and data type",
            )),
        }
    }

    pub(crate) fn match_writer_with_subscription(
        &self,
        writer: Arc<dyn Writer + Send + Sync>,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        self.match_endpoint(
            MatchType::WriterSubscription,
            writer.as_any(),
            BuiltinTopicData::Subscription(subscription_builtin_topic_data),
        )
    }
    pub(crate) fn match_reader_with_publication(
        &self,
        reader: Arc<dyn Reader + Send + Sync>,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) {
        let _ = self.match_endpoint(
            MatchType::ReaderPublication,
            reader.as_any(),
            BuiltinTopicData::Publication(publication_builtin_topic_data),
        );
    }

    fn handle_subscription_builtin_topic_data(
        &self,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
        inline_qos_params: Option<ParameterList>,
        content_filter: Option<ContentFilterProperty>,
    ) {
        let topic_name = subscription_builtin_topic_data.topic_name();
        let endpoint_guid = subscription_builtin_topic_data.endpoint_guid();

        if let Some(inline_qos_params) = inline_qos_params {
            if let Some(status_info) = inline_qos_params.get_status_info() {
                // According to RTPS spec, entity termination requires both
                // DISPOSED and UNREGISTERED status flags to be set
                if status_info.disposed() && status_info.unregistered() {
                    debug!("Received Data(r[UD])");

                    // Terminating endpoint provides GUID via KeyHash, but sometimes sends DATA message without SerializedData payload
                    let terminated_reader_guid = inline_qos_params
                        .get_key_hash()
                        .unwrap_or_else(|| InstanceHandle::from_guid(&endpoint_guid));

                    self.participant.remove_unmatched_reader_from_writer(InstanceHandle::to_guid(
                        &terminated_reader_guid,
                    ));

                    if let Some(mut entry) =
                        self.participant.remote_subscriptions().get_mut(&topic_name)
                    {
                        entry.value_mut().remove(&endpoint_guid);

                        if entry.value().is_empty() {
                            drop(entry);
                            self.participant.remote_subscriptions().remove(&topic_name);
                        }
                    }
                    return;
                }
            } else {
                error!("Inline qos STATUS_INFO not parsed in SubscriptionBuiltinTopicData");
            }
        }

        // First try to find local writer using exact match (find_writer_from_entry)
        // If no exact match, try finding writer using domain ID and topic name only
        let writers = self.participant.find_writers_from_topic_name(&topic_name);

        if !writers.is_empty() {
            for writer in writers {
                let subscription_result = self.match_writer_with_subscription(
                    writer.clone(),
                    subscription_builtin_topic_data.clone(),
                );

                // Apply content filter if present (only for stateful writer and if subscription handling succeeded)
                if let Ok(()) = subscription_result {
                    if let Some(ref filter) = content_filter {
                        if let Some(_stateful_writer) =
                            writer.as_any().downcast_ref::<StatefulWriter>()
                        {
                            self.apply_content_filter_to_reader_proxy(
                                _stateful_writer,
                                endpoint_guid,
                                filter,
                            );
                        }
                    }
                }
            }
        } else {
            debug!(
                "SEDP Logic: No local writer found for topic '{}', buffering remote reader",
                topic_name
            );
        }

        self.participant
            .remote_subscriptions()
            .entry(topic_name)
            .or_default()
            .insert(endpoint_guid, subscription_builtin_topic_data);
    }

    pub(crate) fn handle_stateful_writer_subscription(
        &self,
        writer: &StatefulWriter,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        let endpoint_guid = subscription_builtin_topic_data.endpoint_guid();

        if writer.matched_reader_is_matched(endpoint_guid) {
            if writer
                .matched_reader_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no reader proxy for writer",
                ))?
                .subscription_builtin_topic_data()
                .changeable_qos_equals(&subscription_builtin_topic_data)
            {
                return Ok(());
            }

            // QoS changed - check compatibility first
            if let Err(e) = validate_endpoint_compatibility(
                writer,
                &subscription_builtin_topic_data,
                &writer.publication_builtin_topic_data()?,
                |w, pid| w.update_offered_incompatible_qos_status(pid),
                "writer->reader",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote reader {:?}, now incompatible - removing matching",
                    endpoint_guid
                );
                writer
                    .reader_proxies()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock ReaderProxies: {}", lock_err),
                        )
                    })?
                    .retain(|reader_proxy| reader_proxy.remote_reader_guid() != endpoint_guid);

                writer.update_publication_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                return Err(e);
            }

            // Still compatible - just update builtin_topic_data
            debug!(
                "QoS changed for remote reader {:?}, still compatible - updating builtin_topic_data",
                endpoint_guid
            );
            writer
                .reader_proxies()
                .lock()
                .map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        format!("Failed to lock ReaderProxies: {}", e),
                    )
                })?
                .iter_mut()
                .find(|proxy| proxy.remote_reader_guid() == endpoint_guid)
                .map(|proxy| {
                    proxy.set_subscription_builtin_topic_data(subscription_builtin_topic_data)
                });

            return Ok(());
        }

        validate_endpoint_compatibility(
            writer,
            &subscription_builtin_topic_data,
            &writer.publication_builtin_topic_data()?,
            |w, pid| w.update_offered_incompatible_qos_status(pid),
            "writer->reader",
        )?;

        let mut highest_sent_change_sn = SequenceNumber::UNKNOWN;
        let mut max_acked_sn = SequenceNumber::new(0, 0);

        // For Volatile, assume CacheChanges before matching were already sent and ACKed, so don't resend
        if subscription_builtin_topic_data.durability().kind == DurabilityQosPolicyKind::Volatile {
            let last_sn = writer.last_change_sequence_number();
            highest_sent_change_sn = last_sn;
            max_acked_sn = last_sn;
        }

        let reader_proxy = ReaderProxy::new(
            subscription_builtin_topic_data.endpoint_guid(),
            subscription_builtin_topic_data.endpoint_guid().entity_id(),
            subscription_builtin_topic_data.unicast_locator_list(),
            subscription_builtin_topic_data.multicast_locator_list(),
            highest_sent_change_sn,
            max_acked_sn,
            false,
            true,
            subscription_builtin_topic_data.clone(),
        );

        writer.matched_reader_add(reader_proxy);

        let writer_guid = writer.guid();
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.participant.wlp_logic() {
                let _ = wlp_logic.add_local_writer(writer_guid, writer.liveliness()?);
            }
        }

        writer.update_publication_matched_status(
            1,
            InstanceHandle::from_guid(&subscription_builtin_topic_data.endpoint_guid()),
        );

        if subscription_builtin_topic_data.reliability().kind == ReliabilityQosPolicyKind::Reliable
        {
            self.register_preemptive_heartbeat_timer(
                writer,
                subscription_builtin_topic_data.endpoint_guid(),
            )?;
        }

        Ok(())
    }

    pub(crate) fn handle_stateless_writer_subscription(
        &self,
        writer: &StatelessWriter,
        subscription_builtin_topic_data: SubscriptionBuiltinTopicData,
    ) -> RtpsResult<()> {
        debug!("SEDP Logic: SubscriptionBuiltinTopicData is not reliable");
        let endpoint_guid = subscription_builtin_topic_data.endpoint_guid();

        if writer.matched_reader_is_matched(endpoint_guid) {
            if writer
                .matched_reader_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no reader locator for writer",
                ))?
                .subscription_builtin_topic_data()
                .changeable_qos_equals(&subscription_builtin_topic_data)
            {
                debug!(
                    "[handle_stateless_writer_subscription] Found existing ReaderLocator for GUID: {:?}",
                    subscription_builtin_topic_data.endpoint_guid()
                );
                return Ok(());
            }

            // QoS changed - check compatibility first
            if let Err(e) = validate_endpoint_compatibility(
                writer,
                &subscription_builtin_topic_data,
                &writer.publication_builtin_topic_data()?,
                |w, pid| w.update_offered_incompatible_qos_status(pid),
                "writer->reader",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote reader {:?}, now incompatible - removing matching",
                    endpoint_guid
                );
                writer
                    .reader_locator()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock ReaderLocator: {}", lock_err),
                        )
                    })?
                    .retain(|reader_locator| reader_locator.remote_reader_guid() != endpoint_guid);

                writer.update_publication_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                error!("StatelessWriter compatibility error -> {}", e);
                return Err(e);
            }

            // Still compatible - just update builtin_topic_data
            debug!(
                "QoS changed for remote reader {:?}, still compatible - updating builtin_topic_data",
                endpoint_guid
            );
            writer
                .reader_locator()
                .lock()
                .map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        format!("Failed to lock ReaderLocator: {}", e),
                    )
                })?
                .iter_mut()
                .find(|locator| locator.remote_reader_guid() == endpoint_guid)
                .map(|locator| {
                    locator.set_subscription_builtin_topic_data(subscription_builtin_topic_data)
                });

            return Ok(());
        }

        if let Err(e) = validate_endpoint_compatibility(
            writer,
            &subscription_builtin_topic_data,
            &writer.publication_builtin_topic_data()?,
            |w, pid| w.update_offered_incompatible_qos_status(pid),
            "writer->reader",
        ) {
            error!("StatelessWriter compatibility error -> {}", e);
            return Err(e);
        }

        let mut highest_sent_change_sn = None;

        // For Volatile, assume CacheChanges before matching were already sent, so don't resend
        if subscription_builtin_topic_data.durability().kind == DurabilityQosPolicyKind::Volatile {
            highest_sent_change_sn = Some(writer.last_change_sequence_number());
        }

        for locator in subscription_builtin_topic_data.unicast_locator_list() {
            if !(locator.kind() == LOCATOR_KIND_UDP_V4
                || locator.kind() == LOCATOR_KIND_UDP_V6
                || locator.kind() == LOCATOR_KIND_TCP_V4
                || locator.kind() == LOCATOR_KIND_TCP_V6)
            {
                continue;
            }
            writer.reader_locator_add(ReaderLocator::new(
                locator.clone(),
                highest_sent_change_sn,
                false,
                subscription_builtin_topic_data.endpoint_guid().prefix(),
                subscription_builtin_topic_data.endpoint_guid().entity_id(),
                subscription_builtin_topic_data.clone(),
            ));
        }
        for locator in subscription_builtin_topic_data.multicast_locator_list() {
            // Note: Currently, builtin_topic_data is sent via unicast only.
            // Multicast support for builtin topics may be added in the future if needed.
            // The following code is kept for reference:
            // if !writer.multicast_locator_list().contains(&locator) {
            //     writer.add_multicast_locator(locator.clone());
            // }
            if !(locator.kind() == LOCATOR_KIND_UDP_V4
                || locator.kind() == LOCATOR_KIND_UDP_V6
                || locator.kind() == LOCATOR_KIND_TCP_V4
                || locator.kind() == LOCATOR_KIND_TCP_V6)
            {
                continue;
            }

            let reader_locator = ReaderLocator::new(
                locator.clone(),
                highest_sent_change_sn,
                false,
                subscription_builtin_topic_data.endpoint_guid().prefix(),
                subscription_builtin_topic_data.endpoint_guid().entity_id(),
                subscription_builtin_topic_data.clone(),
            );
            writer.reader_locator_add(reader_locator);
        }

        writer.update_publication_matched_status(
            1,
            InstanceHandle::from_guid(&subscription_builtin_topic_data.endpoint_guid()),
        );

        Ok(())
    }

    fn handle_empty_locator_lists(
        &self,
        subscription_builtin_topic_data: &mut SubscriptionBuiltinTopicData,
        participant_guard: &Participant,
    ) {
        if subscription_builtin_topic_data.unicast_locator_list().is_empty()
        // || subscription_builtin_topic_data.multicast_locator_list().is_empty()
        {
            let remote_participant_data = participant_guard.find_remote_participant_proxy_data(
                subscription_builtin_topic_data.endpoint_guid().prefix(),
            );

            if let Some(remote_participant_data) = remote_participant_data {
                for locator in remote_participant_data.default_unicast_locator_list() {
                    if !subscription_builtin_topic_data.unicast_locator_list().contains(locator) {
                        subscription_builtin_topic_data.add_unicast_locator(locator.clone());
                    }
                }
            }
        }
    }

    fn handle_publication_builtin_topic_data(
        &self,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
        inline_qos_params: Option<ParameterList>,
    ) {
        let topic_name = publication_builtin_topic_data.topic_name().to_string();
        let endpoint_guid = publication_builtin_topic_data.endpoint_guid();

        if let Some(inline_qos_params) = inline_qos_params {
            if let Some(status_info) = inline_qos_params.get_status_info() {
                // According to RTPS spec, entity termination requires both
                // DISPOSED and UNREGISTERED status flags to be set
                if status_info.disposed() && status_info.unregistered() {
                    debug!("Received Data(w[UD])");
                    // Terminating endpoint provides GUID via KeyHash, but sometimes sends DATA message without SerializedData payload
                    let terminated_writer_guid = inline_qos_params
                        .get_key_hash()
                        .unwrap_or_else(|| InstanceHandle::from_guid(&endpoint_guid));

                    let writer_guid = InstanceHandle::to_guid(&terminated_writer_guid);
                    if writer_guid.entity_id().entity_kind().is_user_defined() {
                        if let Some(wlp_logic) = self.participant.wlp_logic() {
                            let _ = wlp_logic.remove_remote_writer(writer_guid);
                        }
                    }

                    self.participant.remove_unmatched_writer_from_reader(InstanceHandle::to_guid(
                        &terminated_writer_guid,
                    ));

                    if let Some(mut entry) =
                        self.participant.remote_publications().get_mut(&topic_name)
                    {
                        entry.value_mut().remove(&endpoint_guid);

                        if entry.value().is_empty() {
                            drop(entry);
                            self.participant.remote_publications().remove(&topic_name);
                        }
                    }
                    return;
                }
            } else {
                error!("Inline qos STATUS_INFO not parsed in PublicationBuiltinTopicData");
            }
        }

        // First try to find local reader using exact match (find_reader_from_entry)
        let readers = self.participant.find_readers_from_topic_name(&topic_name);

        if !readers.is_empty() {
            for reader in readers {
                self.match_reader_with_publication(reader, publication_builtin_topic_data.clone());
            }
        } else {
            // add to pending remote publications
            debug!(
                "SEDP Logic: No local reader found for topic '{}', buffering remote writer",
                topic_name
            );
        }

        self.participant
            .remote_publications()
            .entry(topic_name)
            .or_default()
            .insert(endpoint_guid, publication_builtin_topic_data);
    }

    pub(crate) fn handle_stateless_reader_publication(
        &self,
        reader: &StatelessReader,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        let endpoint_guid = publication_builtin_topic_data.endpoint_guid();

        // Check if writer is already matched to avoid duplicates
        if reader.matched_writer_is_matched(endpoint_guid) {
            if reader
                .matched_writer_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no writer locator for reader",
                ))?
                .publication_builtin_topic_data()
                .changeable_qos_equals(&publication_builtin_topic_data)
            {
                return Ok(());
            }

            // QoS changed - check compatibility first
            if let Err(e) = validate_endpoint_compatibility(
                reader,
                &reader.subscription_builtin_topic_data()?,
                &publication_builtin_topic_data,
                |r, pid| r.update_requested_incompatible_qos_status(pid),
                "reader->writer",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote writer {:?}, now incompatible - removing matching",
                    endpoint_guid
                );
                reader
                    .writer_locators()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock WriterLocators: {}", lock_err),
                        )
                    })?
                    .retain(|writer_locator| writer_locator.remote_writer_guid() != endpoint_guid);

                reader.update_subscription_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                return Err(e);
            }

            // Still compatible - just update builtin_topic_data
            debug!(
                "QoS changed for remote writer {:?}, still compatible - updating builtin_topic_data",
                endpoint_guid
            );
            reader
                .writer_locators()
                .lock()
                .map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        format!("Failed to lock WriterLocators: {}", e),
                    )
                })?
                .iter_mut()
                .find(|locator| locator.remote_writer_guid() == endpoint_guid)
                .map(|locator| {
                    locator.set_publication_builtin_topic_data(publication_builtin_topic_data)
                });

            return Ok(());
        }

        validate_endpoint_compatibility(
            reader,
            &reader.subscription_builtin_topic_data()?,
            &publication_builtin_topic_data,
            |r, pid| r.update_requested_incompatible_qos_status(pid),
            "reader->writer",
        )?;

        let writer_locator = WriterLocator::new(
            endpoint_guid,
            publication_builtin_topic_data.unicast_locator_list(),
            publication_builtin_topic_data.multicast_locator_list(),
            publication_builtin_topic_data.clone(),
        );
        reader.matched_writer_add(writer_locator);

        let writer_guid = endpoint_guid;
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.participant.wlp_logic() {
                let _ = wlp_logic
                    .add_remote_writer(writer_guid, *publication_builtin_topic_data.liveliness());
            }
        }

        reader.update_subscription_matched_status(1, InstanceHandle::from_guid(&endpoint_guid));

        Ok(())
    }

    pub(crate) fn handle_stateful_reader_publication(
        &self,
        reader: &StatefulReader,
        publication_builtin_topic_data: PublicationBuiltinTopicData,
    ) -> RtpsResult<()> {
        let endpoint_guid = publication_builtin_topic_data.endpoint_guid();

        // Check if writer is already matched to avoid duplicates
        if reader.matched_writer_is_matched(endpoint_guid) {
            if reader
                .matched_writer_lookup(endpoint_guid)
                .ok_or(RtpsError::new(
                    RtpsErrorCode::MatchedEntityNotFound,
                    "There is no writer proxy for reader",
                ))?
                .publication_builtin_topic_data()
                .changeable_qos_equals(&publication_builtin_topic_data)
            {
                return Ok(());
            }

            // QoS changed - check compatibility first
            if let Err(e) = validate_endpoint_compatibility(
                reader,
                &reader.subscription_builtin_topic_data()?,
                &publication_builtin_topic_data,
                |r, pid| r.update_requested_incompatible_qos_status(pid),
                "reader->writer",
            ) {
                // Incompatible - remove matching
                debug!(
                    "QoS changed for remote writer {:?}, now incompatible - removing matching",
                    endpoint_guid
                );
                reader
                    .writer_proxies()
                    .lock()
                    .map_err(|lock_err| {
                        RtpsError::new(
                            RtpsErrorCode::LockError,
                            format!("Failed to lock WriterProxies: {}", lock_err),
                        )
                    })?
                    .retain(|writer_proxy| writer_proxy.remote_writer_guid() != endpoint_guid);

                reader.update_subscription_matched_status(
                    -1,
                    InstanceHandle::from_guid(&endpoint_guid),
                );
                return Err(e);
            }

            // Still compatible - just update builtin_topic_data
            debug!(
                "QoS changed for remote writer {:?}, still compatible - updating builtin_topic_data",
                endpoint_guid
            );
            reader
                .writer_proxies()
                .lock()
                .map_err(|e| {
                    RtpsError::new(
                        RtpsErrorCode::LockError,
                        format!("Failed to lock WriterProxies: {}", e),
                    )
                })?
                .iter_mut()
                .find(|proxy| proxy.remote_writer_guid() == endpoint_guid)
                .map(|proxy| {
                    proxy.set_publication_builtin_topic_data(publication_builtin_topic_data)
                });

            return Ok(());
        }

        // Check QoS and Partition compatibility
        validate_endpoint_compatibility(
            reader,
            &reader.subscription_builtin_topic_data()?,
            &publication_builtin_topic_data,
            |r, pid| r.update_requested_incompatible_qos_status(pid),
            "reader->writer",
        )?;

        let writer_proxy = WriterProxy::new(
            publication_builtin_topic_data.endpoint_guid(),
            publication_builtin_topic_data.endpoint_guid().entity_id(),
            publication_builtin_topic_data.unicast_locator_list(),
            publication_builtin_topic_data.multicast_locator_list(),
            0, // data_max_size_serialized
            publication_builtin_topic_data.clone(),
            reader.get_update_status_callback(),
        );

        let writer_guid = endpoint_guid;
        if writer_guid.entity_id().entity_kind().is_user_defined() {
            if let Some(wlp_logic) = self.participant.wlp_logic() {
                let _ = wlp_logic
                    .add_remote_writer(writer_guid, *publication_builtin_topic_data.liveliness());
            }
        }

        reader.matched_writer_add(writer_proxy);
        self.register_preemptive_acknack_timer(
            reader,
            publication_builtin_topic_data.endpoint_guid(),
        )?;

        reader.update_subscription_matched_status(1, InstanceHandle::from_guid(&endpoint_guid));

        Ok(())
    }

    fn register_preemptive_acknack_timer(
        &self,
        stateful_reader: &StatefulReader,
        remote_writer_guid: Guid,
    ) -> RtpsResult<()> {
        let stateful_reader_id = stateful_reader.guid().entity_id();
        let timer_id = format!(
            "preemptive_acknack_{:?}_{:?}",
            stateful_reader_id,
            chrono::Local::now().to_rfc3339()
        );
        let participant = self.participant.clone();
        let callback = move || {
            if let Some(sending_handler) =
                SendingHandler::get_instance_by_participant_guid(participant.guid())
            {
                sending_handler.push_message_and_wake(MessageType::SendPreemptiveAcknack(
                    stateful_reader_id,
                    remote_writer_guid,
                ));
            }
        };

        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                stateful_reader.preemptive_acknack_delay().to_std_duration(),
                false,
                callback,
            );
        }

        Ok(())
    }

    fn register_preemptive_heartbeat_timer(
        &self,
        stateful_writer: &StatefulWriter,
        remote_reader_guid: Guid,
    ) -> RtpsResult<()> {
        let stateful_writer_id = stateful_writer.guid().entity_id();
        let timer_id = format!(
            "preemptive_heartbeat_{:?}_{:?}",
            stateful_writer_id,
            chrono::Local::now().to_rfc3339()
        );

        let participant = self.participant.clone();
        let callback = move || {
            if let Some(sending_handler) =
                SendingHandler::get_instance_by_participant_guid(participant.guid())
            {
                sending_handler.push_message_and_wake(MessageType::SendHeartbeatMessageToOne(
                    stateful_writer_id,
                    remote_reader_guid,
                    true,
                ));
            }
        };

        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                stateful_writer.preemptive_heartbeat_delay().to_std_duration(),
                false,
                callback,
            );
        }

        Ok(())
    }

    fn handle_empty_locator_lists_for_publication(
        &self,
        publication_builtin_topic_data: &mut PublicationBuiltinTopicData,
        participant_guard: &Participant,
    ) {
        if publication_builtin_topic_data.unicast_locator_list().is_empty()
        // || publication_builtin_topic_data.multicast_locator_list().is_empty()
        {
            let remote_participant_data = participant_guard.find_remote_participant_proxy_data(
                publication_builtin_topic_data.endpoint_guid().prefix(),
            );

            if let Some(remote_participant_data) = remote_participant_data {
                for locator in remote_participant_data.default_unicast_locator_list() {
                    if !publication_builtin_topic_data.unicast_locator_list().contains(locator) {
                        publication_builtin_topic_data.add_unicast_locator(locator.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn send_endpoint_termination_message(
        &self,
        builtin_writer_guid: Guid,
        cache_change: Arc<CacheChange>,
    ) -> RtpsResult<()> {
        // Get remote builtin reader for corresponding builtin writer
        let mut remote_guid_list = Vec::new();

        if builtin_writer_guid.entity_id() == EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER {
            if let Ok(reader_proxies) = self
                .participant
                .builtin_endpoints()
                .sedp_builtin_publications_writer
                .reader_proxies()
                .lock()
            {
                remote_guid_list.extend(
                    reader_proxies
                        .iter()
                        .filter(|proxy| {
                            proxy.remote_reader_guid().entity_id()
                                == EntityId::SEDP_BUILTIN_PUBLICATIONS_READER
                        })
                        .map(|proxy| proxy.remote_reader_guid()),
                );
            }
        } else if builtin_writer_guid.entity_id() == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER {
            if let Ok(reader_proxies) = self
                .participant
                .builtin_endpoints()
                .sedp_builtin_subscriptions_writer
                .reader_proxies()
                .lock()
            {
                remote_guid_list.extend(
                    reader_proxies
                        .iter()
                        .filter(|proxy| {
                            proxy.remote_reader_guid().entity_id()
                                == EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER
                        })
                        .map(|proxy| proxy.remote_reader_guid()),
                );
            }
        } else {
            error!("Invalid sender for SEDP termination: {:?}", builtin_writer_guid.entity_id());
        }

        for remote_guid in remote_guid_list {
            let buffer = MessageCreator::create_data_msg(
                cache_change.clone(),
                remote_guid,
                remote_guid.entity_id(),
                builtin_writer_guid.entity_id(),
                None, // No heartbeat
                true, // Use inline QoS (default)
                None, // No content filter for termination messages
            );

            match buffer {
                Ok(buffer) => {
                    if let Err(e) = self.send_discovery_message(&buffer, remote_guid, "termination")
                    {
                        warn!("Failed to send SEDP termination message: {:?}", e);
                    }
                }
                Err(e) => {
                    warn!("Failed to create SEDP DATA (termination) message: {:?}", e);
                }
            };
        }
        Ok(())
    }

    pub(crate) fn send_participant_termination_message_unicast(&self) {
        if let Ok(rtps_message) =
            MessageCreator::create_spdp_msg_with_inline_qos(self.participant.clone())
        {
            // Create Data(p[UD]) RTPS Message
            let buffer = rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian);
            if let Ok(buffer) = buffer {
                // Send to all remote participants
                match self.participant.remote_participant_proxy_datas().clone().lock() {
                    Ok(remote_participant_datas) => {
                        for remote_participant_data in remote_participant_datas.iter() {
                            // Send to remote participant's all metatraffic unicast locators
                            for locator in
                                remote_participant_data.metatraffic_unicast_locator_list()
                            {
                                if locator.kind() == 1 {
                                    // UDPv4
                                    let socket_addr = SocketAddr::V4(SocketAddrV4::new(
                                        locator.to_ip_v4_addr(),
                                        locator.port() as u16,
                                    ));
                                    if let Some(ref sender) = self.sender {
                                        let _ = sender.send(&socket_addr, &buffer);
                                    } else {
                                        debug!("UDP sender not available, skipping endpoint termination message");
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Failed to lock remote_participant_datas: {:?}", e);
                    }
                }
            } else {
                error!("Failed to serialize SPDP message with inline qos");
            }
        }
    }

    /// Apply content filter signature to the most recently added ReaderProxy
    ///
    /// This is a helper function to keep the main logic clean.
    /// It finds the ReaderProxy for the given reader_guid and sets its content filter signatures.
    fn apply_content_filter_to_reader_proxy(
        &self,
        writer: &StatefulWriter,
        reader_guid: Guid,
        content_filter: &ContentFilterProperty,
    ) {
        use crate::rtps::builtin::data::content_filtered_topic::calculate_filter_signature;

        let signature = calculate_filter_signature(&content_filter.filter_expression);

        if let Ok(mut reader_proxies) = writer.reader_proxies().lock() {
            if let Some(reader_proxy) =
                reader_proxies.iter_mut().find(|rp| rp.remote_reader_guid() == reader_guid)
            {
                reader_proxy.set_content_filter_signatures(Some(vec![signature]));
                // debug!("Applied content filter signature to ReaderProxy: {:?}", reader_guid);
            }
        }
    }

    fn handle_gap_message(&self, gap: &Gap, remote_guid: Guid) -> RtpsResult<()> {
        let builtin_endpoint_pair = BuiltinEndpointPair::reader_writer_from_entity_id(
            gap.writer_id,
            self.participant.clone(),
        )?
        .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        let local_reader = builtin_endpoint_pair.reader();

        let stateful_reader =
            local_reader.as_any().downcast_ref::<StatefulReader>().ok_or_else(|| {
                RtpsError::new(
                    RtpsErrorCode::DowncastError,
                    "Failed to downcast Builtin SEDP reader",
                )
            })?;

        let writer_proxies_arc = stateful_reader.writer_proxies();
        let mut writer_proxies = writer_proxies_arc.lock().map_err(|_| {
            RtpsError::new(RtpsErrorCode::LockError, "Failed to acquire writer proxies lock")
        })?;

        let writer_proxy = writer_proxies
            .iter_mut()
            .find(|proxy| proxy.remote_writer_guid() == remote_guid)
            .ok_or_else(|| RtpsError::new(RtpsErrorCode::MatchedEntityNotFound, None))?;

        let mut irrelevant_changes = Vec::new();

        for sn in gap.gap_start.to_i64()..gap.gap_list.bitmap_base().to_i64() {
            irrelevant_changes.push(SequenceNumber::from_i64(sn));
        }

        irrelevant_changes.extend(gap.gap_list.extract_numbers().iter());

        debug!(
            "[SEDP GAP] Marking changes as irrelevant from SN {:?}, to {:?}",
            irrelevant_changes.first(),
            irrelevant_changes.last()
        );

        if irrelevant_changes.last().unwrap_or(&SequenceNumber::new(0, 0))
            > &writer_proxy.expected_sn()
        {
            writer_proxy.set_expected_sn(SequenceNumber::from_i64(
                irrelevant_changes.last().unwrap().to_i64() + 1,
            ));
        }

        for seq_num in irrelevant_changes {
            writer_proxy.irrelevant_change_set(seq_num);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use crate::rtps::{
        entities::participant::Participant,
        logic::spdp_logic::SpdpLogic,
        task::{
            discovery_traffic::{
                discovery_multicast_listening_task::DiscoveryMulticastListeningTask,
                discovery_unicast_listening_task::DiscoveryUnicastListeningTask,
            },
            sending_handler::SendingHandler,
        },
        transport::socket::Socket,
    };

    // Remote DDS must be running
    // For sedp_send test, need sedp-related writer's reader locator, reader proxy
    // And need remote_participant_proxy_datas in participant
    // So run discovery_multicast_listening_task
    // Automatically proceeds to send sedp message(HEARTBEAT)
    #[test]
    #[ignore]
    fn test_send_sedp_message() {
        env_logger::builder().filter_level(log::LevelFilter::Debug).init();

        let domain_id = 10;
        let mut socket = Socket::new(domain_id); //domain_id 0
        socket.create_socket();
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ip()));

        // Socket reset required??
        // socket.close();
        // return;
        let _ = SendingHandler::get_instance(
            participant.clone(),
            Some(socket.sender()),
            socket.tcp_sender(),
        );

        //discovery multicast port : 7400
        //discovery unicast port :  7410
        //user traffic multicast port : 7401
        //user traffic unicast port : 7411
        let mut discovery_multicast_listening_task = DiscoveryMulticastListeningTask::new(
            socket.discovery_multicast_listener(),
            participant.clone(),
        );
        //multicast listening
        thread::Builder::new()
            .name("discovery_traffic_multicast_listening".to_string())
            .spawn(move || {
                // Register thread name for monitoring
                {
                    use crate::rtps::task::thread_monitor::ThreadMonitor;
                    ThreadMonitor::register_current_thread_name(
                        "discovery_traffic_multicast_listening",
                    );
                }

                let _ = discovery_multicast_listening_task.multicast_listening();
                eprintln!("discovery multicast listening thread finished");
            })
            .expect("Failed to create discovery multicast listening thread");

        // Code to verify that errors are being sent
        thread::sleep(std::time::Duration::from_secs(100));
    }

    // Remote DDS must be running
    // SPDP, SEDP execution test
    #[test]
    #[ignore]
    fn test_process_sedp() {
        env_logger::builder().filter_level(log::LevelFilter::Info).init();

        let domain_id = 5;
        let mut socket = Socket::new(domain_id); //domain_id 0
        socket.create_socket();
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ip()));

        let _ = SendingHandler::get_instance(participant.clone(), Some(socket.sender()), None);

        //spdp multicast
        let spdp_logic = SpdpLogic::new(
            participant.clone(),
            Some(socket.sender()),
            None,
            Vec::new(), // No initial peers for test
        );
        spdp_logic.trigger_send_spdp_multicast();
        //discovery multicast port : 7400
        //discovery unicast port :  7410
        //user traffic multicast port : 7401
        //user traffic unicast port : 7411
        let mut discovery_multicast_listening_task = DiscoveryMulticastListeningTask::new(
            socket.discovery_multicast_listener(),
            participant.clone(),
        );
        //multicast listening
        thread::Builder::new()
            .name("discovery_traffic_multicast_listening".to_string())
            .spawn(move || {
                // Register thread name for monitoring
                {
                    use crate::rtps::task::thread_monitor::ThreadMonitor;
                    ThreadMonitor::register_current_thread_name(
                        "discovery_traffic_multicast_listening",
                    );
                }

                let _ = discovery_multicast_listening_task.multicast_listening();
                eprintln!("discovery multicast listening thread finished");
            })
            .expect("Failed to create discovery multicast listening thread");

        let mut discovery_unicast_listening_task = DiscoveryUnicastListeningTask::new(
            socket.discovery_unicast_listener(),
            socket.discovery_tcp_listener(),
            participant.clone(),
        );

        //unicast listening
        thread::Builder::new()
            .name("discovery_traffic_unicast_listening".to_string())
            .spawn(move || {
                let _ = discovery_unicast_listening_task.unicast_listening();
                eprintln!("discovery unicast listening thread finished");
            })
            .expect("Failed to create discovery unicast listening thread");

        // Wait for other threads
        thread::sleep(std::time::Duration::from_secs(100));
    }
}
