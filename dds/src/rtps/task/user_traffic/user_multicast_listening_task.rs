use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::locator::Locator;
use crate::rtps::entities::entity::Entity;
use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::message_processor::multicast_message_processor::MulticastMessageProcessor;
use crate::rtps::logic::user_logic::UserLogic;
use crate::rtps::messages::message_receiver::MessageReceiver;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use bytes::Bytes;
use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

pub(crate) struct UserMulticastListeningTask {
    guid_prefix: GuidPrefix,
    participant: Weak<Participant>,
    user_logic: Arc<Option<UserLogic>>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
    group_locator: Locator,
    stop: Arc<AtomicBool>,
}

impl UserMulticastListeningTask {
    pub(crate) fn new(
        participant: Arc<Participant>,
        group_locator: Locator,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let (_, _, user_logic) = participant.get_logics();
        let guid_prefix = participant.guid().prefix();
        Self {
            guid_prefix,
            participant: Arc::downgrade(&participant),
            user_logic,
            shutdown_waker: Arc::new(OnceLock::new()),
            group_locator,
            stop,
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn multicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        match source {
            MessageSource::Udp { mut listener } => self.listen_mio_poll(&mut listener),
            MessageSource::Shm { .. } => {
                unreachable!("Shm is only used by user-data unicast")
            }
            MessageSource::Stream { .. } => {
                unreachable!("Stream is handled by the stream unicast listening task")
            }
        }
    }

    fn listen_mio_poll(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
    ) -> std::io::Result<()> {
        info!("start user multicast listening (Udp)");
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

            if participant.is_terminated() || self.stop.load(Ordering::Acquire) {
                debug!("Detected termination flag, user traffic multicast listening loop is terminating...");
                let _ = poll.registry().deregister(listener.socket());
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        if participant.is_terminated() || self.stop.load(Ordering::Acquire) {
                            debug!("Detected termination flag during UDP processing");
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }
                        self.process_rtps_message(buffer, from_addr);
                    }
                }
            }
        }
    }

    fn process_rtps_message(&mut self, bytes: Bytes, from_addr: SocketAddr) {
        let mut message_receiver = MessageReceiver::new_multicast(
            self.guid_prefix,
            &from_addr,
            self.group_locator.clone(),
        );
        let rtps_message = message_receiver.init(&bytes);
        if rtps_message.is_err() {
            error!("Failed to parse RTPS message from {:?}", from_addr);
            return;
        }

        let mut user_logic =
            self.user_logic.as_ref().as_ref().expect("UserLogic is not initialized").clone();
        if let Err(e) = user_logic.handle_multicast_rtps_message(message_receiver) {
            debug!("Failed to handle user RTPS message from {:?}: {:?}", from_addr, e);
        }
    }
}
