#![allow(dead_code)]
#![allow(unused_variables)]

use std::io::{self, IoSlice};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::rtps::transport::error::{transport_io_error, TransportErrorCode};

/// Maximum message size for TCP framing (16 MB)
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

/// int2DDS TCP frame magic: "INT2" (0x49 0x4E 0x54 0x32)
const FRAME_MAGIC: [u8; 4] = [0x49, 0x4E, 0x54, 0x32];

/// Magic field size
const MAGIC_SIZE: usize = 4;

/// Write a framed message to a TCP stream.
///
/// Format: [4B length (BE)] [4B magic "INT2"] [NB payload]
/// length = magic(4) + payload size
///
pub(crate) async fn write_framed_message<W>(stream: &mut W, data: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if data.len() > MAX_MESSAGE_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameTooLarge,
            format!("Message too large: {} bytes (max: {} bytes)", data.len(), MAX_MESSAGE_SIZE),
        ));
    }

    let total_payload = MAGIC_SIZE + data.len();
    let len_bytes = (total_payload as u32).to_be_bytes();

    let mut bufs = [IoSlice::new(&len_bytes), IoSlice::new(&FRAME_MAGIC), IoSlice::new(data)];
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

/// Write multiple framed messages as a single `write_vectored` batch.
///
/// Each frame uses the same `[length][magic][payload]` layout as
/// `write_framed_message`, but every frame in `frames` is packed into a flat
/// `IoSlice` vector and submitted in one call — the kernel sees one large
/// contiguous send, which lets TSO/GSO segment on the NIC instead of
/// segmenting once per fragment in software.
///
/// Empty `frames` is a no-op. Partial `write_vectored` returns are handled by
/// advancing the slice cursor until the entire batch drains.
pub(crate) async fn write_framed_batch<W>(stream: &mut W, frames: &[Vec<u8>]) -> io::Result<()>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    if frames.is_empty() {
        return Ok(());
    }

    // Per-frame length prefixes need stable backing storage so each IoSlice
    // can borrow a `&[u8; 4]` from it across the await below.
    let mut lens: Vec<[u8; 4]> = Vec::with_capacity(frames.len());
    for frame in frames {
        if frame.len() > MAX_MESSAGE_SIZE {
            return Err(transport_io_error(
                TransportErrorCode::TcpFrameTooLarge,
                format!(
                    "Message too large: {} bytes (max: {} bytes)",
                    frame.len(),
                    MAX_MESSAGE_SIZE
                ),
            ));
        }
        let total_payload = MAGIC_SIZE + frame.len();
        lens.push((total_payload as u32).to_be_bytes());
    }

    let mut bufs: Vec<IoSlice<'_>> = Vec::with_capacity(frames.len() * 3);
    for (len, frame) in lens.iter().zip(frames.iter()) {
        bufs.push(IoSlice::new(len));
        bufs.push(IoSlice::new(&FRAME_MAGIC));
        bufs.push(IoSlice::new(frame));
    }

    let mut slices: &mut [IoSlice<'_>] = &mut bufs[..];
    while !slices.is_empty() {
        match stream.write_vectored(slices).await? {
            0 => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "write_vectored returned 0 mid-batch",
                ));
            }
            n => IoSlice::advance_slices(&mut slices, n),
        }
    }
    Ok(())
}

/// Read a framed message from a TCP stream (completely).
///
/// Format: [4B length (BE)] [4B magic "INT2"] [NB payload]
/// Returns the payload after validating and stripping magic.
pub(crate) async fn read_framed_message<R>(stream: &mut R) -> io::Result<Vec<u8>>
where
    R: AsyncRead + Unpin + ?Sized,
{
    // Read length + magic together into a stack buffer, then read the payload
    // directly into its own exact-sized Vec. This avoids the extra alloc + memcpy
    // that `data[MAGIC_SIZE..].to_vec()` used to incur on every frame.
    let mut header = [0u8; 4 + MAGIC_SIZE];
    stream.read_exact(&mut header).await?;
    let len = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;

    if len < MAGIC_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidLength,
            format!("Frame too short: {} bytes (minimum {})", len, MAGIC_SIZE),
        ));
    }

    if len > MAX_MESSAGE_SIZE {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameTooLarge,
            format!("Message too large: {} bytes (max: {} bytes)", len, MAX_MESSAGE_SIZE),
        ));
    }

    if header[4..] != FRAME_MAGIC {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidMagic,
            format!(
                "Invalid frame magic: {:02x} {:02x} {:02x} {:02x} (expected INT2)",
                header[4], header[5], header[6], header[7]
            ),
        ));
    }

    let payload_len = len - MAGIC_SIZE;
    let mut payload = Vec::with_capacity(payload_len);
    while payload.len() < payload_len {
        let n = stream.read_buf(&mut payload).await?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "premature EOF mid-payload"));
        }
    }
    Ok(payload)
}

