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
//! The FFI uses raw bytes mode: C users serialize data with IDL-generated code and pass
//! CDR bytes directly via `int2dds_datawriter_write_serialized` /
//! `int2dds_datareader_take_serialized`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::sync::Arc;

use int2dds::{
    core::error::DdsError, infrastructure::status::StatusMask, serialize::cdr::ExtensibilityKind,
    topic::TypeSupport,
};

use crate::data::Int2DdsData;
use crate::raw_type_support::RawTypeSupport;
use crate::status::Int2DdsInconsistentTopicStatus;
use crate::type_info::Int2DdsTypeInfo;

use super::{error::*, qos::Int2DdsTopicQos, types::*};

/// Shared tail for topic creation once a `RawTypeSupport` is fully built: register the
/// type support, resolve QoS (NULL -> default sentinel so profile fallback engages),
/// create the topic, and hand back a boxed `Int2DdsTopic`.
unsafe fn finalize_topic(
    participant_ref: &Int2DdsParticipant,
    topic_name_str: &str,
    dds_type_name: &str,
    type_support: Arc<dyn TypeSupport>,
    qos: *const Int2DdsTopicQos,
    topic_out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    ffi_try!(participant_ref.inner.register_type_support(type_support, dds_type_name));

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
        Box::new(Int2DdsTopic { inner: Arc::new(topic), type_name: dds_type_name.to_string() });
    *topic_out = Box::into_raw(topic_handle);
    INT2DDS_RET_OK
}

/// Map the raw-path FFI `extensibility` code to an `ExtensibilityKind`.
///
/// `-1` selects the library default via the single source of truth
/// (`int2dds_default_extensibility`, currently Appendable); `0`/`1`/`2` map to
/// Final/Appendable/Mutable. Any other value yields `None`, which callers translate
/// to `INT2DDS_RET_INVALID_ARGUMENT`. Resolving `-1` through
/// `int2dds_default_extensibility` keeps the raw path aligned with the Python/C#
/// bindings, which already fall back to that same default when a type omits one.
pub(crate) fn resolve_extensibility(code: i32) -> Option<ExtensibilityKind> {
    let code = if code == -1 { crate::qos::int2dds_default_extensibility() } else { code };
    match code {
        0 => Some(ExtensibilityKind::Final),
        1 => Some(ExtensibilityKind::Appendable),
        2 => Some(ExtensibilityKind::Mutable),
        _ => None,
    }
}

/// Map a `create_topic_with_field_descriptors` field-type code (scalar-only, and a
/// distinct encoding from the `INT2DDS_FIELD_*` constants) to its CDR descriptor type
/// and XTypes `TypeIdentifier`. Returns `None` for unsupported codes.
fn field_descriptor_type(
    code: u32,
) -> Option<(crate::data::CdrFieldType, int2dds::xtypes::TypeIdentifier)> {
    use crate::data::CdrFieldType;
    use int2dds::xtypes::TypeIdentifier;
    Some(match code {
        0 => (CdrFieldType::String, TypeIdentifier::String8),
        1 => (CdrFieldType::Int32, TypeIdentifier::Int32),
        2 => (CdrFieldType::UInt32, TypeIdentifier::Uint32),
        3 => (CdrFieldType::Int16, TypeIdentifier::Int16),
        4 => (CdrFieldType::UInt16, TypeIdentifier::Uint16),
        5 => (CdrFieldType::Int64, TypeIdentifier::Int64),
        6 => (CdrFieldType::UInt64, TypeIdentifier::Uint64),
        7 => (CdrFieldType::Int8, TypeIdentifier::Int8),
        8 => (CdrFieldType::UInt8, TypeIdentifier::Uint8),
        9 => (CdrFieldType::Bool, TypeIdentifier::Boolean),
        _ => return None,
    })
}

/// Create a Topic
///
/// Creates a topic with RawTypeSupport for use with `int2dds_datawriter_write_serialized()`
/// and `int2dds_datareader_take_serialized()`. C users handle CDR serialization themselves
/// using IDL-generated code. Keyed topics require a full TypeObject; use
/// `int2dds_create_topic_with_type_info` or `int2dds_create_topic_with_field_descriptors`.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
/// - `extensibility`: -1 = library default (Appendable per spec; frames a DHEADER, so
///   pass an explicit value or use the type-info path for a Final remote),
///   0 = Final, 1 = Appendable, 2 = Mutable
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

    let ext_kind = match resolve_extensibility(extensibility) {
        Some(k) => k,
        None => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let type_support =
        Arc::new(RawTypeSupport::new_with_key(dds_type_name_str.to_string(), ext_kind, false));

    finalize_topic(
        participant_ref,
        topic_name_str,
        dds_type_name_str,
        type_support as Arc<dyn TypeSupport>,
        qos,
        topic_out,
    )
}

