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
use std::os::raw::c_char;
use std::sync::Arc;

use int2dds::{
    infrastructure::status::StatusMask, serialize::cdr::ExtensibilityKind, topic::qos::TopicQos,
    topic::TypeSupport,
};

use crate::data::Int2DdsData;
use crate::raw_type_support::RawTypeSupport;
use crate::type_info::Int2DdsTypeInfo;

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

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let topic_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

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

/// Create a Topic using a QoS profile path
///
/// Same as `int2dds_create_topic_keyed` but uses a QoS profile path instead of a QoS handle.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
/// - `extensibility`: 0 = Final, 1 = Appendable, 2 = Mutable
/// - `has_key`: whether the data type has key fields
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_with_profile(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    extensibility: i32,
    has_key: bool,
    qos_path: *const std::os::raw::c_char,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(dds_type_name);
    check_null!(qos_path);
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

    let qos_path_str = match CStr::from_ptr(qos_path).to_str() {
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

    let topic = ffi_try!(participant_ref.inner.create_topic_with_profile::<Int2DdsData>(
        topic_name_str,
        dds_type_name_str,
        qos_path_str,
        None,
        StatusMask::default()
    ));

    let topic_handle =
        Box::new(Int2DdsTopic { inner: Arc::new(topic), type_name: dds_type_name_str.to_string() });

    *topic_out = Box::into_raw(topic_handle);

    INT2DDS_RET_OK
}

/// Create a Topic with type information for DDS-XTypes discovery
///
/// Creates a topic using a pre-built `Int2DdsTypeInfo` which provides
/// TypeIdentifier and TypeObject for DDS discovery parameters (0x0069, 0x0072).
/// This enables interoperability with implementations that require type information
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `type_info` must be a valid `Int2DdsTypeInfo` created by `int2dds_type_info_create`
/// - `qos` can be null for default QoS
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_with_type_info(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    type_info: *const Int2DdsTypeInfo,
    qos: *const Int2DdsTopicQos,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(type_info);
    check_null!(topic_out);

    let participant_ref = &*participant;
    let ti = &*type_info;

    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let dds_type_name = &ti.type_name;

    // Build TypeIdentifier and TypeObject from the type info
    let type_identifier = ti.build_type_identifier();
    let type_object = ti.build_type_object();

    // Create RawTypeSupport with type info for discovery
    let type_support = Arc::new(RawTypeSupport::with_type_info(
        dds_type_name.clone(),
        ti.extensibility,
        ti.has_key_field(),
        type_identifier,
        type_object,
    ));

    // Register the RawTypeSupport with the participant
    ffi_try!(participant_ref
        .inner
        .register_type_support(type_support as Arc<dyn TypeSupport>, dds_type_name));

    // NULL qos → default sentinel (engages profile fallback). Non-NULL → use as-is.
    let topic_qos = if qos.is_null() {
        int2dds::infrastructure::qos_kind::QosKind::Default
    } else {
        int2dds::infrastructure::qos_kind::QosKind::Specific((*qos).inner.clone())
    };

    let topic = ffi_try!(participant_ref.inner.create_topic::<Int2DdsData>(
        topic_name_str,
        dds_type_name,
        topic_qos,
        None,
        StatusMask::default()
    ));

    let topic_handle =
        Box::new(Int2DdsTopic { inner: Arc::new(topic), type_name: dds_type_name.clone() });

    *topic_out = Box::into_raw(topic_handle);

    INT2DDS_RET_OK
}

/// Set QoS on a Topic
///
/// # Safety
/// - `topic` must be a valid topic
/// - `qos` must be a valid topic QoS handle
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_set_qos(
    topic: *const Int2DdsTopic,
    qos: *const Int2DdsTopicQos,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(qos);

    let topic_ref = &*topic;
    let qos_ref = &*qos;

    ffi_try!(topic_ref.inner.set_qos(qos_ref.inner.clone()));

    INT2DDS_RET_OK
}

/// Get QoS from a Topic
///
/// The returned handle must be freed with `int2dds_topic_qos_destroy`.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `qos_out` must be a valid pointer to a null pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_qos(
    topic: *const Int2DdsTopic,
    qos_out: *mut *mut Int2DdsTopicQos,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(qos_out);

    let topic_ref = &*topic;
    let qos = ffi_try!(topic_ref.inner.get_qos());
    let boxed = Box::new(Int2DdsTopicQos { inner: qos });
    *qos_out = Box::into_raw(boxed);

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

    std::ptr::copy_nonoverlapping(name_bytes.as_ptr() as *const c_char, name_out, name_bytes.len());

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
        type_name_bytes.as_ptr() as *const c_char,
        type_name_out,
        type_name_bytes.len(),
    );

    INT2DDS_RET_OK
}

