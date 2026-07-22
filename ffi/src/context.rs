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

use std::sync::Arc;

use int2dds::domain::domain_participant_factory::DomainParticipantFactory;

use super::{error::*, qos::Int2DdsParticipantQos, types::*};

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

/// Look up an existing participant on `domain_id`.
///
/// On success `*participant_out` receives a NEW handle aliasing the existing
/// core participant (freed independently with `int2dds_delete_participant`). If
/// no participant exists on that domain, returns `INT2DDS_RET_OK` with
/// `*participant_out` set to null.
///
/// # Safety
/// - `participant_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_lookup_participant(
    _factory: *const Int2DdsParticipantFactory,
    domain_id: i32,
    participant_out: *mut *mut Int2DdsParticipant,
) -> Int2DdsRet {
    check_null!(participant_out);

    let factory = DomainParticipantFactory::get_instance();
    match factory.lookup_participant(domain_id) {
        Ok(Some(participant)) => {
            *participant_out =
                Box::into_raw(Box::new(Int2DdsParticipant { inner: Arc::new(participant) }));
            INT2DDS_RET_OK
        }
        Ok(None) => {
            *participant_out = std::ptr::null_mut();
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Set the factory's `autoenable_created_entities` policy (the only member of
/// DomainParticipantFactoryQos).
///
/// # Safety
/// - none beyond a valid process; the factory handle is ignored (singleton).
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_set_qos(
    _factory: *const Int2DdsParticipantFactory,
    autoenable_created_entities: bool,
) -> Int2DdsRet {
    let factory = DomainParticipantFactory::get_instance();
    let mut qos = match factory.get_qos() {
        Ok(q) => q,
        Err(e) => return dds_error_to_code(&e),
    };
    qos.entity_factory.autoenable_created_entities = autoenable_created_entities;
    match factory.set_qos(qos) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the factory's `autoenable_created_entities` policy.
///
/// # Safety
/// - `autoenable_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_get_qos(
    _factory: *const Int2DdsParticipantFactory,
    autoenable_out: *mut bool,
) -> Int2DdsRet {
    check_null!(autoenable_out);

    let factory = DomainParticipantFactory::get_instance();
    match factory.get_qos() {
        Ok(qos) => {
            *autoenable_out = qos.entity_factory.autoenable_created_entities;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Set the factory's default participant QoS (used when a participant is created
/// with default QoS). Pass a QoS handle, or null to reset to the built-in
/// default.
///
/// # Safety
/// - `qos`, if non-null, must be a valid participant QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_set_default_participant_qos(
    _factory: *const Int2DdsParticipantFactory,
    qos: *const Int2DdsParticipantQos,
) -> Int2DdsRet {
    let factory = DomainParticipantFactory::get_instance();
    let result = if qos.is_null() {
        factory.set_default_participant_qos(int2dds::infrastructure::qos_kind::QosKind::Default)
    } else {
        factory.set_default_participant_qos(int2dds::infrastructure::qos_kind::QosKind::Specific(
            (*qos).inner.clone(),
        ))
    };
    match result {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the factory's default participant QoS. On success `*qos_out` receives a
/// new handle the caller frees with `int2dds_participant_qos_destroy`.
///
/// # Safety
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_domain_participant_factory_get_default_participant_qos(
    _factory: *const Int2DdsParticipantFactory,
    qos_out: *mut *mut Int2DdsParticipantQos,
) -> Int2DdsRet {
    check_null!(qos_out);

    let factory = DomainParticipantFactory::get_instance();
    match factory.get_default_participant_qos() {
        Ok(qos) => {
            *qos_out = Box::into_raw(Box::new(Int2DdsParticipantQos { inner: qos }));
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
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
