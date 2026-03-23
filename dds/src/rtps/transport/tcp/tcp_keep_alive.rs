#![allow(dead_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use log::{debug, info, warn};

use crate::rtps::messages::tcp_control_message::TcpControlMessage;
use crate::rtps::transport::tcp::framing::write_framed_message;

/// Default keepalive interval (30 seconds)
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 30;

/// Default max consecutive missed acks before declaring DEAD
const DEFAULT_MAX_MISSED_ACKS: u32 = 3;

/// Per-connection keepalive state
#[derive(Debug, Clone)]
struct KeepaliveState {
    /// When thisconnection was registered (handshake completed)
    registered_at: Instant,
    /// When we last sent a Keepalive
    last_sent: Option<Instant>,
    /// Number of consecutive keepalives sent without receiving an ack
    missed_acks: u32,
}

/// Manages TCP keepalive lifecycle on control connections
///
/// After handshake completes, the connection is registered for keepalive
/// monitoring. Periodically sends Keepalive messages and expects
/// KeepaliveAck responses. If too many consecutive acks are missed,
/// the connection is declared dead.
pub(crate) struct TcpKeepaliveManager {
    /// Keepalive send interval
    interval: Duration,
    /// Max consecutive missed acks before declaring DEAD
    max_missed_acks: u32,
    /// Per-connection keepalive state
    states: HashMap<SocketAddr, KeepaliveState>,
}

impl TcpKeepaliveManager {
    pub(crate) fn new() -> Self {
        Self {
            interval: Duration::from_secs(DEFAULT_KEEPALIVE_INTERVAL_SECS),
            max_missed_acks: DEFAULT_MAX_MISSED_ACKS,
            states: HashMap::new(),
        }
    }

    pub(crate) fn with_config(interval: Duration, max_missed_acks: u32) -> Self {
        Self { interval, max_missed_acks, states: HashMap::new() }
    }

    /// Register a connection for keepalive monitoring
    /// Called after handshake completes successfully
    pub(crate) fn register(&mut self, addr: SocketAddr) {
        info!("[TcpKeepalive] Registered {:?} for keepalive monitoring", addr);
        self.states.insert(
            addr,
            KeepaliveState { registered_at: Instant::now(), last_sent: None, missed_acks: 0 },
        );
    }

    /// Get connections that need a Keepalive sent
    ///
    /// Returns addresses where the keepalive interval has elapsed
    /// since the last send (or since registration if never sent).
    pub(crate) fn pending_keepalives(&self) -> Vec<SocketAddr> {
        self.states
            .iter()
            .filter(|(_addr, state)| {
                let elapsed = match state.last_sent {
                    Some(sent_at) => sent_at.elapsed(),
                    None => state.registered_at.elapsed(),
                };
                elapsed >= self.interval
            })
            .map(|(addr, _)| *addr)
            .collect()
    }

    /// Record that a Keepalive was sent to a peer
    ///
    /// Increments missed_acks counter. The counter is reset when
    /// KeepaliveAck is received.
    pub(crate) fn mark_sent(&mut self, addr: &SocketAddr) {
        if let Some(state) = self.states.get_mut(addr) {
            state.last_sent = Some(Instant::now());
            state.missed_acks += 1;
            debug!(
                "[TcpKeepalive] Sent keepalive to {:?} (missed_acks={})",
                addr, state.missed_acks
            );
        }
    }

    /// Handle an incoming Keepalive message from a remote peer
    ///
    /// Responds with KeepaliveAck on the same connection.
    pub(crate) fn handle_keepalive<W: Write>(
        &self,
        addr: SocketAddr,
        stream: &mut W,
    ) -> io::Result<()> {
        let ack = TcpControlMessage::KeepaliveAck;
        let bytes = ack.serialize()?;
        write_framed_message(stream, &bytes);

        debug!("[TcpKeepalive] Received Keepalive from {:?}, sent KeepaliveAck", addr);
        Ok(())
    }

    /// Handle an incoming KeepaliveAck from a remote peer
    ///
    /// Resets the missed_acks counter, confirming the peer is alive.
    pub(crate) fn handle_keepalive_ack(&mut self, addr: &SocketAddr) {
        if let Some(state) = self.states.get_mut(addr) {
            state.missed_acks = 0;
            debug!("[TcpKeepalive] Received KeepaliveAck from {:?}", addr);
        }
    }

    /// Check for dead connections
    ///
    /// Returns addresses where consecutive missed acks exceed the threshold.
    /// Dead connections are removed from tracking.
    pub(crate) fn check_dead(&mut self) -> Vec<SocketAddr> {
        let max = self.max_missed_acks;

        let deads: Vec<SocketAddr> = self
            .states
            .iter()
            .filter(|(_, state)| state.missed_acks >= max)
            .map(|(addr, _)| *addr)
            .collect();

        for addr in &deads {
            warn!("[TcpKeepalive] Connection {:?} declared dead (missed_acks >= {})", addr, max);
            self.states.remove(addr);
        }

        deads
    }

