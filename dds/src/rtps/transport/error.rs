//! Transport-layer error types.
//!
//! Provides [`TransportError`] and [`TransportErrorCode`] for structured,
//! transport-specific error reporting.  These are embedded as the *source*
//! of a standard [`std::io::Error`] so that the `TransportPlugin` trait
//! signature (`-> io::Result<()>`) stays unchanged.
//!
//! Upper layers (SEDP / User Logic) can optionally downcast the source to
//! recover the specific code for logging or monitoring, but are **not**
//! required to — they can keep relying on `io::ErrorKind` alone.

use std::fmt;
use std::io;

// ── Error code ──────────────────────────────────────────────────────────────

/// Transport-layer error codes.
///
/// Designed to be embedded inside [`std::io::Error`] via [`TransportError`].
/// RTPS layers do **not** need to interpret these — they exist for
/// diagnostics, logging, and monitoring.
///
/// Code ranges:
///   710–719  TCP connection establishment
///   730–739  TCP connection maintenance / monitoring
///   740–749  TCP framing
///   760–769  TCP internal channel
///   770–779  TCP listener / accept
///   780–789  TLS configuration and handshake
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TransportErrorCode {
    // ── 710: TCP connection establishment ────────────────────────────────
    /// TCP connect() timed out before the peer accepted.
    TcpConnectionTimeout = 710,
    /// TCP connect() received RST — peer is not listening on the target port.
    TcpConnectionRefused = 711,
    /// TCP listener failed to bind the physical port (port already in use, permission denied, etc.).
    TcpBindFailed = 712,

    // ── 730: TCP connection maintenance / monitoring ────────────────────
    /// Send skipped because the peer is in exponential reconnect backoff (its
    /// recent connect attempts keep failing). A deferral, not a hard failure —
    /// the triggering connect failure is reported separately.
    TcpReconnectBackoff = 730,
    /// The previous frame still held the writer admission permit when the send
    /// deadline expired. No byte of this frame left the socket.
    TcpSendDeadlineExpired = 731,

    // ── 740: TCP framing ────────────────────────────────────────────────
    /// Frame magic bytes were not "INT2".
    TcpFrameInvalidMagic = 740,
    /// Frame payload exceeds the maximum allowed size.
    TcpFrameTooLarge = 741,
    /// Frame length field was zero or otherwise invalid.
    TcpFrameInvalidLength = 742,
    /// Frame kind is neither discovery nor user data.
    TcpFrameInvalidKind = 743,

    // ── 760: TCP internal channel ───────────────────────────────────────
    /// Internal channel (discovery or user-data) is full; message dropped.
    TcpChannelFull = 760,
    /// The per-connection buffer holding frames until TCP/TLS connect completes
    /// hit its bound; the frame was dropped.
    TcpConnectBufferFull = 761,

    // ── 770: TCP listener / accept ─────────────────────────────────────
    /// Incoming TCP accept() failed (fd exhaustion, permission, etc.).
    TcpAcceptFailed = 770,
    /// Read error on an accepted connection (RST, EOF, framing error).
    TcpReadError = 771,

    // ── 780: TLS configuration and handshake ────────────────────────────
    /// A required TLS property is missing in the participant QoS.
    TlsMissingProperty = 780,
    /// Failed to read a TLS PEM file (CA, cert, or key).
    TlsFileIoError = 781,
    /// PEM contents are malformed or empty.
    TlsInvalidPem = 782,
    /// `rustls` rejected the configuration (bad cert/key, etc.).
    TlsConfigError = 783,
    /// TLS handshake with the remote peer failed.
    TlsHandshakeFailed = 784,
}

impl TransportErrorCode {
    /// Numeric code suitable for structured logging.
    pub fn as_u32(self) -> u32 {
        self as u32
    }

