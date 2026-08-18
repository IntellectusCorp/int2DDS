use bytes::Bytes;

use crate::rtps::entities::participant::Participant;
use crate::rtps::task::discovery_traffic::discovery_worker::{enqueue_discovery, DiscoverySource};
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use flume::Sender;
use log::{debug, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

pub(crate) struct DiscoveryUnicastListeningTask {
    participant: Weak<Participant>,
    tx: Sender<(Bytes, SocketAddr, DiscoverySource)>,
    queued_bytes: Arc<AtomicUsize>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl DiscoveryUnicastListeningTask {
    pub(crate) fn new(
        participant: Arc<Participant>,
        tx: Sender<(Bytes, SocketAddr, DiscoverySource)>,
        queued_bytes: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            participant: Arc::downgrade(&participant),
            tx,
            queued_bytes,
            shutdown_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn unicast_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
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
        info!("start discovery unicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);

        let port = listener.socket().local_addr().unwrap().port();
        let token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;
        info!("[DiscoveryUnicast] UDP listener registered on port {}", port);

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
                        enqueue_discovery(
                            &self.tx,
                            &self.queued_bytes,
                            buffer,
                            from_addr,
                            DiscoverySource::Unicast,
                        );
                    }
                }
            }
        }
    }

    fn listen_channel(
        &mut self,
        rx: &flume::Receiver<crate::rtps::transport::plugin::IncomingMessage>,
    ) -> std::io::Result<()> {
        info!("start discovery unicast listening (Channel)");

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, discovery unicast channel listening terminating...");
                        return Ok(());
                    }
                    enqueue_discovery(
                        &self.tx,
                        &self.queued_bytes,
                        Bytes::from(msg.data),
                        msg.source,
                        DiscoverySource::Unicast,
                    );
                }
                Err(flume::RecvTimeoutError::Timeout) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, discovery unicast channel listening terminating...");
                        return Ok(());
                    }
                }
                Err(flume::RecvTimeoutError::Disconnected) => {
                    info!("[DiscoveryUnicast] Channel disconnected, stopping listener");
                    return Ok(());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::rtps::common::types::DomainId;
    use crate::rtps::entities::participant::Participant;
    use crate::rtps::transport::socket::Socket;

    #[test]
    #[ignore]
    fn test_discovery_unicast_receive() {
        let domain_id: DomainId = 17;
        let _socket = Socket::new(domain_id);
        let _participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            Vec::new(),
            Vec::new(),
        ));

        // TODO: create transport and take_discovery_source() to get MessageSource
        // let source = transport.take_discovery_source();
        // let mut task = DiscoveryUnicastListeningTask::new(participant);
        // let _ = task.unicast_listening(source);
    }
}
