//! The worker that opens outbound connections without waiting on any one of them.

use std::collections::HashMap;
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use log::warn;
use mio::{Events, Interest, Poll, Token};

use super::{ConnectionKey, Slot, TcpSender};
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};
use crate::rtps::transport::tcp::dial::start_dial;
use crate::rtps::transport::tcp::sync_connection::{tls_client, OutboundConnection};

/// How long the dial worker waits before checking whether it should still run.
const DIAL_WORKER_IDLE_TICK: Duration = Duration::from_millis(200);

pub(super) const DIAL_WAKE_TOKEN: Token = Token(0);

struct Dialing {
    key: ConnectionKey,
    cancelled: Arc<AtomicBool>,
    stream: mio::net::TcpStream,
    deadline: Instant,
    /// Present once the TCP connection is up and its TLS handshake is under way.
    tls: Option<rustls::ClientConnection>,
}

/// Dials still waiting for their peer, all watched by one poll so that none
/// of them waits behind another.
pub(super) struct DialLoop {
    poll: Poll,
    dialing: HashMap<Token, Dialing>,
    by_key: HashMap<ConnectionKey, Token>,
    next_token: usize,
}

impl DialLoop {
    pub(super) fn new(poll: Poll) -> Self {
        Self { poll, dialing: HashMap::new(), by_key: HashMap::new(), next_token: 0 }
    }

    pub(super) fn run_until_shutdown(mut self, weak: Weak<TcpSender>) {
        let mut events = Events::with_capacity(64);
        loop {
            let now = Instant::now();
            let wait = self
                .dialing
                .values()
                .map(|dialing| dialing.deadline.saturating_duration_since(now))
                .min()
                .map_or(DIAL_WORKER_IDLE_TICK, |left| left.min(DIAL_WORKER_IDLE_TICK));
            if let Err(error) = self.poll.poll(&mut events, Some(wait)) {
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                warn!(
                    "TcpSender: dial worker stopped, no new TCP connection can be opened: {}",
                    error
                );
                return;
            }
            // Held for this round only, so the sender can still be dropped
            // while the loop sleeps.
            let Some(sender) = weak.upgrade() else {
                return;
            };
            if sender.shutdown.load(Ordering::Acquire) {
                return;
            }
            let cancelled: Vec<Token> = self
                .dialing
                .iter()
                .filter(|(_, dialing)| dialing.cancelled.load(Ordering::Acquire))
                .map(|(token, _)| *token)
                .collect();
            for token in cancelled {
                self.remove_dial(token);
            }
            for event in events.iter() {
                if event.token() != DIAL_WAKE_TOKEN {
                    self.complete_answered_dial(&sender, event.token());
                }
            }
            // Wakeups coalesce, so the queue itself is the record of what is
            // waiting, not the number of wakeups.
            self.dial_peers_with_queued_frames(&sender);
            self.give_up_on_overdue_dials(&sender);
        }
    }

    fn dial_peers_with_queued_frames(&mut self, sender: &Arc<TcpSender>) {
        let waiting: Vec<(ConnectionKey, Arc<AtomicBool>)> = sender
            .slots
            .iter()
            .filter_map(|slot| match slot.value() {
                Slot::Connecting { cancelled, .. } if !self.by_key.contains_key(slot.key()) => {
                    Some((*slot.key(), Arc::clone(cancelled)))
                }
                _ => None,
            })
            .collect();
        for (key, cancelled) in waiting {
            if cancelled.load(Ordering::Acquire) {
                continue;
            }
            let mut stream = match start_dial(key.0, sender.connect_timeout) {
                Ok(stream) => mio::net::TcpStream::from_std(stream),
                Err(error) => {
                    sender.record_dial_result(key, &cancelled, Err(error));
                    continue;
                }
            };
            self.next_token = self.next_token.wrapping_add(1).max(1);
            let token = Token(self.next_token);
            if let Err(error) =
                self.poll.registry().register(&mut stream, token, Interest::WRITABLE)
            {
                sender.record_dial_result(key, &cancelled, Err(error));
                continue;
            }
            self.by_key.insert(key, token);
            let deadline = Instant::now() + sender.connect_timeout;
            self.dialing.insert(token, Dialing { key, cancelled, stream, deadline, tls: None });
        }
    }

