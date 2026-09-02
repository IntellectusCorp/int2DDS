use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

use log::{debug, info, warn};
use mio::{Events, Poll, Waker};

use crate::rtps::entities::participant::Participant;
use crate::rtps::task::discovery_traffic::discovery_unicast_listening_task::DiscoveryUnicastListeningTask;
use crate::rtps::task::user_traffic::user_unicast_listening_task::UserUnicastListeningTask;
use crate::rtps::transport::plugin::MessageSource;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tcp::framing::TcpFrameKind;
use crate::rtps::transport::tokens::ListenerToken;

pub(crate) struct StreamUnicastListeningTask {
    participant: Weak<Participant>,
    discovery_task: DiscoveryUnicastListeningTask,
    user_task: UserUnicastListeningTask,
    shutdown_waker: Arc<OnceLock<Arc<Waker>>>,
}

impl StreamUnicastListeningTask {
    pub(crate) fn new(participant: Arc<Participant>) -> Self {
        Self {
            participant: Arc::downgrade(&participant),
            discovery_task: DiscoveryUnicastListeningTask::new(Arc::clone(&participant)),
            user_task: UserUnicastListeningTask::new(participant),
            shutdown_waker: Arc::new(OnceLock::new()),
        }
    }

    pub(crate) fn set_shutdown_waker(&mut self, handle: Arc<OnceLock<Arc<Waker>>>) {
        self.shutdown_waker = handle;
    }

    pub(crate) fn stream_listening(&mut self, source: MessageSource) -> std::io::Result<()> {
        let MessageSource::Stream { mut listener, shared } = source else {
            unreachable!("stream listening requires a stream message source")
        };
        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);
        let waker = Arc::new(Waker::new(poll.registry(), ListenerToken::Shutdown.to_mio())?);
        let _ = self.shutdown_waker.set(Arc::clone(&waker));
        shared.set_self_delivery_waker(waker);
        let listener_token = match listener.register(poll.registry()) {
            Ok(token) => token,
            Err(error) => {
                shared.clear_self_delivery_waker();
                return Err(error);
            }
        };
        let poll_timeout = Duration::from_millis(100);

        info!("start stream unicast listening");
        loop {
            match poll.poll(&mut events, Some(poll_timeout)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    shared.clear_self_delivery_waker();
                    let _ = listener.deregister_all(poll.registry());
                    return Err(error);
                }
            }

            if participant.is_terminated() {
                debug!("stream unicast listening loop is terminating");
                shared.clear_self_delivery_waker();
                let _ = listener.deregister_all(poll.registry());
                return Ok(());
            }

            listener.expire_connections(poll.registry(), Instant::now());
            while let Some(message) = shared.try_receive_self_delivery() {
                if participant.is_terminated() {
                    shared.clear_self_delivery_waker();
                    let _ = listener.deregister_all(poll.registry());
                    return Ok(());
                }
                self.user_task.process_rtps_message(message.data, message.source);
            }
            for event in &events {
                let token = event.token();
                if token == listener_token {
                    if event.is_readable() {
                        if let Err(error) = listener.accept_ready(poll.registry()) {
                            shared.clear_self_delivery_waker();
                            let _ = listener.deregister_all(poll.registry());
                            return Err(error);
                        }
                    }
                    continue;
                }
                if token == ListenerToken::Shutdown.to_mio()
                    || !(event.is_readable() || event.is_writable())
                {
                    continue;
                }

                loop {
                    match listener.get_message(token, poll.registry()) {
                        Ok(Some(message)) => match message.kind {
                            TcpFrameKind::Discovery => {
                                let _ = self
                                    .discovery_task
                                    .process_rtps_message(message.data, message.source);
                            }
                            TcpFrameKind::UserData => {
                                self.user_task.process_rtps_message(message.data, message.source);
                            }
                        },
                        Ok(None) => break,
                        Err(error) => {
                            warn!("TCP connection read failed: {error}");
                            break;
                        }
                    }
                }
            }
        }
    }
}
