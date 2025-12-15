use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::shm::ShmListener;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tcp::tcp_listener::TcpListener;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) struct UserUnicastListeningTask {
    guid_prefix: GuidPrefix,
    user_unicast_listener: Option<UdpListener>,
    tcp_listener: Option<TcpListener>,
    shm_listener: Option<ShmListener>,
    participant: Weak<Participant>,
    user_logic: Arc<Option<UserLogic>>,
}

impl UserUnicastListeningTask {
    pub(crate) fn new(
        user_unicast_listener: Option<UdpListener>,
        tcp_listener: Option<TcpListener>,
        shm_listener: Option<ShmListener>,
        participant: Arc<Participant>,
    ) -> Self {
        // Extract TCP sender from participant if available (for Hybrid mode)
        // In Hybrid mode, we need to pass both UDP and TCP senders to UserLogic
        // let tcp_sender = None; // TCP sender not needed for receiving, only for sending via UserLogic
        // let user_logic = UserLogic::new(participant.clone(), Some(sender.clone()), tcp_sender);
        let (_, _, user_logic) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self {
            guid_prefix,
            user_unicast_listener,
            tcp_listener,
            shm_listener,
            participant: Arc::downgrade(&participant),
            user_logic,
        }
    }

    pub(crate) fn unicast_listening(&mut self) -> std::io::Result<()> {
        info!("start user unicast listening");
        let mut poll = Poll::new().unwrap();
        let mut events = Events::with_capacity(MAX_EVENTS);

        // Register UDP listener if present
        let udp_token = if let Some(listener) = &mut self.user_unicast_listener {
            let token = Token(listener.socket().local_addr().unwrap().port() as usize);
            poll.registry().register(listener.socket(), token, Interest::READABLE)?;
            info!(
                "[UserUnicast] UDP listener registered on port {}",
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
                info!("[UserUnicast] TCP listener registered on port {}", port);
                Some(token)
            } else {
                None
            }
        } else {
            None
        };

        let has_shm_listener = self.shm_listener.is_some();

        if udp_token.is_none() && tcp_token.is_none() && !has_shm_listener {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No listener (UDP, TCP, or SHM) is set",
            ));
        }

        if has_shm_listener {
            info!("[UserUnicast] SHM listener enabled for user data");
        }

        // Use zero timeout when SHM is enabled for immediate processing
        let poll_timeout = if has_shm_listener {
            Duration::from_nanos(0) // No wait for SHM - busy poll
        } else {
            Duration::from_millis(100)
        };

        let participant = self.participant.upgrade().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::Other, "Participant already dropped")
        })?;

        loop {
            poll.poll(&mut events, Some(poll_timeout))?;

            if participant.is_terminated() {
                debug!("Detected global termination flag, user traffic unicast listening loop is terminating...");
                // deregister before return
                if let Some(listener) = &mut self.user_unicast_listener {
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
            let mut messages_to_process: Vec<(Vec<u8>, SocketAddr)> = Vec::new();

            for event in events.iter() {
                // Handle UDP events
                if Some(event.token()) == udp_token && event.is_readable() {
                    if let Some(listener) = &mut self.user_unicast_listener {
                        while let Some((buffer, from_addr)) = listener.get_message() {
                            if participant.is_terminated() {
                                debug!("Detected global termination flag during UDP processing");
                                if let Some(listener) = &mut self.user_unicast_listener {
                                    let _ = poll.registry().deregister(listener.socket());
                                }
                                if let Some(tcp_listener) = &mut self.tcp_listener {
                                    if let Some(socket) = tcp_listener.socket_mut() {
                                        let _ = poll.registry().deregister(socket);
                                    }
                                }
                                return Ok(());
                            }

                            messages_to_process.push((buffer.to_vec(), from_addr));
                        }
                    }
                }

                // Handle TCP listener events (new connections)
                if Some(event.token()) == tcp_token && event.is_readable() {
                    if let Some(tcp_listener) = &mut self.tcp_listener {
                        // Accept new connections and register them with poll
                        while let Ok(Some(addr)) = tcp_listener.accept(Some(poll.registry())) {
                            info!(
                                "[UserUnicast] Accepted and registered TCP connection from {:?}",
                                addr
                            );
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
                                        info!("[UserUnicast] Received TCP message from {:?}, {} bytes", addr, buffer.len());
                                        messages_to_process.push((buffer, addr));
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
                                            "[UserUnicast] TCP connection from {:?} closed: {}",
                                            addr, e
                                        );
                                        tcp_listener.remove_connection(&addr);
                                        break;
                                    }
                                    Err(e) => {
                                        error!(
                                            "[UserUnicast] Error reading TCP from {:?}: {}",
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

            // Poll SHM listener for messages (SHM doesn't use mio events)
            let mut shm_had_data = false;
            if let Some(shm_listener) = &mut self.shm_listener {
                while let Some((buffer, from_addr)) = shm_listener.get_message() {
                    if participant.is_terminated() {
                        return Ok(());
                    }
                    messages_to_process.push((buffer.to_vec(), from_addr));
                    shm_had_data = true;
                }
            }

            // Process all collected messages
            for (buffer, from_addr) in messages_to_process {
                self.process_rtps_message(&buffer, from_addr);
            }

            // If SHM enabled but no data, yield CPU briefly to avoid 100% usage
            if has_shm_listener && !shm_had_data && events.is_empty() {
                std::thread::sleep(Duration::from_micros(10));
            }
        }
    }

    fn process_rtps_message(&mut self, buffer: &[u8], from_addr: SocketAddr) {
        use bytes::Bytes;

        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        let bytes = Bytes::copy_from_slice(buffer);
        let rtps_message = message_receiver.init(&bytes);
        if rtps_message.is_err() {
            error!("Failed to parse RTPS message from {:?}", from_addr);
            return;
        }
        let mut user_logic =
            self.user_logic.as_ref().as_ref().expect("UserLogic is not initialized").clone();

        if let Err(e) = user_logic.handle_rtps_message(message_receiver) {
            debug!("Failed to handle user RTPS message from {:?}: {:?}", from_addr, e);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::common::types::DomainId;
    use crate::rtps::entities::participant::Participant;
    use crate::rtps::task::user_traffic::user_unicast_listening_task::UserUnicastListeningTask;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_user_unicast_receive() {
        let domain_id: DomainId = 17;
        let mut socket = Socket::new(domain_id); //domain_id 0
        socket.create_socket();
        // When socket reset is needed
        let participant =
            Arc::new(Participant::new(domain_id, socket.participant_id(), socket.working_ip()));

        //user multicast port : 7400
        //user unicast port : 7410
        //user traffic multicast port : 7401
        //user traffic unicast port : 7411
        let mut user_unicast_listening_task = UserUnicastListeningTask::new(
            socket.user_traffic_unicast_listener(),
            socket.user_traffic_tcp_listener(),
            socket.shm_listener(),
            participant,
        );
        let _ = user_unicast_listening_task.unicast_listening();
    }
}
