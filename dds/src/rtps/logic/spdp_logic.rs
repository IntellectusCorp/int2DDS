//! SPDP (Simple Participant Discovery Protocol) logic implementation.
//!
//! This module implements the SPDP protocol for discovering domain participants.
//! SPDP participants periodically announce themselves and process announcements
//! from remote participants to maintain the participant discovery database.

use std::{
    sync::{Arc, Mutex, Weak},
    time::{Duration as StdDuration, Instant},
};

use speedy::{Endianness, Writable};

use crate::rtps::{
    common::{
        guid::Guid,
        locator::Locator,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        time::RtpsDuration,
        types::DomainId,
    },
    entities::{entity::Entity, participant::Participant, writer::Writer},
    logic::common::{impl_participant_accessor, ParticipantAccessor},
    logic::message_processor::participant_message_processor::ParticipantMessageProcessor,
    messages::message_creator::MessageCreator,
    task::sending_handler::{MessageType, SendingHandler},
    transport::plugin::{SendTarget, TransportPlugin},
};
use crate::utils::timer::{timer_handler::TimerHandler, timer_id::TimerId};

#[derive(Clone)]
pub(crate) struct SpdpLogic {
    participant: Weak<Participant>,
    transport: Arc<dyn TransportPlugin>,
    initial_peers: Vec<std::net::SocketAddr>,
    timer_handler: Arc<Mutex<TimerHandler>>,
}

impl_participant_accessor!(SpdpLogic);

impl ParticipantMessageProcessor for SpdpLogic {}

impl SpdpLogic {
    pub(crate) fn new(
        participant: Arc<Participant>,
        transport: Arc<dyn TransportPlugin>,
        initial_peers: Vec<std::net::SocketAddr>,
    ) -> Self {
        let timer_handler = TimerHandler::get_instance(participant.guid().prefix());
        Self { participant: Arc::downgrade(&participant), transport, initial_peers, timer_handler }
    }

    pub(crate) fn start_spdp(&self) -> RtpsResult<()> {
        self.trigger_send_spdp_multicast()
    }

    pub(crate) fn is_participant_terminated(&self) -> RtpsResult<bool> {
        Ok(self.get_upgraded_participant()?.is_terminated())
    }

    // Trigger SPDP multicast transmission
    pub(crate) fn trigger_send_spdp_multicast(&self) -> RtpsResult<()> {
        // Get necessary data
        let participant = self.get_upgraded_participant()?;

        let domain_id = participant.domain_id();
        let heartbeat_period = match participant.spdp_builtin_participant_writer().lock() {
            Ok(writer) => writer.heartbeat_period(),
            Err(e) => {
                log::error!("Failed to acquire spdp builtin participant writer lock: {}", e);
                RtpsDuration::from_seconds_f64(2.0)
            }
        };
        // Actually request SPDP multicast transmission
        let sending_handler = SendingHandler::get_instance(participant.clone(), None);
        sending_handler.push_message_and_wake(MessageType::PeriodicParticipantDataMulticast(
            None,
            heartbeat_period.to_std_duration(),
            domain_id,
            self.create_spdp_message()?,
        ));

        Ok(())
    }

    // Logic to actually send the data
    // After sending, start chaining after thread sleep (because it's easier to call sending handler from thread)
    pub(crate) fn send_periodic_participant_data_multicast(
        &mut self,
        start_time: Option<Instant>,
        duration: StdDuration,
        domain_id: DomainId,
        data: Option<Arc<Vec<u8>>>,
    ) -> RtpsResult<()> {
        let start = Instant::now();
        match data {
            Some(ref data) => {
                if self.initial_peers.is_empty() {
                    let _ = self.transport.send(data, &SendTarget::MulticastDiscovery);
                    log::debug!("discovery multicast packet send");
                } else {
                    self.send_spdp_to_initial_peers(data);
                }
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

        let timer_id = TimerId::SpdpMulticast { domain_id };

        let data_arc = Arc::new(data);
        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                remaining_duration,
                false, // one-shot timer
                {
                    let participant_weak = self.participant.clone();
                    let data_arc = data_arc.clone();
                    move || {
                        if let Some(participant) = participant_weak.upgrade() {
                            if !participant.is_terminated() {
                                let sending_handler =
                                    SendingHandler::get_instance(participant, None);
                                sending_handler.push_message_and_wake(
                                    MessageType::PeriodicParticipantDataMulticast(
                                        Some(Instant::now()),
                                        duration,
                                        domain_id,
                                        (*data_arc).clone(),
                                    ),
                                );
                            }
                        }
                    }
                },
            );
        } else {
            log::error!("Failed to acquire timer handler lock for SPDP multicast sending");
        }

        Ok(())
    }

