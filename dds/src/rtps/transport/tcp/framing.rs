//! Framing for RTPS messages carried over TCP.
//!
//! The message goes out unchanged apart from an 8-byte vendor length
//! submessage inserted right after the 20-byte RTPS header, so the stream
//! stays a valid sequence of RTPS messages. Its length field covers the whole
//! message including the RTPS header and the length submessage itself.
//!
//! The length submessage is kept in the message handed upwards: a capture and
//! the bytes RTPS processing sees are then the same thing. The submessage
//! parser skips an unknown vendor id by its octetsToNextHeader, so nothing
//! above the transport has to know about it.

use std::io::{self, IoSlice};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::rtps::common::types::{PROTOCOL_RTPS, RTPS_HEADER_LENGTH};
use crate::rtps::messages::submessage_id::SubmessageId;
use crate::rtps::messages::traffic_class::carries_builtin_writer;
use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};

pub(crate) const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
const RTPS_HEADER_SIZE: usize = RTPS_HEADER_LENGTH as usize;
const MSG_LEN_SIZE: usize = 8;
const STREAM_HEADER_SIZE: usize = RTPS_HEADER_SIZE + MSG_LEN_SIZE;
const MSG_LEN_FLAGS: u8 = 0x01;
const MSG_LEN_OCTETS_TO_NEXT_HEADER: u16 = 4;
const MAX_CACHED_BUFFERS: usize = 16;
const MAX_CACHED_BUFFER_CAPACITY: usize = 256 * 1024;

/// Bounded pool for TCP payload allocations. A buffer returns only after every
/// `Bytes` clone held by RTPS processing has been dropped.
#[derive(Default)]
pub(crate) struct TcpBufferPool {
    buffers: Mutex<Vec<Vec<u8>>>,
}

impl TcpBufferPool {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn take_owner(self: &Arc<Self>, len: usize) -> PooledBuffer {
        let mut cached = self.buffers.lock().expect("TCP buffer pool lock");
        let candidate = cached
            .iter()
            .enumerate()
            .filter(|(_, buffer)| buffer.capacity() >= len)
            .min_by_key(|(_, buffer)| buffer.capacity())
            .map(|(index, _)| index);
        let mut data = candidate
            .map(|index| cached.swap_remove(index))
            .or_else(|| cached.pop())
            .unwrap_or_default();
        drop(cached);

        data.clear();
        data.resize(len, 0);
        PooledBuffer { data, pool: Arc::clone(self) }
    }

    fn recycle(&self, mut data: Vec<u8>) {
        if data.capacity() > MAX_CACHED_BUFFER_CAPACITY {
            return;
        }
        data.clear();
        let mut cached = self.buffers.lock().expect("TCP buffer pool lock");
        if cached.len() < MAX_CACHED_BUFFERS {
            cached.push(data);
        }
    }

    pub(crate) fn copy_from_slice(self: &Arc<Self>, source: &[u8]) -> Bytes {
        let mut owner = self.take_owner(source.len());
        owner.data.copy_from_slice(source);
        Bytes::from_owner(owner)
    }

    #[cfg(test)]
    fn cached_count(&self) -> usize {
        self.buffers.lock().expect("TCP buffer pool lock").len()
    }
}

struct PooledBuffer {
    data: Vec<u8>,
    pool: Arc<TcpBufferPool>,
}

impl AsRef<[u8]> for PooledBuffer {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        self.pool.recycle(std::mem::take(&mut self.data));
    }
}

/// Routing class of a frame. The send side picks it per target; the receive
/// side recovers it from the message content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum TcpFrameKind {
    Discovery = 0x01,
    UserData = 0x02,
}

/// Fully decoded TCP frame. The payload is moved to the RTPS listener channel
/// without another copy.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TcpFrame {
    pub(crate) kind: TcpFrameKind,
    pub(crate) payload: Bytes,
}

