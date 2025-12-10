//! SPDP (Simple Participant Discovery Protocol) logic implementation.
//!
//! This module implements the SPDP protocol for discovering domain participants.
//! SPDP participants periodically announce themselves and process announcements
//! from remote participants to maintain the participant discovery database.

use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration as StdDuration, Instant},
};

use rand;
use speedy::{Endianness, Writable};

use crate::{
    common::builtin::topic::{
        publication_builtin_topic_data::PublicationBuiltinTopicData,
        subscription_builtin_topic_data::SubscriptionBuiltinTopicData,
    },
    infrastructure::liveliness_monitor::LivelinessMonitor,
    rtps::{
        builtin::{
            data::{
                builtin_endpoint_set::BuiltinEndpointFlag,
                spdp_discovered_participant_data::SPDPDiscoveredParticipantData,
            },
            spdp_builtin_participant_writer::SPDPbuiltinParticipantWriter,
        },
        common::{
            entity_id::EntityId,
            guid::Guid,
            locator::{
                Locator, LOCATOR_KIND_TCP_V4, LOCATOR_KIND_TCP_V6, LOCATOR_KIND_UDP_V4,
                LOCATOR_KIND_UDP_V6,
            },
            rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
            sequence::SequenceNumber,
            time::RtpsDuration,
            types::DomainId,
        },
        entities::{
            entity::Entity,
            participant::Participant,
            reader::{Reader, StatefulReader, WriterProxy},
            writer::{
                has_reader_locator::HasReaderLocator, reader_locator::ReaderLocator,
                reader_proxy::ReaderProxy, StatefulWriter, Writer,
            },
        },
        messages::message_creator::MessageCreator,
        task::{
            sending_handler::{MessageType, SendingHandler},
            timer_handler::TimerHandler,
        },
        transport::{Transport, TransportSender, TransportType},
    },
};

#[derive(Clone)]
pub(crate) struct SpdpLogic {
    participant: Weak<Participant>,
    sender: Option<Arc<TransportSender>>,
    tcp_sender: Option<Arc<TransportSender>>,
    initial_peers: Vec<std::net::SocketAddr>,
    timer_handler: Arc<Mutex<TimerHandler>>,
    participant_monitor: Arc<Mutex<Option<LivelinessMonitor>>>,
}

