//! Length-delimited framing for RTPS messages carried over TCP.
//!
//! Wire format:
//! `[4B body length, BE][4B magic "INT2"][1B kind][payload]`.
//! The body length covers magic, kind, and payload (`payload.len() + 5`).

use std::io::{self, IoSlice};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};

pub(crate) const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
const FRAME_MAGIC: [u8; 4] = *b"INT2";
const MAGIC_SIZE: usize = FRAME_MAGIC.len();
const KIND_SIZE: usize = 1;
const BODY_PREFIX_SIZE: usize = MAGIC_SIZE + KIND_SIZE;
const HEADER_SIZE: usize = 4 + BODY_PREFIX_SIZE;
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

/// Routing class carried explicitly by every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum TcpFrameKind {
    Discovery = 0x01,
    UserData = 0x02,
}

impl TcpFrameKind {
    fn from_wire(value: u8) -> io::Result<Self> {
        match value {
            0x01 => Ok(Self::Discovery),
            0x02 => Ok(Self::UserData),
            other => Err(transport_io_error(
                TransportErrorCode::TcpFrameInvalidKind,
                format!("invalid TCP frame kind: 0x{other:02x}"),
            )),
        }
    }
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

/// Write one complete frame. Vectored I/O avoids concatenating the header and
/// payload into a second allocation on plaintext TCP.
pub(crate) async fn write_framed_message<W>(
    stream: &mut W,
    kind: TcpFrameKind,
    payload: &[u8],
) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    validate_payload_size(payload.len())?;

    let body_len = BODY_PREFIX_SIZE + payload.len();
    let len_bytes = (body_len as u32).to_be_bytes();
    let kind_bytes = [kind as u8];
    let mut bufs = [
        IoSlice::new(&len_bytes),
        IoSlice::new(&FRAME_MAGIC),
        IoSlice::new(&kind_bytes),
        IoSlice::new(payload),
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
    let mut header = [0u8; HEADER_SIZE];
    stream.read_exact(&mut header).await?;

    let body_len = u32::from_be_bytes(header[..4].try_into().expect("four-byte length")) as usize;
    if body_len < BODY_PREFIX_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidLength,
            format!("frame body too short: {body_len} bytes (minimum {BODY_PREFIX_SIZE})"),
        ));
    }

    let payload_len = body_len - BODY_PREFIX_SIZE;
    validate_payload_size(payload_len)?;

    if header[4..8] != FRAME_MAGIC {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidMagic,
            format!("invalid TCP frame magic: {:02x?}", &header[4..8]),
        ));
    }
    let kind = TcpFrameKind::from_wire(header[8])?;

    let payload = match pool {
        Some(pool) => {
            let mut owner = pool.take_owner(payload_len);
            stream.read_exact(&mut owner.data).await?;
            Bytes::from_owner(owner)
        }
        None => {
            let mut payload = vec![0u8; payload_len];
            stream.read_exact(&mut payload).await?;
            Bytes::from(payload)
        }
    };
    Ok(TcpFrame { kind, payload })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn round_trip_both_kinds() {
        for kind in [TcpFrameKind::Discovery, TcpFrameKind::UserData] {
            let mut wire = Vec::new();
            write_framed_message(&mut wire, kind, b"RTPS-payload").await.unwrap();

            assert_eq!(&wire[..4], &(5u32 + 12).to_be_bytes());
            assert_eq!(&wire[4..8], b"INT2");
            assert_eq!(wire[8], kind as u8);

            let frame = read_framed_message(&mut Cursor::new(wire)).await.unwrap();
            assert_eq!(frame.kind, kind);
            assert_eq!(frame.payload.as_ref(), b"RTPS-payload");
        }
    }

    #[tokio::test]
    async fn interleaved_kinds_keep_stream_boundaries() {
        let messages = [
            (TcpFrameKind::Discovery, b"one".as_slice()),
            (TcpFrameKind::UserData, b"two".as_slice()),
            (TcpFrameKind::Discovery, b"three".as_slice()),
        ];
        let mut wire = Vec::new();
        for (kind, payload) in messages {
            write_framed_message(&mut wire, kind, payload).await.unwrap();
        }

        let mut cursor = Cursor::new(wire);
        for (kind, payload) in messages {
            let frame = read_framed_message(&mut cursor).await.unwrap();
            assert_eq!(frame.kind, kind);
            assert_eq!(frame.payload.as_ref(), payload);
        }
    }

    #[tokio::test]
    async fn rejects_invalid_magic_kind_and_length() {
        let cases = [
            [0, 0, 0, 5, b'B', b'A', b'D', b'!', 1],
            [0, 0, 0, 5, b'I', b'N', b'T', b'2', 0xff],
            [0, 0, 0, 4, b'I', b'N', b'T', b'2', 1],
        ];
        for wire in cases {
            assert!(read_framed_message(&mut Cursor::new(wire)).await.is_err());
        }
    }

    #[tokio::test]
    async fn payload_limit_is_consistent_on_write_and_read() {
        let oversized = vec![0u8; MAX_PAYLOAD_SIZE + 1];
        assert!(write_framed_message(&mut Vec::new(), TcpFrameKind::UserData, &oversized)
            .await
            .is_err());

        let body_len = BODY_PREFIX_SIZE + oversized.len();
        let mut header = Vec::from((body_len as u32).to_be_bytes());
        header.extend_from_slice(&FRAME_MAGIC);
        header.push(TcpFrameKind::UserData as u8);
        assert!(read_framed_message(&mut Cursor::new(header)).await.is_err());
    }

    #[tokio::test]
    async fn pooled_payload_returns_after_last_bytes_clone_drops() {
        let pool = TcpBufferPool::new();
        let mut wire = Vec::new();
        write_framed_message(&mut wire, TcpFrameKind::UserData, b"pooled").await.unwrap();
        let frame = read_framed_message_pooled(&mut Cursor::new(wire), &pool).await.unwrap();
        let clone = frame.payload.clone();
        drop(frame);
        assert_eq!(pool.cached_count(), 0);
        drop(clone);
        assert_eq!(pool.cached_count(), 1);
    }
}