pub(crate) fn validate_payload_size(size: usize) -> io::Result<()> {
    if size > MAX_PAYLOAD_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameTooLarge,
            format!("payload too large: {size} bytes (max: {MAX_PAYLOAD_SIZE} bytes)"),
        ));
    }
    Ok(())
}

/// Write one complete message. Vectored I/O keeps the synthesized length
/// submessage from forcing a copy of the caller's message.
pub(crate) async fn write_framed_message<W>(stream: &mut W, message: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    validate_payload_size(message.len())?;
    if message.len() < RTPS_HEADER_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidLength,
            format!("message shorter than an RTPS header: {} bytes", message.len()),
        ));
    }

    let total_len = (message.len() + MSG_LEN_SIZE) as u32;
    let mut msg_len = [0u8; MSG_LEN_SIZE];
    msg_len[0] = SubmessageId::MSG_LEN.as_u8();
    msg_len[1] = MSG_LEN_FLAGS;
    msg_len[2..4].copy_from_slice(&MSG_LEN_OCTETS_TO_NEXT_HEADER.to_le_bytes());
    msg_len[4..].copy_from_slice(&total_len.to_le_bytes());

    let mut bufs = [
        IoSlice::new(&message[..RTPS_HEADER_SIZE]),
        IoSlice::new(&msg_len),
        IoSlice::new(&message[RTPS_HEADER_SIZE..]),
    ];
    let mut slices: &mut [IoSlice<'_>] = &mut bufs;
    while !slices.is_empty() {
        match stream.write_vectored(slices).await? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "write_vectored returned 0 mid-frame",
                ));
            }
            n => IoSlice::advance_slices(&mut slices, n),
        }
    }
    Ok(())
}

/// Read, validate, and decode one complete frame.
pub(crate) async fn read_framed_message<R>(stream: &mut R) -> io::Result<TcpFrame>
where
    R: AsyncRead + Unpin + ?Sized,
{
    read_framed_message_inner(stream, None).await
}

pub(crate) async fn read_framed_message_pooled<R>(
    stream: &mut R,
    pool: &Arc<TcpBufferPool>,
) -> io::Result<TcpFrame>
where
    R: AsyncRead + Unpin + ?Sized,
{
    read_framed_message_inner(stream, Some(pool)).await
}

async fn read_framed_message_inner<R>(
    stream: &mut R,
    pool: Option<&Arc<TcpBufferPool>>,
) -> io::Result<TcpFrame>
where
    R: AsyncRead + Unpin + ?Sized,
{
    let mut prefix = [0u8; STREAM_HEADER_SIZE];
    stream.read_exact(&mut prefix).await?;

    if prefix[..PROTOCOL_RTPS.len()] != PROTOCOL_RTPS {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidMagic,
            format!("stream is not RTPS: {:02x?}", &prefix[..PROTOCOL_RTPS.len()]),
        ));
    }
    if prefix[RTPS_HEADER_SIZE] != SubmessageId::MSG_LEN.as_u8() {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameMissingMsgLen,
            format!("leading submessage is 0x{:02x}", prefix[RTPS_HEADER_SIZE]),
        ));
    }

    let length_bytes: [u8; 4] =
        prefix[RTPS_HEADER_SIZE + 4..].try_into().expect("four-byte length");
    let total_len = if prefix[RTPS_HEADER_SIZE + 1] & 0x01 != 0 {
        u32::from_le_bytes(length_bytes)
    } else {
        u32::from_be_bytes(length_bytes)
    } as usize;
    if total_len < STREAM_HEADER_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidLength,
            format!("message too short: {total_len} bytes (minimum {STREAM_HEADER_SIZE})"),
        ));
    }
    validate_payload_size(total_len - MSG_LEN_SIZE)?;

    let payload = match pool {
        Some(pool) => {
            let mut owner = pool.take_owner(total_len);
            owner.data[..STREAM_HEADER_SIZE].copy_from_slice(&prefix);
            stream.read_exact(&mut owner.data[STREAM_HEADER_SIZE..]).await?;
            Bytes::from_owner(owner)
        }
        None => {
            let mut message = vec![0u8; total_len];
            message[..STREAM_HEADER_SIZE].copy_from_slice(&prefix);
            stream.read_exact(&mut message[STREAM_HEADER_SIZE..]).await?;
            Bytes::from(message)
        }
    };

    let kind = if carries_builtin_writer(&payload) {
        TcpFrameKind::Discovery
    } else {
        TcpFrameKind::UserData
    };
    Ok(TcpFrame { kind, payload })
}

