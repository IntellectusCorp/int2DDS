//! Error types for the DDS-RPC layer.
//!
//! Covers both infrastructure-level failures (DDS errors, timeouts) and
//! application-level exceptions returned by service implementations.

use std::fmt;

use int2dds::dcps::core::error::DdsError;

use crate::types::RemoteExceptionCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DdsRpcError<E = ()> {
    Remote(RemoteExceptionCode),
    Dds(DdsError),
    Timeout,
    UserException(E),
}

impl<E: fmt::Debug> fmt::Display for DdsRpcError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DdsRpcError::Remote(code) => write!(f, "remote exception: {:?}", code),
            DdsRpcError::Dds(err) => write!(f, "DDS error: {:?}", err),
            DdsRpcError::Timeout => write!(f, "timeout"),
            DdsRpcError::UserException(ex) => write!(f, "user exception: {:?}", ex),
        }
    }
}

impl<E: fmt::Debug> std::error::Error for DdsRpcError<E> {}

impl<E> From<DdsError> for DdsRpcError<E> {
    fn from(err: DdsError) -> Self {
        DdsRpcError::Dds(err)
    }
}

impl<E> From<RemoteExceptionCode> for DdsRpcError<E> {
    fn from(code: RemoteExceptionCode) -> Self {
        DdsRpcError::Remote(code)
    }
}

impl<E> DdsRpcError<E> {
    /// Convert from `DdsRpcError<()>` to `DdsRpcError<E>`.
    pub fn from_untyped(err: DdsRpcError) -> Self {
        match err {
            DdsRpcError::Remote(c) => DdsRpcError::Remote(c),
            DdsRpcError::Dds(e) => DdsRpcError::Dds(e),
            DdsRpcError::Timeout => DdsRpcError::Timeout,
            DdsRpcError::UserException(()) => unreachable!(),
        }
    }
}

pub type DdsRpcResult<T, E = ()> = Result<T, DdsRpcError<E>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_exception_variant() {
        let err: DdsRpcError<String> = DdsRpcError::UserException("test".to_string());
        match err {
            DdsRpcError::UserException(s) => assert_eq!(s, "test"),
            _ => panic!("expected UserException"),
        }
    }

    #[test]
    fn test_backward_compat_default_type_param() {
        // E = () by default — existing code compiles without change
        let err: DdsRpcError = DdsRpcError::Timeout;
        assert_eq!(format!("{}", err), "timeout");
    }

    #[test]
    fn test_from_dds_error_generic() {
        let dds_err = DdsError::Error("fail".to_string());
        let err: DdsRpcError<String> = dds_err.into();
        assert!(matches!(err, DdsRpcError::Dds(_)));
    }
}