/// Create a ContentFilteredTopic
///
/// Creates a content-filtered topic that filters data based on a SQL-like expression.
/// The filter expression uses SQL-92 syntax with parameters referenced as %0, %1, etc.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `related_topic` must be a valid topic created on the same participant
/// - `filter_expression` must be a valid null-terminated C string (e.g., "color = %0")
/// - `expression_parameters` must be a valid array of null-terminated C strings, or null if count is 0
/// - `expression_parameters_count` is the number of parameters
/// - `cft_out` must be a valid pointer to a null pointer
/// - The returned ContentFilteredTopic must be freed with `int2dds_delete_contentfilteredtopic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_contentfilteredtopic(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    related_topic: *const Int2DdsTopic,
    filter_expression: *const std::os::raw::c_char,
    expression_parameters: *const *const std::os::raw::c_char,
    expression_parameters_count: usize,
    cft_out: *mut *mut Int2DdsContentFilteredTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(related_topic);
    check_null!(filter_expression);
    check_null!(cft_out);

    let participant_ref = &*participant;
    let related_topic_ref = &*related_topic;

    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let filter_expression_str = match CStr::from_ptr(filter_expression).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // Convert C string array to Vec<String>
    let mut params = Vec::new();
    if expression_parameters_count > 0 {
        if expression_parameters.is_null() {
            return INT2DDS_RET_INVALID_ARGUMENT;
        }
        for i in 0..expression_parameters_count {
            let param_ptr = *expression_parameters.add(i);
            if param_ptr.is_null() {
                return INT2DDS_RET_INVALID_ARGUMENT;
            }
            match CStr::from_ptr(param_ptr).to_str() {
                Ok(s) => params.push(s.to_string()),
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            }
        }
    }

    let cft = ffi_try!(participant_ref.inner.create_contentfilteredtopic::<Int2DdsData>(
        topic_name_str,
        &*related_topic_ref.inner,
        filter_expression_str,
        params,
    ));

    let type_name = related_topic_ref.type_name.clone();
    let cft_handle = Box::new(Int2DdsContentFilteredTopic { inner: cft, type_name });
    *cft_out = Box::into_raw(cft_handle);

    INT2DDS_RET_OK
}

