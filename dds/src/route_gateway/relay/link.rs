//! The link between two gateways.
//!
//! A datagram loses its own framing the moment it enters a stream, so each one
//! is length prefixed. The tag that follows the length says which of the two
//! LAN ports the datagram entered by, which is the same port the peer gateway
//! must deliver it to on the other side.

use std::io::{self, Read, Write};

/// Wide enough for a fragmented RTPS datagram, narrow enough that a corrupted
/// length cannot make the relay allocate without bound.
const MAX_FRAME_LEN: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channel {
    Metatraffic,
    UserData,
}

impl Channel {
    fn tag(self) -> u8 {
        match self {
            Channel::Metatraffic => 0,
            Channel::UserData => 1,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(Channel::Metatraffic),
            1 => Some(Channel::UserData),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) channel: Channel,
    pub(crate) payload: Vec<u8>,
}

pub(crate) fn write_frame<W: Write>(
    writer: &mut W,
    channel: Channel,
    payload: &[u8],
) -> io::Result<()> {
    if payload.len() + 1 > MAX_FRAME_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "frame too large"));
    }
    let length = (payload.len() + 1) as u32;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&[channel.tag()])?;
    writer.write_all(payload)?;
    writer.flush()
}

pub(crate) fn read_frame<R: Read>(reader: &mut R) -> io::Result<Frame> {
    let mut length_bytes = [0u8; 4];
    reader.read_exact(&mut length_bytes)?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FRAME_LEN {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid frame length"));
    }

    let mut tag = [0u8; 1];
    reader.read_exact(&mut tag)?;
    let channel = Channel::from_tag(tag[0])
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unknown frame channel"))?;

    let mut payload = vec![0u8; length - 1];
    reader.read_exact(&mut payload)?;

    Ok(Frame { channel, payload })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_survive_a_round_trip() {
        let mut stream = Vec::new();
        write_frame(&mut stream, Channel::Metatraffic, b"discovery").unwrap();
        write_frame(&mut stream, Channel::UserData, b"sample").unwrap();

        let mut cursor = stream.as_slice();
        assert_eq!(
            read_frame(&mut cursor).unwrap(),
            Frame { channel: Channel::Metatraffic, payload: b"discovery".to_vec() }
        );
        assert_eq!(
            read_frame(&mut cursor).unwrap(),
            Frame { channel: Channel::UserData, payload: b"sample".to_vec() }
        );
        assert!(read_frame(&mut cursor).is_err());
    }

    #[test]
    fn oversized_length_is_rejected_without_allocating() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&u32::MAX.to_be_bytes());
        stream.push(0);

        let err = read_frame(&mut stream.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn unknown_channel_tag_is_rejected() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&2u32.to_be_bytes());
        stream.extend_from_slice(&[9, 0]);

        let err = read_frame(&mut stream.as_slice()).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
