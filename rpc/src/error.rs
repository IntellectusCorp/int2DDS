//! DDS-RPC error types (7.11.1.3)

use std::fmt;

use int2dds::dcps::core::error::DdsError;

use crate::types::RemoteExceptionCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdsRpcError {
    Remote(RemoteExceptionCode),
    Dds(DdsError),
    Timeout,
}

impl fmt::Display for DdsRpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DdsRpcError::Remote(code) => write!(f, "remote exception: {:?}", code),
            DdsRpcError::Dds(err) => write!(f, "DDS error: {:?}", err),
            DdsRpcError::Timeout => write!(f, "timeout"),
        }
    }
}

impl std::error::Error for DdsRpcError {}

impl From<DdsError> for DdsRpcError {
    fn from(err: DdsError) -> Self {
        DdsRpcError::Dds(err)
    }
}

impl From<RemoteExceptionCode> for DdsRpcError {
    fn from(code: RemoteExceptionCode) -> Self {
        DdsRpcError::Remote(code)
    }
}

pub type DdsRpcResult<T> = Result<T, DdsRpcError>;
