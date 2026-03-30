use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor as _;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(crate) struct UserUnicastListeningTask {
    guid_prefix: GuidPrefix,
    participant: Weak<Participant>,
    user_logic: Arc<Option<UserLogic>>,
}

impl UserUnicastListeningTask {
    pub(crate) fn new(participant: Arc<Participant>) -> Self {
        let (_, _, user_logic) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self { guid_prefix, participant: Arc::downgrade(&participant), user_logic }
    }

    pub(crate) fn unicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        match source {
            MessageSource::MioPoll { mut listener } => self.listen_mio_poll(&mut listener),
            MessageSource::Channel { rx } => self.listen_channel(&rx),
        }
    }

    fn listen_mio_poll(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
    ) -> std::io::Result<()> {
        info!("start user unicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let token = Token(listener.socket().local_addr().unwrap().port() as usize);
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;
        info!(
            "[UserUnicast] UDP listener registered on port {}",
            listener.socket().local_addr().unwrap().port()
        );

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!("Poll interrupted in user unicast listening task, continuing: {}", e);
                    continue;
                }
                Err(e) => return Err(e),
            }

            if participant.is_terminated() {
                debug!("Detected global termination flag, user traffic unicast listening loop is terminating...");
                let _ = poll.registry().deregister(listener.socket());
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        if participant.is_terminated() {
                            debug!("Detected global termination flag during UDP processing");
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }
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
        info!("start user unicast listening (Channel)");

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, user unicast channel listening terminating...");
                        return Ok(());
                    }
                    self.process_rtps_message(&msg.data, msg.source);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, user unicast channel listening terminating...");
                        return Ok(());
                    }
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    info!("[UserUnicast] Channel disconnected, stopping listener");
                    return Ok(());
                }
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
    use crate::rtps::transport::plugin::MessageSource;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_user_unicast_receive() {
        let domain_id: DomainId = 17;
        let _socket = Socket::new(domain_id);
        let participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            None,
        ));

        // TODO: create transport and take_user_data_source() to get MessageSource
        // let source = transport.take_user_data_source();
        // let mut task = UserUnicastListeningTask::new(participant);
        // let _ = task.unicast_listening(source);
    }
}
