#![allow(dead_code)]
#![allow(unused_variables)]

use std::io;
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
/// Single write_all to prevent TCP segmentation of header vs body.
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
    let mut buf = Vec::with_capacity(4 + total_payload);
    buf.extend_from_slice(&(total_payload as u32).to_be_bytes());
    buf.extend_from_slice(&FRAME_MAGIC);
    buf.extend_from_slice(data);
    stream.write_all(&buf).await?;
    stream.flush().await?;

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
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

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

    let mut data = vec![0u8; len];
    stream.read_exact(&mut data).await?;

    // Validate magic
    if data[0..4] != FRAME_MAGIC {
        return Err(transport_io_error(
            TransportErrorCode::TcpFrameInvalidMagic,
            format!(
                "Invalid frame magic: {:02x} {:02x} {:02x} {:02x} (expected INT2)",
                data[0], data[1], data[2], data[3]
            ),
        ));
    }

    // Return payload after magic
    Ok(data[MAGIC_SIZE..].to_vec())
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
