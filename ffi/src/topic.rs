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
//! The FFI uses Int2DdsData type with DynamicTypeSupport for CDR serialization/deserialization.
//! The type system is registered with the DomainParticipant before topic creation.

use std::ffi::{CStr, CString};
use std::sync::Arc;

use int2dds::{infrastructure::status::StatusMask, topic::qos::TopicQos, topic::TypeSupport};

use crate::data::Int2DdsData;
use crate::dynamic_type_support::DynamicTypeSupport;
use crate::type_descriptor::Int2DdsTypeDescriptor;

use super::{error::*, qos::Int2DdsTopicQos, types::*};

/// Create a Topic
///
/// Creates a topic with Int2DdsData type and registers the DynamicTypeSupport
/// for CDR serialization/deserialization.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `type_desc` must be a valid type descriptor
/// - `qos` can be null for default QoS
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    type_desc: *const Int2DdsTypeDescriptor,
    qos: *const Int2DdsTopicQos,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(type_desc);
    check_null!(topic_out);

    let participant_ref = &*participant;

    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // Clone the type descriptor into Arc
    let type_desc_ref = &*type_desc;
    let type_descriptor = Arc::new(Int2DdsTypeDescriptor {
        type_name: type_desc_ref.type_name.clone(),
        fields: type_desc_ref.fields.clone(),
        field_indices: type_desc_ref.field_indices.clone(),
        extensibility: type_desc_ref.extensibility,
        next_member_id: type_desc_ref.fields.len() as u32,
        xcdr_version: type_desc_ref.xcdr_version,
    });

    // Create DynamicTypeSupport
    let type_support = Arc::new(DynamicTypeSupport::new(type_descriptor.clone()));
    let type_name = &type_descriptor.type_name;

    // Register the DynamicTypeSupport with the participant BEFORE creating the topic.
    // This ensures the DDS core uses our TypeSupport for serialization/deserialization
    // instead of the placeholder DynamicTypeSupportDefault.
    ffi_try!(participant_ref
        .inner
        .register_type_support(type_support.clone() as Arc<dyn TypeSupport>, type_name));

    let topic_qos = if qos.is_null() { TopicQos::default() } else { (*qos).inner.clone() };

    // Create topic using Int2DdsData type
    // The registered DynamicTypeSupport will handle serialization when writing/reading
    let topic = ffi_try!(participant_ref.inner.create_topic::<Int2DdsData>(
        topic_name_str,
        type_name,
        topic_qos,
        None,
        StatusMask::default()
    ));

    let topic_handle =
        Box::new(Int2DdsTopic { inner: Arc::new(topic), type_support, type_descriptor });

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

    // Destructure Box to move Arc out
    let Int2DdsTopic { inner: topic_arc, type_support: _ts, type_descriptor: _td } =
        *Box::from_raw(topic);

    // Try to unwrap Arc without cloning (succeeds if this is the only reference)
    let topic_obj = match Arc::try_unwrap(topic_arc) {
        Ok(t) => t,
        Err(_arc) => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };

    // Get the participant to delete the topic
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

    // Use the type descriptor's type name
    let type_name = &topic_ref.type_descriptor.type_name;

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

/// Get the type descriptor of a Topic
///
/// Returns a pointer to the type descriptor. The returned pointer is valid
/// as long as the topic is not deleted.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `desc_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_type_descriptor(
    topic: *const Int2DdsTopic,
    desc_out: *mut *const Int2DdsTypeDescriptor,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(desc_out);

    let topic_ref = &*topic;
    *desc_out = Arc::as_ptr(&topic_ref.type_descriptor);

    INT2DDS_RET_OK
}
