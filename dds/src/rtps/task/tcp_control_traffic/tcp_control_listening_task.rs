#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::messages::tcp_control_message::TcpControlMessage;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tcp::framing::write_framed_message;
use crate::rtps::transport::tcp::tcp_handshake::{HandshakeResult, TcpHandshakeManager};
use crate::rtps::transport::tcp::tcp_keep_alive::TcpKeepaliveManager;
use crate::rtps::transport::tcp::tcp_listener::TcpListener;
use crate::rtps::transport::tcp::tcp_sender::TcpSender;
use crate::rtps::transport::TransportSender;

/// TCP control connection listening task
///
/// Manages the lifecycle of TCP control connections:
/// - Accepts incoming control connections
/// - Processes DDS handshake (Handshake / HandshakeAck)
/// - Handles keepalive (Keepalive / KeepaliveAck)
/// - Handles connection close (Close)
///
/// Runs in its own thread, following the same pattern as
/// DiscoveryUnicastListeningTask and UserUnicastListeningTask.
pub(crate) struct TcpControlListeningTask {
    /// Control TCP listener (accepts incoming control connections)
    control_listener: Option<TcpListener>,

    /// DDS handshake state machine
    handshake_manager: TcpHandshakeManager,

    /// Keepalive state machine
    keepalive_manager: TcpKeepaliveManager,

    /// TCP sender for establishing data connections after handshake
    tcp_sender: Option<Arc<TransportSender>>,

    /// Participant reference (for termination check)
    participant: Weak<Participant>,
}

impl TcpControlListeningTask {
    pub(crate) fn new(
        control_listener: Option<TcpListener>,
        tcp_sender: Option<Arc<TransportSender>>,
        participant: Arc<Participant>,
        local_data_port: u16,
    ) -> Self {
        let guid_prefix = participant.guid().prefix();
        let domain_id = participant.domain_id();

        Self {
            control_listener,
            handshake_manager: TcpHandshakeManager::new(guid_prefix, domain_id, local_data_port),
            keepalive_manager: TcpKeepaliveManager::new(),
            tcp_sender,
            participant: Arc::downgrade(&participant),
        }
    }

