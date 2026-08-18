use crate::rtps::entities::participant::Participant;
use crate::rtps::logic::spdp_logic::SpdpLogic;
use crate::rtps::task::discovery_traffic::discovery_worker::{enqueue_discovery, DiscoverySource};
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use bytes::Bytes;
use flume::Sender;
use log::{debug, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

pub(crate) struct DiscoveryMulticastListeningTask {
    spdp_logic: Arc<Option<SpdpLogic>>,
    tx: Sender<(Bytes, SocketAddr, DiscoverySource)>,
    queued_bytes: Arc<AtomicUsize>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl DiscoveryMulticastListeningTask {
    pub(crate) fn new(
        participant: Arc<Participant>,
        tx: Sender<(Bytes, SocketAddr, DiscoverySource)>,
        queued_bytes: Arc<AtomicUsize>,
    ) -> Self {
        let (spdp_logic, _, _) = participant.get_logics();
        Self { spdp_logic, tx, queued_bytes, shutdown_waker: Arc::new(OnceLock::new()) }
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
        info!("start discovery multicast listening (MioPoll)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);

        let port = listener.socket().local_addr().unwrap().port();
        let token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(100))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!(
                        "Poll interrupted in discovery multicast listening task, continuing: {}",
                        e
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }

            let spdp_logic =
                self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
            if let Ok(true) = spdp_logic.is_participant_terminated() {
                debug!("Detected global termination flag, discovery multicast listening loop is terminating...");
                let _ = poll.registry().deregister(listener.socket());
                listener.close();
                return Ok(());
            }

            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        let spdp_logic = self
                            .spdp_logic
                            .as_ref()
                            .as_ref()
                            .expect("SpdpLogic is not initialized");
                        if let Ok(true) = spdp_logic.is_participant_terminated() {
                            debug!("Detected global termination flag, discovery multicast listening loop is terminating...");
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }

                        enqueue_discovery(
                            &self.tx,
                            &self.queued_bytes,
                            buffer,
                            from_addr,
                            DiscoverySource::Multicast,
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
        info!("start discovery multicast listening (Channel)");

        loop {
            match rx.recv_timeout(Duration::from_millis(100)) {
                Ok(msg) => {
                    let spdp_logic =
                        self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
                    if let Ok(true) = spdp_logic.is_participant_terminated() {
                        debug!("Detected global termination flag, discovery multicast channel listening terminating...");
                        return Ok(());
                    }
                    enqueue_discovery(
                        &self.tx,
                        &self.queued_bytes,
                        Bytes::from(msg.data),
                        msg.source,
                        DiscoverySource::Multicast,
                    );
                }
                Err(flume::RecvTimeoutError::Timeout) => {
                    let spdp_logic =
                        self.spdp_logic.as_ref().as_ref().expect("SpdpLogic is not initialized");
                    if let Ok(true) = spdp_logic.is_participant_terminated() {
                        debug!("Detected global termination flag, discovery multicast channel listening terminating...");
                        return Ok(());
                    }
                }
                Err(flume::RecvTimeoutError::Disconnected) => {
                    info!("[DiscoveryMulticast] Channel disconnected, stopping listener");
                    return Ok(());
                }
            }
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
    fn test_discovery_multicast_receive() {
        let domain_id = 10;
        let _socket = Socket::new(domain_id);
        let _participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            Vec::new(),
            Vec::new(),
        ));

        // TODO: create transport and get MessageSource for discovery multicast
        // let mut task = DiscoveryMulticastListeningTask::new(participant);
        // let _ = task.multicast_listening(source);
    }
}
