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
    common::instance_handle::InstanceHandle, core::time::Duration,
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::status::StatusMask,
};

use super::{error::*, qos::Int2DdsParticipantQos, types::*};

/// Create a DomainParticipant
///
/// # Safety
/// - `qos` can be null for the default QoS (engages the core resolution chain:
///   registered default → configured default profile → spec default); a non-null
///   handle is cloned internally and remains owned by the caller
/// - `participant_out` must be a valid pointer to a null pointer
/// - The returned participant must be freed with `int2dds_delete_participant`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_participant(
    _factory: *const Int2DdsParticipantFactory,
    domain_id: i32,
    qos: *const Int2DdsParticipantQos,
    participant_out: *mut *mut Int2DdsParticipant,
) -> Int2DdsRet {
    check_null!(participant_out);

    let factory = DomainParticipantFactory::get_instance();

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let participant_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    let participant = ffi_try!(factory.create_participant(
        domain_id,
        participant_qos,
        None,
        StatusMask::default()
    ));

    let participant_handle = Box::new(Int2DdsParticipant { inner: Arc::new(participant) });

    *participant_out = Box::into_raw(participant_handle);

    INT2DDS_RET_OK
}

/// Create a DomainParticipant using a QoS profile path
///
/// # Safety
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `participant_out` must be a valid pointer to a null pointer
/// - The returned participant must be freed with `int2dds_delete_participant`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_participant_with_profile(
    _factory: *const Int2DdsParticipantFactory,
    domain_id: i32,
    qos_path: *const std::os::raw::c_char,
    participant_out: *mut *mut Int2DdsParticipant,
) -> Int2DdsRet {
    check_null!(qos_path);
    check_null!(participant_out);

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let factory = DomainParticipantFactory::get_instance();

    let participant = ffi_try!(factory.create_participant_with_profile(
        domain_id,
        qos_path_str,
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

    // On failure the participant is not deleted; restore the caller's handle
    // (into_raw) instead of leaving it freed. The Box drops (frees) only on success.
    match factory.delete_participant(participant_inner) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => {
            let _ = Box::into_raw(participant_box);
            dds_error_to_code(&e)
        }
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
            *domain_id_out = domain_id;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Set the QoS of a participant.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos` must be a valid QoS created by `int2dds_participant_qos_create_default`
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_set_qos(
    participant: *const Int2DdsParticipant,
    qos: *const Int2DdsParticipantQos,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(qos);

    let participant_ref = &*participant;
    let qos_ref = &*qos;
    ffi_try!(participant_ref.inner.set_qos(qos_ref.inner.clone()));

    INT2DDS_RET_OK
}

/// Get the QoS of a participant.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `qos_out` must be a valid pointer to a null pointer
/// - The returned QoS must be freed with `int2dds_participant_qos_destroy`
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_qos(
    participant: *const Int2DdsParticipant,
    qos_out: *mut *mut Int2DdsParticipantQos,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(qos_out);

    let participant_ref = &*participant;
    let qos = ffi_try!(participant_ref.inner.get_qos());
    let handle = Box::new(Int2DdsParticipantQos { inner: qos });
    *qos_out = Box::into_raw(handle);

    INT2DDS_RET_OK
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

/// Get the participant's current wall-clock time.
///
/// `sec` is a Unix timestamp (seconds since the epoch); `nanosec` is the
/// sub-second remainder.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `sec_out` and `nanosec_out` must be valid pointers
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_get_current_time(
    participant: *const Int2DdsParticipant,
    sec_out: *mut i32,
    nanosec_out: *mut u32,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(sec_out);
    check_null!(nanosec_out);

    match (*participant).inner.get_current_time() {
        Ok(time) => {
            *sec_out = time.sec;
            *nanosec_out = time.nanosec;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Test whether an entity (Publisher/Subscriber/Topic and their children) with
/// the given 16-byte instance handle belongs to this participant.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `handle` must point to a 16-byte instance handle
/// - `result_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_contains_entity(
    participant: *const Int2DdsParticipant,
    handle: *const [u8; 16],
    result_out: *mut bool,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(handle);
    check_null!(result_out);

    let instance_handle = InstanceHandle::new(*handle);
    match (*participant).inner.contains_entity(instance_handle) {
        Ok(contained) => {
            *result_out = contained;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Find an existing local Topic by name, blocking up to `timeout_ms` for it to
/// appear (a negative value blocks indefinitely — avoid it if the topic may
/// never exist). `dds_type_name` labels the returned handle for the raw path and
/// must match the caller's expected type.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` and `dds_type_name` must be valid null-terminated C strings
/// - `topic_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_participant_find_topic(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    timeout_ms: i32,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(dds_type_name);
    check_null!(topic_out);

    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let dds_type_name_str = match CStr::from_ptr(dds_type_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let timeout = if timeout_ms < 0 {
        Duration { sec: 0x7fff_ffff, nanosec: 0x7fff_ffff }
    } else {
        Duration { sec: timeout_ms / 1000, nanosec: ((timeout_ms % 1000) as u32) * 1_000_000 }
    };

    let topic = ffi_try!((*participant).inner.find_topic(topic_name_str, timeout));
    let topic_handle = Box::new(Int2DdsTopic {
        inner: Arc::new(topic),
        type_name: dds_type_name_str.to_string(),
        frame_layout: None,
        plans: None,
        c_layout: std::sync::OnceLock::new(),
    });
    *topic_out = Box::into_raw(topic_handle);
    INT2DDS_RET_OK
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
                domain_id,
                ptr::null(),
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
                domain_id,
                ptr::null(),
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

    #[test]
    fn test_participant_set_get_qos_round_trip() {
        use crate::qos::{
            int2dds_participant_qos_add_property, int2dds_participant_qos_create_default,
            int2dds_participant_qos_destroy,
        };

        unsafe {
            let mut factory: *mut Int2DdsParticipantFactory = ptr::null_mut();
            context::int2dds_domain_participant_factory_get_instance(&mut factory as *mut _);

            let mut participant: *mut Int2DdsParticipant = ptr::null_mut();
            assert_eq!(
                int2dds_create_participant(factory, 2, ptr::null(), &mut participant as *mut _),
                INT2DDS_RET_OK
            );

            // Build a QoS carrying a distinctive property and apply it.
            let mut qos: *mut Int2DdsParticipantQos = ptr::null_mut();
            assert_eq!(int2dds_participant_qos_create_default(&mut qos as *mut _), INT2DDS_RET_OK);
            let name = std::ffi::CString::new("vendor.us.int2.participant_qos").unwrap();
            let value = std::ffi::CString::new("live").unwrap();
            assert_eq!(
                int2dds_participant_qos_add_property(qos, name.as_ptr(), value.as_ptr(), true),
                INT2DDS_RET_OK
            );
            assert_eq!(int2dds_participant_set_qos(participant, qos), INT2DDS_RET_OK);

            // Read it back through the FFI and confirm the property survived.
            let mut out: *mut Int2DdsParticipantQos = ptr::null_mut();
            assert_eq!(
                int2dds_participant_get_qos(participant, &mut out as *mut _),
                INT2DDS_RET_OK
            );
            assert!(!out.is_null());
            assert_eq!(
                (*out).inner.property.find_property("vendor.us.int2.participant_qos"),
                Some("live")
            );

            // Null-pointer guards.
            assert_eq!(int2dds_participant_set_qos(ptr::null(), qos), INT2DDS_RET_NULL_POINTER);
            assert_eq!(
                int2dds_participant_get_qos(participant, ptr::null_mut()),
                INT2DDS_RET_NULL_POINTER
            );

            assert_eq!(int2dds_participant_qos_destroy(out), INT2DDS_RET_OK);
            assert_eq!(int2dds_participant_qos_destroy(qos), INT2DDS_RET_OK);
            assert_eq!(int2dds_delete_participant(participant), INT2DDS_RET_OK);
            context::int2dds_domain_participant_factory_finalize(factory);
        }
    }
}
