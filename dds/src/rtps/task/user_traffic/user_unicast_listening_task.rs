use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::shm::ShmListener;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::udp::udp_listener::UdpListener;
use crossbeam_channel::Receiver;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) struct UserUnicastListeningTask {
    guid_prefix: GuidPrefix,
    user_unicast_listener: Option<UdpListener>,
    tcp_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
    shm_listener: Option<ShmListener>,
    participant: Weak<Participant>,
    user_logic: Arc<Option<UserLogic>>,
}

impl UserUnicastListeningTask {
    pub(crate) fn new(
        user_unicast_listener: Option<UdpListener>,
        tcp_rx: Option<Receiver<(Vec<u8>, SocketAddr)>>,
        shm_listener: Option<ShmListener>,
        participant: Arc<Participant>,
    ) -> Self {
        let (_, _, user_logic) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self {
            guid_prefix,
            user_unicast_listener,
            tcp_rx,
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

        let has_tcp_rx = self.tcp_rx.is_some();
        let has_shm_listener = self.shm_listener.is_some();

        if has_tcp_rx {
            info!("[UserUnicast] TCP channel receiver enabled");
        }

        if udp_token.is_none() && !has_tcp_rx && !has_shm_listener {
            return Err(std::io::Error::other("No listener (UDP, TCP channel, or SHM) is set"));
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

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match poll.poll(&mut events, Some(poll_timeout)) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!("Poll interrupted in user unicast listening task, continuing: {}", e);
                    continue;
                }
                Err(e) => return Err(e),
            }

            if participant.is_terminated() {
                debug!("Detected global termination flag, user traffic unicast listening loop is terminating...");
                if let Some(listener) = &mut self.user_unicast_listener {
                    let _ = poll.registry().deregister(listener.socket());
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
        let mut socket = Socket::new(domain_id);
        socket.create_socket();
        let participant = Arc::new(Participant::new(
            domain_id,
            socket.participant_id(),
            socket.working_ips(),
            None,
        ));

        let mut user_unicast_listening_task = UserUnicastListeningTask::new(
            socket.user_traffic_unicast_listener(),
            socket.tcp_user_data_rx(),
            socket.shm_listener(),
            participant,
        );
        let _ = user_unicast_listening_task.unicast_listening();
    }
}
