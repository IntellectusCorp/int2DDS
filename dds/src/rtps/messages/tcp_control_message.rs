use std::io::{self, Cursor, Read, Write};

use crate::rtps::common::guid::GuidPrefix;

/// Control message kind for TCP connection management
///
/// Used on the control connection to manage the lifecycle of TCP sessions.
/// Data connections carry only RTPS messages and do not use these kinds.
///
/// Wire format (wrapped by length-prefix framing):
///   [len(4B)][kind(1B)][payload(variable)]

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TcpControlMessageKind {
    Handshake = 0x01,
    HandshakeAck = 0x02,
    Keepalive = 0x03,
    KeepaliveAck = 0x04,
    Close = 0x05,
}

impl TryFrom<u8> for TcpControlMessageKind {
    type Error = io::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::Handshake),
            0x02 => Ok(Self::HandshakeAck),
            0x03 => Ok(Self::Keepalive),
            0x04 => Ok(Self::KeepaliveAck),
            0x05 => Ok(Self::Close),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Unknown control message kind: 0x{:02X}", value),
            )),
        }
    }
}

/// Handshake payload exchanged when establishing a control connection
///
/// Contains the minimum information needed to identify the remote participant
/// and establish the corresponding data connection.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HandshakeData {
    pub guid_prefix: GuidPrefix,
    pub domain_id: u32,
    pub data_port: u16,
}

impl HandshakeData {
    const SERIALIZED_SIZE: usize = 12 + 4 + 2; // 18 Bytes

    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.guid_prefix)?;
        writer.write_all(&self.domain_id.to_be_bytes())?;
        writer.write_all(&self.data_port.to_be_bytes())?;
        Ok(())
    }

    fn deserialize<R: Read>(reader: &mut R) -> io::Result<Self> {
        let mut guid_prefix_bytes = [0u8; 12];
        reader.read_exact(&mut guid_prefix_bytes)?;

        let mut domain_id_bytes = [0u8; 4];
        reader.read_exact(&mut domain_id_bytes)?;

        let mut data_port_bytes = [0u8; 2];
        reader.read_exact(&mut data_port_bytes)?;

        Ok(Self {
            guid_prefix: guid_prefix_bytes,
            domain_id: u32::from_be_bytes(domain_id_bytes),
            data_port: u16::from_be_bytes(data_port_bytes),
        })
    }
}

/// TCP control message
///
/// Sent over the control connection to manage TCP session lifecycle.
/// The data connection carries only raw RTPS messages with no control wrapper.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TcpControlMessage {
    Handshake(HandshakeData),
    HandshakeAck(HandshakeData),
    Keepalive,
    KeepaliveAck,
    Close,
}

impl TcpControlMessage {
    pub(crate) fn serialize(&self) -> io::Result<Vec<u8>> {
        let mut buf = Vec::new();

        match self {
            Self::Handshake(data) => {
                buf.push(TcpControlMessageKind::Handshake as u8);
                data.serialize(&mut buf)?;
            }
            Self::HandshakeAck(data) => {
                buf.push(TcpControlMessageKind::HandshakeAck as u8);
                data.serialize(&mut buf)?;
            }
            Self::Keepalive => {
                buf.push(TcpControlMessageKind::Keepalive as u8);
            }
            Self::KeepaliveAck => {
                buf.push(TcpControlMessageKind::KeepaliveAck as u8);
            }
            Self::Close => {
                buf.push(TcpControlMessageKind::Close as u8);
            }
        }
        Ok(buf)
    }

    pub(crate) fn deserialize(data: &[u8]) -> io::Result<Self> {
        if data.is_empty() {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Empty control message"));
        }

        let kind = TcpControlMessageKind::try_from(data[0])?;
        let mut reader = Cursor::new(&data[1..]);

        match kind {
            TcpControlMessageKind::Handshake => {
                let handshake = HandshakeData::deserialize(&mut reader)?;
                Ok(Self::Handshake(handshake))
            }
            TcpControlMessageKind::HandshakeAck => {
                let handshake = HandshakeData::deserialize(&mut reader)?;
                Ok(Self::HandshakeAck(handshake))
            }
            TcpControlMessageKind::Keepalive => Ok(Self::Keepalive),
            TcpControlMessageKind::KeepaliveAck => Ok(Self::KeepaliveAck),
            TcpControlMessageKind::Close => Ok(Self::Close),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_guid_prefix() -> GuidPrefix {
        [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
    }

    #[test]
    fn test_handshake_roundtrip() {
        let data =
            HandshakeData { guid_prefix: test_guid_prefix(), domain_id: 77, data_port: 7411 };

        let msg = TcpControlMessage::Handshake(data.clone());
        let bytes = msg.serialize().unwrap();
        let decoded = TcpControlMessage::deserialize(&bytes).unwrap();

        assert_eq!(decoded, TcpControlMessage::Handshake(data));
    }

    #[test]
    fn test_handshake_ack_roundtrip() {
        let data = HandshakeData { guid_prefix: [0xAA; 12], domain_id: 0, data_port: 26661 };

        let msg = TcpControlMessage::HandshakeAck(data.clone());
        let bytes = msg.serialize().unwrap();
        let decoded = TcpControlMessage::deserialize(&bytes).unwrap();

        assert_eq!(decoded, TcpControlMessage::HandshakeAck(data));
    }

    #[test]
    fn test_keepalive_roundtrip() {
        let bytes = TcpControlMessage::Keepalive.serialize().unwrap();
        assert_eq!(bytes, vec![0x03]);

        let decoded = TcpControlMessage::deserialize(&bytes).unwrap();
        assert_eq!(decoded, TcpControlMessage::Keepalive);
    }

    #[test]
    fn test_keepalive_ack_roundtrip() {
        let bytes = TcpControlMessage::KeepaliveAck.serialize().unwrap();
        assert_eq!(bytes, vec![0x04]);

        let decoded = TcpControlMessage::deserialize(&bytes).unwrap();
        assert_eq!(decoded, TcpControlMessage::KeepaliveAck);
    }

    #[test]
    fn test_close_roundtrip() {
        let bytes = TcpControlMessage::Close.serialize().unwrap();
        assert_eq!(bytes, vec![0x05]);

        let decoded = TcpControlMessage::deserialize(&bytes).unwrap();
        assert_eq!(decoded, TcpControlMessage::Close);
    }

    #[test]
    fn test_unknown_kind() {
        let result = TcpControlMessage::deserialize(&[0xFF]);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_data() {
        let result = TcpControlMessage::deserialize(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_handshake_serialized_size() {
        let data = HandshakeData { guid_prefix: [0u8; 12], domain_id: 0, data_port: 0 };

        let msg = TcpControlMessage::Handshake(data);
        let bytes = msg.serialize().unwrap();

        // kind(1) + guid_prefix(12) + domain_id(4) + data_port(2) = 19
        assert_eq!(bytes.len(), 1 + HandshakeData::SERIALIZED_SIZE);
    }
}
