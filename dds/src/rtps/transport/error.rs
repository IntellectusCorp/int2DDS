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
///   720–729  TCP handshake (3-step)
///   730–739  TCP connection maintenance / monitoring
///   740–749  TCP framing
///   750–759  TCP control protocol
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

    // ── 720: TCP 3-step handshake ───────────────────────────────────────
    /// PEER_HELLO was sent but the response was not PEER_HELLO_ACK.
    TcpHandshakeHelloFailed = 720,
    /// PORT_RESERVE was rejected or the response was not PORT_RESERVE_ACK.
    TcpHandshakeReserveFailed = 721,
    /// PORT_BIND was rejected (invalid/expired cookie) or no PORT_BIND_ACK received.
    TcpHandshakeBindFailed = 722,

    // ── 730: TCP connection maintenance / monitoring ────────────────────
    /// Send skipped because the peer is in exponential reconnect backoff (its
    /// recent connect attempts keep failing). A deferral, not a hard failure —
    /// the triggering connect failure is reported separately.
    TcpReconnectBackoff = 730,

    // ── 740: TCP framing ────────────────────────────────────────────────
    /// Frame magic bytes were not "INT2".
    TcpFrameInvalidMagic = 740,
    /// Frame payload exceeds the maximum allowed size.
    TcpFrameTooLarge = 741,
    /// Frame length field was zero or otherwise invalid.
    TcpFrameInvalidLength = 742,

    // ── 750: TCP control protocol ───────────────────────────────────────
    /// Received an unknown or unparseable control message type.
    TcpControlProtocolError = 750,
    /// PORT_RESERVE requested a logical port that does not exist.
    TcpControlInvalidPort = 751,
    /// PORT_BIND presented an invalid or expired cookie.
    TcpControlInvalidCookie = 752,

    // ── 760: TCP internal channel ───────────────────────────────────────
    /// Internal channel (discovery or user-data) is full; message dropped.
    TcpChannelFull = 760,

    // ── 770: TCP listener / accept ─────────────────────────────────────
    /// Incoming TCP accept() failed (fd exhaustion, permission, etc.).
    TcpAcceptFailed = 770,
    /// Read error on an accepted connection (RST, EOF, framing error).
    TcpReadError = 771,
    /// Failed to send a control response to a connected peer.
    TcpControlSendFailed = 772,
    /// An accepted connection was closed before any handshake because its source
    /// IP is not a declared peer.
    TcpInboundRejected = 773,

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

            Self::TcpHandshakeHelloFailed
            | Self::TcpHandshakeReserveFailed
            | Self::TcpHandshakeBindFailed => io::ErrorKind::InvalidData,

            Self::TcpReconnectBackoff => io::ErrorKind::WouldBlock,

            Self::TcpFrameInvalidMagic | Self::TcpFrameTooLarge | Self::TcpFrameInvalidLength => {
                io::ErrorKind::InvalidData
            }

            Self::TcpControlProtocolError
            | Self::TcpControlInvalidPort
            | Self::TcpControlInvalidCookie => io::ErrorKind::InvalidData,

            Self::TcpChannelFull => io::ErrorKind::WouldBlock,

            Self::TcpAcceptFailed => io::ErrorKind::ConnectionAborted,
            Self::TcpReadError => io::ErrorKind::ConnectionReset,
            Self::TcpControlSendFailed => io::ErrorKind::BrokenPipe,
            Self::TcpInboundRejected => io::ErrorKind::ConnectionRefused,

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

            Self::TcpHandshakeHelloFailed => "TCP PEER_HELLO handshake failed",
            Self::TcpHandshakeReserveFailed => "TCP PORT_RESERVE handshake failed",
            Self::TcpHandshakeBindFailed => "TCP PORT_BIND handshake failed",

            Self::TcpReconnectBackoff => "peer in reconnect backoff",

            Self::TcpFrameInvalidMagic => "TCP frame invalid magic",
            Self::TcpFrameTooLarge => "TCP frame too large",
            Self::TcpFrameInvalidLength => "TCP frame invalid length",

            Self::TcpControlProtocolError => "TCP unknown control message",
            Self::TcpControlInvalidPort => "TCP PORT_RESERVE invalid port",
            Self::TcpControlInvalidCookie => "TCP PORT_BIND invalid cookie",

            Self::TcpChannelFull => "TCP internal channel full",

            Self::TcpAcceptFailed => "TCP accept failed",
            Self::TcpReadError => "TCP read error on accepted connection",
            Self::TcpControlSendFailed => "TCP control response send failed",
            Self::TcpInboundRejected => "TCP inbound connection rejected",

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
