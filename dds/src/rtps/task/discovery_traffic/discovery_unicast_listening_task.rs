use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crossbeam_channel::Receiver;
use log::{debug, info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) struct DiscoveryUnicastListeningTask {
    guid_prefix: GuidPrefix,
    discovery_unicast_listener: Option<UdpListener>,
    tcp_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
    participant: Weak<Participant>,
    sedp_logic: Arc<Option<SedpLogic>>,
}

impl DiscoveryUnicastListeningTask {
    pub(crate) fn new(
        discovery_unicast_listener: Option<UdpListener>,
        tcp_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
        participant: Arc<Participant>,
    ) -> Self {
        let (_, sedp_logic, _) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self {
            guid_prefix,
            discovery_unicast_listener,
            tcp_rx,
            participant: Arc::downgrade(&participant),
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

        let participant = self.participant.upgrade().ok_or_else(|| {
            RtpsError::new(RtpsErrorCode::ArcUpgradeError, "Participant already dropped")
        })?;
        let mut sedp_logic = self
            .sedp_logic
            .as_ref()
            .as_ref()
            .ok_or_else(|| {
                RtpsError::new(RtpsErrorCode::DataNotSet, "SpdpLogic is not initialized")
            })?
            .clone();

        sedp_logic.handle_rtps_message(message_receiver.clone())?;
        if let Some(mut wlp_logic) = participant.wlp_logic() {
            wlp_logic.handle_rtps_message(message_receiver)?;
        }
        Ok(())
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
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ips()));

        let mut discovery_unicast_listening_task = DiscoveryUnicastListeningTask::new(
            socket.discovery_unicast_listener(),
            socket.tcp_discovery_rx(),
            participant,
        );
        let _ = discovery_unicast_listening_task.unicast_listening();
    }
}
