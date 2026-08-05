use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::unicast_message_processor::UnicastMessageProcessor;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use bytes::Bytes;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::net::SocketAddr;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

pub(crate) struct UserMulticastListeningTask {
    guid_prefix: GuidPrefix,
    participant: Weak<Participant>,
    user_logic: Arc<Option<UserLogic>>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl UserMulticastListeningTask {
    pub(crate) fn new(participant: Arc<Participant>) -> Self {
        let (_, _, user_logic) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self {
            guid_prefix,
            participant: Arc::downgrade(&participant),
            user_logic,
            shutdown_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn multicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        match source {
            MessageSource::MioPoll { mut listener } => self.listen_mio_poll(&mut listener),
            MessageSource::Channel { rx } => self.listen_channel(&rx),
            MessageSource::MioPollWithShm { .. } => {
                unreachable!("MioPollWithShm is only used by user-data unicast")
            }
        }
    }

    fn listen_mio_poll(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
    ) -> std::io::Result<()> {
        info!("start user multicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);
        let port = listener.socket().local_addr().unwrap().port();
        let token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;
        info!("[UserMulticast] UDP listener registered on port {}", port);

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!("Poll interrupted in user multicast listening task, continuing: {}", e);
                    continue;
                }
                Err(e) => return Err(e),
            }

            if participant.is_terminated() {
                debug!("Detected global termination flag, user traffic multicast listening loop is terminating...");
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
                        self.process_rtps_message(buffer, from_addr);
                    }
                }
            }
        }
    }

    fn listen_channel(
        &mut self,
        _rx: &flume::Receiver<crate::rtps::transport::plugin::IncomingMessage>,
    ) -> std::io::Result<()> {
        unreachable!("channel is only used by user-data unicast")
    }

    fn process_rtps_message(&mut self, bytes: Bytes, from_addr: SocketAddr) {
        let mut message_receiver = MessageReceiver::new(self.guid_prefix, &from_addr);
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

    use crate::rtps::entities::participant::Participant;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_user_multicast_receive() {
        let domain_id = 10;
        let _socket = Socket::new(domain_id);
        let _participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            Vec::new(),
            Vec::new(),
        ));

        // TODO: create transport and get MessageSource for user multicast
        // let mut task = UserMulticastListeningTask::new(participant);
        // let _ = task.multicast_listening(source);
    }
}
