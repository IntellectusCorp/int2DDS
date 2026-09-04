//! # ReadCondition / QueryCondition
//!
//! Functions for creating and using ReadConditions and QueryConditions on a
//! DataReader.
//!
//! ## Overview
//!
//! A `ReadCondition` triggers when the DataReader has samples matching a set of
//! sample/view/instance state masks. A `QueryCondition` extends it with a
//! SQL-92 content filter evaluated against sample content.
//!
//! Both are `Condition`s: attach them to a WaitSet with
//! `int2dds_waitset_attach_readcondition` to wait for matching data, and read
//! the matching samples with `int2dds_datareader_take_serialized_batch_w_readcondition` /
//! `int2dds_datareader_read_serialized_batch_w_readcondition`.
//!
//! ## Content filtering scope
//!
//! `QueryCondition` content filtering evaluates the SQL expression against the
//! sample's CDR bytes. Like `ContentFilteredTopic`, this requires the topic to
//! have been created with field descriptors
//! (`int2dds_create_topic_with_field_descriptors`); on a plain raw topic the
//! expression cannot be evaluated and the read returns an error. `ReadCondition`
//! state filtering has no such requirement and always applies.

use std::os::raw::c_char;

use int2dds::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};

use super::{error::*, types::*};

/// Build the single-element state-mask slices used by the whole FFI, matching
/// the convention in `int2dds_datareader_take_serialized_w_states`.
#[inline]
fn state_masks(
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
) -> ([SampleStateKind; 1], [ViewStateKind; 1], [InstanceStateKind; 1]) {
    (
        [SampleStateKind::from_bits_truncate(sample_state_mask)],
        [ViewStateKind::from_bits_truncate(view_state_mask)],
        [InstanceStateKind::from_bits_truncate(instance_state_mask)],
    )
}

/// Create a ReadCondition on a DataReader.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_readcondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_create_readcondition(
    reader: *const Int2DdsDataReader,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    condition_out: *mut *mut Int2DdsReadCondition,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(condition_out);

    let reader_ref = &*reader;
    let (ss, vs, is) = state_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let read_condition = match reader_ref.inner.create_readcondition(&ss, &vs, &is) {
        Ok(c) => c,
        Err(e) => return dds_error_to_code(&e),
    };

    let inner = read_condition.clone().into();
    let handle =
        Box::new(Int2DdsReadCondition { inner, kind: ReadConditionKind::Read(read_condition) });
    *condition_out = Box::into_raw(handle);

    INT2DDS_RET_OK
}

/// Create a QueryCondition on a DataReader.
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `query_expression` must be a valid null-terminated C string
/// - `query_parameters` must be a valid array of `query_parameters_count`
///   null-terminated C strings, or null if the count is 0
/// - `condition_out` must be a valid pointer to a null pointer
/// - The returned condition must be freed with `int2dds_readcondition_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_create_querycondition(
    reader: *const Int2DdsDataReader,
    sample_state_mask: u32,
    view_state_mask: u32,
    instance_state_mask: u32,
    query_expression: *const c_char,
    query_parameters: *const *const c_char,
    query_parameters_count: usize,
    condition_out: *mut *mut Int2DdsReadCondition,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(query_expression);
    check_null!(condition_out);

    let reader_ref = &*reader;

    let expr = match std::ffi::CStr::from_ptr(query_expression).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let params = match collect_c_strings(query_parameters, query_parameters_count) {
        Ok(p) => p,
        Err(ret) => return ret,
    };

    let (ss, vs, is) = state_masks(sample_state_mask, view_state_mask, instance_state_mask);

    let query_condition = match reader_ref.inner.create_querycondition(&ss, &vs, &is, expr, params)
    {
        Ok(c) => c,
        Err(e) => return dds_error_to_code(&e),
    };

    let inner = query_condition.clone().into();
    let handle =
        Box::new(Int2DdsReadCondition { inner, kind: ReadConditionKind::Query(query_condition) });
    *condition_out = Box::into_raw(handle);

    INT2DDS_RET_OK
}

