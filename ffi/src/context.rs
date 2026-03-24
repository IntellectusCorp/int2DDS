//! # DomainParticipantFactory
//!
//! Factory initialization and finalization functions.
//!
//! ## Usage
//!
//! Every application must call `int2dds_domain_participant_factory_get_instance` before
//! using any other functions, and `int2dds_domain_participant_factory_finalize` when done
//! to properly clean up resources.
//!
//! ```c
//! Int2DdsParticipantFactory* factory;
//! int2dds_domain_participant_factory_get_instance(&factory);
//! // ... use DDS ...
//! int2dds_domain_participant_factory_finalize(factory);
//! ```

use super::{error::*, types::*};

/// Get the DomainParticipantFactory singleton instance
///
/// # Safety
/// - `factory_out` must be a valid pointer to a null pointer
/// - The returned factory must be freed with `int2dds_domain_participant_factory_finalize`
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_get_instance(
    factory_out: *mut *mut Int2DdsParticipantFactory,
) -> Int2DdsRet {
    check_null!(factory_out);

    // Initialize logging if not already done
    // let _ = env_logger::try_init();

    let factory = Box::new(Int2DdsParticipantFactory { _initialized: true });

    *factory_out = Box::into_raw(factory);

    INT2DDS_RET_OK
}

/// Finalize the DomainParticipantFactory and free resources
///
/// # Safety
/// - `factory` must be a valid factory created by `int2dds_domain_participant_factory_get_instance`
/// - `factory` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_finalize(
    factory: *mut Int2DdsParticipantFactory,
) -> Int2DdsRet {
    if factory.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    // Convert back to Box and drop
    let _factory = Box::from_raw(factory);

    INT2DDS_RET_OK
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr;

    #[test]
    fn test_factory_get_instance_finalize() {
        unsafe {
            let mut factory: *mut Int2DdsParticipantFactory = ptr::null_mut();

            // Test get_instance
            let ret = int2dds_domain_participant_factory_get_instance(&mut factory as *mut _);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!factory.is_null());

            // Test finalize
            let ret = int2dds_domain_participant_factory_finalize(factory);
            assert_eq!(ret, INT2DDS_RET_OK);
        }
    }

    #[test]
    fn test_factory_null_pointer() {
        unsafe {
            let ret = int2dds_domain_participant_factory_get_instance(ptr::null_mut());
            assert_eq!(ret, INT2DDS_RET_NULL_POINTER);

            let ret = int2dds_domain_participant_factory_finalize(ptr::null_mut());
            assert_eq!(ret, INT2DDS_RET_NULL_POINTER);
        }
    }
}
