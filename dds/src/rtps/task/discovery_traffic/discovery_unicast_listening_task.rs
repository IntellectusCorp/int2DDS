use crate::common::instance_handle::InstanceHandle;
use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::participant_message_processor::ParticipantMessageProcessor as _;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crate::serialize::pl_cdr::InlineQosParameters;
use crossbeam_channel::Receiver;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) struct DiscoveryUnicastListeningTask {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    discovery_unicast_listener: Option<UdpListener>,
    tcp_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
    participant: Weak<Participant>,
    spdp_logic: Arc<Option<SpdpLogic>>,
    sedp_logic: Arc<Option<SedpLogic>>,
}

impl DiscoveryUnicastListeningTask {
    pub(crate) fn new(
        discovery_unicast_listener: Option<UdpListener>,
        tcp_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
        participant: Arc<Participant>,
    ) -> Self {
        let (spdp_logic, sedp_logic, _) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        let domain_id = participant.domain_id();
        Self {
            guid_prefix,
            domain_id,
            discovery_unicast_listener,
            tcp_rx,
            participant: Arc::downgrade(&participant),
            spdp_logic,
            sedp_logic,
        }
    }

    pub(crate) fn unicast_listening(&mut self) -> std::io::Result<()> {
        info!("start discovery unicast listening");
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(MAX_EVENTS);

        // Register UDP listener if present
        let udp_token = if let Some(listener) = &mut self.discovery_unicast_listener {
            let token = Token(listener.socket().local_addr().unwrap().port() as usize);
            poll.registry().register(listener.socket(), token, Interest::READABLE)?;
            info!(
                "[DiscoveryUnicast] UDP listener registered on port {}",
                listener.socket().local_addr().unwrap().port()
            );
            Some(token)
        } else {
            None
        };

        let has_tcp_rx = self.tcp_rx.is_some();
        if has_tcp_rx {
            info!("[DiscoveryUnicast] TCP channel receiver enabled");
        }

        if udp_token.is_none() && !has_tcp_rx {
            return Err(std::io::Error::other("No listener (UDP or TCP channel) is set"));
        }

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!(
                        "Poll interrupted in discovery unicast listening task, continuing: {}",
                        e
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }

            if participant.is_terminated() {
                debug!("Detected global termination flag, discovery unicast listening loop is terminating...");
                if let Some(listener) = &mut self.discovery_unicast_listener {
                    let _ = poll.registry().deregister(listener.socket());
                }
                return Ok(());
            }

            // Collect messages first to avoid borrow checker issues
            let mut messages_to_process: Vec<(Vec<u8>, SocketAddr)> = Vec::new();

            for event in events.iter() {
                debug!(
                    "[DiscoveryUnicast] Event: token={:?}, readable={}, writable={}",
                    event.token(),
                    event.is_readable(),
                    event.is_writable()
                );

                // Handle UDP events
                if Some(event.token()) == udp_token && event.is_readable() {
                    if let Some(listener) = &mut self.discovery_unicast_listener {
                        while let Some((buffer, from_addr)) = listener.get_message() {
                            if participant.is_terminated() {
                                debug!("Detected global termination flag during UDP processing");
                                if let Some(listener) = &mut self.discovery_unicast_listener {
                                    let _ = poll.registry().deregister(listener.socket());
                                }
                                return Ok(());
                            }

                            messages_to_process.push((buffer.to_vec(), from_addr));
                        }
                    }
                }
            }

            // Drain TCP channel (non-blocking)
            if let Some(tcp_rx) = &self.tcp_rx {
                while let Ok((buffer, from_addr)) = tcp_rx.try_recv() {
                    if participant.is_terminated() {
                        return Ok(());
                    }
                    messages_to_process.push((buffer, from_addr));
                }
            }

            // Process all collected messages
            for (buffer, from_addr) in messages_to_process {
                let _ = self.process_rtps_message(&buffer, from_addr);
            }
        }
    }

    fn process_rtps_message(&mut self, buffer: &[u8], from_addr: SocketAddr) -> RtpsResult<()> {
        use bytes::Bytes;

        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        let bytes = Bytes::copy_from_slice(buffer);
        let rtps_message = message_receiver.init(&bytes)?;

        // Ignore messages sent by myself
        if rtps_message.header.guid_prefix() == self.guid_prefix {
            debug!(
                "[DiscoveryUnicast] Filtering out self-sent message - guid_prefix: {:?}",
                self.guid_prefix
            );
            return Ok(());
        }

        // Check if this is an SPDP message (TCP mode sends SPDP via unicast)
        if self.is_spdp_message(&message_receiver) {
            let participant_proxy_data =
                message_receiver.extract_participant_proxy_data(self.domain_id);
            if let Some((participant_proxy_data, inline_qos_params)) = participant_proxy_data {
                let spdp_logic = self.spdp_logic.as_ref().as_ref().ok_or_else(|| {
                    RtpsError::new(RtpsErrorCode::DataNotSet, "SpdpLogic is not initialized")
                })?;

                let is_termination = inline_qos_params
                    .as_ref()
                    .and_then(|qos| qos.get_status_info())
                    .is_some_and(|status| status.disposed() || status.unregistered());

                if is_termination {
                    let terminated_participant_guid = inline_qos_params
                        .as_ref()
                        .and_then(|qos| qos.get_key_hash())
                        .unwrap_or_else(|| {
                            InstanceHandle::from_guid(&participant_proxy_data.participant_guid())
                        });

                    if let Err(e) = spdp_logic.handle_participant_termination_message(
                        &terminated_participant_guid.to_guid(),
                    ) {
                        error!("Failed to handle participant termination message: {:?}", e);
                    }
                } else if let Err(e) =
                    spdp_logic.handle_discovered_participant_data(participant_proxy_data)
                {
                    error!("[DiscoveryUnicast] Failed to handle SPDP data: {:?}", e);
                }
            }
            return Ok(());
        }

        // SEDP processing
        let participant = self.participant.upgrade().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped")
        })?;
        let mut sedp_logic = self
            .sedp_logic
            .as_ref()
            .as_ref()
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::DataNotSet, "SedpLogic is not initialized")
            })?
            .clone();

        sedp_logic.handle_rtps_message(message_receiver.clone())?;
        if let Some(mut wlp_logic) = participant.wlp_logic() {
            wlp_logic.handle_rtps_message(message_receiver)?;
        }
        Ok(())
    }

    /// Check if the RTPS message contains SPDP data (writer_id == SPDP_BUILTIN_PARTICIPANT_WRITER)
    fn is_spdp_message(&self, message_receiver: &MessageReceiver) -> bool {
        use crate::rtps::common::entity_id::EntityId;
        use crate::rtps::messages::message_receiver::TypedSubmessage;

        for submessage in message_receiver.parse_submessages() {
            if let TypedSubmessage::Data(_, data) = submessage {
                if data.writer_id == EntityId::SPDP_BUILTIN_PARTICIPANT_WRITER {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::common::types::DomainId;
    use crate::rtps::entities::participant::Participant;
    use crate::rtps::task::discovery_traffic::discovery_unicast_listening_task::DiscoveryUnicastListeningTask;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_discovery_unicast_receive() {
        let domain_id: DomainId = 17;
        let mut socket = Socket::new(domain_id);
        socket.create_socket();
        let participant = Arc::new(Participant::new(
            domain_id,
            socket.participant_id(),
            socket.working_ips(),
            None,
        ));

        let mut discovery_unicast_listening_task = DiscoveryUnicastListeningTask::new(
            socket.discovery_unicast_listener(),
            socket.tcp_discovery_rx(),
            participant,
        );
        let _ = discovery_unicast_listening_task.unicast_listening();
    }
}
