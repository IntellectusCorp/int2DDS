//! # Topic Management
//!
//! Functions for creating and managing DDS Topics.
//!
//! ## Overview
//!
//! Topics define the subject of data exchange between Publishers and Subscribers.
//! Each Topic has a name and a type, and is associated with a DomainParticipant.
//!
//! ## Data Type
//!
//! The FFI uses raw bytes mode: C users serialize data with IDL-generated code
//! and pass CDR bytes directly via `int2dds_write_serialized` / `int2dds_take_serialized`.

use std::ffi::{CStr, CString};
use std::sync::Arc;

use int2dds::{
    infrastructure::status::StatusMask, serialize::cdr::ExtensibilityKind, topic::qos::TopicQos,
    topic::TypeSupport,
};

use crate::data::Int2DdsData;
use crate::raw_type_support::RawTypeSupport;

use super::{error::*, qos::Int2DdsTopicQos, types::*};

/// Create a Topic
///
/// Creates a topic with RawTypeSupport for use with `int2dds_write_serialized()`
/// and `int2dds_take_serialized()`. C users handle CDR serialization themselves
/// using IDL-generated code.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
/// - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
/// - `qos` can be null for default QoS
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    extensibility: i32,
    qos: *const Int2DdsTopicQos,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    int2dds_create_topic_keyed(
        participant,
        topic_name,
        dds_type_name,
        extensibility,
        false,
        qos,
        topic_out,
    )
}

/// Create a Topic with key support
///
/// Same as `int2dds_create_topic` but with an explicit `has_key` parameter.
/// Use this when the data type has key fields for instance management
/// (register_instance, unregister_instance, dispose, lookup_instance).
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
/// - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
/// - `has_key`: whether the data type has key fields
/// - `qos` can be null for default QoS
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_keyed(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    extensibility: i32,
    has_key: bool,
    qos: *const Int2DdsTopicQos,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(dds_type_name);
    check_null!(topic_out);

    let participant_ref = &*participant;

    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let dds_type_name_str = match CStr::from_ptr(dds_type_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let ext_kind = match extensibility {
        0 => ExtensibilityKind::Final,
        1 => ExtensibilityKind::Appendable,
        2 => ExtensibilityKind::Mutable,
        _ => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // Create RawTypeSupport
    let type_support =
        Arc::new(RawTypeSupport::new_with_key(dds_type_name_str.to_string(), ext_kind, has_key));

    // Register the RawTypeSupport with the participant
    ffi_try!(participant_ref
        .inner
        .register_type_support(type_support as Arc<dyn TypeSupport>, dds_type_name_str));

    let topic_qos = if qos.is_null() { TopicQos::default() } else { (*qos).inner.clone() };

    let topic = ffi_try!(participant_ref.inner.create_topic::<Int2DdsData>(
        topic_name_str,
        dds_type_name_str,
        topic_qos,
        None,
        StatusMask::default()
    ));

    let topic_handle =
        Box::new(Int2DdsTopic { inner: Arc::new(topic), type_name: dds_type_name_str.to_string() });

    *topic_out = Box::into_raw(topic_handle);

    INT2DDS_RET_OK
}

/// Delete a Topic
///
/// # Safety
/// - `topic` must be a valid topic created by `int2dds_create_topic`
/// - `topic` must not be used after this call
/// - All DataReaders and DataWriters using this topic must be deleted first
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_topic(topic: *mut Int2DdsTopic) -> Int2DdsRet {
    if topic.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let topic_ref = &*topic;
    if Arc::strong_count(&topic_ref.inner) != 1 {
        return INT2DDS_RET_PRECONDITION_NOT_MET;
    }

    let Int2DdsTopic { inner: topic_arc, type_name: _tn } = *Box::from_raw(topic);

    let topic_obj = match Arc::try_unwrap(topic_arc) {
        Ok(t) => t,
        Err(_arc) => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };

    let participant = match topic_obj.get_participant() {
        Ok(p) => p,
        Err(e) => return dds_error_to_code(&e),
    };

    match participant.delete_topic(topic_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Get the name of a Topic
///
/// # Safety
/// - `topic` must be a valid topic
/// - `name_out` must be a valid pointer to a char buffer
/// - `name_size` is the size of the buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_name(
    topic: *const Int2DdsTopic,
    name_out: *mut std::os::raw::c_char,
    name_size: usize,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(name_out);

    let topic_ref = &*topic;

    let name = topic_ref.inner.get_name();
    let name_cstr = match CString::new(name) {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_ERROR,
    };

    let name_bytes = name_cstr.as_bytes_with_nul();
    if name_bytes.len() > name_size {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(name_bytes.as_ptr() as *const i8, name_out, name_bytes.len());

    INT2DDS_RET_OK
}

/// Get the type name of a Topic
///
/// # Safety
/// - `topic` must be a valid topic
/// - `type_name_out` must be a valid pointer to a char buffer
/// - `type_name_size` is the size of the buffer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_type_name(
    topic: *const Int2DdsTopic,
    type_name_out: *mut std::os::raw::c_char,
    type_name_size: usize,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(type_name_out);

    let topic_ref = &*topic;

    let type_name = &topic_ref.type_name;

    let type_name_cstr = match CString::new(type_name.as_str()) {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_ERROR,
    };

    let type_name_bytes = type_name_cstr.as_bytes_with_nul();
    if type_name_bytes.len() > type_name_size {
        return INT2DDS_RET_ERROR;
    }

    std::ptr::copy_nonoverlapping(
        type_name_bytes.as_ptr() as *const i8,
        type_name_out,
        type_name_bytes.len(),
    );

    INT2DDS_RET_OK
}
