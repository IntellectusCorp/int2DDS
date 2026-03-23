use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use log::{debug, info, warn};

use crate::rtps::common::guid::GuidPrefix;
use crate::rtps::common::types::DomainId;
use crate::rtps::messages::tcp_control_message::{HandshakeData, TcpControlMessage};
use crate::rtps::transport::tcp::framing::{write_framed_message, FramedReader};

/// Default handshake timeout (10 secs)
const DEFAULT_HANDSHAKE_TIMEOUT_SECS: u64 = 10;

/// State of a TCP connection during handshake
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConnectionState {
    // TCP connected, waiting to send or receive Handshake
    Connected,
    // Handshake sent, waiting for HandshakeAck
    HandshakeSent(Instant),
    // Handshake received, HandshakeAck sent - ready for data connection
    Established(HandshakeData),
}

/// Result of a handshake
#[derive(Debug)]
pub(crate) enum HandshakeResult {
    // Handshake completed, remote participant info returned
    Completed(HandshakeData),
    // Handshake in progress, need more data
    InProgress,
    // Handshake timed out
    TimedOut(SocketAddr),
}

/// Manages DDS-level handshake over TCP control connections
///
/// After OS-level TCP connect/accept, this module performs a DDS handshake
/// to exchange participant identity (GuidPrefix, DomainId, data port)
/// before allowing RTPS message exchange on the data connection.
pub(crate) struct TcpHandshakeManager {
    /// Local participant info for handshake
    local_guid_prefix: GuidPrefix,
    local_domain_id: DomainId,
    local_data_port: u16,

    /// Connection states: SocketAddr -> state
    states: HashMap<SocketAddr, ConnectionState>,

    /// Handshake timeout duration
    timeout: Duration,
}

impl TcpHandshakeManager {
    pub(crate) fn new(
        local_guid_prefix: GuidPrefix,
        local_domain_id: DomainId,
        local_data_port: u16,
    ) -> Self {
        Self {
            local_guid_prefix,
            local_domain_id,
            local_data_port,
            states: HashMap::new(),
            timeout: Duration::from_secs(DEFAULT_HANDSHAKE_TIMEOUT_SECS),
        }
    }

    /// Build the local handshake data
    fn local_handshake_data(&self) -> HandshakeData {
        HandshakeData {
            guid_prefix: self.local_guid_prefix,
            domain_id: self.local_domain_id,
            data_port: self.local_data_port,
        }
    }

    /// Initiate handshake as the connecting side (client)
    ///
    /// Called after TCP connect succeeds. Sends Handshake message
    /// and transitions to HandshakeSent state.
    pub(crate) fn initiate_handshake<W: io::Write>(
        &mut self,
        addr: SocketAddr,
        stream: &mut W,
    ) -> io::Result<()> {
        let msg = TcpControlMessage::Handshake(self.local_handshake_data());
        let bytes = msg.serialize()?;
        write_framed_message(stream, &bytes)?;

        self.states.insert(addr, ConnectionState::HandshakeSent(Instant::now()));
        info!("[TcpHandshake] Sent Handshake to {:?}", addr);
        Ok(())
    }

    /// Process a received control message during handshake
    ///
    /// Called when framed data arrives on a control connection that
    /// has not yet completed handshake.
    ///
    /// Returns:
    /// - `Ok(HandshakeResult::Completed(data))` — handshake done, data connection can proceed
    /// - `Ok(HandshakeResult::InProgress)` — waiting for more messages
    /// - `Err` — protocol error (unexpected message, invalid data)
    pub(crate) fn handle_message<W: io::Write>(
        &mut self,
        addr: SocketAddr,
        data: &[u8],
        stream: &mut W,
    ) -> io::Result<HandshakeResult> {
        let msg = TcpControlMessage::deserialize(data)?;

        match msg {
            // Server side: received Handshake from client
            TcpControlMessage::Handshake(remote_data) => {
                info!(
                    "[TcpHandshake] Received Handshake from {:?}, domain_id={}, data_port={}",
                    addr, remote_data.domain_id, remote_data.data_port,
                );

                // Validate domain_id
                if remote_data.domain_id != self.local_domain_id {
                    warn!(
                        "[TcpHandshake] Domain mismatch from {:?}: local={}, remote={}",
                        addr, self.local_domain_id, remote_data.domain_id,
                    );
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "Domain ID mismatch: local={}, remote={}",
                            self.local_domain_id, remote_data.domain_id,
                        ),
                    ));
                }

