#![allow(dead_code)]
#![allow(unused_variables)]

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::DomainId;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use log::{info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

pub(crate) struct UserMulticastListeningTask {
    guid_prefix: GuidPrefix,
    domain_id: DomainId,
    user_logic: Arc<Option<UserLogic>>,
}

impl UserMulticastListeningTask {
    pub(crate) fn new(participant: Arc<Participant>) -> Self {
        let guid_prefix = participant.guid().prefix();
        let domain_id = participant.domain_id();
        let (_, _, user_logic) = participant.get_logics();
        Self { guid_prefix, domain_id, user_logic }
    }

    pub(crate) fn multicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        match source {
            MessageSource::MioPoll { mut listener } => self.listen_mio_poll(&mut listener),
            MessageSource::Channel { rx } => self.listen_channel(&rx),
        }
    }

    fn listen_mio_poll(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
    ) -> std::io::Result<()> {
        info!("start user multicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let token = Token(listener.socket().local_addr().unwrap().port() as usize);
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_secs(1))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!("Poll interrupted in user multicast listening task, continuing: {}", e);
                    continue;
                }
                Err(e) => return Err(e),
            }
            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        self.process_rtps_message(&buffer, from_addr);
                    }
                }
            }
        }
    }

    fn listen_channel(
        &mut self,
        rx: &crossbeam_channel::Receiver<crate::rtps::transport::plugin::IncomingMessage>,
    ) -> std::io::Result<()> {
        info!("start user multicast listening (Channel)");

        loop {
            match rx.recv() {
                Ok(msg) => {
                    self.process_rtps_message(&msg.data, msg.source);
                }
                Err(_) => {
                    info!("[UserMulticast] Channel disconnected, stopping listener");
                    return Ok(());
                }
            }
        }
    }

    fn process_rtps_message(&mut self, buffer: &[u8], from_addr: SocketAddr) {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
        let rtps_message = message_receiver.init(&bytes::Bytes::copy_from_slice(buffer));
        if rtps_message.is_err() {
            log::error!("Failed to parse RTPS message");
            return;
        }

        // self.user_logic.handle_rtps_message(message_receiver);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::entities::participant::Participant;
    use crate::rtps::task::user_traffic::user_multicast_listening_task::UserMulticastListeningTask;
    use crate::rtps::transport::plugin::MessageSource;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_user_multicast_receive() {
        let domain_id = 10;
        let _socket = Socket::new(domain_id);
        let participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            None,
        ));

        // TODO: create transport and get MessageSource for user multicast
        // let mut task = UserMulticastListeningTask::new(participant);
        // let _ = task.multicast_listening(source);
    }
}
