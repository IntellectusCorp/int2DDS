//! # Error Handling
//!
//! Converts Rust `DdsError` values to C-compatible integer return codes.

use int2dds::core::error::DdsError;

/// FFI return codes
pub type Int2DdsRet = i32;

// Success
pub const INT2DDS_RET_OK: Int2DdsRet = 0;

// General errors
pub const INT2DDS_RET_ERROR: Int2DdsRet = 1;
pub const INT2DDS_RET_TIMEOUT: Int2DdsRet = 2;
pub const INT2DDS_RET_UNSUPPORTED: Int2DdsRet = 3;
pub const INT2DDS_RET_INVALID_ARGUMENT: Int2DdsRet = 11;

// DDS-specific errors
pub const INT2DDS_RET_ALREADY_DELETED: Int2DdsRet = 20;
pub const INT2DDS_RET_NOT_ENABLED: Int2DdsRet = 21;
pub const INT2DDS_RET_IMMUTABLE_POLICY: Int2DdsRet = 22;
pub const INT2DDS_RET_INCONSISTENT_POLICY: Int2DdsRet = 23;
pub const INT2DDS_RET_PRECONDITION_NOT_MET: Int2DdsRet = 24;
pub const INT2DDS_RET_OUT_OF_RESOURCES: Int2DdsRet = 25;
pub const INT2DDS_RET_ILLEGAL_OPERATION: Int2DdsRet = 26;
pub const INT2DDS_RET_NO_DATA: Int2DdsRet = 27;

// FFI-boundary errors
pub const INT2DDS_RET_NULL_POINTER: Int2DdsRet = 100;
pub const INT2DDS_RET_BUFFER_TOO_SMALL: Int2DdsRet = 101;

pub const INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND: Int2DdsRet = 200;
pub const INT2DDS_RET_DYNAMIC_TYPE_MISMATCH: Int2DdsRet = 201;
pub const INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE: Int2DdsRet = 202;
pub const INT2DDS_RET_DYNAMIC_TIMEOUT: Int2DdsRet = 203;
pub const INT2DDS_RET_DYNAMIC_DECODE_ERROR: Int2DdsRet = 204;

/// Convert DdsError to error code
pub fn dds_error_to_code(error: &DdsError) -> Int2DdsRet {
    // Only Error carries a reason; clear otherwise to avoid stale reads.
    match error {
        DdsError::Error(msg) => crate::last_error::set_last_error(msg),
        _ => crate::last_error::clear_last_error(),
    }
    match error {
        DdsError::Error(_) => INT2DDS_RET_ERROR,
        DdsError::Timeout => INT2DDS_RET_TIMEOUT,
        DdsError::AlreadyDeleted => INT2DDS_RET_ALREADY_DELETED,
        DdsError::NotEnabled => INT2DDS_RET_NOT_ENABLED,
        DdsError::ImmutablePolicy => INT2DDS_RET_IMMUTABLE_POLICY,
        DdsError::InconsistentPolicy => INT2DDS_RET_INCONSISTENT_POLICY,
        DdsError::PreconditionNotMet => INT2DDS_RET_PRECONDITION_NOT_MET,
        DdsError::OutOfResources => INT2DDS_RET_OUT_OF_RESOURCES,
        DdsError::IllegalOperation => INT2DDS_RET_ILLEGAL_OPERATION,
        DdsError::Unsupported => INT2DDS_RET_UNSUPPORTED,
        DdsError::BadParameter => INT2DDS_RET_INVALID_ARGUMENT,
        DdsError::NoData => INT2DDS_RET_NO_DATA,
    }
}

/// Check if a pointer is null and return error code if so
#[macro_export]
macro_rules! check_null {
    ($ptr:expr) => {
        if $ptr.is_null() {
            $crate::last_error::set_last_error(concat!(
                "null pointer argument: ",
                stringify!($ptr)
            ));
            return $crate::error::INT2DDS_RET_NULL_POINTER;
        }
    };
}

/// Convert DdsResult to FFI return code, returning error code on failure
#[macro_export]
macro_rules! ffi_try {
    ($expr:expr) => {
        match $expr {
            Ok(val) => val,
            Err(e) => return $crate::error::dds_error_to_code(&e),
        }
    };
}

/// Bail out of an FFI function with an error message (returns `INT2DDS_RET_ERROR`).
/// Routes through `dds_error_to_code` so code/message mapping stays in one place.
#[macro_export]
macro_rules! ffi_bail {
    ($msg:expr) => {
        return $crate::error::dds_error_to_code(&::int2dds::core::error::DdsError::Error(
            ($msg).into(),
        ))
    };
}
