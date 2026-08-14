use bytes::Bytes;

use crate::rtps::entities::participant::Participant;
use crate::rtps::task::user_traffic::user_worker::USER_RECV_QUEUE_BYTE_CAP;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::shm::shm_listener::ShmListener;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tokens::ListenerToken;
use flume::{Sender, TrySendError};
use log::{debug, info, warn};
use mio::{Events, Interest, Poll, Waker};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

pub(crate) struct UserUnicastListeningTask {
    participant: Weak<Participant>,
    tx: Sender<(Bytes, SocketAddr)>,
    queued_bytes: Arc<AtomicUsize>,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl UserUnicastListeningTask {
    pub(crate) fn new(
        participant: Arc<Participant>,
        tx: Sender<(Bytes, SocketAddr)>,
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
            MessageSource::MioPollWithShm { mut listener, mut shm } => {
                self.listen_mio_poll_with_shm(&mut listener, &mut shm)
            }
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

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);

        let port = listener.socket().local_addr().unwrap().port();
        let token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;
        info!("[UserUnicast] UDP listener registered on port {}", port);

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
                        self.enqueue(buffer, from_addr);
                    }
                }
            }
        }
    }

    fn listen_mio_poll_with_shm(
        &mut self,
        listener: &mut crate::rtps::transport::udp::udp_listener::UdpListener,
        shm: &mut ShmListener,
    ) -> std::io::Result<()> {
        info!("start user unicast listening (MioPoll + SHM)");
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(waker);

        let port = listener.socket().local_addr().unwrap().port();
        let token = ListenerToken::Udp(port).to_mio();
        poll.registry().register(listener.socket(), token, Interest::READABLE)?;
        info!("[UserUnicast] UDP listener registered on port {} (SHM enabled)", port);

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        // Zero-timeout poll so SHM ring-buffer polling proceeds without waiting
        // on UDP events. Yield the CPU briefly when both sources are idle.
        let poll_timeout = Duration::from_nanos(0);

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
                debug!("Detected global termination flag, user unicast (SHM) loop terminating...");
                let _ = poll.registry().deregister(listener.socket());
                return Ok(());
            }

            // Drain UDP unicast
            for event in events.iter() {
                if event.token() == token && event.is_readable() {
                    while let Some((buffer, from_addr)) = listener.get_message() {
                        if participant.is_terminated() {
                            let _ = poll.registry().deregister(listener.socket());
                            return Ok(());
                        }
                        self.enqueue(buffer, from_addr);
                    }
                }
            }

            // Drain SHM (no fd, polled directly)
            let mut shm_had_data = false;
            while let Some((buffer, from_addr)) = shm.get_message() {
                if participant.is_terminated() {
                    let _ = poll.registry().deregister(listener.socket());
                    return Ok(());
                }
                self.enqueue(buffer, from_addr);
                shm_had_data = true;
            }

            // Yield CPU briefly when both sources are idle to avoid 100% CPU.
            if !shm_had_data && events.is_empty() {
                std::thread::sleep(Duration::from_micros(10));
            }
        }
    }

    fn listen_channel(
        &mut self,
        rx: &flume::Receiver<crate::rtps::transport::plugin::IncomingMessage>,
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
                    self.enqueue(Bytes::from(msg.data), msg.source);
                }
                Err(flume::RecvTimeoutError::Timeout) => {
                    if participant.is_terminated() {
                        debug!("Detected global termination flag, user unicast channel listening terminating...");
                        return Ok(());
                    }
                }
                Err(flume::RecvTimeoutError::Disconnected) => {
                    info!("[UserUnicast] Channel disconnected, stopping listener");
                    return Ok(());
                }
            }
        }
    }

    // Hand the datagram to the worker without blocking the socket thread. The
    // byte cap bounds queued payload; over it we drop and rely on retransmit.
    fn enqueue(&self, buffer: Bytes, from_addr: SocketAddr) {
        let len = buffer.len();

        if self.queued_bytes.load(Ordering::Acquire) + len > USER_RECV_QUEUE_BYTE_CAP {
            debug!(
                "user recv queue over byte cap, dropping datagram from {:?} (len {})",
                from_addr, len
            );
            return;
        }

        match self.tx.try_send((buffer, from_addr)) {
            Ok(()) => {
                self.queued_bytes.fetch_add(len, Ordering::AcqRel);
            }
            Err(TrySendError::Full(_)) => {
                debug!(
                    "user recv queue count backstop full, dropping datagram from {:?}",
                    from_addr
                );
            }
            Err(TrySendError::Disconnected(_)) => {
                debug!("user worker gone, dropping datagram from {:?}", from_addr);
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
    fn test_user_unicast_receive() {
        let domain_id: DomainId = 17;
        let _socket = Socket::new(domain_id);
        let _participant = Arc::new(Participant::new(
            domain_id,
            _socket.participant_id(),
            _socket.working_ips(),
            Vec::new(),
            Vec::new(),
        ));

        // TODO: create transport and take_user_data_source() to get MessageSource
        // let source = transport.take_user_data_source();
        // let mut task = UserUnicastListeningTask::new(participant);
        // let _ = task.unicast_listening(source);
    }
}
