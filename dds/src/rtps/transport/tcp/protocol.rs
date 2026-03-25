#![allow(dead_code)]
#![allow(unused_variables)]

use std::io::{self, Read, Write};

use crate::rtps::common::guid::GuidPrefix;

/// TCP control protocol magic bytes: "INT2"
const PROTOCOL_MAGIC: [u8; 4] = [b'I', b'N', b'T', b'2'];

/// Protocol version
const PROTOCOL_VERSION_MAJOR: u8 = 1;
const PROTOCOL_VERSION_MINOR: u8 = 0;

/// Control message type identifiers
/// These values (0x01~0x07) do not overlap with RTPS magic (0x52='R'),
/// allowing safe classification of TCP frame payloads.
const MSG_TYPE_BIND_REQUEST: u8 = 0x01;
const MSG_TYPE_BIND_RESPONSE: u8 = 0x02;
const MSG_TYPE_KEEPALIVE: u8 = 0x04;
const MSG_TYPE_KEEPALIVE_ACK: u8 = 0x05;
const MSG_TYPE_CLOSE: u8 = 0x07;

/// BindRequest body size (excluding msg_type byte)
/// [4B magic][1B major][1B minor][12B guid_prefix][4B domain_id][4B participant_id]
/// [2B listener_port][1B bind_type][2B logical_port] = 31 bytes
const BIND_REQUEST_BODY_SIZE: usize = 31;

/// BindResponse body size (excluding msg_type byte)
/// [1B status][12B guid_prefix][4B domain_id][4B participant_id][2B listener_port] = 23 bytes
const BIND_RESPONSE_BODY_SIZE: usize = 23;

/// Bind type: what kind of TCP connection this is
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindType {
    /// Control channel (keepalive, liveliness, connectino management)
    Control = 0x00,
    /// RTPS data channel (discovery or user data, identified by logical_port)
    RtpsData = 0x01,
}

impl BindType {
    fn from_u8(value: u8) -> io::Result<Self> {
        match value {
            0x00 => Ok(BindType::Control),
            0x01 => Ok(BindType::RtpsData),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown bind type: 0x{:02X}", value),
            )),
        }
    }
}

/// Bind response status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindStatus {
    Ok = 0x00,
    DomainMismatch = 0x01,
    InvalidRequest = 0x02,
    Rejected = 0x03,
}

impl BindStatus {
    fn from_u8(value: u8) -> io::Result<Self> {
        match value {
            0x00 => Ok(BindStatus::Ok),
            0x01 => Ok(BindStatus::DomainMismatch),
            0x02 => Ok(BindStatus::InvalidRequest),
            0x03 => Ok(BindStatus::Rejected),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown bind status: 0x{:02X}", value),
            )),
        }
    }
}

/// Tcp control message types
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ControlMsg {
    BindRequest(BindRequest),
    BindResponse(BindResponse),
    Keepalive,
    KeepaliveAck,
    Close,
}

/// Connection initiator -> acceptor
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BindRequest {
    pub(crate) guid_prefix: GuidPrefix,
    pub(crate) domain_id: u32,
    pub(crate) participant_id: u32,
    pub(crate) listener_port: u16,
    pub(crate) bind_type: BindType,
    pub(crate) logical_port: u16,
}

/// Acceptor -> connection initiator
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BindResponse {
    pub(crate) status: BindStatus,
    pub(crate) guid_prefix: GuidPrefix,
    pub(crate) domain_id: u32,
    pub(crate) participant_id: u32,
    pub(crate) listener_port: u16,
}