    fn complete_answered_dial(&mut self, sender: &Arc<TcpSender>, token: Token) {
        let Some(dialing) = self.dialing.get_mut(&token) else {
            return;
        };
        if dialing.tls.is_some() {
            self.drive_tls_handshake(sender, token);
            return;
        }
        let outcome = match dialing.stream.take_error() {
            Ok(Some(error)) | Err(error) => Err(error),
            Ok(None) => match dialing.stream.peer_addr() {
                Ok(_) => Ok(()),
                // Woken before the handshake finished; the next event settles it.
                Err(error) if error.kind() == io::ErrorKind::NotConnected => return,
                Err(error) => Err(error),
            },
        };
        let tls = match (outcome, sender.tls_config.as_deref()) {
            (Err(error), _) => Err(error),
            (Ok(()), None) => Ok(None),
            (Ok(()), Some(config)) => tls_client(config).map(Some),
        };
        match tls {
            Ok(Some(tls)) => {
                dialing.tls = Some(tls);
                dialing.deadline = Instant::now() + sender.tls_handshake_timeout;
                self.drive_tls_handshake(sender, token);
            }
            Ok(None) => {
                let Some(Dialing { key, cancelled, stream, .. }) = self.remove_dial(token) else {
                    return;
                };
                let result = OutboundConnection::plain(stream.into(), &sender.shared.tuning);
                sender.record_dial_result(key, &cancelled, result);
            }
            Err(error) => {
                if let Some(Dialing { key, cancelled, .. }) = self.remove_dial(token) {
                    sender.record_dial_result(key, &cancelled, Err(error));
                }
            }
        }
    }

    /// Take the handshake as far as the socket allows right now, then leave it
    /// to the next event: a peer that stops answering holds only its own dial.
    fn drive_tls_handshake(&mut self, sender: &Arc<TcpSender>, token: Token) {
        let Some(dialing) = self.dialing.get_mut(&token) else {
            return;
        };
        let Some(tls) = dialing.tls.as_mut() else {
            return;
        };
        let error = match tls.complete_io(&mut dialing.stream) {
            Ok(_) if !tls.is_handshaking() => None,
            Ok(_) => Some(io::Error::from(io::ErrorKind::WouldBlock)),
            Err(error) => Some(error),
        };
        match error {
            Some(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let interest = if tls.wants_write() {
                    Interest::READABLE.add(Interest::WRITABLE)
                } else {
                    Interest::READABLE
                };
                if let Err(error) =
                    self.poll.registry().reregister(&mut dialing.stream, token, interest)
                {
                    if let Some(Dialing { key, cancelled, .. }) = self.remove_dial(token) {
                        sender.record_dial_result(key, &cancelled, Err(error));
                    }
                }
            }
            Some(error) => {
                if let Some(Dialing { key, cancelled, .. }) = self.remove_dial(token) {
                    let error = transport_io_error(
                        TransportErrorCode::TlsHandshakeFailed,
                        format!("TLS handshake with {} failed: {error}", key.0),
                    );
                    sender.record_dial_result(key, &cancelled, Err(error));
                }
            }
            None => {
                let Some(Dialing { key, cancelled, stream, tls: Some(tls), .. }) =
                    self.remove_dial(token)
                else {
                    return;
                };
                let result =
                    OutboundConnection::over_tls(stream.into(), tls, &sender.shared.tuning);
                sender.record_dial_result(key, &cancelled, result);
            }
        }
    }

    fn give_up_on_overdue_dials(&mut self, sender: &Arc<TcpSender>) {
        let now = Instant::now();
        let overdue: Vec<Token> = self
            .dialing
            .iter()
            .filter(|(_, dialing)| dialing.deadline <= now)
            .map(|(token, _)| *token)
            .collect();
        for token in overdue {
            let Some(Dialing { key, cancelled, tls, .. }) = self.remove_dial(token) else {
                continue;
            };
            let error = match tls {
                Some(_) => transport_io_error(
                    TransportErrorCode::TlsHandshakeFailed,
                    format!("TLS handshake with {} timed out", key.0),
                ),
                None => io::Error::new(
                    io::ErrorKind::TimedOut,
                    format!("TCP connect to {} got no answer", key.0),
                ),
            };
            sender.record_dial_result(key, &cancelled, Err(error));
        }
    }

    fn remove_dial(&mut self, token: Token) -> Option<Dialing> {
        let mut dialing = self.dialing.remove(&token)?;
        self.by_key.remove(&dialing.key);
        let _ = self.poll.registry().deregister(&mut dialing.stream);
        Some(dialing)
    }
}