    /// Check if a connection is being monitored
    pub(crate) fn is_monitored(&self, addr: &SocketAddr) -> bool {
        self.states.contains_key(addr)
    }

    /// Remove connection from keepalive monitoring
    pub(crate) fn remove(&mut self, addr: &SocketAddr) {
        self.states.remove(addr);
        debug!("[TcpKeepalive] Removed {:?} from keepalive monitoring", addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use crate::rtps::transport::tcp::framing::read_framed_message;

    fn test_addr(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn test_register_and_monitor() {
        let mut manager = TcpKeepaliveManager::new();
        let addr = test_addr(9000);

        assert!(!manager.is_monitored(&addr));

        manager.register(addr);
        assert!(manager.is_monitored(&addr));
    }

    #[test]
    fn test_pending_keepalives_initial() {
        let mut manager = TcpKeepaliveManager::with_config(Duration::from_millis(0), 3);
        let addr = test_addr(9000);

        manager.register(addr);
        std::thread::sleep(Duration::from_millis(1));

        let pending = manager.pending_keepalives();
        assert_eq!(pending, vec![addr]);
    }

    #[test]
    fn test_pending_keepalives_after_send() {
        let mut manager = TcpKeepaliveManager::with_config(Duration::from_secs(60), 3);
        let addr = test_addr(9000);

        manager.register(addr);
        manager.mark_sent(&addr);

        // interval not elapsed yet
        let pending = manager.pending_keepalives();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_mark_sent_increments_missed_acks() {
        let mut manager = TcpKeepaliveManager::with_config(Duration::from_millis(0), 3);
        let addr = test_addr(9000);

        manager.register(addr);

        manager.mark_sent(&addr);
        manager.mark_sent(&addr);
        manager.mark_sent(&addr);

        let dead = manager.check_dead();
        assert_eq!(dead, vec![addr]);
        assert!(!manager.is_monitored(&addr)); // removed after dead
    }

    #[test]
    fn test_keepalive_ack_resets_counter() {
        let mut manager = TcpKeepaliveManager::with_config(Duration::from_millis(0), 3);
        let addr = test_addr(9000);

        manager.register(addr);

        // Send 2 keepalives without ack
        manager.mark_sent(&addr);
        manager.mark_sent(&addr);

        // Receive ack → resets counter
        manager.handle_keepalive_ack(&addr);

        // Send 1 more → missed_acks = 1, not dead
        manager.mark_sent(&addr);

        let dead = manager.check_dead();
        assert!(dead.is_empty());
    }

    #[test]
    fn test_handle_keepalive_sends_ack() {
        let manager = TcpKeepaliveManager::new();
        let addr = test_addr(9000);

        let mut wire = Vec::new();
        manager.handle_keepalive(addr, &mut wire).unwrap();

        // Verify KeepaliveAck was written
        let ack_bytes = read_framed_message(&mut Cursor::new(&wire)).unwrap();
        let msg = TcpControlMessage::deserialize(&ack_bytes).unwrap();
        assert_eq!(msg, TcpControlMessage::KeepaliveAck);
    }

    #[test]
    fn test_not_dead_below_threshold() {
        let mut manager = TcpKeepaliveManager::with_config(Duration::from_millis(0), 3);
        let addr = test_addr(9000);

        manager.register(addr);

        manager.mark_sent(&addr); // missed = 1
        manager.mark_sent(&addr); // missed = 2

        let dead = manager.check_dead();
        assert!(dead.is_empty()); // 2 < 3
    }

    #[test]
    fn test_remove() {
        let mut manager = TcpKeepaliveManager::new();
        let addr = test_addr(9000);

        manager.register(addr);
        assert!(manager.is_monitored(&addr));

        manager.remove(&addr);
        assert!(!manager.is_monitored(&addr));
    }

    #[test]
    fn test_multiple_connections() {
        let mut manager = TcpKeepaliveManager::with_config(Duration::from_millis(0), 2);
        let addr_a = test_addr(9000);
        let addr_b = test_addr(9001);

        manager.register(addr_a);
        manager.register(addr_b);

        // A: 2 missed → dead
        manager.mark_sent(&addr_a);
        manager.mark_sent(&addr_a);

        // B: 1 missed → alive
        manager.mark_sent(&addr_b);

        let dead = manager.check_dead();
        assert_eq!(dead, vec![addr_a]);
        assert!(!manager.is_monitored(&addr_a));
        assert!(manager.is_monitored(&addr_b));
    }
}
