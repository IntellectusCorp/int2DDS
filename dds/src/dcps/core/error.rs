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
/// Return code representing the different errors
pub type ReturnCode = i32;

const _RETCODE_OK: ReturnCode = 0;
const RETCODE_ERROR: ReturnCode = 1;
const RETCODE_UNSUPPORTED: ReturnCode = 2;
const RETCODE_BAD_PARAMETER: ReturnCode = 3;
const RETCODE_PRECONDITION_NOT_MET: ReturnCode = 4;
const RETCODE_OUT_OF_RESOURCES: ReturnCode = 5;
const RETCODE_NOT_ENABLED: ReturnCode = 6;
const RETCODE_IMMUTABLE_POLICY: ReturnCode = 7;
const RETCODE_INCONSISTENT_POLICY: ReturnCode = 8;
const RETCODE_ALREADY_DELETED: ReturnCode = 9;
const RETCODE_TIMEOUT: ReturnCode = 10;
const RETCODE_NO_DATA: ReturnCode = 11;
const RETCODE_ILLEGAL_OPERATION: ReturnCode = 12;

impl From<DdsError> for ReturnCode {
    fn from(e: DdsError) -> Self {
        match e {
            DdsError::Error(_) => RETCODE_ERROR,
            DdsError::Unsupported => RETCODE_UNSUPPORTED,
            DdsError::BadParameter => RETCODE_BAD_PARAMETER,
            DdsError::PreconditionNotMet => RETCODE_PRECONDITION_NOT_MET,
            DdsError::OutOfResources => RETCODE_OUT_OF_RESOURCES,
            DdsError::NotEnabled => RETCODE_NOT_ENABLED,
            DdsError::ImmutablePolicy => RETCODE_IMMUTABLE_POLICY,
            DdsError::InconsistentPolicy => RETCODE_INCONSISTENT_POLICY,
            DdsError::AlreadyDeleted => RETCODE_ALREADY_DELETED,
            DdsError::Timeout => RETCODE_TIMEOUT,
            DdsError::NoData => RETCODE_NO_DATA,
            DdsError::IllegalOperation => RETCODE_ILLEGAL_OPERATION,
        }
    }
}