/// Collect a C array of null-terminated strings into a `Vec<String>`.
unsafe fn collect_c_strings(
    ptr: *const *const c_char,
    count: usize,
) -> Result<Vec<String>, Int2DdsRet> {
    let mut out = Vec::with_capacity(count);
    if count > 0 {
        if ptr.is_null() {
            return Err(INT2DDS_RET_INVALID_ARGUMENT);
        }
        for i in 0..count {
            let p = *ptr.add(i);
            if p.is_null() {
                return Err(INT2DDS_RET_INVALID_ARGUMENT);
            }
            match std::ffi::CStr::from_ptr(p).to_str() {
                Ok(s) => out.push(s.to_string()),
                Err(_) => return Err(INT2DDS_RET_INVALID_ARGUMENT),
            }
        }
    }
    Ok(out)
}

/// Get the trigger value of a Read/QueryCondition.
///
/// # Safety
/// - `condition` must be a valid read condition
/// - `value_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_readcondition_get_trigger_value(
    condition: *const Int2DdsReadCondition,
    value_out: *mut bool,
) -> Int2DdsRet {
    check_null!(condition);
    check_null!(value_out);

    let condition_ref = &*condition;
    match condition_ref.inner.get_trigger_value() {
        Ok(value) => {
            *value_out = value;
            INT2DDS_RET_OK
        }
        Err(e) => dds_error_to_code(&e),
    }
}

/// Replace the parameters of a QueryCondition's SQL expression.
///
/// Returns `INT2DDS_RET_INVALID_ARGUMENT` if the condition is a ReadCondition
/// (no query expression) or the parameter count does not match the expression.
///
/// # Safety
/// - `condition` must be a valid read condition
/// - `query_parameters` must be a valid array of `query_parameters_count`
///   null-terminated C strings, or null if the count is 0
#[no_mangle]
pub unsafe extern "C" fn int2dds_querycondition_set_query_parameters(
    condition: *const Int2DdsReadCondition,
    query_parameters: *const *const c_char,
    query_parameters_count: usize,
) -> Int2DdsRet {
    check_null!(condition);

    let condition_ref = &*condition;
    let params = match collect_c_strings(query_parameters, query_parameters_count) {
        Ok(p) => p,
        Err(ret) => return ret,
    };

    match &condition_ref.kind {
        ReadConditionKind::Query(qc) => match qc.set_query_parameters(params) {
            Ok(()) => INT2DDS_RET_OK,
            Err(e) => dds_error_to_code(&e),
        },
        ReadConditionKind::Read(_) => INT2DDS_RET_INVALID_ARGUMENT,
    }
}

/// Delete a Read/QueryCondition.
///
/// # Safety
/// - `condition` must be a valid read condition
/// - `condition` must not be used after this call
/// - The condition should be detached from any WaitSets first
#[no_mangle]
pub unsafe extern "C" fn int2dds_readcondition_delete(
    condition: *mut Int2DdsReadCondition,
) -> Int2DdsRet {
    if condition.is_null() {
        return INT2DDS_RET_NULL_POINTER;
    }
    // Take ownership; the box frees on drop at the end of every path below (the FFI
    // contract forbids reusing `condition` after this call, so we never re-leak it).
    // Detach from the owning reader so the core drops its retained Arc, and surface a
    // core delete failure to the caller. An expired reader Weak means the reader (and
    // its condition list) is already gone — nothing to detach, which is success.
    let boxed = Box::from_raw(condition);
    match &boxed.kind {
        ReadConditionKind::Read(rc) => match rc.get_datareader::<crate::data::Int2DdsData>() {
            Ok(reader) => match reader.delete_readcondition(rc.clone()) {
                Ok(()) => INT2DDS_RET_OK,
                Err(e) => dds_error_to_code(&e),
            },
            Err(_) => INT2DDS_RET_OK,
        },
        ReadConditionKind::Query(qc) => match qc.get_datareader::<crate::data::Int2DdsData>() {
            Ok(reader) => match reader.delete_readcondition(qc.clone()) {
                Ok(()) => INT2DDS_RET_OK,
                Err(e) => dds_error_to_code(&e),
            },
            Err(_) => INT2DDS_RET_OK,
        },
    }
}

