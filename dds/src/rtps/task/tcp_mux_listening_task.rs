use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use log::{debug, error, info, warn};
use mio::{Events, Interest, Poll, Token};

use crate::rtps::entities::participant::Participant;
use crate::rtps::transport::socket::MAX_EVENTS;
use crate::rtps::transport::tcp::tcp_mux_listener::TcpMuxListener;

/// Token for the MuxListener's TCP listener socket
const MUX_LISTENER_TOKEN: Token = Token(0);

/// Poll timeout in milliseconds
const POLL_TIMEOUT_MS: u64 = 100;

/// Keepalive check interval
const KEEPALIVE_CHECK_INTERVAL_SECS: u64 = 10;

/// Task that runs the mio event loop for a TcpMuxListener.
///
/// Spawned as a dedicated thread. Handles:
/// - TCP accept -> new connections (AwaitingBind state)
/// - Data arrival -> frame classification -> control message handling or channel routing
/// - Periodic keepalive on control connections
/// - Graceful shutdown via particpant termination flag
pub(crate) struct TcpMuxListeningTask {
    mux_listener: TcpMuxListener,
    participant: Weak<Participant>,
}

impl TcpMuxListeningTask {
    pub(crate) fn new(mux_listener: TcpMuxListener, participant: Arc<Participant>) -> Self {
        Self { mux_listener, participant: Arc::downgrade(&participant) }
    }

    /// Main event loop, Blocks until participant is terminated.
    pub(crate) fn run(&mut self) -> std::io::Result<()> {
        info!("[TcpMuxListeningTask] Starting on port {}", self.mux_listener.port());

        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(MAX_EVENTS);

        // Register the TCP listener socket for accept events
        if let Some(listener) = self.mux_listener.listener_mut() {
            poll.registry().register(listener, MUX_LISTENER_TOKEN, Interest::READABLE)?;
        } else {
            return Err(std::io::Error::other("MuxListener not initialized"));
        }

        let participant = self
            .participant
            .upgrade()
            .ok_or_else(|| std::io::Error::other("Participant already dropped"))?;

        let mut last_keepalive_check = Instant::now();
        let keepalive_interval = Duration::from_secs(KEEPALIVE_CHECK_INTERVAL_SECS);

        loop {
            match poll.poll(&mut events, Some(Duration::from_millis(POLL_TIMEOUT_MS))) {
                Ok(()) => {}
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {
                    warn!("[TcpMuxListeningTask] Poll interrupted, continuing");
                    continue;
                }
                Err(e) => return Err(e),
            }

            // Check termination
            if participant.is_terminated() {
                debug!("[TcpMuxListeningTask] Termination flag detected, shutting down");
                // Deregister listener
                if let Some(listener) = self.mux_listener.listener_mut() {
                    let _ = poll.registry().deregister(listener);
                }
                self.mux_listener.close();
                return Ok(());
            }

            // Process events
            for event in events.iter() {
                if event.token() == MUX_LISTENER_TOKEN && event.is_readable() {
                    // Accept new connections
                    loop {
                        match self.mux_listener.accept(poll.registry()) {
                            Ok(Some(_token)) => {
                                // Connection accepted and registered, continue accepting
                            }
                            Ok(None) => break, // No more pending connections
                            Err(e) => {
                                error!("[TcpMuxListeningTask] Accept error: {:?}", e);
                                break;
                            }
                        }
                    }
                } else if event.is_readable() {
                    // Data available on an accepted connection
                    self.mux_listener.on_readable(event.token(), poll.registry());
                }
            }

            // Peirodic keepalive check
            if last_keepalive_check.elapsed() >= keepalive_interval {
                last_keepalive_check = Instant::now();

                let dead_peers = self.mux_listener.send_keepalives();
                for guid in dead_peers {
                    warn!("[TcpMuxListeningTask] Removing dead peer {:?}", guid);
                    self.mux_listener.remove_peer(guid, poll.registry());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Integration tests require full participant setup — placed in integration test suite
}