    /// Main event loop for TCP control connection management
    pub(crate) fn control_listening(&mut self) -> io::Result<()> {
        info!("[TcpControl] start control listening");

        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        // Register control TCP listener
        let listener_token = if let Some(listener) = &mut self.control_listener {
            let port = listener.port();
            if let Some(socket) = listener.socket_mut() {
                let token = Token(port as usize + 20000); // Offset to avoid collision
                poll.registry().register(socket, token, Interest::READABLE)?;
                info!("[TcpControl] Control listener registered on port {}", port);
                Some(token)
            } else {
                None
            }
        } else {
            None
        };

        if listener_token.is_none() {
            return Err(io::Error::other("No control TCP listener is set"));
        }

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| io::Error::other("Participant already dropped"))?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    warn!("[TcpControl] Poll interrupted, continuing: {}", e);
                    continue;
                }
                Err(e) => return Err(e),
            }

            // Check termination
            if participant.is_terminated() {
                debug!("[TcpControl] Detected termination flag, exiting...");
                self.deregister_all(&poll);
                return Ok(());
            }

            // Collect control messages from TCP connections
            let mut control_messages: Vec<(Vec<u8>, SocketAddr)> = Vec::new();

            for event in events.iter() {
                debug!(
                    "[TcpControl] Event: token={:?}, readable={}",
                    event.token(),
                    event.is_readable()
                );

                // Handle new control connections
                if Some(event.token()) == listener_token && event.is_readable() {
                    if let Some(listener) = &mut self.control_listener {
                        while let Ok(Some(addr)) = listener.accept(Some(poll.registry())) {
                            info!("[TcpControl] Accepted control connection from {:?}", addr);
                        }
                    }
                }

                // Handle data from existing control connections
                let is_connection = self
                    .control_listener
                    .as_ref()
                    .map(|l| l.is_connection_token(event.token()))
                    .unwrap_or(false);

                if is_connection && event.is_readable() {
                    if let Some(listener) = &mut self.control_listener {
                        if let Some(addr) = listener.get_addr_from_token(event.token()) {
                            // Edge-triggered: drain all available data
                            loop {
                                match listener.read_framed_message(&addr) {
                                    Ok(Some(buffer)) => {
                                        debug!(
                                            "[TcpControl] Received control message from {:?}, {} bytes",
                                            addr,
                                            buffer.len()
                                        );
                                        control_messages.push((buffer, addr));
                                    }
                                    Ok(None) => {
                                        continue;
                                    }

                                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                        break;
                                    }
                                    Err(ref e)
                                        if e.kind() == io::ErrorKind::UnexpectedEof
                                            || e.kind() == io::ErrorKind::ConnectionReset =>
                                    {
                                        warn!(
                                            "[TcpControl] Control connection from {:?} closed: {}",
                                            addr, e
                                        );
                                        listener.remove_connection(&addr);
                                        self.handshake_manager.remove(&addr);
                                        self.keepalive_manager.remove(&addr);
                                        break;
                                    }
                                    Err(e) => {
                                        error!("[TcpControl] Error reading from {:?}: {}", addr, e);
                                        listener.remove_connection(&addr);
                                        self.handshake_manager.remove(&addr);
                                        self.keepalive_manager.remove(&addr);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Process collected control messages
            for (buffer, from_addr) in control_messages {
                if let Err(e) = self.process_control_message(&buffer, from_addr) {
                    warn!(
                        "[TcpControl] Failed to process control message from {:?}: {}",
                        from_addr, e
                    );
                }
            }

            // Periodic tasks: keepalive send + dead check + handshake timeout
            self.periodic_tasks();
        }
    }

    /// Process a single TCP control message
    fn process_control_message(&mut self, buffer: &[u8], from_addr: SocketAddr) -> io::Result<()> {
        let msg = TcpControlMessage::deserialize(buffer)?;

        match msg {
            TcpControlMessage::Handshake(_) | TcpControlMessage::HandshakeAck(_) => {
                // Get the stream to write response (HandshakeAck)
                let listener = self.control_listener.as_mut().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotConnected, "Control listener not available")
                })?;
                let stream = listener.get_connection_mut(&from_addr).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        format!("No connection for {:?}", from_addr),
                    )
                })?;

                match self.handshake_manager.handle_message(from_addr, buffer, stream)? {
                    HandshakeResult::Completed(remote_data) => {
                        info!(
                            "[TcpControl] Handshake completed with {:?}, remote data_port={}",
                            from_addr, remote_data.data_port
                        );
                        // Register for keepalive monitoring
                        self.keepalive_manager.register(from_addr);

                        // Establish data connection to remote's data port
                        let data_addr = SocketAddr::new(from_addr.ip(), remote_data.data_port);
                        if let Some(sender) = &self.tcp_sender {
                            if let TransportSender::Tcp(tcp_sender) = sender.as_ref() {
                                if let Err(e) = tcp_sender.connect_data(&data_addr) {
                                    warn!(
                                        "[TcpControl] Failed to connect data to {:?}: {}",
                                        data_addr, e
                                    );
                                }
                            }
                        }
                    }
                    HandshakeResult::InProgress => {
                        debug!("[TcpControl] Handshake in progress with {:?}", from_addr);
                    }
                    HandshakeResult::TimedOut(addr) => {
                        warn!("[TcpControl] Handshake timed out with {:?}", addr);
                        if let Some(listener) = &mut self.control_listener {
                            listener.remove_connection(&addr);
                        }
                    }
                }
            }

            TcpControlMessage::Keepalive => {
                let listener = self.control_listener.as_mut().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotConnected, "Control listener not available")
                })?;
                let stream = listener.get_connection_mut(&from_addr).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotConnected,
                        format!("No connection for {:?}", from_addr),
                    )
                })?;

                self.keepalive_manager.handle_keepalive(from_addr, stream)?;
            }

            TcpControlMessage::KeepaliveAck => {
                self.keepalive_manager.handle_keepalive_ack(&from_addr);
            }

            TcpControlMessage::Close => {
                info!("[TcpControl] Received Close from {:?}", from_addr);
                self.cleanup_connection(&from_addr);
            }
        }
        Ok(())
    }

    /// Periodic tasks: send keepalives, check dead connections, check handshake timeouts
    fn periodic_tasks(&mut self) {
        // Send pending keepalives
        if let Some(listener) = &mut self.control_listener {
            for addr in self.keepalive_manager.pending_keepalives() {
                if let Some(stream) = listener.get_connection_mut(&addr) {
                    let msg = TcpControlMessage::Keepalive;
                    match msg.serialize() {
                        Ok(bytes) => match write_framed_message(stream, &bytes) {
                            Ok(()) => {
                                self.keepalive_manager.mark_sent(&addr);
                            }
                            Err(e) => {
                                warn!("[TcpControl] Failed to send keepalive to {:?}: {}", addr, e);
                                // Connection broken, will be caught by check_dead
                            }
                        },
                        Err(e) => {
                            error!("[TcpControl] Failed to serialize keepalive: {}", e);
                        }
                    }
                }
            }
        }

        // Check for dead connections
        let dead_addrs = self.keepalive_manager.check_dead();
        for addr in dead_addrs {
            warn!("[TcpControl] Dead connection detected: {:?}", addr);
            self.cleanup_connection(&addr);
        }

        // Check for handshake timeouts
        let timed_out = self.handshake_manager.check_timeouts();
        for addr in timed_out {
            warn!("[TcpControl] Handshake timeout: {:?}", addr);
            self.cleanup_connection(&addr);
        }
    }

    /// Clean up all state for a connection
    fn cleanup_connection(&mut self, addr: &SocketAddr) {
        if let Some(listener) = &mut self.control_listener {
            listener.remove_connection(addr);
        }
        self.handshake_manager.remove(addr);
        self.keepalive_manager.remove(addr);

        // Disconnect data connection as well
        if let Some(sender) = &self.tcp_sender {
            if let TransportSender::Tcp(tcp_sender) = sender.as_ref() {
                tcp_sender.disconnect(addr);
            }
        }

        debug!("[TcpControl] Cleaned up connection {:?}", addr);
    }

    /// Deregister all sockets from poll before shutdown
    fn deregister_all(&mut self, poll: &Poll) {
        if let Some(listener) = &mut self.control_listener {
            if let Some(socket) = listener.socket_mut() {
                let _ = poll.registry().deregister(socket);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require full Participant setup,
    // so unit tests for this task are limited.
    // The individual managers (handshake, keepalive) have their own unit tests.
}