/// Classification of a TCP frame payload
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TcpFrameKind {
    /// Rtps data message (payload starts with RTPS magic: 0x52545053)
    RtpsData,
    /// TCP control message (payload[0] in 0x01..=0x7F)
    Control,
    /// Unknown or invalid format
    Unknown,
}

/// RTPS protocol magic bytes: "RTPS" (0x52, 0x54, 0x50, 0x53)
const RTPS_MAGIC: [u8; 4] = [0x52, 0x54, 0x50, 0x53];

/// Classify a frame payload as RTPS data or TCP control message.
///
/// Called on the payload AFTER the magic has been stripped by read_framed_message/FramedReader.
///
/// Classification rules:
/// - If payload starts with RTPS magic (0x52545053), it is RtpsData
/// - If payload[0] matches a known control message type (0x01..=0x07), it is Control
/// - Otherwise, Unknown
pub(crate) fn classify_frame(payload: &[u8]) -> TcpFrameKind {
    if payload.len() >= 4 && payload[0..4] == RTPS_MAGIC {
        return TcpFrameKind::RtpsData;
    }

    if !payload.is_empty() && payload[0] >= 0x01 && payload[0] <= 0x07 {
        return TcpFrameKind::Control;
    }

    TcpFrameKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_write_and_read_framed_message() {
        let test_data = b"Hello, TCP Framing!";
        let mut buffer = Vec::new();

        write_framed_message(&mut buffer, test_data).await.unwrap();

        // Verify: [4B length][4B magic "INT2"][payload]
        let total_payload = MAGIC_SIZE + test_data.len();
        assert_eq!(buffer.len(), 4 + total_payload);
        assert_eq!(&buffer[0..4], &(total_payload as u32).to_be_bytes());
        assert_eq!(&buffer[4..8], &FRAME_MAGIC);
        assert_eq!(&buffer[8..], test_data);

        // Read back — returns payload without magic
        let mut cursor = Cursor::new(buffer);
        let read_data = read_framed_message(&mut cursor).await.unwrap();
        assert_eq!(read_data, test_data);
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let messages = vec![b"First".to_vec(), b"Second message".to_vec(), b"Third".to_vec()];
        let mut buffer = Vec::new();

        for msg in &messages {
            write_framed_message(&mut buffer, msg).await.unwrap();
        }

        let mut cursor = Cursor::new(buffer);
        for expected in &messages {
            let read_data = read_framed_message(&mut cursor).await.unwrap();
            assert_eq!(&read_data, expected);
        }
    }

    #[tokio::test]
    async fn test_write_framed_batch_roundtrip() {
        let messages: Vec<Vec<u8>> = vec![
            b"alpha".to_vec(),
            b"beta-second-frame".to_vec(),
            vec![0xCDu8; 4096],
            b"".to_vec(),
            b"tail".to_vec(),
        ];

        let mut buffer = Vec::new();
        write_framed_batch(&mut buffer, &messages).await.unwrap();

        let mut cursor = Cursor::new(buffer);
        for expected in &messages {
            let read_data = read_framed_message(&mut cursor).await.unwrap();
            assert_eq!(&read_data, expected);
        }
        // No bytes left over.
        assert_eq!(cursor.position() as usize, cursor.get_ref().len());
    }

    #[tokio::test]
    async fn test_write_framed_batch_matches_per_frame() {
        let messages: Vec<Vec<u8>> = vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()];

        let mut batched = Vec::new();
        write_framed_batch(&mut batched, &messages).await.unwrap();

        let mut sequential = Vec::new();
        for msg in &messages {
            write_framed_message(&mut sequential, msg).await.unwrap();
        }

        // Batched and per-frame must produce byte-identical streams.
        assert_eq!(batched, sequential);
    }

    #[tokio::test]
    async fn test_write_framed_batch_empty_is_noop() {
        let mut buffer = Vec::new();
        write_framed_batch(&mut buffer, &[]).await.unwrap();
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn test_write_framed_batch_rejects_oversize() {
        let oversize = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let frames = vec![b"ok".to_vec(), oversize];
        let mut buffer = Vec::new();
        let result = write_framed_batch(&mut buffer, &frames).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn test_invalid_magic_rejected() {
        // Manually write frame with wrong magic
        let mut buffer = Vec::new();
        let payload = b"test";
        let total = MAGIC_SIZE + payload.len();
        buffer.extend_from_slice(&(total as u32).to_be_bytes());
        buffer.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // wrong magic
        buffer.extend_from_slice(payload);

        let mut cursor = Cursor::new(buffer);
        let result = read_framed_message(&mut cursor).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid frame magic"));
    }

    #[tokio::test]
    async fn test_message_too_large_write() {
        let large_data = vec![0u8; MAX_MESSAGE_SIZE + 1];
        let mut buffer = Vec::new();

        let result = write_framed_message(&mut buffer, &large_data).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn test_large_valid_message() {
        let test_data = vec![0xAB; 1024 * 1024]; // 1 MB
        let mut buffer = Vec::new();

        write_framed_message(&mut buffer, &test_data).await.unwrap();

        let mut cursor = Cursor::new(buffer);
        let read_data = read_framed_message(&mut cursor).await.unwrap();

        assert_eq!(read_data.len(), test_data.len());
        assert_eq!(read_data, test_data);
    }

    #[test]
    fn test_classify_frame_rtps() {
        // RTPS magic: "RTPS" = [0x52, 0x54, 0x50, 0x53]
        let rtps_payload = vec![0x52, 0x54, 0x50, 0x53, 0x02, 0x03, 0x00, 0x00];
        assert_eq!(classify_frame(&rtps_payload), TcpFrameKind::RtpsData);
    }

    #[test]
    fn test_classify_frame_control() {
        // BindRequest (0x01)
        assert_eq!(classify_frame(&[0x01, 0x49, 0x4E, 0x54, 0x32]), TcpFrameKind::Control);
        // BindResponse (0x02)
        assert_eq!(classify_frame(&[0x02, 0x00]), TcpFrameKind::Control);
        // Keepalive (0x04)
        assert_eq!(classify_frame(&[0x04]), TcpFrameKind::Control);
        // KeepaliveAck (0x05)
        assert_eq!(classify_frame(&[0x05]), TcpFrameKind::Control);
        // Close (0x07)
        assert_eq!(classify_frame(&[0x07]), TcpFrameKind::Control);
    }

    #[test]
    fn test_classify_frame_unknown() {
        assert_eq!(classify_frame(&[]), TcpFrameKind::Unknown);
        assert_eq!(classify_frame(&[0x00]), TcpFrameKind::Unknown);
        assert_eq!(classify_frame(&[0x10, 0x20]), TcpFrameKind::Unknown);
        // 0x52 alone (without full RTPS magic) — not enough bytes for RTPS, not in control range
        assert_eq!(classify_frame(&[0x52]), TcpFrameKind::Unknown);
    }

    #[test]
    fn test_classify_frame_no_overlap() {
        // Verify that RTPS magic first byte (0x52) is outside control range (0x01..=0x07)
        // so classification is always unambiguous
        assert!(0x52 > 0x07);

        // A payload starting with 0x52 but not matching full RTPS magic
        let non_rtps = vec![0x52, 0x00, 0x00, 0x00];
        assert_eq!(classify_frame(&non_rtps), TcpFrameKind::Unknown);
    }
}