/// Take samples matching a Read/QueryCondition, returned as serialized bytes.
///
/// For a ReadCondition the condition's state masks are applied directly on the
/// serialized cache. For a QueryCondition the SQL content filter is additionally
/// evaluated (requires field descriptors on the topic; see the module note).
///
/// # Safety
/// - `reader` must be a valid datareader
/// - `condition` must be a valid read condition created from `reader`
/// - `seq_out` must be a valid pointer to a null pointer
/// - On `INT2DDS_RET_OK` the returned sequence must be freed with `int2dds_sample_seq_delete`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_take_serialized_batch_w_readcondition(
    reader: *const Int2DdsDataReader,
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
) -> Int2DdsRet {
    read_or_take_w_readcondition(reader, condition, max_samples, seq_out, true)
}

/// Read samples matching a Read/QueryCondition (samples remain in the cache).
///
/// # Safety
/// - Same as `int2dds_datareader_take_serialized_batch_w_readcondition`
#[no_mangle]
pub unsafe extern "C" fn int2dds_datareader_read_serialized_batch_w_readcondition(
    reader: *const Int2DdsDataReader,
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
) -> Int2DdsRet {
    read_or_take_w_readcondition(reader, condition, max_samples, seq_out, false)
}

unsafe fn read_or_take_w_readcondition(
    reader: *const Int2DdsDataReader,
    condition: *const Int2DdsReadCondition,
    max_samples: i32,
    seq_out: *mut *mut Int2DdsSampleSeq,
    take: bool,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(condition);
    check_null!(seq_out);

    let reader_ref = &*reader;
    let condition_ref = &*condition;

    let result: Result<Vec<(bytes::Bytes, _)>, _> = match &condition_ref.kind {
        // ReadCondition: pure state filter -> apply directly on the serialized cache.
        ReadConditionKind::Read(rc) => {
            let ss = rc.get_sample_state_mask();
            let vs = rc.get_view_state_mask();
            let is = rc.get_instance_state_mask();
            if take {
                reader_ref.inner.take_serialized(max_samples, ss, vs, is)
            } else {
                reader_ref.inner.read_serialized(max_samples, ss, vs, is)
            }
        }
        // QueryCondition: state + SQL content filter -> typed path, extract the
        // retained CDR bytes from each matching sample.
        ReadConditionKind::Query(qc) => {
            let typed = if take {
                reader_ref.inner.take_w_condition(max_samples, qc.clone())
            } else {
                reader_ref.inner.read_w_condition(max_samples, qc.clone())
            };
            typed.map(|samples| {
                let mut out = Vec::with_capacity(samples.len());
                for s in samples {
                    let info = s.sample_info();
                    let bytes: bytes::Bytes = match s.data() {
                        Ok(d) => match d.cdr_bytes {
                            Some(b) => bytes::Bytes::copy_from_slice(b.as_slice()),
                            None => bytes::Bytes::new(),
                        },
                        // Info-only (invalid-data) sample: no payload.
                        Err(_) => bytes::Bytes::new(),
                    };
                    out.push((bytes, info));
                }
                out
            })
        }
    };

    match result {
        Ok(samples) if samples.is_empty() => {
            *seq_out = std::ptr::null_mut();
            INT2DDS_RET_NO_DATA
        }
        Ok(samples) => {
            *seq_out = Box::into_raw(Box::new(Int2DdsSampleSeq { samples }));
            INT2DDS_RET_OK
        }
        Err(int2dds::dcps::core::error::DdsError::NoData) => {
            *seq_out = std::ptr::null_mut();
            INT2DDS_RET_NO_DATA
        }
        Err(e) => dds_error_to_code(&e),
    }
}