                // Send HandshakeAck
                let ack = TcpControlMessage::HandshakeAck(self.local_handshake_data());
                let bytes = ack.serialize()?;
                write_framed_message(stream, &bytes)?;

                self.states.insert(addr, ConnectionState::Established(remote_data.clone()));
                info!("[TcpHandshake] Sent HandshakeAck to {:?}", addr);

                Ok(HandshakeResult::Completed(remote_data))
            }

            // Client side: received HandshakeAck from server
            TcpControlMessage::HandshakeAck(remote_data) => match self.states.get(&addr) {
                Some(ConnectionState::HandshakeSent(_)) => {
                    info!(
                            "[TcpHandshake] Received HandshakeAck from {:?}, domain_id={}, data_port={}",
                            addr, remote_data.domain_id, remote_data.data_port,
                        );

                    self.states.insert(addr, ConnectionState::Established(remote_data.clone()));

                    Ok(HandshakeResult::Completed(remote_data))
                }
                other => {
                    warn!(
                        "[TcpHandshake] Unexpected HandshakeAck from {:?}, state={:?}",
                        addr, other,
                    );
                    Err(io::Error::new(io::ErrorKind::InvalidData, "Unexpected HandshakeAck"))
                }
            },

            other => {
                warn!(
                    "[TcpHandshake] Unexpected message during handshake from {:?}: {:?}",
                    addr, other,
                );
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Unexpected message during handshake: {:?}", other),
                ))
            }
        }
    }

    /// Check if a connection has completed handshake
    pub(crate) fn is_established(&self, addr: &SocketAddr) -> bool {
        matches!(self.states.get(addr), Some(ConnectionState::Established(_)))
    }

    /// Get the remote handshake date for an established connection
    pub(crate) fn remote_data(&self, addr: &SocketAddr) -> Option<&HandshakeData> {
        match self.states.get(addr) {
            Some(ConnectionState::Established(data)) => Some(data),
            _ => None,
        }
    }

    /// Check for timed-out handshakes and return their addresses
    pub(crate) fn check_timeouts(&mut self) -> Vec<SocketAddr> {
        let timeout = self.timeout;

        // filtering handshakeAck Message un-received connections
        let timed_out: Vec<SocketAddr> = self
            .states
            .iter()
            .filter_map(|(addr, state)| match state {
                ConnectionState::HandshakeSent(sent_at) if sent_at.elapsed() >= timeout => {
                    Some(*addr)
                }
                _ => None,
            })
            .collect();

        for addr in &timed_out {
            warn!("[TcpHandshake] Handshake timed out for {:?}", addr);
            self.states.remove(addr);
        }

        timed_out
    }

    /// Remove connection state (called when connection is closed)
    pub(crate) fn remove(&mut self, addr: &SocketAddr) {
        self.states.remove(addr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    use crate::rtps::transport::tcp::framing::read_framed_message;

    const TEST_GUID_A: GuidPrefix = [0x01; 12];
    const TEST_GUID_B: GuidPrefix = [0x02; 12];
    const TEST_DOMAIN: DomainId = 77;
    const TEST_PORT_A: u16 = 7411;
    const TEST_PORT_B: u16 = 7413;

    fn test_addr(port: u16) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], port))
    }

    #[test]
    fn test_full_handshake_flow() {
        let addr_a = test_addr(9000);
        let addr_b = test_addr(9001);

        let mut manager_a = TcpHandshakeManager::new(TEST_GUID_A, TEST_DOMAIN, TEST_PORT_A);
        let mut manager_b = TcpHandshakeManager::new(TEST_GUID_B, TEST_DOMAIN, TEST_PORT_B);

        // Step 1: A initiates handshake → sends Handshake to B
        let mut wire_a_to_b = Vec::new();
        manager_a.initiate_handshake(addr_b, &mut wire_a_to_b).unwrap();
        assert!(!manager_a.is_established(&addr_b));

        // Step 2: B receives Handshake, sends HandshakeAck
        let handshake_bytes = read_framed_message(&mut Cursor::new(&wire_a_to_b)).unwrap();
        let mut wire_b_to_a = Vec::new();
        let result = manager_b.handle_message(addr_a, &handshake_bytes, &mut wire_b_to_a).unwrap();

        match result {
            HandshakeResult::Completed(data) => {
                assert_eq!(data.guid_prefix, TEST_GUID_A);
                assert_eq!(data.domain_id, TEST_DOMAIN);
                assert_eq!(data.data_port, TEST_PORT_A);
            }
            _ => panic!("Expected Completed"),
        }
        assert!(manager_b.is_established(&addr_a));

        // Step 3: A receives HandshakeAck
        let ack_bytes = read_framed_message(&mut Cursor::new(&wire_b_to_a)).unwrap();
        let result = manager_a.handle_message(addr_b, &ack_bytes, &mut io::sink()).unwrap();

        match result {
            HandshakeResult::Completed(data) => {
                assert_eq!(data.guid_prefix, TEST_GUID_B);
                assert_eq!(data.domain_id, TEST_DOMAIN);
                assert_eq!(data.data_port, TEST_PORT_B);
            }
            _ => panic!("Expected Completed"),
        }
        assert!(manager_a.is_established(&addr_b));
    }

    #[test]
    fn test_domain_mismatch() {
        let addr = test_addr(9000);

        let mut manager = TcpHandshakeManager::new(TEST_GUID_B, 99, TEST_PORT_B);

        // Handshake with domain_id=77, but manager expects 99
        let handshake = TcpControlMessage::Handshake(HandshakeData {
            guid_prefix: TEST_GUID_A,
            domain_id: TEST_DOMAIN, // 77 != 99
            data_port: TEST_PORT_A,
        });
        let bytes = handshake.serialize().unwrap();

        let result = manager.handle_message(addr, &bytes, &mut io::sink());
        assert!(result.is_err());
    }

    #[test]
    fn test_unexpected_ack_without_handshake() {
        let addr = test_addr(9000);

        let mut manager = TcpHandshakeManager::new(TEST_GUID_A, TEST_DOMAIN, TEST_PORT_A);

        let ack = TcpControlMessage::HandshakeAck(HandshakeData {
            guid_prefix: TEST_GUID_B,
            domain_id: TEST_DOMAIN,
            data_port: TEST_PORT_B,
        });
        let bytes = ack.serialize().unwrap();

        let result = manager.handle_message(addr, &bytes, &mut io::sink());
        assert!(result.is_err());
    }

    #[test]
    fn test_timeout_check() {
        let addr = test_addr(9000);

        let mut manager = TcpHandshakeManager {
            local_guid_prefix: TEST_GUID_A,
            local_domain_id: TEST_DOMAIN,
            local_data_port: TEST_PORT_A,
            states: HashMap::new(),
            timeout: Duration::from_millis(0), // instant timeout for test
        };

        // Insert a HandshakeSent state with past timestamp
        manager.states.insert(addr, ConnectionState::HandshakeSent(Instant::now()));

        // Should detect timeout
        std::thread::sleep(Duration::from_millis(1));
        let timed_out = manager.check_timeouts();
        assert_eq!(timed_out, vec![addr]);
        assert!(!manager.states.contains_key(&addr));
    }

    #[test]
    fn test_remove() {
        let addr = test_addr(9000);

        let mut manager = TcpHandshakeManager::new(TEST_GUID_A, TEST_DOMAIN, TEST_PORT_A);

        manager.states.insert(addr, ConnectionState::Connected);
        assert!(manager.states.contains_key(&addr));

        manager.remove(&addr);
        assert!(!manager.states.contains_key(&addr));
    }
}