impl ControlMsg {
    /// Serialize a control message into a byte payload.
    /// The first byte is the message type identifier.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        match self {
            ControlMsg::BindRequest(req) => {
                let mut buf = Vec::with_capacity(1 + BIND_REQUEST_BODY_SIZE);
                buf.push(MSG_TYPE_BIND_REQUEST);
                buf.extend_from_slice(&PROTOCOL_MAGIC);
                buf.push(PROTOCOL_VERSION_MAJOR);
                buf.push(PROTOCOL_VERSION_MINOR);
                buf.extend_from_slice(&req.guid_prefix);
                buf.extend_from_slice(&req.domain_id.to_be_bytes());
                buf.extend_from_slice(&req.participant_id.to_be_bytes());
                buf.extend_from_slice(&req.listener_port.to_be_bytes());
                buf.push(req.bind_type as u8);
                buf.extend_from_slice(&req.logical_port.to_be_bytes());
                buf
            }
            ControlMsg::BindResponse(resp) => {
                let mut buf = Vec::with_capacity(1 + BIND_RESPONSE_BODY_SIZE);
                buf.push(MSG_TYPE_BIND_RESPONSE);
                buf.push(resp.status as u8);
                buf.extend_from_slice(&resp.guid_prefix);
                buf.extend_from_slice(&resp.domain_id.to_be_bytes());
                buf.extend_from_slice(&resp.participant_id.to_be_bytes());
                buf.extend_from_slice(&resp.listener_port.to_be_bytes());
                buf
            }
            ControlMsg::Keepalive => vec![MSG_TYPE_KEEPALIVE],
            ControlMsg::KeepaliveAck => vec![MSG_TYPE_KEEPALIVE_ACK],
            ControlMsg::Close => vec![MSG_TYPE_CLOSE],
        }
    }

    /// Deserialize a control message from a payload buffer.
    /// The first byte must be the message type identifier.
    pub(crate) fn from_bytes(payload: &[u8]) -> io::Result<Self> {
        if payload.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Empty control message payload",
            ));
        }

        match payload[0] {
            MSG_TYPE_BIND_REQUEST => Self::parse_bind_request(&payload[1..]),
            MSG_TYPE_BIND_RESPONSE => Self::parse_bind_response(&payload[1..]),
            MSG_TYPE_KEEPALIVE => Ok(ControlMsg::Keepalive),
            MSG_TYPE_KEEPALIVE_ACK => Ok(ControlMsg::KeepaliveAck),
            MSG_TYPE_CLOSE => Ok(ControlMsg::Close),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown control message type: 0x{:02X}", other),
            )),
        }
    }

    fn parse_bind_request(body: &[u8]) -> io::Result<Self> {
        if body.len() < BIND_REQUEST_BODY_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "BindRequest too short: {} bytes (expected {})",
                    body.len(),
                    BIND_REQUEST_BODY_SIZE
                ),
            ));
        }

        // Validate magic
        if body[0..4] != PROTOCOL_MAGIC {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid protocol magic: {:?}", &body[0..4]),
            ));
        }

        // Version (bytes 4,5) — log but accept any version for forward compatibility
        let _major = body[4];
        let _minor = body[5];

        let mut guid_prefix: GuidPrefix = [0u8; 12];
        guid_prefix.copy_from_slice(&body[6..18]);

        let domain_id = u32::from_be_bytes(body[18..22].try_into().unwrap());
        let participant_id = u32::from_be_bytes(body[22..26].try_into().unwrap());
        let listener_port = u16::from_be_bytes(body[26..28].try_into().unwrap());
        let bind_type = BindType::from_u8(body[28])?;
        let logical_port = u16::from_be_bytes(body[29..31].try_into().unwrap());

        Ok(ControlMsg::BindRequest(BindRequest {
            guid_prefix,
            domain_id,
            participant_id,
            listener_port,
            bind_type,
            logical_port,
        }))
    }

    fn parse_bind_response(body: &[u8]) -> io::Result<Self> {
        if body.len() < BIND_RESPONSE_BODY_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "BindResponse too short: {} bytes (expected {})",
                    body.len(),
                    BIND_RESPONSE_BODY_SIZE
                ),
            ));
        }

        let status = BindStatus::from_u8(body[0])?;

        let mut guid_prefix: GuidPrefix = [0u8; 12];
        guid_prefix.copy_from_slice(&body[1..13]);

        let domain_id = u32::from_be_bytes(body[13..17].try_into().unwrap());
        let participant_id = u32::from_be_bytes(body[17..21].try_into().unwrap());
        let listener_port = u16::from_be_bytes(body[21..23].try_into().unwrap());

        Ok(ControlMsg::BindResponse(BindResponse {
            status,
            guid_prefix,
            domain_id,
            participant_id,
            listener_port,
        }))
    }
}

