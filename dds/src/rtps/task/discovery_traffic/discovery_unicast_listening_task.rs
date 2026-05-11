use bytes::Bytes;

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::rtps_error_code::{RtpsError, RtpsErrorCode, RtpsResult};
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::sedp_logic::SedpLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tcp::tcp_listener::TcpListener;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token, Waker};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

const SHUTDOWN_WAKE_TOKEN: Token = Token(usize::MAX - 1);

pub(crate) struct DiscoveryUnicastListeningTask {
    guid_prefix: GuidPrefix,
    discovery_unicast_listener: Option<UdpListener>,
    tcp_listener: Option<TcpListener>,
    participant: Weak<Participant>,
    sedp_logic: Arc<Option<SedpLogic>>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl DiscoveryUnicastListeningTask {
    pub(crate) fn new(
        discovery_unicast_listener: Option<UdpListener>,
        tcp_listener: Option<TcpListener>,
        participant: Arc<Participant>,
    ) -> Self {
        let (_, sedp_logic, _) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self {
            guid_prefix,
            discovery_unicast_listener,
            tcp_listener,
            participant: Arc::downgrade(&participant),
            sedp_logic,
            shutdown_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn unicast_listening(&mut self) -> std::io::Result<()> {
        info!("start discovery unicast listening");
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), SHUTDOWN_WAKE_TOKEN)?);
        let _ = self.shutdown_waker.set(waker);

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

        // Register TCP listener if present
        let tcp_token = if let Some(tcp_listener) = &mut self.tcp_listener {
            let port = tcp_listener.port();
            if let Some(socket) = tcp_listener.socket_mut() {
                // Use a different token range for TCP (add 10000 to avoid collision)
                let token = Token(port as usize + 10000);
                poll.registry().register(socket, token, Interest::READABLE)?;
                info!("[DiscoveryUnicast] TCP listener registered on port {}", port);
                Some(token)
            } else {
                None
            }
        } else {
            None
        };

        if udp_token.is_none() && tcp_token.is_none() {
            return Err(std::io::Error::other("No listener (UDP or TCP) is set"));
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
                // deregister before return
                if let Some(listener) = &mut self.discovery_unicast_listener {
                    let _ = poll.registry().deregister(listener.socket());
                }
                if let Some(tcp_listener) = &mut self.tcp_listener {
                    if let Some(socket) = tcp_listener.socket_mut() {
                        let _ = poll.registry().deregister(socket);
                    }
                }
                return Ok(());
            }

            // Collect messages first to avoid borrow checker issues
            let mut messages_to_process: Vec<(Bytes, SocketAddr)> = Vec::new();

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
                                // deregister before return
                                if let Some(listener) = &mut self.discovery_unicast_listener {
                                    let _ = poll.registry().deregister(listener.socket());
                                }
                                if let Some(tcp_listener) = &mut self.tcp_listener {
                                    if let Some(socket) = tcp_listener.socket_mut() {
                                        let _ = poll.registry().deregister(socket);
                                    }
                                }
                                return Ok(());
                            }

                            messages_to_process.push((buffer, from_addr));
                        }
                    }
                }

                // Handle TCP listener events (new connections)
                if Some(event.token()) == tcp_token && event.is_readable() {
                    if let Some(tcp_listener) = &mut self.tcp_listener {
                        // Accept new connections and register them with poll
                        while let Ok(Some(addr)) = tcp_listener.accept(Some(poll.registry())) {
                            info!("[DiscoveryUnicast] Accepted and registered TCP connection from {:?}", addr);
                        }
                    }
                }

                // Handle TCP connection events (data ready to read)
                // Check if this token belongs to a TCP connection using token_to_addr map
                let is_tcp_connection = self
                    .tcp_listener
                    .as_ref()
                    .map(|l| l.is_connection_token(event.token()))
                    .unwrap_or(false);

                if is_tcp_connection && event.is_readable() {
                    if let Some(tcp_listener) = &mut self.tcp_listener {
                        // Get the address for this token
                        if let Some(addr) = tcp_listener.get_addr_from_token(event.token()) {
                            // Edge-triggered mode: Read ALL available data until WouldBlock
                            loop {
                                match tcp_listener.read_framed_message(&addr) {
                                    Ok(Some(buffer)) => {
                                        info!("[DiscoveryUnicast] Received TCP message from {:?}, {} bytes", addr, buffer.len());
                                        messages_to_process.push((Bytes::from(buffer), addr));
                                        // Continue reading more messages
                                    }
                                    Ok(None) => {
                                        // CRITICAL: In edge-triggered mode, MUST continue reading!
                                        // Ok(None) means incomplete message in buffer, but socket might have more data.
                                        // If we break here and socket is still readable, no new poll event will fire.
                                        // Keep trying until we get WouldBlock to drain the socket completely.
                                        continue;
                                    }
                                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                        // No more data available, break the loop
                                        break;
                                    }
                                    Err(ref e)
                                        if e.kind() == std::io::ErrorKind::UnexpectedEof
                                            || e.kind() == std::io::ErrorKind::ConnectionReset =>
                                    {
                                        warn!(
                                            "[DiscoveryUnicast] TCP connection from {:?} closed: {}",
                                            addr, e
                                        );
                                        tcp_listener.remove_connection(&addr);
                                        break;
                                    }
                                    Err(e) => {
                                        error!(
                                            "[DiscoveryUnicast] Error reading TCP from {:?}: {}",
                                            addr, e
                                        );
                                        tcp_listener.remove_connection(&addr);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Process all collected messages
            for (bytes, from_addr) in messages_to_process {
                let _ = self.process_rtps_message(bytes, from_addr);
            }
        }
    }

    fn process_rtps_message(&mut self, bytes: Bytes, from_addr: SocketAddr) -> RtpsResult<()> {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
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
    use crate::rtps::transport::TransportConfig;

    #[test]
    #[ignore]
    fn test_discovery_unicast_receive() {
        let domain_id: DomainId = 17;
        let mut socket = Socket::new(domain_id, TransportConfig::default()); //domain_id 0
        socket.create_socket();
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ips()));

        //discovery multicast port : 7400
        //discovery unicast port : 7410
        //user traffic multicast port : 7401
        //user traffic unicast port : 7411
        let mut discovery_unicast_listening_task = DiscoveryUnicastListeningTask::new(
            socket.discovery_unicast_listener(),
            socket.discovery_tcp_listener(),
            participant,
        );
        let _ = discovery_unicast_listening_task.unicast_listening();
    }
}