    /// Suggest an [`io::ErrorKind`] that best represents this transport error.
    pub fn to_io_error_kind(self) -> io::ErrorKind {
        match self {
            Self::TcpConnectionTimeout => io::ErrorKind::TimedOut,
            Self::TcpConnectionRefused => io::ErrorKind::ConnectionRefused,
            Self::TcpBindFailed => io::ErrorKind::AddrInUse,

            Self::TcpReconnectBackoff | Self::TcpSendDeadlineExpired => io::ErrorKind::WouldBlock,

            Self::TcpFrameInvalidMagic
            | Self::TcpFrameTooLarge
            | Self::TcpFrameInvalidLength
            | Self::TcpFrameInvalidKind => io::ErrorKind::InvalidData,

            Self::TcpChannelFull | Self::TcpConnectBufferFull => io::ErrorKind::WouldBlock,

            Self::TcpAcceptFailed => io::ErrorKind::ConnectionAborted,
            Self::TcpReadError => io::ErrorKind::ConnectionReset,
            Self::TlsMissingProperty => io::ErrorKind::InvalidInput,
            Self::TlsFileIoError => io::ErrorKind::NotFound,
            Self::TlsInvalidPem | Self::TlsConfigError => io::ErrorKind::InvalidData,
            Self::TlsHandshakeFailed => io::ErrorKind::ConnectionAborted,
        }
    }

    /// Default human-readable description.
    pub fn default_message(self) -> &'static str {
        match self {
            Self::TcpConnectionTimeout => "TCP connection timed out",
            Self::TcpConnectionRefused => "TCP connection refused",
            Self::TcpBindFailed => "TCP listener bind failed",

            Self::TcpReconnectBackoff => "peer in reconnect backoff",
            Self::TcpSendDeadlineExpired => "TCP send deadline expired before writer admission",

            Self::TcpFrameInvalidMagic => "TCP frame invalid magic",
            Self::TcpFrameTooLarge => "TCP frame too large",
            Self::TcpFrameInvalidLength => "TCP frame invalid length",
            Self::TcpFrameInvalidKind => "TCP frame invalid kind",

            Self::TcpChannelFull => "TCP internal channel full",
            Self::TcpConnectBufferFull => "TCP connect-window buffer full",

            Self::TcpAcceptFailed => "TCP accept failed",
            Self::TcpReadError => "TCP read error on accepted connection",
            Self::TlsMissingProperty => "TLS required property missing",
            Self::TlsFileIoError => "TLS PEM file I/O error",
            Self::TlsInvalidPem => "TLS PEM contents invalid",
            Self::TlsConfigError => "TLS rustls configuration rejected",
            Self::TlsHandshakeFailed => "TLS handshake with peer failed",
        }
    }
}

impl fmt::Display for TransportErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}({})", self, self.as_u32())
    }
}

// ── TransportError ──────────────────────────────────────────────────────────

/// A structured transport-layer error.
///
/// Implements [`std::error::Error`] so it can be used as the *source* of
/// an [`io::Error`]:
///
/// ```ignore
/// Err(TransportError::new(TransportErrorCode::TcpConnectionTimeout,
///     format!("Connection timeout to {addr}")).into_io_error())
/// ```
#[derive(Debug)]
pub struct TransportError {
    pub code: TransportErrorCode,
    pub message: String,
}

impl TransportError {
    pub fn new(code: TransportErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    /// Convert into an [`io::Error`] with this error as the source.
    /// The [`io::ErrorKind`] is derived from the error code so that
    /// callers who only inspect `ErrorKind` still get reasonable behaviour.
    pub fn into_io_error(self) -> io::Error {
        let kind = self.code.to_io_error_kind();
        io::Error::new(kind, self)
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Transport Error [{}] {:?}: {}", self.code.as_u32(), self.code, self.message)
    }
}

impl std::error::Error for TransportError {}

// ── Convenience ─────────────────────────────────────────────────────────────

/// Shorthand for creating an [`io::Error`] from a transport error code.
///
/// ```ignore
/// return Err(transport_io_error(TcpConnectionTimeout, format!("to {addr}")));
/// ```
pub fn transport_io_error(code: TransportErrorCode, message: impl Into<String>) -> io::Error {
    TransportError::new(code, message).into_io_error()
}
