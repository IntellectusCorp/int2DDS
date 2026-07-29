//! Error types for DDS operations.
//!
//! This module defines the error types returned by DDS API operations. The `DdsError` enum
//! represents all possible error conditions that can occur in the DDS system, following the
//! DDS specification's return code semantics.
//!
//! Most DDS operations return a `DdsResult<T>`, which is an alias for `Result<T, DdsError>`.

use std::fmt;

pub type DdsResult<T> = Result<T, DdsError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdsError {
    Error(String),
    Unsupported,
    BadParameter,
    PreconditionNotMet,
    OutOfResources,
    NotEnabled,
    ImmutablePolicy,
    InconsistentPolicy,
    AlreadyDeleted,
    Timeout,
    NoData,
    IllegalOperation,
}

impl fmt::Display for DdsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DdsError::Error(msg) => write!(f, "Error: {}", msg),
            DdsError::Unsupported => write!(f, "Unsupported operation"),
            DdsError::BadParameter => write!(f, "Bad parameter"),
            DdsError::PreconditionNotMet => write!(f, "Precondition not met"),
            DdsError::OutOfResources => write!(f, "Out of resources"),
            DdsError::NotEnabled => write!(f, "Not enabled"),
            DdsError::ImmutablePolicy => write!(f, "Immutable policy"),
            DdsError::InconsistentPolicy => write!(f, "Inconsistent policy"),
            DdsError::AlreadyDeleted => write!(f, "Already deleted"),
            DdsError::Timeout => write!(f, "Timeout"),
            DdsError::NoData => write!(f, "No data"),
            DdsError::IllegalOperation => write!(f, "Illegal operation"),
        }
    }
}
impl std::error::Error for DdsError {}

/// Return code representing the different errors
pub type ReturnCode = i32;

/// pub: reused by DDS-RPC crate
pub const RETCODE_OK: ReturnCode = 0;