/// Write a control message to a TCP stream using the same framing as RTPS data.
/// Format: [4B length (big-endian)][control message payload]
pub(crate) fn write_control_message<W: Write>(stream: &mut W, msg: &ControlMsg) -> io::Result<()> {
    let payload = msg.to_bytes();
    let len = payload.len() as u32;
    stream.write_all(&len.to_be_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()?;
    Ok(())
}

/// Read a control message from a TCP stream using the same framing as RTPS data.
/// Format: [4B length (big-endian)][control message payload]
pub(crate) fn read_control_message<R: Read>(stream: &mut R) -> io::Result<ControlMsg> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid control message length: 0",
        ));
    }

    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload)?;

    ControlMsg::from_bytes(&payload)
}

/// Check if a payload is a control message (vs RTPS data).
/// Control messages have first byte in range 0x01..=0x07.
/// RTPS messages start with magic 0x52545053 ('RTPS'), first byte 0x52.
pub(crate) fn is_control_message(payload: &[u8]) -> bool {
    if payload.is_empty() {
        return false;
    }
    matches!(
        payload[0],
        MSG_TYPE_BIND_REQUEST
            | MSG_TYPE_BIND_RESPONSE
            | MSG_TYPE_KEEPALIVE
            | MSG_TYPE_KEEPALIVE_ACK
            | MSG_TYPE_CLOSE
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_bind_request_roundtrip() {
        let guid_prefix: GuidPrefix =
            [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C];
        let msg = ControlMsg::BindRequest(BindRequest {
            guid_prefix,
            domain_id: 0,
            participant_id: 3,
            listener_port: 7400,
            bind_type: BindType::RtpsData,
            logical_port: 7416,
        });

        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), 1 + BIND_REQUEST_BODY_SIZE); // 32 bytes
        assert_eq!(bytes[0], MSG_TYPE_BIND_REQUEST);

        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_bind_response_roundtrip() {
        let guid_prefix: GuidPrefix = [0xAA; 12];
        let msg = ControlMsg::BindResponse(BindResponse {
            status: BindStatus::Ok,
            guid_prefix,
            domain_id: 0,
            participant_id: 2,
            listener_port: 7400,
        });

        let bytes = msg.to_bytes();
        assert_eq!(bytes.len(), 1 + BIND_RESPONSE_BODY_SIZE); // 24 bytes
        assert_eq!(bytes[0], MSG_TYPE_BIND_RESPONSE);

        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, msg);
    }

    #[test]
    fn test_keepalive_roundtrip() {
        let msg = ControlMsg::Keepalive;
        let bytes = msg.to_bytes();
        assert_eq!(bytes, vec![MSG_TYPE_KEEPALIVE]);

        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, ControlMsg::Keepalive);
    }

    #[test]
    fn test_keepalive_ack_roundtrip() {
        let msg = ControlMsg::KeepaliveAck;
        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, ControlMsg::KeepaliveAck);
    }

    #[test]
    fn test_close_roundtrip() {
        let msg = ControlMsg::Close;
        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, ControlMsg::Close);
    }

    #[test]
    fn test_write_and_read_control_message() {
        let guid_prefix: GuidPrefix = [0x11; 12];
        let msg = ControlMsg::BindRequest(BindRequest {
            guid_prefix,
            domain_id: 1,
            participant_id: 0,
            listener_port: 7650,
            bind_type: BindType::Control,
            logical_port: 0,
        });

        let mut buffer = Vec::new();
        write_control_message(&mut buffer, &msg).unwrap();

        let mut cursor = Cursor::new(buffer);
        let read_msg = read_control_message(&mut cursor).unwrap();

        assert_eq!(read_msg, msg);
    }

    #[test]
    fn test_bind_type_control_logical_port_ignored() {
        let msg = ControlMsg::BindRequest(BindRequest {
            guid_prefix: [0; 12],
            domain_id: 0,
            participant_id: 0,
            listener_port: 7400,
            bind_type: BindType::Control,
            logical_port: 0, // ignored for Control
        });

        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();

        if let ControlMsg::BindRequest(req) = parsed {
            assert_eq!(req.bind_type, BindType::Control);
        } else {
            panic!("Expected BindRequest");
        }
    }

    #[test]
    fn test_bind_response_domain_mismatch() {
        let msg = ControlMsg::BindResponse(BindResponse {
            status: BindStatus::DomainMismatch,
            guid_prefix: [0; 12],
            domain_id: 0,
            participant_id: 0,
            listener_port: 7400,
        });

        let bytes = msg.to_bytes();
        let parsed = ControlMsg::from_bytes(&bytes).unwrap();

        if let ControlMsg::BindResponse(resp) = parsed {
            assert_eq!(resp.status, BindStatus::DomainMismatch);
        } else {
            panic!("Expected BindResponse");
        }
    }

    #[test]
    fn test_is_control_message() {
        // Control messages
        assert!(is_control_message(&[MSG_TYPE_BIND_REQUEST]));
        assert!(is_control_message(&[MSG_TYPE_BIND_RESPONSE]));
        assert!(is_control_message(&[MSG_TYPE_KEEPALIVE]));
        assert!(is_control_message(&[MSG_TYPE_KEEPALIVE_ACK]));
        assert!(is_control_message(&[MSG_TYPE_CLOSE]));

        // RTPS magic starts with 0x52 ('R')
        assert!(!is_control_message(&[0x52, 0x54, 0x50, 0x53]));

        // Empty
        assert!(!is_control_message(&[]));

        // Unknown type
        assert!(!is_control_message(&[0x10]));
    }

    #[test]
    fn test_invalid_magic() {
        let mut bytes = ControlMsg::BindRequest(BindRequest {
            guid_prefix: [0; 12],
            domain_id: 0,
            participant_id: 0,
            listener_port: 7400,
            bind_type: BindType::Control,
            logical_port: 0,
        })
        .to_bytes();

        // Corrupt magic
        bytes[1] = 0xFF;
        let result = ControlMsg::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_bind_request() {
        let bytes = vec![MSG_TYPE_BIND_REQUEST, 0x01, 0x02]; // Too short
        let result = ControlMsg::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_bind_response() {
        let bytes = vec![MSG_TYPE_BIND_RESPONSE, 0x00, 0x01]; // Too short
        let result = ControlMsg::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_control_messages_stream() {
        let messages = vec![
            ControlMsg::BindRequest(BindRequest {
                guid_prefix: [0xAA; 12],
                domain_id: 0,
                participant_id: 1,
                listener_port: 7400,
                bind_type: BindType::RtpsData,
                logical_port: 7412,
            }),
            ControlMsg::BindResponse(BindResponse {
                status: BindStatus::Ok,
                guid_prefix: [0xBB; 12],
                domain_id: 0,
                participant_id: 2,
                listener_port: 7400,
            }),
            ControlMsg::Keepalive,
            ControlMsg::KeepaliveAck,
            ControlMsg::Close,
        ];

        let mut buffer = Vec::new();
        for msg in &messages {
            write_control_message(&mut buffer, msg).unwrap();
        }

        let mut cursor = Cursor::new(buffer);
        for expected in &messages {
            let read_msg = read_control_message(&mut cursor).unwrap();
            assert_eq!(&read_msg, expected);
        }
    }
}