/// Create a Topic using a QoS profile path
///
/// Same as `int2dds_create_topic` but uses a QoS profile path instead of a QoS handle.
///
/// # Safety
/// - `participant` must be a valid participant
/// - `topic_name` must be a valid null-terminated C string
/// - `dds_type_name` must be a valid null-terminated C string (DDS registration name)
/// - `extensibility`: -1 = library default (Appendable per spec; frames a DHEADER, so
///   pass an explicit value or use the type-info path for a Final remote),
///   0 = Final, 1 = Appendable, 2 = Mutable
/// - `qos_path` must be a valid null-terminated UTF-8 string (e.g. "Library::Profile")
/// - `topic_out` must be a valid pointer to a null pointer
/// - The returned topic must be freed with `int2dds_delete_topic`
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_with_profile(
    participant: *const Int2DdsParticipant,
    topic_name: *const std::os::raw::c_char,
    dds_type_name: *const std::os::raw::c_char,
    extensibility: i32,
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

    let ext_kind = match resolve_extensibility(extensibility) {
        Some(k) => k,
        None => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let type_support =
        Arc::new(RawTypeSupport::new_with_key(dds_type_name_str.to_string(), ext_kind, false));

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
/// TypeIdentifier and TypeObject for DDS discovery parameters (0x0075/0x0069, 0x0072).
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

    // Create RawTypeSupport with type info for discovery. Keyed types compute instance
    // keys through the canonical DynamicData path built from the full TypeObject.
    let mut raw_type_support = RawTypeSupport::with_type_info_and_deps(
        dds_type_name.clone(),
        ti.extensibility,
        ti.has_key_field(),
        type_identifier,
        type_object,
        ti.dependency_closure(),
    );

    // Flat CDR field descriptors so ContentFilteredTopic / QueryCondition filters work on
    // generated (type_info) topics. None when any member is non-flat (nested/collection/
    // enum/float/wide-string) — filtering then stays unavailable, as before.
    if let Some(descriptors) = ti.cdr_field_descriptors() {
        raw_type_support.set_all_fields(descriptors);
    }

    finalize_topic(
        participant_ref,
        topic_name_str,
        dds_type_name,
        Arc::new(raw_type_support) as Arc<dyn TypeSupport>,
        qos,
        topic_out,
    )
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

    let topic_box = Box::from_raw(topic);
    if Arc::strong_count(&topic_box.inner) != 1 {
        let _ = Box::into_raw(topic_box);
        return INT2DDS_RET_PRECONDITION_NOT_MET;
    }

    let topic_obj = (*topic_box.inner).clone();

    // Already deleted through another handle to the same topic (one from
    // `int2dds_participant_find_topic`): a retry can never succeed, so release
    // this handle rather than restoring it.
    let participant = match topic_obj.get_participant() {
        Ok(p) => p,
        Err(DdsError::AlreadyDeleted) => return INT2DDS_RET_OK,
        Err(e) => {
            let _ = Box::into_raw(topic_box);
            return dds_error_to_code(&e);
        }
    };

    // On failure the topic is not deleted; restore the caller's handle
    // (into_raw) instead of leaving it freed. The Box drops (frees) only on success.
    match participant.delete_topic(topic_obj) {
        Ok(()) | Err(DdsError::AlreadyDeleted) => INT2DDS_RET_OK,
        Err(e) => {
            let _ = Box::into_raw(topic_box);
            dds_error_to_code(&e)
        }
    }
}

/// Get the inconsistent topic status for a Topic
///
/// Reports how many times a remote topic with the same name but an
/// incompatible type was discovered. Reading the status resets its
/// `total_count_change` and clears the INCONSISTENT_TOPIC status flag.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `status_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_get_inconsistent_topic_status(
    topic: *const Int2DdsTopic,
    status_out: *mut Int2DdsInconsistentTopicStatus,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(status_out);

    let topic_ref = &*topic;

    match topic_ref.inner.get_inconsistent_topic_status() {
        Ok(status) => {
            *status_out = Int2DdsInconsistentTopicStatus::from(&status);
            INT2DDS_RET_OK
        }
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
        Err(_) => ffi_bail!("topic name contains interior NUL byte"),
    };

    let name_bytes = name_cstr.as_bytes_with_nul();
    if name_bytes.len() > name_size {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
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
        Err(_) => ffi_bail!("topic type name contains interior NUL byte"),
    };

    let type_name_bytes = type_name_cstr.as_bytes_with_nul();
    if type_name_bytes.len() > type_name_size {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
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
        &related_topic_ref.inner,
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

    let cft_box = Box::from_raw(cft);

    let participant = match cft_box.inner.get_participant() {
        Ok(p) => p,
        Err(e) => {
            let _ = Box::into_raw(cft_box);
            return dds_error_to_code(&e);
        }
    };

    // On failure the CFT is not deleted (the core orphans it for retry); restore the
    // caller's handle instead of leaving it freed. The Box drops (frees) only on success.
    match participant.delete_contentfilteredtopic(cft_box.inner.clone()) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => {
            let _ = Box::into_raw(cft_box);
            dds_error_to_code(&e)
        }
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

/// # Safety
/// - `cft` must be a valid content filtered topic
/// - `filter_expression` must be a valid null-terminated C string
/// - `expression_parameters` must be a valid array of `expression_parameters_count`
///   null-terminated C strings, or null if the count is 0
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

/// # Safety
/// - `cft` must be a valid content filtered topic
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

/// Create a Topic with full field descriptors for reader-side CFT filtering.
///
/// Extends int2dds_create_topic by also providing all field metadata (name, type)
/// needed for get_field_value() support. This enables ContentFilteredTopic
/// reader-side filtering in the serialized path.
///
/// # Safety
/// - Same as int2dds_create_topic
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

    let ext_kind = match resolve_extensibility(extensibility) {
        Some(k) => k,
        None => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    // No field structure provided: keep the prior name-only behavior (advertise no type
    // info) rather than a bogus empty-struct TypeObject that could turn a name match into
    // a structural mismatch against a real multi-field peer.
    if field_count == 0 {
        let type_support =
            RawTypeSupport::new_with_key(dds_type_name_str.to_string(), ext_kind, false);
        return finalize_topic(
            participant_ref,
            topic_name_str,
            dds_type_name_str,
            Arc::new(type_support) as Arc<dyn TypeSupport>,
            qos,
            topic_out,
        );
    }

    check_null!(field_names);
    check_null!(field_types);
    check_null!(field_is_key);

    // Build XTypes type info (for discovery) and CDR field descriptors (for
    // ContentFilteredTopic get_field_value) from the same flat fields.
    use crate::data::CdrFieldDescriptor;

    let mut all_fields = Vec::new();
    let mut ti = Int2DdsTypeInfo::new(dds_type_name_str.to_string(), ext_kind);

    for i in 0..field_count {
        let name_ptr = *field_names.add(i);
        if name_ptr.is_null() {
            return INT2DDS_RET_INVALID_ARGUMENT;
        }
        let name = match CStr::from_ptr(name_ptr).to_str() {
            Ok(s) => s.to_string(),
            Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
        };

        let is_key = *field_is_key.add(i);
        let (cdr_type, xtypes_id) = match field_descriptor_type(*field_types.add(i)) {
            Some(t) => t,
            None => return INT2DDS_RET_INVALID_ARGUMENT,
        };

        all_fields.push(CdrFieldDescriptor { name: name.clone(), field_type: cdr_type, is_key });

        let flags = if is_key { crate::type_info::INT2DDS_MEMBER_KEY } else { 0 };
        ti.push_field(name, xtypes_id, flags);
    }

    // Advertise TypeIdentifier/TypeObject (0x0075) like the derive macro, while keeping
    // CDR field descriptors for ContentFilteredTopic. Instance keys are computed from the
    // full TypeObject via the canonical DynamicData path.
    let has_key = all_fields.iter().any(|f| f.is_key);
    let mut type_support = RawTypeSupport::with_type_info(
        dds_type_name_str.to_string(),
        ext_kind,
        has_key,
        ti.build_type_identifier(),
        ti.build_type_object(),
    );
    type_support.set_all_fields(all_fields);

    finalize_topic(
        participant_ref,
        topic_name_str,
        dds_type_name_str,
        Arc::new(type_support) as Arc<dyn TypeSupport>,
        qos,
        topic_out,
    )
}

#[cfg(test)]
mod tests {
    use super::field_descriptor_type;
    use int2dds::xtypes::TypeIdentifier;

    #[test]
    fn field_descriptor_type_maps_scalar_codes() {
        let expected = [
            (0u32, TypeIdentifier::String8),
            (1, TypeIdentifier::Int32),
            (2, TypeIdentifier::Uint32),
            (3, TypeIdentifier::Int16),
            (4, TypeIdentifier::Uint16),
            (5, TypeIdentifier::Int64),
            (6, TypeIdentifier::Uint64),
            (7, TypeIdentifier::Int8),
            (8, TypeIdentifier::Uint8),
            (9, TypeIdentifier::Boolean),
        ];
        for (code, tid) in expected {
            let (_, got) = field_descriptor_type(code).expect("scalar code must map");
            assert_eq!(got, tid, "field_descriptor code {code} maps to wrong TypeIdentifier");
        }
        assert!(field_descriptor_type(10).is_none(), "code 10 must be rejected");
    }
}
