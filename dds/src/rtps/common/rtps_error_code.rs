//! Error types and result handling for RTPS operations.
//!
//! This module defines the RTPS error types and result wrappers used throughout
//! the RTPS layer for error handling and propagation.

use std::fmt;

use log::debug;

pub type RtpsResult<T> = Result<T, RtpsError>;

#[derive(Debug, Clone)]
pub struct RtpsError {
    pub code: RtpsErrorCode,
    pub message: String,
}

impl std::error::Error for RtpsError {}

impl RtpsError {
    pub fn new<M>(code: RtpsErrorCode, message: M) -> Self
    where
        M: IntoMessage,
    {
        let message = message.into_message().unwrap_or_else(|| code.default_message().to_string());
        let error = RtpsError { code, message };
        debug!("{}", error);
        error
    }
}

impl fmt::Display for RtpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RTPS Error [{}] {:?}: {}", self.code.as_i32(), self.code, self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RtpsErrorCode {
    // IO/Serialization/Parsing
    Io = 100,
    BufferTooShortForRtpsHeader = 101,
    InvalidRtpsMagic = 102,
    InvalidRtpsHeader = 103,
    InvalidSubmessageHeader = 104,
    InvalidSubmessageBody = 105,
    UnsupportedSubmessageType = 106,
    SerializationError = 107,
    DeserializationError = 108,
    InvalidDestinationGuid = 109,

    // Validation/Type
    InvalidEntityKind = 200,
    DowncastError = 201,
    DataNotSet = 202,

    // Synchronization/System/Runtime
    LockError = 300,
    ThreadJoinError = 301,
    ArcUpgradeError = 302,

    // Existence/Matching/Discovery
    RtpsEntityNotFound = 400,
    MatchedEntityNotFound = 401,
    BuiltinEndpointNotFound = 402,
    MatchedEntityAlreadyExists = 403,
    DataReaderCacheNotSet = 404,
    WriterCacheNotSet = 405,
    NotInitialized = 406,

    // RTPS Logic error
    QosIncompatible = 500,
    PartitionIncompatible = 501,
    InternalLogicError = 502,
    TopicKindIncompatible = 503,

    // Error propagated from DDS layer
    DdsError = 600,

    // Transport send error
    NotSent = 700,
    PeerDisconnected = 701,

    // For unexpected error
    Unknown = 999,
}

impl RtpsErrorCode {
    pub fn as_i32(&self) -> i32 {
        *self as i32
    }

    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            // IO/Serialization/Parsing
            100 => Some(RtpsErrorCode::Io),
            101 => Some(RtpsErrorCode::BufferTooShortForRtpsHeader),
            102 => Some(RtpsErrorCode::InvalidRtpsMagic),
            103 => Some(RtpsErrorCode::InvalidRtpsHeader),
            104 => Some(RtpsErrorCode::InvalidSubmessageHeader),
            105 => Some(RtpsErrorCode::InvalidSubmessageBody),
            106 => Some(RtpsErrorCode::UnsupportedSubmessageType),
            107 => Some(RtpsErrorCode::SerializationError),
            108 => Some(RtpsErrorCode::DeserializationError),
            109 => Some(RtpsErrorCode::InvalidDestinationGuid),

            // Validation/Type/State
            200 => Some(RtpsErrorCode::InvalidEntityKind),
            201 => Some(RtpsErrorCode::DowncastError),
            202 => Some(RtpsErrorCode::DataNotSet),

            // Synchronization/System/Runtime
            300 => Some(RtpsErrorCode::LockError),
            301 => Some(RtpsErrorCode::ThreadJoinError),
            302 => Some(RtpsErrorCode::ArcUpgradeError),

            // Existence/Matching/Discovery
            400 => Some(RtpsErrorCode::RtpsEntityNotFound),
            401 => Some(RtpsErrorCode::MatchedEntityNotFound),
            402 => Some(RtpsErrorCode::BuiltinEndpointNotFound),
            403 => Some(RtpsErrorCode::MatchedEntityAlreadyExists),
            404 => Some(RtpsErrorCode::DataReaderCacheNotSet),
            405 => Some(RtpsErrorCode::WriterCacheNotSet),
            406 => Some(RtpsErrorCode::NotInitialized),

