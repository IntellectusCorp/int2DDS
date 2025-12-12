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
//! The FFI uses RawData type internally, which means serialization/deserialization
//! must be handled on the C side using CDR format.

use std::ffi::{CStr, CString};
use std::sync::Arc;

use int2dds::{
    infrastructure::status::StatusMask,
    topic::{qos::TopicQos, RawData},
};

use super::{error::*, qos::Int2DdsTopicQos, types::*};

/// Create a Topic
///
/// Creates a topic using RawData type for FFI. The C application is responsible
/// for serializing/deserializing data in CDR format.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `type_name` must be a valid null-terminated C string
/// - `qos` can be null for default QoS
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    type_name: *const std::os::raw::c_char,
    qos: *const Int2DdsTopicQos,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(type_name);
    check_null!(topic_out);

    let participant_ref = &*participant;

    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let type_name_str = match CStr::from_ptr(type_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let topic_qos = if qos.is_null() { TopicQos::default() } else { (*qos).inner.clone() };

    // Create topic using RawData type
    // The type_name parameter is used when registering with the participant
    let topic = ffi_try!(participant_ref.inner.create_topic::<RawData>(
        topic_name_str,
        type_name_str,
        topic_qos,
        None,
        StatusMask::default()
    ));

    let topic_handle = Box::new(Int2DdsTopic { inner: Arc::new(topic) });

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

    // Just drop the Arc reference
    let _topic = Box::from_raw(topic);

    INT2DDS_RET_OK
}

/// Get the name of a Topic
///
/// # Safety
/// - `topic` must be a valid topic
/// - `name_out` must be a valid pointer to a char buffer
/// - `name_size` is the size of the buffer
/// - Returns the number of bytes written (excluding null terminator)
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_name(
    topic: *const Int2DdsTopic,
    name_out: *mut std::os::raw::c_char,
    name_size: usize,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(name_out);

    let topic_ref = &*topic;

    // According to trait definition, get_name() returns &str directly
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

    // According to trait definition, get_type_name() returns &str directly
    let type_name = topic_ref.inner.get_type_name();

    let type_name_cstr = match CString::new(type_name) {
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