/// Delete a ContentFilteredTopic
///
/// # Safety
/// - `cft` must be a valid ContentFilteredTopic created by `int2dds_create_contentfilteredtopic`
/// - `cft` must not be used after this call
/// - All DataReaders using this ContentFilteredTopic must be deleted first
#[no_mangle]
pub unsafe extern "C" fn int2dds_delete_contentfilteredtopic(
    cft: *mut Int2DdsContentFilteredTopic,
) -> Int2DdsRet {
    if cft.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let Int2DdsContentFilteredTopic { inner: cft_obj, type_name: _tn } = *Box::from_raw(cft);

    let participant = match cft_obj.get_participant() {
        Ok(p) => p,
        Err(e) => return dds_error_to_code(&e),
    };

    match participant.delete_contentfilteredtopic(cft_obj) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Update the expression parameters of an existing ContentFilteredTopic
///
/// This updates only the parameter values for the current filter expression.
///
/// # Safety
/// - `cft` must be a valid ContentFilteredTopic created by `int2dds_create_contentfilteredtopic`
/// - `expression_parameters` must be a valid array of null-terminated C strings, or null if count is 0
/// - `expression_parameters_count` is the number of parameters
#[no_mangle]
pub unsafe extern "C" fn int2dds_contentfilteredtopic_set_expression_parameters(
    cft: *mut Int2DdsContentFilteredTopic,
    expression_parameters: *const *const std::os::raw::c_char,
    expression_parameters_count: usize,
) -> Int2DdsRet {
    if cft.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let cft_ref = &mut *cft;

    let mut params = Vec::new();
    if expression_parameters_count > 0 {
        if expression_parameters.is_null() {
            return INT2DDS_RET_INVALID_ARGUMENT;
        }
        for i in 0..expression_parameters_count {
            let param_ptr = *expression_parameters.add(i);
            if param_ptr.is_null() {
                return INT2DDS_RET_INVALID_ARGUMENT;
            }
            match CStr::from_ptr(param_ptr).to_str() {
                Ok(s) => params.push(s.to_string()),
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            }
        }
    }

    match cft_ref.inner.set_expression_parameters(params) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_contentfilteredtopic_set_filter_expression(
    cft: *mut Int2DdsContentFilteredTopic,
    filter_expression: *const std::os::raw::c_char,
    expression_parameters: *const *const std::os::raw::c_char,
    expression_parameters_count: usize,
) -> Int2DdsRet {
    if cft.is_null() || filter_expression.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let cft_ref = &mut *cft;
    let filter_expression = match CStr::from_ptr(filter_expression).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let mut params = Vec::new();
    if expression_parameters_count > 0 {
        if expression_parameters.is_null() {
            return INT2DDS_RET_INVALID_ARGUMENT;
        }
        for i in 0..expression_parameters_count {
            let param_ptr = *expression_parameters.add(i);
            if param_ptr.is_null() {
                return INT2DDS_RET_INVALID_ARGUMENT;
            }
            match CStr::from_ptr(param_ptr).to_str() {
                Ok(s) => params.push(s.to_string()),
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            }
        }
    }

    match cft_ref.inner.set_filter_expression(filter_expression, params) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_contentfilteredtopic_set_enabled(
    cft: *mut Int2DdsContentFilteredTopic,
    enabled: bool,
) -> Int2DdsRet {
    if cft.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }

    let cft_ref = &mut *cft;
    match cft_ref.inner.set_enabled(enabled) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Create a Topic with key field metadata for compute_key() support.
///
/// Same as int2dds_create_topic_keyed but additionally accepts key field
/// descriptors that enable instance handle computation from CDR data.
/// This is needed when the remote publisher does not include KEY_HASH
/// in inline QoS (e.g., CoreDX).
///
/// # Safety
/// - Same as int2dds_create_topic_keyed
/// - field_indices/field_types must point to field_count elements, or be null if field_count is 0
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_keyed_with_key_fields(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    extensibility: i32,
    has_key: bool,
    qos: *const Int2DdsTopicQos,
    field_indices: *const u32,
    field_types: *const u32,
    field_count: usize,
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

    // Build key field metadata
    use crate::raw_type_support::{KeyFieldInfo, KeyFieldType};
    let mut key_fields = Vec::new();
    if field_count > 0 && !field_indices.is_null() && !field_types.is_null() {
        for i in 0..field_count {
            let field_type = match *field_types.add(i) {
                0 => KeyFieldType::String,
                1 => KeyFieldType::Int32,
                2 => KeyFieldType::UInt32,
                3 => KeyFieldType::Int16,
                4 => KeyFieldType::UInt16,
                5 => KeyFieldType::Int64,
                6 => KeyFieldType::UInt64,
                7 => KeyFieldType::Int8,
                8 => KeyFieldType::UInt8,
                9 => KeyFieldType::Bool,
                _ => return INT2DDS_RET_INVALID_ARGUMENT,
            };
            key_fields
                .push(KeyFieldInfo { field_index: *field_indices.add(i) as usize, field_type });
        }
    }

    // Create RawTypeSupport with key fields
    let mut type_support =
        RawTypeSupport::new_with_key(dds_type_name_str.to_string(), ext_kind, has_key);
    type_support.set_key_fields(key_fields);

    // Register the RawTypeSupport with the participant
    ffi_try!(participant_ref
        .inner
        .register_type_support(Arc::new(type_support) as Arc<dyn TypeSupport>, dds_type_name_str));

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

/// Create a Topic with full field descriptors for reader-side CFT filtering.
///
/// Extends int2dds_create_topic_keyed_with_key_fields by also providing
/// all field metadata (name, type) needed for get_field_value() support.
/// This enables ContentFilteredTopic reader-side filtering in the serialized path.
///
/// # Safety
/// - Same as int2dds_create_topic_keyed
/// - field_names: array of null-terminated C strings (field_count elements)
/// - field_types: array of u32 type IDs (field_count elements)
/// - field_is_key: array of bool (field_count elements)
/// - field_count: number of fields
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_with_field_descriptors(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    extensibility: i32,
    has_key: bool,
    qos: *const Int2DdsTopicQos,
    field_names: *const *const std::os::raw::c_char,
    field_types: *const u32,
    field_is_key: *const bool,
    field_count: usize,
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

    // Build field descriptors and key fields
    use crate::data::{CdrFieldDescriptor, CdrFieldType};
    use crate::raw_type_support::{KeyFieldInfo, KeyFieldType};

    let mut all_fields = Vec::new();
    let mut key_fields = Vec::new();

    if field_count > 0 {
        check_null!(field_names);
        check_null!(field_types);
        check_null!(field_is_key);

        for i in 0..field_count {
            let name_ptr = *field_names.add(i);
            if name_ptr.is_null() {
                return INT2DDS_RET_INVALID_ARGUMENT;
            }
            let name = match CStr::from_ptr(name_ptr).to_str() {
                Ok(s) => s.to_string(),
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            };

            let type_id = *field_types.add(i);
            let is_key = *field_is_key.add(i);

            let cdr_type = match type_id {
                0 => CdrFieldType::String,
                1 => CdrFieldType::Int32,
                2 => CdrFieldType::UInt32,
                3 => CdrFieldType::Int16,
                4 => CdrFieldType::UInt16,
                5 => CdrFieldType::Int64,
                6 => CdrFieldType::UInt64,
                7 => CdrFieldType::Int8,
                8 => CdrFieldType::UInt8,
                9 => CdrFieldType::Bool,
                _ => return INT2DDS_RET_INVALID_ARGUMENT,
            };

            all_fields.push(CdrFieldDescriptor {
                name: name.clone(),
                field_type: cdr_type,
                is_key,
            });

            if is_key {
                let key_type = match type_id {
                    0 => KeyFieldType::String,
                    1 => KeyFieldType::Int32,
                    2 => KeyFieldType::UInt32,
                    3 => KeyFieldType::Int16,
                    4 => KeyFieldType::UInt16,
                    5 => KeyFieldType::Int64,
                    6 => KeyFieldType::UInt64,
                    7 => KeyFieldType::Int8,
                    8 => KeyFieldType::UInt8,
                    9 => KeyFieldType::Bool,
                    _ => return INT2DDS_RET_INVALID_ARGUMENT,
                };
                key_fields.push(KeyFieldInfo { field_index: i, field_type: key_type });
            }
        }
    }

    // Create RawTypeSupport with both key fields and all fields
    let mut type_support =
        RawTypeSupport::new_with_key(dds_type_name_str.to_string(), ext_kind, has_key);
    type_support.set_key_fields(key_fields);
    type_support.set_all_fields(all_fields);

    // Register the RawTypeSupport with the participant
    ffi_try!(participant_ref
        .inner
        .register_type_support(Arc::new(type_support) as Arc<dyn TypeSupport>, dds_type_name_str));

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