            // RTPS Logic error
            500 => Some(RtpsErrorCode::QosIncompatible),
            501 => Some(RtpsErrorCode::PartitionIncompatible),
            502 => Some(RtpsErrorCode::InternalLogicError),
            503 => Some(RtpsErrorCode::TopicKindIncompatible),

            // Resource shortage
            600 => Some(RtpsErrorCode::DdsError),

            // Transport send error
            700 => Some(RtpsErrorCode::NotSent),
            701 => Some(RtpsErrorCode::PeerDisconnected),

            999 => Some(RtpsErrorCode::Unknown),

            _ => None,
        }
    }

    pub fn default_message(&self) -> &'static str {
        match self {
            RtpsErrorCode::BufferTooShortForRtpsHeader => "Buffer is too short for RTPS header",
            RtpsErrorCode::InvalidRtpsMagic => "Invalid RTPS magic number",
            RtpsErrorCode::InvalidRtpsHeader => "Invalid RTPS header",
            RtpsErrorCode::InvalidSubmessageHeader => "Invalid RTPS submessage header",
            RtpsErrorCode::InvalidSubmessageBody => "Invalid RTPS submessage body",
            RtpsErrorCode::Io => "IO Error",
            RtpsErrorCode::UnsupportedSubmessageType => "Unsupported RTPS submessage type",
            RtpsErrorCode::SerializationError => "Serialization Error",
            RtpsErrorCode::DeserializationError => "Deserialization Error",
            RtpsErrorCode::InvalidDestinationGuid => "Invalid Destination Guid",
            RtpsErrorCode::InvalidEntityKind => "Invalid entity kind",
            RtpsErrorCode::LockError => "Failed to acquire lock",
            RtpsErrorCode::ThreadJoinError => "Failed to join thread",
            RtpsErrorCode::RtpsEntityNotFound => "RTPS entity not found",
            RtpsErrorCode::DowncastError => "Failed to downcast RTPS entity",
            RtpsErrorCode::DataNotSet => "Data Not Set",
            RtpsErrorCode::MatchedEntityNotFound => "Matched remote entity not found",
            RtpsErrorCode::BuiltinEndpointNotFound => "Builtin endpoint not found",
            RtpsErrorCode::MatchedEntityAlreadyExists => "Matched remote entity already exists",
            RtpsErrorCode::QosIncompatible => "QoS policies are incompatible",
            RtpsErrorCode::PartitionIncompatible => "Partition policies are incompatible",
            RtpsErrorCode::InternalLogicError => "Internal Logic Error",
            RtpsErrorCode::TopicKindIncompatible => "TopicKind mismatch between writer and reader",
            RtpsErrorCode::DataReaderCacheNotSet => "DataReader cache not set",
            RtpsErrorCode::WriterCacheNotSet => "Writer Cache not set",
            RtpsErrorCode::NotInitialized => "Not Initialized",
            RtpsErrorCode::DdsError => "DDS error",
            RtpsErrorCode::ArcUpgradeError => "Failed to upgrade Weak reference to Arc",
            RtpsErrorCode::NotSent => "Message not sent",
            RtpsErrorCode::PeerDisconnected => "Remote peer connection permanently lost",
            RtpsErrorCode::Unknown => "Unknown",
        }
    }
}

/// Helper trait - handles &str, String, Option<String>
pub trait IntoMessage {
    fn into_message(self) -> Option<String>;
}

impl IntoMessage for Option<String> {
    fn into_message(self) -> Option<String> {
        self
    }
}

impl IntoMessage for String {
    fn into_message(self) -> Option<String> {
        Some(self)
    }
}

impl IntoMessage for &str {
    fn into_message(self) -> Option<String> {
        Some(self.to_string())
    }
}