/// Builds a minimal RTPS message carrying a single DATA submessage whose
/// `writerId` has the given entity kind.
#[cfg(test)]
pub(crate) fn test_message(writer_kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut message = vec![0u8; RTPS_HEADER_SIZE];
    message[..PROTOCOL_RTPS.len()].copy_from_slice(&PROTOCOL_RTPS);
    message.extend_from_slice(&[SubmessageId::DATA.as_u8(), 0x01]);
    message.extend_from_slice(&((20 + payload.len()) as u16).to_le_bytes());
    message.extend_from_slice(&[0, 0, 16, 0]); // extraFlags, octetsToInlineQos
    message.extend_from_slice(&[0, 0, 0, 0]); // readerId
    message.extend_from_slice(&[0, 1, 0, writer_kind]);
    message.extend_from_slice(&[0; 8]); // writerSN
    message.extend_from_slice(payload);
    message
}

/// Wraps a message the way the wire carries it, for tests that compare against
/// what a listener hands out.
#[cfg(test)]
pub(crate) fn test_framed(message: &[u8]) -> Vec<u8> {
    let mut framed = Vec::from(&message[..RTPS_HEADER_SIZE]);
    framed.extend_from_slice(&[SubmessageId::MSG_LEN.as_u8(), MSG_LEN_FLAGS]);
    framed.extend_from_slice(&MSG_LEN_OCTETS_TO_NEXT_HEADER.to_le_bytes());
    framed.extend_from_slice(&((message.len() + MSG_LEN_SIZE) as u32).to_le_bytes());
    framed.extend_from_slice(&message[RTPS_HEADER_SIZE..]);
    framed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const BUILTIN_WRITER: u8 = 0xC2;
    const USER_WRITER: u8 = 0x02;

    #[tokio::test]
    async fn round_trip_recovers_the_kind_from_the_message() {
        for (writer_kind, expected) in
            [(BUILTIN_WRITER, TcpFrameKind::Discovery), (USER_WRITER, TcpFrameKind::UserData)]
        {
            let message = test_message(writer_kind, b"payload");
            let mut wire = Vec::new();
            write_framed_message(&mut wire, &message).await.unwrap();

            assert_eq!(&wire[..RTPS_HEADER_SIZE], &message[..RTPS_HEADER_SIZE]);
            assert_eq!(wire[RTPS_HEADER_SIZE], SubmessageId::MSG_LEN.as_u8());
            assert_eq!(wire[RTPS_HEADER_SIZE + 1], MSG_LEN_FLAGS);
            assert_eq!(&wire[RTPS_HEADER_SIZE + 2..RTPS_HEADER_SIZE + 4], &4u16.to_le_bytes());
            assert_eq!(
                &wire[RTPS_HEADER_SIZE + 4..STREAM_HEADER_SIZE],
                &((message.len() + MSG_LEN_SIZE) as u32).to_le_bytes()
            );
            assert_eq!(wire.len(), message.len() + MSG_LEN_SIZE);

            let frame = read_framed_message(&mut Cursor::new(wire.clone())).await.unwrap();
            assert_eq!(frame.kind, expected);
            // The length submessage stays in what RTPS processing receives.
            assert_eq!(frame.payload.as_ref(), wire.as_slice());
        }
    }

    #[tokio::test]
    async fn interleaved_messages_keep_stream_boundaries() {
        let messages = [
            test_message(BUILTIN_WRITER, b"one"),
            test_message(USER_WRITER, b"two"),
            test_message(BUILTIN_WRITER, b"three"),
        ];
        let mut wire = Vec::new();
        for message in &messages {
            write_framed_message(&mut wire, message).await.unwrap();
        }

        let mut cursor = Cursor::new(wire);
        for message in &messages {
            let frame = read_framed_message(&mut cursor).await.unwrap();
            assert_eq!(frame.payload.as_ref(), test_framed(message).as_slice());
        }
    }

    #[tokio::test]
    async fn a_big_endian_length_submessage_is_accepted() {
        let message = test_message(USER_WRITER, b"payload");
        let mut wire = Vec::new();
        write_framed_message(&mut wire, &message).await.unwrap();
        wire[RTPS_HEADER_SIZE + 1] = 0x00;
        wire[RTPS_HEADER_SIZE + 2..RTPS_HEADER_SIZE + 4].copy_from_slice(&4u16.to_be_bytes());
        let total_len = (message.len() + MSG_LEN_SIZE) as u32;
        wire[RTPS_HEADER_SIZE + 4..STREAM_HEADER_SIZE].copy_from_slice(&total_len.to_be_bytes());

        let frame = read_framed_message(&mut Cursor::new(wire.clone())).await.unwrap();
        assert_eq!(frame.payload.as_ref(), wire.as_slice());
    }

    #[tokio::test]
    async fn rejects_a_foreign_stream_and_a_broken_length() {
        let message = test_message(USER_WRITER, b"payload");
        let framed = |mutate: fn(&mut Vec<u8>)| async move {
            let mut wire = Vec::new();
            write_framed_message(&mut wire, &test_message(USER_WRITER, b"payload")).await.unwrap();
            mutate(&mut wire);
            read_framed_message(&mut Cursor::new(wire)).await
        };

        assert!(framed(|wire| wire[0] = b'X').await.is_err());
        assert!(framed(|wire| wire[RTPS_HEADER_SIZE] = SubmessageId::DATA.as_u8()).await.is_err());
        assert!(framed(|wire| {
            wire[RTPS_HEADER_SIZE + 4..RTPS_HEADER_SIZE + 8].copy_from_slice(&1u32.to_le_bytes())
        })
        .await
        .is_err());
        assert!(read_framed_message(&mut Cursor::new(message)).await.is_err());
    }

    #[tokio::test]
    async fn message_limit_is_consistent_on_write_and_read() {
        let oversized = test_message(USER_WRITER, &vec![0u8; MAX_PAYLOAD_SIZE]);
        assert!(write_framed_message(&mut Vec::new(), &oversized).await.is_err());

        let mut prefix = Vec::from(&oversized[..RTPS_HEADER_SIZE]);
        prefix.extend_from_slice(&[SubmessageId::MSG_LEN.as_u8(), MSG_LEN_FLAGS]);
        prefix.extend_from_slice(&4u16.to_le_bytes());
        prefix.extend_from_slice(&((oversized.len() + MSG_LEN_SIZE) as u32).to_le_bytes());
        assert!(read_framed_message(&mut Cursor::new(prefix)).await.is_err());
    }

    #[tokio::test]
    async fn pooled_payload_returns_after_last_bytes_clone_drops() {
        let pool = TcpBufferPool::new();
        let mut wire = Vec::new();
        write_framed_message(&mut wire, &test_message(USER_WRITER, b"pooled")).await.unwrap();
        let frame = read_framed_message_pooled(&mut Cursor::new(wire), &pool).await.unwrap();
        let clone = frame.payload.clone();
        drop(frame);
        assert_eq!(pool.cached_count(), 0);
        drop(clone);
        assert_eq!(pool.cached_count(), 1);
    }
}