    /// Send SPDP message to initial peers via unicast.
    /// Transport plugin handles the actual mechanism (UDP/TCP).
    pub(crate) fn send_spdp_to_initial_peers(&self, data: &[u8]) {
        if self.initial_peers.is_empty() {
            return;
        }

        log::debug!(
            "[SPDP] Sending to {} initial peers: {:?}",
            self.initial_peers.len(),
            self.initial_peers
        );

        for peer_addr in &self.initial_peers {
            if let std::net::SocketAddr::V4(v4) = peer_addr {
                let locator = Locator::from_ip_v4_addr_and_port(v4.ip(), v4.port() as u32);
                match self.transport.send(data, &SendTarget::UnicastDiscovery(&locator)) {
                    Ok(_) => {
                        log::debug!("[SPDP] Sent discovery message to initial peer {}", peer_addr);
                    }
                    Err(e) => {
                        log::warn!("[SPDP] Failed to send to initial peer {}: {}", peer_addr, e);
                    }
                }
            }
        }
    }

    /// Method to notify the network that the Participant has been terminated after deleting my Participant
    pub(crate) fn send_participant_termination_message_multicast(&self) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

        if let Ok(rtps_message) =
            MessageCreator::create_spdp_msg_with_inline_qos(participant.clone())
        {
            let buffer = rtps_message.write_to_vec_with_ctx(Endianness::LittleEndian);
            if let Ok(buffer) = buffer {
                if self.initial_peers.is_empty() {
                    let _ = self.transport.send(&buffer, &SendTarget::MulticastDiscovery);
                    log::debug!("discovery multicast packet send");
                } else {
                    self.send_spdp_to_initial_peers(&buffer);
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
        let participant = self.get_upgraded_participant()?;

        // Cancel liveliness monitoring before unmatch to prevent spurious LOST events
        if let Ok(monitor) = participant.liveliness_monitor().lock() {
            if let Some(monitor) = monitor.as_ref() {
                monitor.cancel_writer(*terminated_participant_guid);
            }
        }

        participant.unmatch_with_remote_participant(terminated_participant_guid);

        Ok(())
    }

    pub(crate) fn create_spdp_message(&self) -> RtpsResult<Option<Arc<Vec<u8>>>> {
        // Create extended discovery rtps message for SPDP
        let data = match MessageCreator::create_spdp_msg(self.get_upgraded_participant()?.clone()) {
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use crate::common::env::{get_network_interface, get_network_ip};
    use crate::rtps::entities::participant::Participant;
    use crate::rtps::logic::spdp_logic::SpdpLogic;
    use crate::rtps::transport::plugin::TransportPluginFactory;
    use crate::rtps::transport::{get_transport_type, socket::Socket};
    use crate::test_utils::unique_domain_id;

    #[test]
    #[ignore]
    fn test_send_spdp_multicast() {
        let domain_id = unique_domain_id() as u32;
        let mut socket = Socket::new(domain_id);
        let transport = TransportPluginFactory::create(
            get_transport_type(),
            domain_id,
            socket.participant_id(),
            get_network_ip().unwrap_or_default(),
            get_network_interface().unwrap_or_default(),
            socket.working_ips().iter().map(|ip| ip.to_string()).collect(),
        )
        .expect("Failed to create transport plugin");
        socket.set_transport(Arc::from(transport));

        let participant = Arc::new(Participant::new(
            domain_id,
            socket.participant_id(),
            socket.working_ips(),
            None,
        ));

        let spdp_logic = SpdpLogic::new(
            participant.clone(),
            socket.transport(),
            Vec::new(), // No initial peers for test
        );
        spdp_logic.trigger_send_spdp_multicast().unwrap();

        // Code to verify if it sends multiple times
        thread::sleep(std::time::Duration::from_secs(100));
    }
}
