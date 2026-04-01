#![allow(dead_code)]
#![allow(unused_variables)]

//! int2DDS TCP Control Protocol (v2)
//!
//! ## Control Messages
//!
//! All control messages start with a 1-byte msg_type after the frame magic.
//! The msg_type ranges (0x01~0x0A) do not overlap with RTPS magic (0x52='R').
//!
//! | Type | Name             | Direction        | Payload                    |
//! |------|------------------|------------------|----------------------------|
//! | 0x01 | PEER_HELLO       | Client → Server  | 16B server_locator         |
//! | 0x02 | PEER_HELLO_ACK   | Server → Client  | (empty)                    |
//! | 0x03 | PORT_RESERVE     | Client → Server  | 2B logical_port (BE)       |
//! | 0x04 | PORT_RESERVE_ACK | Server → Client  | 16B connection_cookie      |
//! | 0x05 | PORT_BIND        | Client → Server  | 16B connection_cookie      |
//! | 0x06 | PORT_BIND_ACK    | Server → Client  | (empty)                    |
//! | 0x08 | KEEPALIVE        | Either           | (empty)                    |
//! | 0x09 | KEEPALIVE_ACK    | Either           | (empty)                    |
//! | 0x0A | ERROR            | Server → Client  | 2B code + 2B len + string  |

use std::io;
use std::net::Ipv4Addr;

// ─── Message Types ──────────────────────────────────────────────────────────

pub(crate) const MSG_PEER_HELLO: u8 = 0x01;
pub(crate) const MSG_PEER_HELLO_ACK: u8 = 0x02;
pub(crate) const MSG_PORT_RESERVE: u8 = 0x03;
pub(crate) const MSG_PORT_RESERVE_ACK: u8 = 0x04;
pub(crate) const MSG_PORT_BIND: u8 = 0x05;
pub(crate) const MSG_PORT_BIND_ACK: u8 = 0x06;
pub(crate) const MSG_KEEPALIVE: u8 = 0x08;
pub(crate) const MSG_KEEPALIVE_ACK: u8 = 0x09;
pub(crate) const MSG_ERROR: u8 = 0x0A;

// ─── Locator Encoding ───────────────────────────────────────────────────────

/// Encode an IPv4 address + port into a 16-byte locator.
///
/// Format: `[4B zeros][4B zeros][0xFF 0xFF][2B port BE][4B IPv4]`
pub(crate) fn encode_locator(ip: Ipv4Addr, port: u16) -> [u8; 16] {
    let mut loc = [0u8; 16];
    loc[8] = 0xFF;
    loc[9] = 0xFF;
    loc[10..12].copy_from_slice(&port.to_be_bytes());
    loc[12..16].copy_from_slice(&ip.octets());
    loc
}

/// Decode an IPv4 address + port from a 16-byte locator.
pub(crate) fn decode_locator(loc: &[u8; 16]) -> (Ipv4Addr, u16) {
    let port = u16::from_be_bytes([loc[10], loc[11]]);
    let ip = Ipv4Addr::new(loc[12], loc[13], loc[14], loc[15]);
    (ip, port)
}

// ─── Control Message ────────────────────────────────────────────────────────

/// A parsed control message (payload after frame magic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ControlMsg {
    /// Client announces its listener address.
    PeerHello { locator: [u8; 16] },

    /// Server acknowledges peer hello.
    PeerHelloAck,

    /// Client requests reservation of a logical port.
    PortReserve { logical_port: u16 },

    /// Server confirms reservation with a cookie.
    PortReserveAck { cookie: [u8; 16] },

    /// Client binds a data connection using a previously issued cookie.
    PortBind { cookie: [u8; 16] },

    /// Server confirms the data connection is bound.
    PortBindAck,

    /// Connection keepalive ping.
    Keepalive,

    /// Keepalive acknowledgment.
    KeepaliveAck,

    /// Error response with code and message.
    Error { code: u16, message: String },
}

impl ControlMsg {
    /// Serialize to bytes (the payload portion, after frame magic).
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            ControlMsg::PeerHello { locator } => {
                let mut buf = Vec::with_capacity(17);
                buf.push(MSG_PEER_HELLO);
                buf.extend_from_slice(locator);
                buf
            }
            ControlMsg::PeerHelloAck => vec![MSG_PEER_HELLO_ACK],

            ControlMsg::PortReserve { logical_port } => {
                let mut buf = Vec::with_capacity(3);
                buf.push(MSG_PORT_RESERVE);
                buf.extend_from_slice(&logical_port.to_be_bytes());
                buf
            }
            ControlMsg::PortReserveAck { cookie } => {
                let mut buf = Vec::with_capacity(17);
                buf.push(MSG_PORT_RESERVE_ACK);
                buf.extend_from_slice(cookie);
                buf
            }

            ControlMsg::PortBind { cookie } => {
                let mut buf = Vec::with_capacity(17);
                buf.push(MSG_PORT_BIND);
                buf.extend_from_slice(cookie);
                buf
            }
            ControlMsg::PortBindAck => vec![MSG_PORT_BIND_ACK],

            ControlMsg::Keepalive => vec![MSG_KEEPALIVE],
            ControlMsg::KeepaliveAck => vec![MSG_KEEPALIVE_ACK],

