//! What the TLS layer refused, kept until the application asks for it.

use std::{collections::VecDeque, net::SocketAddr, sync::Mutex};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::transport::tls::{identity::IDENTITY_LEN, rejection::TlsRejectionReason};

/// An unauthenticated peer can raise events at will, so the oldest ones give
/// way rather than memory growing with its attempts.
const CAPACITY: usize = 256;

/// Which side opened the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TlsDirection {
    Dialed,
    Accepted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TlsEvent {
    /// A handshake was refused, so the peer's certificate is not known.
    Rejected { address: SocketAddr, direction: TlsDirection, reason: TlsRejectionReason },
    /// An established connection was closed because a reloaded policy refuses its peer.
    ClosedByPolicy {
        address: SocketAddr,
        direction: TlsDirection,
        identity: Option<[u8; IDENTITY_LEN]>,
        reason: TlsRejectionReason,
    },
    /// An authenticated peer named a GUID prefix its certificate does not own,
    /// so its connection is closed.
    IdentityMismatch {
        address: SocketAddr,
        identity: Option<[u8; IDENTITY_LEN]>,
        claimed_prefix: GuidPrefix,
    },
}

/// The events taken in one call, and how many were lost since the previous one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsEvents {
    pub events: Vec<TlsEvent>,
    pub dropped: u64,
}

#[derive(Default)]
struct Pending {
    events: VecDeque<TlsEvent>,
    dropped: u64,
}

#[derive(Default)]
pub(crate) struct TlsEventLog {
    pending: Mutex<Pending>,
}

impl TlsEventLog {
    pub(crate) fn record(&self, event: TlsEvent) {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        if pending.events.len() == CAPACITY {
            pending.events.pop_front();
            pending.dropped += 1;
        }
        pending.events.push_back(event);
    }

    pub(crate) fn take(&self) -> TlsEvents {
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        TlsEvents {
            events: pending.events.drain(..).collect(),
            dropped: std::mem::take(&mut pending.dropped),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, SocketAddr};

    use super::*;

    fn rejected(port: u16) -> TlsEvent {
        TlsEvent::Rejected {
            address: SocketAddr::from((Ipv4Addr::LOCALHOST, port)),
            direction: TlsDirection::Accepted,
            reason: TlsRejectionReason::Expired,
        }
    }

    #[test]
    fn take_returns_the_events_in_order_and_leaves_none() {
        let log = TlsEventLog::default();
        log.record(rejected(1));
        log.record(rejected(2));

        assert_eq!(log.take(), TlsEvents { events: vec![rejected(1), rejected(2)], dropped: 0 });
        assert_eq!(log.take(), TlsEvents::default());
    }

    #[test]
    fn a_full_log_gives_up_its_oldest_events_and_counts_them() {
        let log = TlsEventLog::default();
        for port in 0..CAPACITY as u16 + 2 {
            log.record(rejected(port));
        }

        let taken = log.take();
        assert_eq!(taken.dropped, 2);
        assert_eq!(taken.events.len(), CAPACITY);
        assert_eq!(taken.events.first(), Some(&rejected(2)));
        assert_eq!(taken.events.last(), Some(&rejected(CAPACITY as u16 + 1)));

        assert_eq!(log.take(), TlsEvents::default());
    }
}
