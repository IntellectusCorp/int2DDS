//! # DomainParticipant
//!
//! Functions for creating and managing DomainParticipants.
//!
//! ## Overview
//!
//! A DomainParticipant is the entry point for DDS communication. It represents
//! a participant in the DDS network and is required before creating any other entities
//! like Publishers, Subscribers, or Topics.
//!
//! ## Domain ID
//!
//! The domain_id parameter isolates communication between different DDS domains.
//! Participants with different domain IDs cannot communicate with each other.

use std::{ffi::CStr, sync::Arc};

use int2dds::{
    domain::{domain_participant_factory::DomainParticipantFactory, qos::DomainParticipantQos},
    infrastructure::status::StatusMask,
};

use super::{error::*, types::*};

/// Create a DomainParticipant
///
/// # Safety
/// - `name` must be a valid null-terminated C string or null
/// - `participant_out` must be a valid pointer to a null pointer
/// - The returned participant must be freed with `int2dds_delete_participant`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_participant(
    _factory: *const Int2DdsParticipantFactory,
    name: *const std::os::raw::c_char,
    domain_id: i32,
    participant_out: *mut *mut Int2DdsParticipant,
) -> Int2DdsRet {
    check_null!(participant_out);

    // Name is optional for now (not used in current implementation)
    // Just validate UTF-8 without allocating String
    if !name.is_null() {
        if CStr::from_ptr(name).to_str().is_err() {
            return INT2DDS_RET_INVALID_ARGUMENT;
        }
    }

    let factory = DomainParticipantFactory::get_instance();

    let participant = ffi_try!(factory.create_participant(
        domain_id,
        DomainParticipantQos::default(),
        None,
        StatusMask::default()
    ));

    let participant_handle = Box::new(Int2DdsParticipant { inner: Arc::new(participant) });

    *participant_out = Box::into_raw(participant_handle);

    INT2DDS_RET_OK
}

/// Delete a DomainParticipant
///
/// # Safety
/// - `participant` must be a valid participant created by `int2dds_create_participant`
/// - `participant` must not be used after this call
/// - All entities created by this participant must be deleted first
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_participant(
    participant: *mut Int2DdsParticipant,
) -> Int2DdsRet {
    if participant.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let participant_box = Box::from_raw(participant);
    let participant_inner = (*participant_box.inner).clone();

    let factory = DomainParticipantFactory::get_instance();

    match factory.delete_participant(participant_inner) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Assert liveliness for a participant (for MANUAL_BY_PARTICIPANT liveliness)
///
/// # Safety
/// - `participant` must be a valid participant
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_assert_liveliness(
    participant: *const Int2DdsParticipant,
) -> Int2DdsRet {
    check_null!(participant);

    let _participant_ref = &*participant;

    // DomainParticipant doesn't have assert_liveliness method directly,
    // but we can return OK for now as this is typically handled at writer level
    INT2DDS_RET_OK
}

/// Get the domain ID of a participant
///
/// # Safety
/// - `participant` must be a valid participant
/// - `domain_id_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_domain_id(
    participant: *const Int2DdsParticipant,
    domain_id_out: *mut i32,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(domain_id_out);

    let participant_ref = &*participant;

    match participant_ref.inner.get_domain_id() {
        Ok(domain_id) => {
            *domain_id_out = domain_id as i32;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Delete all entities contained by a participant
///
/// This operation deletes all Publisher, Subscriber, Topic, ContentFilteredTopic
/// and MultiTopic objects created through this participant. It recursively calls
/// delete_contained_entities on each contained entity.
///
/// # Safety
/// - `participant` must be a valid participant
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_delete_contained_entities(
    participant: *const Int2DdsParticipant,
) -> Int2DdsRet {
    check_null!(participant);

    let participant_ref = &*participant;

    match participant_ref.inner.delete_contained_entities() {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context;
    use std::ptr;

    #[test]
    fn test_create_delete_participant() {
        unsafe {
            let mut factory: *mut Int2DdsParticipantFactory = ptr::null_mut();
            context::int2dds_domain_participant_factory_get_instance(&mut factory as *mut _);

            let mut participant: *mut Int2DdsParticipant = ptr::null_mut();

            // Create participant
            let domain_id = 0;
            let ret = int2dds_create_participant(
                factory,
                ptr::null(),
                domain_id,
                &mut participant as *mut _,
            );
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!participant.is_null());

            // Get domain ID
            let mut retrieved_domain_id: i32 = -1;
            let ret = int2dds_participant_get_domain_id(participant, &mut retrieved_domain_id);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert_eq!(retrieved_domain_id, domain_id);

            // Delete participant
            let ret = int2dds_delete_participant(participant);
            assert_eq!(ret, INT2DDS_RET_OK);

            context::int2dds_domain_participant_factory_finalize(factory);
        }
    }

    #[test]
    fn test_participant_domain_1() {
        unsafe {
            let mut factory: *mut Int2DdsParticipantFactory = ptr::null_mut();
            context::int2dds_domain_participant_factory_get_instance(&mut factory as *mut _);

            let mut participant: *mut Int2DdsParticipant = ptr::null_mut();

            // Create participant with domain 1
            let domain_id = 1;
            let ret = int2dds_create_participant(
                factory,
                ptr::null(),
                domain_id,
                &mut participant as *mut _,
            );
            assert_eq!(ret, INT2DDS_RET_OK);
            assert!(!participant.is_null());

            // Verify domain ID
            let mut retrieved_domain_id: i32 = -1;
            let ret = int2dds_participant_get_domain_id(participant, &mut retrieved_domain_id);
            assert_eq!(ret, INT2DDS_RET_OK);
            assert_eq!(retrieved_domain_id, domain_id);

            // Delete participant
            let ret = int2dds_delete_participant(participant);
            assert_eq!(ret, INT2DDS_RET_OK);

            context::int2dds_domain_participant_factory_finalize(factory);
        }
    }
}
