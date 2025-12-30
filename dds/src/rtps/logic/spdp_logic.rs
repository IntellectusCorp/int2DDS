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

use crate::rtps::{
    common::{
        guid::Guid,
        rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult},
        time::RtpsDuration,
        types::DomainId,
    },
    entities::{participant::Participant, writer::Writer},
    logic::common::{impl_participant_accessor, ParticipantAccessor},
    logic::message_processor::participant_message_processor::ParticipantMessageProcessor,
    messages::message_creator::MessageCreator,
    task::{
        sending_handler::{MessageType, SendingHandler},
        timer_handler::TimerHandler,
    },
    transport::{Transport, TransportSender, TransportType},
};

#[derive(Clone)]
pub(crate) struct SpdpLogic {
    participant: Weak<Participant>,
    sender: Option<Arc<TransportSender>>,
    tcp_sender: Option<Arc<TransportSender>>,
    initial_peers: Vec<std::net::SocketAddr>,
    timer_handler: Arc<Mutex<TimerHandler>>,
}

impl_participant_accessor!(SpdpLogic);

impl ParticipantMessageProcessor for SpdpLogic {}

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
        }
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
        let sending_handler = SendingHandler::get_instance(participant.clone(), None, None);
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
                // Send via multicast (UDP)
                if let Some(ref sender) = self.sender {
                    let _ = sender.send_multicast(domain_id, data);
                    log::debug!("discovery multicast packet send");
                } else {
                    log::debug!("UDP sender not available, skipping SPDP multicast");
                }

                // Also send to initial peers via TCP (if configured and in TCP/Hybrid mode)
                self.send_spdp_to_initial_peers(data);
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

        let data_arc = Arc::new(data);
        if let Ok(timer_handler) = self.timer_handler.lock() {
            timer_handler.add_timer(
                timer_id,
                remaining_duration,
                false, // one-shot timer
                {
                    let participant = self.get_upgraded_participant()?;
                    let data_arc = data_arc.clone();
                    move || {
                        let sending_handler =
                            SendingHandler::get_instance(participant.clone(), None, None);
                        sending_handler.push_message_and_wake(
                            MessageType::PeriodicParticipantDataMulticast(
                                Some(Instant::now()),
                                duration,
                                domain_id,
                                (*data_arc).clone(),
                            ),
                        );
                    }
                },
            );
        } else {
            log::error!("Failed to acquire timer handler lock for SPDP multicast sending");
        }

        Ok(())
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

    /// Method to notify the network that the Participant has been terminated after deleting my Participant
    pub(crate) fn send_participant_termination_message_multicast(&self) -> RtpsResult<()> {
        let participant = self.get_upgraded_participant()?;

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

    use crate::rtps::entities::participant::Participant;
    use crate::rtps::logic::spdp_logic::SpdpLogic;
    use crate::rtps::task::sending_handler::SendingHandler;
    use crate::rtps::transport::socket::Socket;
    use crate::test_utils::unique_domain_id;

    #[test]
    #[ignore]
    fn test_send_spdp_multicast() {
        let domain_id = unique_domain_id() as u32;
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