impl SpdpLogic {
    pub(crate) fn new(
        participant: Arc<Participant>,
        sender: Option<Arc<TransportSender>>,
        tcp_sender: Option<Arc<TransportSender>>,
        initial_peers: Vec<std::net::SocketAddr>,
    ) -> Self {
        let timer_handler = TimerHandler::get_instance(participant.clone());
        Self {
            participant: Arc::downgrade(&participant),
            sender,
            tcp_sender,
            initial_peers,
            timer_handler,
            participant_monitor: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn is_participant_terminated(&self) -> RtpsResult<bool> {
        Ok(self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?
            .is_terminated())
    }

    pub(crate) fn exist_reader_locator_writer(
        &self,
        writer: Arc<Mutex<dyn HasReaderLocator>>,
        locator: Locator,
    ) -> bool {
        match writer.lock() {
            Ok(writer_) => {
                let reader_locator = writer_.reader_locator();
                for l in reader_locator {
                    if l.locator() == locator {
                        return true;
                    }
                }
                false
            }
            Err(e) => {
                log::error!("Failed to acquire spdp writer lock: {}", e);
                false
            }
        }
    }

    pub(crate) fn exist_reader_proxy_writer(
        &self,
        writer: Arc<StatefulWriter>,
        participant_guid: Guid,
    ) -> bool {
        let reader_proxy_ = writer.reader_proxies();

        match reader_proxy_.lock() {
            Ok(reader_proxy_) => {
                for p in reader_proxy_.iter() {
                    if p.remote_reader_guid().prefix() == participant_guid.prefix() {
                        return true;
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to acquire spdp writer lock: {}", e);
            }
        }
        false
    }

    //TODO: delete
    // fn new_cache_change(
    //     spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    //     writer_guid: Guid,
    //     entity_id: EntityId,
    //     sequence_number: SequenceNumber,
    // ) -> CacheChange {
    //     let empty_data: [u8; 0] = [];
    //     CacheChange::new(
    //         ChangeKind::Alive,
    //         writer_guid,
    //         InstanceHandle::from_guid(&spdp_discovered_participant_data.participant_guid()),
    //         sequence_number,
    //         Arc::new(empty_data),
    //         Some(Time::now()),
    //     )
    // }

    // Add data received through SPDP to spdp_builtin_participant_writer
    fn add_reader_locator_spdp_builtin_participant_writer(
        &self,
        spdp_builtin_participant_writer: Arc<Mutex<SPDPbuiltinParticipantWriter>>,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    ) {
        // Add reader locator to SPDP builtin participant writer
        // SPDP sends to everyone once the locator list is registered
        for locator in spdp_discovered_participant_data.metatraffic_unicast_locator_list() {
            if !self.exist_reader_locator_writer(
                spdp_builtin_participant_writer.clone(),
                locator.clone(),
            ) {
                match spdp_builtin_participant_writer.lock() {
                    Ok(spdp_builtin_participant_writer) => {
                        if !(locator.kind() == LOCATOR_KIND_UDP_V4
                            || locator.kind() == LOCATOR_KIND_UDP_V6
                            || locator.kind() == LOCATOR_KIND_TCP_V4
                            || locator.kind() == LOCATOR_KIND_TCP_V6)
                        {
                            continue;
                        }
                        let reader_locator = ReaderLocator::new(
                            locator.clone(),
                            None,
                            false,
                            spdp_discovered_participant_data.guid_prefix(),
                            EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
                            SubscriptionBuiltinTopicData::default(),
                        );

                        spdp_builtin_participant_writer.reader_locator_add(reader_locator);

                        //TODO: delete
                        // let instance_handle = InstanceHandle::from_guid(
                        //     &spdp_discovered_participant_data.participant_guid(),
                        // );

                        //TODO: delete
                        // Create cache change if it doesn't exist
                        // if !spdp_builtin_participant_writer.exist_cache_change(instance_handle) {
                        //     let change: CacheChange = SpdpLogic::new_cache_change(
                        //         spdp_discovered_participant_data.clone(),
                        //         spdp_builtin_participant_writer.guid(),
                        //         EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER,
                        //         sequence_number,
                        //     );
                        //     spdp_builtin_participant_writer
                        //         .writer_cache()
                        //         .lock()
                        //         .unwrap()
                        //         .add_change(Arc::new(change));
                        // }
                    }
                    Err(e) => {
                        log::error!(
                            "Failed to acquire spdp builtin participant writer lock: {}",
                            e
                        );
                    }
                }
            }
        }
    }

    fn add_reader_proxy_to_writer(
        &self,
        writer: Arc<StatefulWriter>,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
        entity_id: EntityId,
    ) {
        // SEDP is stateful and holds a locator list in reader proxy
        if !self.exist_reader_proxy_writer(
            writer.clone(),
            spdp_discovered_participant_data.participant_guid(),
        ) {
            let reader_proxy = ReaderProxy::new(
                Guid::new(spdp_discovered_participant_data.participant_guid().prefix(), entity_id),
                entity_id,
                spdp_discovered_participant_data.metatraffic_unicast_locator_list().clone(),
                spdp_discovered_participant_data.metatraffic_multicast_locator_list().clone(),
                writer.last_change_sequence_number(),
                SequenceNumber::new(0, 0),
                false,
                true,
                // 8.5.4.1 According to the DDS specification, the reliability QoS for these built-in Entities is set to 'reliable.'
                SubscriptionBuiltinTopicData::default(),
            );
            writer.matched_reader_add(reader_proxy);
        }
    }

    fn add_writer_proxy_to_reader(
        &self,
        reader: Arc<StatefulReader>,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
        entity_id: EntityId,
    ) {
        let writer_guid =
            Guid::new(spdp_discovered_participant_data.participant_guid().prefix(), entity_id);

        if !reader.matched_writer_is_matched(writer_guid) {
            let reader_proxy = WriterProxy::new(
                writer_guid,
                entity_id,
                spdp_discovered_participant_data.metatraffic_unicast_locator_list().clone(),
                spdp_discovered_participant_data.metatraffic_multicast_locator_list().clone(),
                0,
                PublicationBuiltinTopicData::default(),
                reader.get_update_status_callback(),
            );
            reader.matched_writer_add(reader_proxy);
        }
    }

    // Process SPDP message received from socket listener
    pub(crate) fn handle_multicast_spdp_message(
        &self,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    ) -> RtpsResult<()> {
        let participant_guid = spdp_discovered_participant_data.participant_guid();
        log::debug!(
            "handle_multicast_spdp_message {:?} / {:?}",
            participant_guid,
            spdp_discovered_participant_data.metatraffic_unicast_locator_list()
        );

        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;

        let domain_id = participant.domain_id();

        if domain_id != spdp_discovered_participant_data.domain_id() {
            log::trace!(
                "SPDP message domain ID mismatch: local {}, remote {}. Ignoring.",
                domain_id,
                spdp_discovered_participant_data.domain_id()
            );
            return Ok(());
        }

        let mut datas: Vec<SPDPDiscoveredParticipantData> = Vec::new();

        let result = match participant.remote_participant_proxy_datas().lock() {
            Ok(remote_participant_datas) => {
                remote_participant_datas.iter().any(|remote_participant_data| {
                    if remote_participant_data.participant_guid() == participant_guid {
                        log::debug!(
                            "handle_multicast_spdp_message already exist {:?} / {:?}",
                            participant_guid,
                            spdp_discovered_participant_data.metatraffic_unicast_locator_list()
                        );
                        true
                    } else {
                        false
                    }
                })
            }
            Err(e) => {
                log::error!("Failed to lock remote_participant_datas: {:?}", e);
                false
            }
        };

        if result {
            return Ok(());
        }

        // Add reader_locator/reader_proxy to writer according to remote endpointset
        // Create reader proxy and reader locator for builtin writers
        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PARTICIPANT_DETECTOR)
        {
            self.add_reader_locator_spdp_builtin_participant_writer(
                participant.spdp_builtin_participant_writer(),
                spdp_discovered_participant_data.clone(),
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_READER)
        {
            self.add_reader_proxy_to_writer(
                participant.builtin_participant_message_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::BUILTIN_ENDPOINT_PARTICIPANT_MESSAGE_DATA_WRITER)
        {
            self.add_writer_proxy_to_reader(
                participant.builtin_participant_message_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::P2P_BUILTIN_PARTICIPANT_MESSAGE_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_DETECTOR)
        {
            self.add_reader_proxy_to_writer(
                participant.sedp_builtin_publications_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_PUBLICATIONS_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_PUBLICATIONS_ANNOUNCER)
        {
            self.add_writer_proxy_to_reader(
                participant.sedp_builtin_publications_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_PUBLICATIONS_WRITER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_DETECTOR)
        {
            self.add_reader_proxy_to_writer(
                participant.sedp_builtin_subscriptions_writer(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_READER,
            );
        }

        if spdp_discovered_participant_data
            .available_builtin_endpoints()
            .contains(BuiltinEndpointFlag::DISC_BUILTIN_ENDPOINT_SUBSCRIPTIONS_ANNOUNCER)
        {
            self.add_writer_proxy_to_reader(
                participant.sedp_builtin_subscriptions_reader(),
                spdp_discovered_participant_data.clone(),
                EntityId::SEDP_BUILTIN_SUBSCRIPTIONS_WRITER,
            );
        }

        participant.add_remote_participant_proxy_data(spdp_discovered_participant_data.clone());

        datas.push(spdp_discovered_participant_data.clone());
        for data in datas {
            // Proceed with SEDP after acquiring remote participant information
            // Send SEDP SPDP Message
            let _ = self.trigger_send_sedp_message(Arc::new(data.clone()));
            //SEDP ...
        }

        Ok(())
    }

    // Logic to actually send the data
    // After sending, start chaining after thread sleep (because it's easier to call sending handler from thread)
    pub(crate) fn send_spdp_multicast(
        &mut self,
        start_time: Option<Instant>,
        duration: StdDuration,
        domain_id: DomainId,
        data: Option<Arc<Vec<u8>>>,
    ) -> RtpsResult<()> {
        let start = Instant::now();
        match data {
            Some(ref data) => {
                // Send via multicast (UDP)
                if let Some(ref sender) = self.sender {
                    let _ = sender.send_multicast(domain_id, &data);
                    log::debug!("discovery multicast packet send");
                } else {
                    log::debug!("UDP sender not available, skipping SPDP multicast");
                }

                // Also send to initial peers via TCP (if configured and in TCP/Hybrid mode)
                self.send_spdp_to_initial_peers(&data);
            }
            None => {
                log::error!("spdp message is not set");
            }
        }

        // Calculate remaining duration for timer
        let elapsed = start.elapsed();
        let remaining_duration = match start_time {
            Some(start_time) => {
                let elapsed = start_time.elapsed();
                duration.checked_sub(elapsed).unwrap_or(StdDuration::from_millis(0))
            }
            None => duration.checked_sub(elapsed).unwrap_or(StdDuration::from_millis(0)),
        };

        // Generate unique timer ID
        let timer_id = format!(
            "spdp_multicast_{}_{}_{}",
            domain_id,
            start.elapsed().as_nanos(),
            rand::random::<u32>()
        );

        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;
        let data_arc = Arc::new(data);
        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                remaining_duration,
                false, // one-shot timer
                {
                    let participant = participant.clone();
                    let data_arc = data_arc.clone();
                    move || {
                        let sending_handler =
                            SendingHandler::get_instance(participant.clone(), None, None);
                        sending_handler.push_message_and_wake(MessageType::SpdpMulticast(
                            Some(Instant::now()),
                            duration,
                            domain_id,
                            (*data_arc).clone(),
                        ));
                    }
                },
            );
        } else {
            log::error!("Failed to acquire timer handler lock for SPDP multicast sending");
        }

        Ok(())
    }

    pub(crate) fn init_spdp_multicast(&self) -> RtpsResult<Option<Arc<Vec<u8>>>> {
        // Create broadcasting rtps message for SPDP

        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;

        let data = match MessageCreator::create_spdp_msg(participant.clone()) {
            Ok(rtps_message) => {
                match rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian) {
                    Ok(data) => Some(Arc::new(data)),
                    Err(e) => {
                        log::error!("Failed to write SPDP message: {:?}", e);
                        None
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to create SPDP message: {:?}", e);
                None
            }
        };

        Ok(data)
    }

    /// Send SPDP message to initial peers via TCP unicast
    /// Only works in TCP or Hybrid transport modes
    pub(crate) fn send_spdp_to_initial_peers(&self, data: &[u8]) {
        // Check transport mode - only send to initial peers in TCP or Hybrid mode
        let transport_type = crate::rtps::transport::get_transport_type();

        if transport_type != TransportType::TCP && transport_type != TransportType::Hybrid {
            return; // Skip for UDP-only mode
        }

        // Skip if no initial peers configured
        if self.initial_peers.is_empty() {
            log::warn!("[SPDP] No initial peers configured! TCP discovery will not work.");
            return;
        }

        log::info!(
            "[SPDP] Sending to {} initial peers: {:?}",
            self.initial_peers.len(),
            self.initial_peers
        );

        // Send to each initial peer via TCP
        if let Some(ref tcp_sender) = self.tcp_sender {
            for peer_addr in &self.initial_peers {
                match tcp_sender.send(peer_addr, data) {
                    Ok(_bytes_sent) => {
                        // log::debug!(
                        //     "[SPDP] Successfully sent {} bytes to initial peer {:?}",
                        //     _bytes_sent,
                        //     peer_addr
                        // );
                    }
                    Err(_e) => {
                        // log::error!(
                        //     "[SPDP] Failed to send SPDP to initial peer {:?}: {:?}",
                        //     peer_addr,
                        //     _e
                        // );
                    }
                }
            }
        } else {
            log::error!("[SPDP] TCP sender NOT available! Cannot send SPDP to initial peers.");
        }
    }

    pub(crate) fn start_spdp(&self) -> RtpsResult<()> {
        self.trigger_send_spdp_multicast()
    }

    // Trigger SPDP multicast transmission
    pub(crate) fn trigger_send_spdp_multicast(&self) -> RtpsResult<()> {
        // Get necessary data
        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;

        let domain_id = participant.domain_id();
        let heartbeat_period = match participant.spdp_builtin_participant_writer().lock() {
            Ok(writer) => writer.heartbeat_period(),
            Err(e) => {
                log::error!("Failed to acquire spdp builtin participant writer lock: {}", e);
                RtpsDuration::from_seconds_f64(2.0)
            }
        };
        // Actually request SPDP multicast transmission
        let sending_handler = SendingHandler::get_instance(participant.clone(), None, None);
        sending_handler.push_message_and_wake(MessageType::SpdpMulticast(
            None,
            heartbeat_period.to_std_duration(),
            domain_id,
            self.init_spdp_multicast()?,
        ));

        Ok(())
    }

    // Trigger SEDP
    fn trigger_send_sedp_message(
        &self,
        spdp_discovered_participant_data: Arc<SPDPDiscoveredParticipantData>,
    ) -> RtpsResult<()> {
        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;

        // Get necessary data
        let heartbeat_period = match participant.spdp_builtin_participant_writer().lock() {
            Ok(writer) => writer.heartbeat_period(),
            Err(e) => {
                log::error!("Failed to acquire spdp builtin participant writer lock: {}", e);
                RtpsDuration::from_seconds_f64(2.0)
            }
        };
        let sending_handler = SendingHandler::get_instance(participant.clone(), None, None);
        // sending_handler.push_message_and_wake(MessageType::Sedp(
        //     heartbeat_period.to_std_duration(),
        //     spdp_discovered_participant_data.clone(),
        //     self.init_spdp_multicast(),
        // ));
        sending_handler.push_message_and_wake(MessageType::SedpSpdp(
            None,
            heartbeat_period.to_std_duration(),
            spdp_discovered_participant_data.clone(),
            self.init_spdp_multicast()?,
        ));
        sending_handler.push_message_and_wake(MessageType::SedpPublication(
            None,
            heartbeat_period.to_std_duration(),
            Arc::new(spdp_discovered_participant_data.guid_prefix()),
        ));
        sending_handler.push_message_and_wake(MessageType::SedpSubscription(
            None,
            heartbeat_period.to_std_duration(),
            Arc::new(spdp_discovered_participant_data.guid_prefix()),
        ));
        // sending_handler.push_message_and_wake(MessageType::SedpTopic(
        //     None,
        //     heartbeat_period.to_std_duration().clone(),
        //     spdp_discovered_participant_data.clone(),
        // ));
        // Send initial P2P heartbeat (first=1, last=0, count=1) to indicate empty cache
        if let Some(wlp_logic) = participant.wlp_logic() {
            if let Err(e) = wlp_logic.send_liveliness_heartbeat(
                false,
                false,
                None,
                Some(spdp_discovered_participant_data.guid_prefix()),
            ) {
                log::error!("Failed to send initial P2P heartbeat: {}", e);
            }
        }

        Ok(())
    }

    /// Method to notify the network that the Participant has been terminated after deleting my Participant
    pub(crate) fn send_participant_termination_message_multicast(&self) -> RtpsResult<()> {
        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;

        if let Ok(rtps_message) =
            MessageCreator::create_spdp_msg_with_inline_qos(participant.clone())
        {
            let buffer = rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian);
            if let Ok(buffer) = buffer {
                if let Some(ref sender) = self.sender {
                    let _ = sender.send_multicast(participant.domain_id(), &buffer);
                    log::debug!("discovery multicast packet send");
                } else {
                    log::debug!("UDP sender not available, skipping SPDP termination multicast");
                }
            } else {
                log::error!("Failed to serialize SPDP message with inline qos");
            }
        }

        Ok(())
    }

    /// Method to handle remote participant termination
    pub(crate) fn handle_participant_termination_message(
        &self,
        terminated_participant_guid: &Guid,
    ) -> RtpsResult<()> {
        // Cancel liveliness monitoring before unmatch to prevent spurious LOST events
        if let Ok(monitor) = self.participant_monitor.lock() {
            if let Some(monitor) = monitor.as_ref() {
                monitor.cancel_writer(*terminated_participant_guid);
            }
        }

        let participant = self
            .participant
            .upgrade()
            .ok_or(RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped"))?;

        participant.unmatch_with_remote_participant(terminated_participant_guid);

        Ok(())
    }

    pub(crate) fn handle_participant_liveliness(
        &self,
        spdp_discovered_participant_data: SPDPDiscoveredParticipantData,
    ) -> RtpsResult<()> {
        let participant_guid = spdp_discovered_participant_data.participant_guid();
        if let Ok(mut monitor) = self.participant_monitor.lock() {
            if monitor.is_none() {
                let participant = self.participant.upgrade().ok_or(RtpsError::new(
                    RtpsErrorCode::ArcUpgradeError,
                    "Participant already dropped",
                ))?;
                let callback = Arc::new(move |participant_guid: Guid| {
                    Self::handle_participant_lost(participant.clone(), &participant_guid)
                });
                *monitor = Some(LivelinessMonitor::new(callback));
            }

            if let Some(monitor) = monitor.as_ref() {
                monitor.track_writer(
                    &participant_guid,
                    spdp_discovered_participant_data.lease_duration().into(),
                );
            }
        }

        Ok(())
    }

    fn handle_participant_lost(participant: Arc<Participant>, guid: &Guid) -> bool {
        participant.unmatch_with_remote_participant(guid);
        true
    }

    /// Shutdown participant liveliness monitor and join its thread
    pub(crate) fn shutdown_liveliness_monitor(&self) {
        if let Ok(mut monitor) = self.participant_monitor.lock() {
            if let Some(ref mut m) = *monitor {
                m.shutdown();
            }
            *monitor = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use crate::rtps::entities::participant::Participant;
    use crate::rtps::logic::spdp_logic::SpdpLogic;
    use crate::rtps::task::sending_handler::SendingHandler;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_send_spdp_multicast() {
        let domain_id = 10;
        let mut socket = Socket::new(domain_id);
        socket.create_socket();
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ip()));

        // socket.create_sender();
        let _ = SendingHandler::get_instance(
            participant.clone(),
            Some(socket.sender()),
            socket.tcp_sender(),
        );

        let spdp_logic = SpdpLogic::new(
            participant.clone(),
            Some(socket.sender()),
            socket.tcp_sender(),
            Vec::new(), // No initial peers for test
        );
        spdp_logic.trigger_send_spdp_multicast().unwrap();

        // Code to verify if it sends multiple times
        thread::sleep(std::time::Duration::from_secs(100));
    }
}