            ControlMsg::Error { code, message } => {
                let mut buf = Vec::with_capacity(5 + message.len());
                buf.push(MSG_ERROR);
                buf.extend_from_slice(&code.to_be_bytes());
                buf.extend_from_slice(&(message.len() as u16).to_be_bytes());
                buf.extend_from_slice(message.as_bytes());
                buf
            }
        }
    }

    /// Deserialize from bytes (payload after frame magic).
    pub(crate) fn from_bytes(payload: &[u8]) -> io::Result<Self> {
        if payload.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Empty control payload"));
        }

        match payload[0] {
            MSG_PEER_HELLO => {
                if payload.len() < 17 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "PeerHello too short"));
                }
                let mut locator = [0u8; 16];
                locator.copy_from_slice(&payload[1..17]);
                Ok(ControlMsg::PeerHello { locator })
            }
            MSG_PEER_HELLO_ACK => Ok(ControlMsg::PeerHelloAck),

            MSG_PORT_RESERVE => {
                if payload.len() < 3 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "PortReserve too short",
                    ));
                }
                let logical_port = u16::from_be_bytes([payload[1], payload[2]]);
                Ok(ControlMsg::PortReserve { logical_port })
            }
            MSG_PORT_RESERVE_ACK => {
                if payload.len() < 17 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "PortReserveAck too short",
                    ));
                }
                let mut cookie = [0u8; 16];
                cookie.copy_from_slice(&payload[1..17]);
                Ok(ControlMsg::PortReserveAck { cookie })
            }

            MSG_PORT_BIND => {
                if payload.len() < 17 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "PortBind too short"));
                }
                let mut cookie = [0u8; 16];
                cookie.copy_from_slice(&payload[1..17]);
                Ok(ControlMsg::PortBind { cookie })
            }
            MSG_PORT_BIND_ACK => Ok(ControlMsg::PortBindAck),

            MSG_KEEPALIVE => Ok(ControlMsg::Keepalive),
            MSG_KEEPALIVE_ACK => Ok(ControlMsg::KeepaliveAck),

            MSG_ERROR => {
                if payload.len() < 5 {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Error too short"));
                }
                let code = u16::from_be_bytes([payload[1], payload[2]]);
                let msg_len = u16::from_be_bytes([payload[3], payload[4]]) as usize;
                let message = if payload.len() >= 5 + msg_len {
                    String::from_utf8_lossy(&payload[5..5 + msg_len]).to_string()
                } else {
                    String::new()
                };
                Ok(ControlMsg::Error { code, message })
            }

            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown control message type: 0x{:02X}", other),
            )),
        }
    }

    /// Get the message type name for logging.
    pub(crate) fn type_name(&self) -> &'static str {
        match self {
            ControlMsg::PeerHello { .. } => "PEER_HELLO",
            ControlMsg::PeerHelloAck => "PEER_HELLO_ACK",
            ControlMsg::PortReserve { .. } => "PORT_RESERVE",
            ControlMsg::PortReserveAck { .. } => "PORT_RESERVE_ACK",
            ControlMsg::PortBind { .. } => "PORT_BIND",
            ControlMsg::PortBindAck => "PORT_BIND_ACK",
            ControlMsg::Keepalive => "KEEPALIVE",
            ControlMsg::KeepaliveAck => "KEEPALIVE_ACK",
            ControlMsg::Error { .. } => "ERROR",
        }
    }
}

// ─── Cookie Generator ───────────────────────────────────────────────────────

/// Generate a connection cookie with a monotonically increasing first byte.
/// Returns a 16-byte cookie: [counter, 0, 0, ...]
pub(crate) fn generate_cookie(counter: &mut u8) -> [u8; 16] {
    let mut cookie = [0u8; 16];
    cookie[0] = *counter;
    *counter = counter.wrapping_add(1);
    cookie
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_hello_roundtrip() {
        let loc = encode_locator(Ipv4Addr::new(172, 19, 117, 172), 7400);
        let msg = ControlMsg::PeerHello { locator: loc };
        let bytes = msg.to_bytes();
        assert_eq!(bytes[0], MSG_PEER_HELLO);
        assert_eq!(bytes.len(), 17);

        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_port_reserve_roundtrip() {
        let msg = ControlMsg::PortReserve { logical_port: 7410 };
        let bytes = msg.to_bytes();
        assert_eq!(bytes, vec![0x03, 0x1C, 0xF2]);

        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_port_reserve_ack_roundtrip() {
        let mut counter = 0x31u8;
        let cookie = generate_cookie(&mut counter);
        assert_eq!(cookie[0], 0x31);
        assert_eq!(counter, 0x32);

        let msg = ControlMsg::PortReserveAck { cookie };
        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_port_bind_roundtrip() {
        let mut cookie = [0u8; 16];
        cookie[0] = 0x31;
        let msg = ControlMsg::PortBind { cookie };
        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_keepalive_roundtrip() {
        let msg = ControlMsg::Keepalive;
        let bytes = msg.to_bytes();
        assert_eq!(bytes, vec![MSG_KEEPALIVE]);
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_error_roundtrip() {
        let msg = ControlMsg::Error { code: 0x0001, message: "no matching port".to_string() };
        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_locator_encoding() {
        let loc = encode_locator(Ipv4Addr::new(172, 19, 117, 172), 7401);
        assert_eq!(loc, [0, 0, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0x1C, 0xE9, 0xAC, 0x13, 0x75, 0xAC]);
        let (ip, port) = decode_locator(&loc);
        assert_eq!(ip, Ipv4Addr::new(172, 19, 117, 172));
        assert_eq!(port, 7401);
    }

    #[test]
    fn test_all_simple_types() {
        for msg in [
            ControlMsg::PeerHelloAck,
            ControlMsg::PortBindAck,
            ControlMsg::Keepalive,
            ControlMsg::KeepaliveAck,
        ] {
            let bytes = msg.to_bytes();
            assert_eq!(bytes.len(), 1);
            let parsed = ControlMsg::from_bytes(&bytes).unwrap();
            assert_eq!(parsed, msg);
        }
    }
}
