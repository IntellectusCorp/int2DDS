//! # Dynamic XTypes
//!
//! PHASE 1 of the FFI <-> Rust xtypes parity effort. See:
//!  - `docs/superpowers/specs/2026-04-09-ffi-xtypes-parity-roadmap.md`
//!  - `docs/superpowers/specs/2026-04-09-ffi-dynamic-subscriber-phase1-design.md`
//!
//! This module exposes a C-callable surface that lets a subscriber:
//!  - Discover a publisher's TypeObject at runtime
//!  - Introspect it (extensibility, members, kinds, member_ids, flags)
//!  - Register a topic for it via `RawTypeSupport`
//!  - Decode received CDR sample bytes by field name (primitive + string only,
//!    all extensibilities supported)

use int2dds::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData;
use int2dds::serialize::cdr::XcdrDeserializer;
use int2dds::serialize::DeserializerReader;
use int2dds::xtypes::{
    CompleteStructType, CompleteTypeObject, ExtensibilityKind, TypeIdentifier, TypeObject,
};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Arc;

use crate::error::*;
use crate::type_info::{
    INT2DDS_FIELD_BOOL, INT2DDS_FIELD_BYTE, INT2DDS_FIELD_CHAR16, INT2DDS_FIELD_CHAR8,
    INT2DDS_FIELD_FLOAT32, INT2DDS_FIELD_FLOAT64, INT2DDS_FIELD_INT16, INT2DDS_FIELD_INT32,
    INT2DDS_FIELD_INT64, INT2DDS_FIELD_INT8, INT2DDS_FIELD_STRING, INT2DDS_FIELD_UINT16,
    INT2DDS_FIELD_UINT32, INT2DDS_FIELD_UINT64, INT2DDS_FIELD_UINT8, INT2DDS_FIELD_WSTRING,
    INT2DDS_MEMBER_EXTERNAL, INT2DDS_MEMBER_KEY, INT2DDS_MEMBER_MUST_UNDERSTAND,
    INT2DDS_MEMBER_OPTIONAL,
};
use crate::types::{Int2DdsParticipant, Int2DdsSubscriber};

/// C-visible per-member information returned by `int2dds_type_object_member_info`.
#[repr(C)]
pub struct Int2DdsMemberInfo {
    pub member_id: u32,
    pub kind: i32,
    pub flags: i32,
}

fn type_identifier_to_kind(id: &TypeIdentifier) -> Option<i32> {
    Some(match id {
        TypeIdentifier::Boolean => INT2DDS_FIELD_BOOL,
        TypeIdentifier::Byte => INT2DDS_FIELD_BYTE,
        TypeIdentifier::Char8 => INT2DDS_FIELD_CHAR8,
        TypeIdentifier::Char16 => INT2DDS_FIELD_CHAR16,
        TypeIdentifier::Int8 => INT2DDS_FIELD_INT8,
        TypeIdentifier::Int16 => INT2DDS_FIELD_INT16,
        TypeIdentifier::Int32 => INT2DDS_FIELD_INT32,
        TypeIdentifier::Int64 => INT2DDS_FIELD_INT64,
        TypeIdentifier::Uint8 => INT2DDS_FIELD_UINT8,
        TypeIdentifier::Uint16 => INT2DDS_FIELD_UINT16,
        TypeIdentifier::Uint32 => INT2DDS_FIELD_UINT32,
        TypeIdentifier::Uint64 => INT2DDS_FIELD_UINT64,
        TypeIdentifier::Float32 => INT2DDS_FIELD_FLOAT32,
        TypeIdentifier::Float64 => INT2DDS_FIELD_FLOAT64,
        TypeIdentifier::String8 => INT2DDS_FIELD_STRING,
        TypeIdentifier::String8Small { .. } => INT2DDS_FIELD_STRING,
        TypeIdentifier::String8Large { .. } => INT2DDS_FIELD_STRING,
        TypeIdentifier::String16 => INT2DDS_FIELD_WSTRING,
        TypeIdentifier::String16Small { .. } => INT2DDS_FIELD_WSTRING,
        TypeIdentifier::String16Large { .. } => INT2DDS_FIELD_WSTRING,
        _ => return None,
    })
}

/// Opaque handle wrapping a discovered TypeObject.
pub struct Int2DdsTypeObject {
    pub(crate) inner: TypeObject,
}

impl Int2DdsTypeObject {
    /// Construct an Int2DdsTypeObject from a raw `TypeObject`.
    #[doc(hidden)]
    pub fn from_type_object(to: TypeObject) -> Self {
        Int2DdsTypeObject { inner: to }
    }

    /// Return a reference to the inner CompleteStructType, or None if the
    /// TypeObject is not a struct (PHASE 1 only handles struct types).
    pub(crate) fn as_struct(&self) -> Option<&CompleteStructType> {
        match &self.inner {
            TypeObject::Complete(CompleteTypeObject::Struct(s)) => Some(s),
            _ => None,
        }
    }
}

/// Opaque handle wrapping a discovered PublicationBuiltinTopicData sample.
/// Owned by the caller; destroy via `int2dds_publication_data_destroy`.
pub struct Int2DdsPublicationBuiltinData {
    pub(crate) inner: PublicationBuiltinTopicData,
}

unsafe impl Send for Int2DdsPublicationBuiltinData {}
unsafe impl Sync for Int2DdsPublicationBuiltinData {}

/// Destroy a publication builtin data handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_data_destroy(p: *mut Int2DdsPublicationBuiltinData) {
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

/// Helper: copy a Rust &str into a caller-supplied C buffer.
/// Writes required length (without the NUL) into *out_len and returns
/// DYNAMIC_DECODE_ERROR if the supplied buffer is too small.
unsafe fn copy_str_to_c(
    s: &str,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(buf);
    check_null!(out_len);
    let bytes = s.as_bytes();
    *out_len = bytes.len();
    if buf_len < bytes.len() + 1 {
        return INT2DDS_RET_DYNAMIC_DECODE_ERROR;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, buf, bytes.len());
    *buf.add(bytes.len()) = 0;
    INT2DDS_RET_OK
}

/// Copy the topic name of a discovered publication into `buf`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_data_topic_name(
    p: *const Int2DdsPublicationBuiltinData,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(p);
    let name = (*p).inner.topic_name();
    copy_str_to_c(&name, buf, buf_len, out_len)
}

/// Copy the type name of a discovered publication into `buf`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_data_type_name(
    p: *const Int2DdsPublicationBuiltinData,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(p);
    let name = (*p).inner.type_name();
    copy_str_to_c(&name, buf, buf_len, out_len)
}

/// Take a clone of the TypeObject embedded in a publication discovery sample.
/// Caller owns the returned handle and must destroy it via `int2dds_type_object_destroy`.
/// Returns DYNAMIC_FIELD_NOT_FOUND if the publication did not carry a TypeObject.
#[no_mangle]
pub unsafe extern "C" fn int2dds_publication_data_take_type_object(
    p: *const Int2DdsPublicationBuiltinData,
    out: *mut *mut Int2DdsTypeObject,
) -> Int2DdsRet {
    check_null!(p);
    check_null!(out);
    let to = match (*p).inner.type_object() {
        Some(t) => t.clone(),
        None => return INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    };
    let h = Box::new(Int2DdsTypeObject { inner: to });
    *out = Box::into_raw(h);
    INT2DDS_RET_OK
}

/// Get the builtin subscriber for discovery topics.
#[no_mangle]
pub unsafe extern "C" fn int2dds_get_builtin_subscriber(
    participant: *const Int2DdsParticipant,
    out: *mut *mut Int2DdsSubscriber,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(out);
    let p = &*participant;
    let sub = ffi_try!(p.inner.get_builtin_subscriber());
    let handle = Box::new(Int2DdsSubscriber { inner: Arc::new(sub) });
    *out = Box::into_raw(handle);
    INT2DDS_RET_OK
}

/// Take one DCPSPublication discovery sample, optionally filtered by topic
/// name. Blocks up to `timeout_ms` milliseconds (negative = infinite). Returns
/// DYNAMIC_TIMEOUT on no match.
#[no_mangle]
pub unsafe extern "C" fn int2dds_take_publication_data(
    builtin_sub: *const Int2DdsSubscriber,
    topic_name_filter: *const c_char,
    timeout_ms: i32,
    out: *mut *mut Int2DdsPublicationBuiltinData,
) -> Int2DdsRet {
    use int2dds::core::time::Duration;
    use int2dds::infrastructure::status::StatusMask;
    use int2dds::infrastructure::wait_set::WaitSet;
    use int2dds::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};

    check_null!(builtin_sub);
    check_null!(out);

    let filter: Option<&str> = if topic_name_filter.is_null() {
        None
    } else {
        match CStr::from_ptr(topic_name_filter).to_str() {
            Ok(s) => Some(s),
            Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
        }
    };

    let sub = &(*builtin_sub).inner;
    let reader = ffi_try!(sub.lookup_datareader::<PublicationBuiltinTopicData>("DCPSPublication"));

    let mut cond = ffi_try!(reader.get_statuscondition()).clone();
    let _ = cond.set_enabled_statuses(StatusMask::DATA_AVAILABLE);
    let waitset = WaitSet::new();
    let _ = waitset.attach_condition(cond);

    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64))
    };

    loop {
        // Try to read FIRST before waiting — handles data that arrived before
        // the WaitSet was attached (DATA_AVAILABLE already consumed).
        let _ = reader.get_status_changes();
        if let Ok(samples) = reader.read(
            100,
            &[SampleStateKind::ANY_SAMPLE_STATE],
            &[ViewStateKind::ANY_VIEW_STATE],
            &[InstanceStateKind::ANY_INSTANCE_STATE],
        ) {
            for sample in samples.iter() {
                if let Ok(data) = sample.data() {
                    if let Some(want) = filter {
                        if data.topic_name() != want {
                            continue;
                        }
                    }
                    let h = Box::new(Int2DdsPublicationBuiltinData { inner: data });
                    *out = Box::into_raw(h);
                    return INT2DDS_RET_OK;
                }
            }
        }

        // No matching data yet — wait for new arrivals
        let wait_dur = match deadline {
            None => Duration { sec: 1, nanosec: 0 },
            Some(d) => {
                let now = std::time::Instant::now();
                if now >= d {
                    return INT2DDS_RET_DYNAMIC_TIMEOUT;
                }
                let remaining_ms = (d - now).as_millis().min(1000) as i32;
                Duration {
                    sec: remaining_ms / 1000,
                    nanosec: ((remaining_ms % 1000) as u32) * 1_000_000,
                }
            }
        };
        let _ = waitset.wait(wait_dur);
    }
}

/// High-level helper: wait until a publication for `topic_name` is discovered
/// AND its TypeObject is present, then return the TypeObject and type name.
#[no_mangle]
pub unsafe extern "C" fn int2dds_wait_for_type_object(
    participant: *const Int2DdsParticipant,
    topic_name: *const c_char,
    timeout_ms: i32,
    type_obj_out: *mut *mut Int2DdsTypeObject,
    type_name_buf: *mut c_char,
    type_name_buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(type_obj_out);
    check_null!(type_name_buf);
    check_null!(out_len);

    let mut builtin: *mut Int2DdsSubscriber = std::ptr::null_mut();
    let r = int2dds_get_builtin_subscriber(participant, &mut builtin);
    if r != INT2DDS_RET_OK {
        return r;
    }

    let deadline = if timeout_ms < 0 {
        None
    } else {
        Some(std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms as u64))
    };

    let result = loop {
        let remaining_ms: i32 = match deadline {
            None => -1,
            Some(d) => {
                let now = std::time::Instant::now();
                if now >= d {
                    break INT2DDS_RET_DYNAMIC_TIMEOUT;
                }
                (d - now).as_millis() as i32
            }
        };

        let mut pdata: *mut Int2DdsPublicationBuiltinData = std::ptr::null_mut();
        let r = int2dds_take_publication_data(builtin, topic_name, remaining_ms, &mut pdata);
        if r == INT2DDS_RET_DYNAMIC_TIMEOUT {
            break INT2DDS_RET_DYNAMIC_TIMEOUT;
        }
        if r != INT2DDS_RET_OK {
            break r;
        }

        let mut to: *mut Int2DdsTypeObject = std::ptr::null_mut();
        let rt = int2dds_publication_data_take_type_object(pdata, &mut to);
        if rt == INT2DDS_RET_OK {
            let r2 = int2dds_publication_data_type_name(
                pdata,
                type_name_buf,
                type_name_buf_len,
                out_len,
            );
            int2dds_publication_data_destroy(pdata);
            if r2 != INT2DDS_RET_OK {
                int2dds_type_object_destroy(to);
                break r2;
            }
            *type_obj_out = to;
            break INT2DDS_RET_OK;
        }
        int2dds_publication_data_destroy(pdata);
        // No type_object on this publication — keep waiting.
    };

    // Drop the builtin subscriber handle wrapper (decrements Arc; underlying
    // subscriber remains owned by the participant).
    drop(Box::from_raw(builtin));
    result
}

/// Destroy a TypeObject handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_object_destroy(t: *mut Int2DdsTypeObject) {
    if !t.is_null() {
        drop(Box::from_raw(t));
    }
}

/// Return extensibility: 0 = Final, 1 = Appendable, 2 = Mutable.
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_object_extensibility(
    t: *const Int2DdsTypeObject,
    out: *mut i32,
) -> Int2DdsRet {
    check_null!(t);
    check_null!(out);
    let s = match (*t).as_struct() {
        Some(s) => s,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };
    let kind = s.struct_flags.extensibility();
    *out = match kind {
        ExtensibilityKind::Final => 0,
        ExtensibilityKind::Appendable => 1,
        ExtensibilityKind::Mutable => 2,
    };
    INT2DDS_RET_OK
}

/// Return the number of struct members.
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_object_member_count(
    t: *const Int2DdsTypeObject,
    out: *mut u32,
) -> Int2DdsRet {
    check_null!(t);
    check_null!(out);
    let s = match (*t).as_struct() {
        Some(s) => s,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };
    *out = s.member_seq.len() as u32;
    INT2DDS_RET_OK
}

/// Fill `out` with member info at `index`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_object_member_info(
    t: *const Int2DdsTypeObject,
    index: u32,
    out: *mut Int2DdsMemberInfo,
) -> Int2DdsRet {
    check_null!(t);
    check_null!(out);
    let s = match (*t).as_struct() {
        Some(s) => s,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };
    let m = match s.member_seq.get(index as usize) {
        Some(m) => m,
        None => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let kind = match type_identifier_to_kind(&m.common.member_type_id) {
        Some(k) => k,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };
    let mf = &m.common.member_flags;
    let mut flags: i32 = 0;
    if mf.is_key() {
        flags |= INT2DDS_MEMBER_KEY;
    }
    if mf.is_optional() {
        flags |= INT2DDS_MEMBER_OPTIONAL;
    }
    if mf.is_must_understand() {
        flags |= INT2DDS_MEMBER_MUST_UNDERSTAND;
    }
    if mf.is_external() {
        flags |= INT2DDS_MEMBER_EXTERNAL;
    }
    *out = Int2DdsMemberInfo { member_id: m.common.member_id, kind, flags };
    INT2DDS_RET_OK
}

/// Copy member name at `index` into `buf`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_object_member_name(
    t: *const Int2DdsTypeObject,
    index: u32,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(t);
    check_null!(buf);
    check_null!(out_len);
    let s = match (*t).as_struct() {
        Some(s) => s,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };
    let m = match s.member_seq.get(index as usize) {
        Some(m) => m,
        None => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let name = m.detail.name.as_bytes();
    *out_len = name.len();
    if buf_len < name.len() + 1 {
        return INT2DDS_RET_DYNAMIC_DECODE_ERROR;
    }
    std::ptr::copy_nonoverlapping(name.as_ptr() as *const c_char, buf, name.len());
    *buf.add(name.len()) = 0;
    INT2DDS_RET_OK
}

/// Find a member index by name.
#[no_mangle]
pub unsafe extern "C" fn int2dds_type_object_find_member(
    t: *const Int2DdsTypeObject,
    name: *const c_char,
    index_out: *mut u32,
) -> Int2DdsRet {
    check_null!(t);
    check_null!(name);
    check_null!(index_out);
    let s = match (*t).as_struct() {
        Some(s) => s,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };
    let needle = match CStr::from_ptr(name).to_str() {
        Ok(n) => n,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    for (i, m) in s.member_seq.iter().enumerate() {
        if m.detail.name == needle {
            *index_out = i as u32;
            return INT2DDS_RET_OK;
        }
    }
    INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND
}

use int2dds::infrastructure::status::StatusMask;
use int2dds::serialize::cdr::ExtensibilityKind as CdrExtensibilityKind;
use int2dds::topic::{qos::TopicQos, TypeSupport};

use crate::data::Int2DdsData;
use crate::qos::Int2DdsTopicQos;
use crate::raw_type_support::RawTypeSupport;
use crate::types::Int2DdsTopic;

/// Register a topic backed by a discovered TypeObject. The TypeObject is cloned
/// internally; the caller still owns and must destroy `type_obj`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_with_type_object(
    participant: *const Int2DdsParticipant,
    topic_name: *const c_char,
    type_name: *const c_char,
    type_obj: *const Int2DdsTypeObject,
    qos: *const Int2DdsTopicQos,
    out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(type_name);
    check_null!(type_obj);
    check_null!(out);

    let p = &*participant;
    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let type_name_str = match CStr::from_ptr(type_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };

    let s = match (*type_obj).as_struct() {
        Some(s) => s,
        None => return INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE,
    };

    // Map xtypes ExtensibilityKind -> serialize::cdr::ExtensibilityKind expected
    // by RawTypeSupport.
    let extensibility = match s.struct_flags.extensibility() {
        ExtensibilityKind::Final => CdrExtensibilityKind::Final,
        ExtensibilityKind::Appendable => CdrExtensibilityKind::Appendable,
        ExtensibilityKind::Mutable => CdrExtensibilityKind::Mutable,
    };
    let has_key = s.member_seq.iter().any(|m| m.common.member_flags.is_key());

    // Clone the TypeObject and derive a CompleteTypeId TypeIdentifier from it.
    let to_clone = (*type_obj).inner.clone();
    let hash = to_clone.compute_hash();
    let type_identifier = TypeIdentifier::CompleteTypeId(hash.clone());

    let type_support = Arc::new(RawTypeSupport::with_type_info(
        type_name_str.to_string(),
        extensibility,
        has_key,
        type_identifier,
        to_clone,
    ));

    ffi_try!(p.inner.register_type_support(type_support as Arc<dyn TypeSupport>, type_name_str));

    let topic_qos = if qos.is_null() { TopicQos::default() } else { (*qos).inner.clone() };

    let topic = ffi_try!(p.inner.create_topic::<Int2DdsData>(
        topic_name_str,
        type_name_str,
        topic_qos,
        None,
        StatusMask::default(),
    ));

    let topic_handle =
        Box::new(Int2DdsTopic { inner: Arc::new(topic), type_name: type_name_str.to_string() });
    *out = Box::into_raw(topic_handle);
    INT2DDS_RET_OK
}

/// Skip exactly one member of the given primitive/string kind from the reader,
/// honoring CDR alignment. Returns `DYNAMIC_UNSUPPORTED_TYPE` for any kind
/// outside PHASE 1 scope (sequences, arrays, nested structs, etc.).
fn skip_member_by_kind(reader: &mut XcdrDeserializer<'_>, kind: i32) -> Result<(), Int2DdsRet> {
    match kind {
        k if k == INT2DDS_FIELD_BOOL
            || k == INT2DDS_FIELD_BYTE
            || k == INT2DDS_FIELD_CHAR8
            || k == INT2DDS_FIELD_INT8
            || k == INT2DDS_FIELD_UINT8 =>
        {
            // 1-byte primitives: no alignment needed.
            reader.skip(1).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
        }
        k if k == INT2DDS_FIELD_INT16 || k == INT2DDS_FIELD_UINT16 || k == INT2DDS_FIELD_CHAR16 => {
            reader.align(2);
            reader.skip(2).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
        }
        k if k == INT2DDS_FIELD_INT32
            || k == INT2DDS_FIELD_UINT32
            || k == INT2DDS_FIELD_FLOAT32 =>
        {
            reader.align(4);
            reader.skip(4).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
        }
        k if k == INT2DDS_FIELD_INT64
            || k == INT2DDS_FIELD_UINT64
            || k == INT2DDS_FIELD_FLOAT64 =>
        {
            reader.align(8);
            reader.skip(8).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
        }
        k if k == INT2DDS_FIELD_STRING => {
            let len = reader.deserialize_u32().map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
            reader.skip(len as usize).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
        }
        k if k == INT2DDS_FIELD_WSTRING => {
            let len = reader.deserialize_u32().map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
            reader
                .skip((len as usize).saturating_mul(2))
                .map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
        }
        _ => return Err(INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE),
    }
    Ok(())
}

/// Position a freshly initialized CDR reader at the value bytes of the named
/// field. Returns `(positioned_reader, member's CDR kind)` on success.
#[allow(dead_code)]
fn position_reader_at_field<'a>(
    bytes: &'a [u8],
    type_obj: &Int2DdsTypeObject,
    field_name: &str,
) -> Result<(XcdrDeserializer<'a>, i32), Int2DdsRet> {
    let s = type_obj.as_struct().ok_or(INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE)?;

    // Locate the target member by name.
    let (target_idx, target_member) = s
        .member_seq
        .iter()
        .enumerate()
        .find(|(_, m)| m.detail.name == field_name)
        .ok_or(INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND)?;

    let actual_kind = type_identifier_to_kind(&target_member.common.member_type_id)
        .ok_or(INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE)?;

    let mut reader = XcdrDeserializer::new(bytes).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;

    match s.struct_flags.extensibility() {
        ExtensibilityKind::Final => {
            // Positional skip of each preceding member.
            for m in s.member_seq.iter().take(target_idx) {
                let k = type_identifier_to_kind(&m.common.member_type_id)
                    .ok_or(INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE)?;
                skip_member_by_kind(&mut reader, k)?;
            }
        }
        ExtensibilityKind::Appendable => {
            // Read DHEADER, then positional skip bounded by its end offset.
            let object_size =
                reader.read_dheader().map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
            let dh_end = reader.get_position() + object_size as usize;
            for m in s.member_seq.iter().take(target_idx) {
                if reader.get_position() >= dh_end {
                    return Err(INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND);
                }
                let k = type_identifier_to_kind(&m.common.member_type_id)
                    .ok_or(INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE)?;
                skip_member_by_kind(&mut reader, k)?;
            }
            if reader.get_position() >= dh_end {
                return Err(INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND);
            }
        }
        ExtensibilityKind::Mutable => {
            // Walk EMHEADERs looking for the matching member_id.
            let object_size =
                reader.read_dheader().map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
            let dh_end = reader.get_position() + object_size as usize;
            let want_id = target_member.common.member_id;
            loop {
                if reader.get_position() >= dh_end || reader.is_at_sentinel() {
                    return Err(INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND);
                }
                let (id, data_len) =
                    reader.read_member_header().map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
                if id == want_id {
                    return Ok((reader, actual_kind));
                }
                reader.skip(data_len as usize).map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)?;
            }
        }
    }

    Ok((reader, actual_kind))
}

use int2dds::serialize::cdr::XcdrDeserialize;

macro_rules! define_dynamic_primitive_getter {
    ($fn_name:ident, $rust_ty:ty, $expected_kind:expr $(, $also:expr)*) => {
        /// Read a primitive field by name from a serialized CDR sample.
        ///
        /// Returns `INT2DDS_RET_DYNAMIC_TYPE_MISMATCH` if the field is not one of
        /// the accepted CDR kinds for this getter.
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(
            bytes: *const u8,
            len: usize,
            type_obj: *const Int2DdsTypeObject,
            field_name: *const c_char,
            out: *mut $rust_ty,
        ) -> Int2DdsRet {
            check_null!(bytes);
            check_null!(type_obj);
            check_null!(field_name);
            check_null!(out);
            let slice = std::slice::from_raw_parts(bytes, len);
            let name = match CStr::from_ptr(field_name).to_str() {
                Ok(s) => s,
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            };
            let (mut reader, kind) = match position_reader_at_field(slice, &*type_obj, name) {
                Ok(rk) => rk,
                Err(e) => return e,
            };
            let accepted: &[i32] = &[$expected_kind $(, $also)*];
            if !accepted.contains(&kind) {
                return INT2DDS_RET_DYNAMIC_TYPE_MISMATCH;
            }
            match <$rust_ty as XcdrDeserialize>::deserialize_xcdr(&mut reader) {
                Ok(v) => {
                    *out = v;
                    INT2DDS_RET_OK
                }
                Err(_) => INT2DDS_RET_DYNAMIC_DECODE_ERROR,
            }
        }
    };
}

define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_bool, bool, INT2DDS_FIELD_BOOL);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_i8, i8, INT2DDS_FIELD_INT8);
define_dynamic_primitive_getter!(
    int2dds_dynamic_sample_get_u8,
    u8,
    INT2DDS_FIELD_UINT8,
    INT2DDS_FIELD_BYTE
);
define_dynamic_primitive_getter!(
    int2dds_dynamic_sample_get_byte,
    u8,
    INT2DDS_FIELD_BYTE,
    INT2DDS_FIELD_UINT8
);
define_dynamic_primitive_getter!(
    int2dds_dynamic_sample_get_char8,
    u8,
    INT2DDS_FIELD_CHAR8,
    INT2DDS_FIELD_INT8,
    INT2DDS_FIELD_UINT8,
    INT2DDS_FIELD_BYTE
);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_i16, i16, INT2DDS_FIELD_INT16);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_u16, u16, INT2DDS_FIELD_UINT16);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_i32, i32, INT2DDS_FIELD_INT32);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_u32, u32, INT2DDS_FIELD_UINT32);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_i64, i64, INT2DDS_FIELD_INT64);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_u64, u64, INT2DDS_FIELD_UINT64);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_f32, f32, INT2DDS_FIELD_FLOAT32);
define_dynamic_primitive_getter!(int2dds_dynamic_sample_get_f64, f64, INT2DDS_FIELD_FLOAT64);

/// Read a string field by name from a serialized CDR sample.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_sample_get_string(
    bytes: *const u8,
    len: usize,
    type_obj: *const Int2DdsTypeObject,
    field_name: *const c_char,
    out_buf: *mut c_char,
    buf_cap: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(bytes);
    check_null!(type_obj);
    check_null!(field_name);
    check_null!(out_buf);
    check_null!(out_len);
    let slice = std::slice::from_raw_parts(bytes, len);
    let name = match CStr::from_ptr(field_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let (mut reader, kind) = match position_reader_at_field(slice, &*type_obj, name) {
        Ok(rk) => rk,
        Err(e) => return e,
    };
    if kind != INT2DDS_FIELD_STRING {
        return INT2DDS_RET_DYNAMIC_TYPE_MISMATCH;
    }
    let s = match String::deserialize_xcdr(&mut reader) {
        Ok(v) => v,
        Err(_) => return INT2DDS_RET_DYNAMIC_DECODE_ERROR,
    };
    copy_str_to_c(&s, out_buf, buf_cap, out_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_object_extensibility_basic() {
        use int2dds::xtypes::{
            CompleteStructType, CompleteTypeObject, ExtensibilityKind, TypeFlag, TypeObject,
        };

        let flags = TypeFlag::new(ExtensibilityKind::Mutable, false, false);
        let st = CompleteStructType::new(flags, "T".to_string(), None);
        let to = TypeObject::Complete(CompleteTypeObject::Struct(st));
        let handle = Box::into_raw(Box::new(Int2DdsTypeObject { inner: to }));

        let mut ext: i32 = -1;
        let ret = unsafe { int2dds_type_object_extensibility(handle, &mut ext) };
        assert_eq!(ret, INT2DDS_RET_OK);
        assert_eq!(ext, 2);

        unsafe { int2dds_type_object_destroy(handle) };
    }

    #[test]
    fn type_object_member_count_three_members() {
        use int2dds::xtypes::{
            CompleteStructMember, CompleteStructType, CompleteTypeObject, ExtensibilityKind,
            MemberFlag, TryConstructKind, TypeFlag, TypeIdentifier, TypeObject,
        };

        let flags = TypeFlag::new(ExtensibilityKind::Final, false, false);
        let mut st = CompleteStructType::new(flags, "T".to_string(), None);
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            let mf = MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);
            st.add_member(CompleteStructMember::new(
                i as u32,
                mf,
                TypeIdentifier::Int32,
                name.to_string(),
            ));
        }
        let to = TypeObject::Complete(CompleteTypeObject::Struct(st));
        let handle = Box::into_raw(Box::new(Int2DdsTypeObject { inner: to }));

        let mut count: u32 = 0;
        let ret = unsafe { int2dds_type_object_member_count(handle, &mut count) };
        assert_eq!(ret, INT2DDS_RET_OK);
        assert_eq!(count, 3);

        unsafe { int2dds_type_object_destroy(handle) };
    }

    #[test]
    fn introspection_round_trip_basic_struct() {
        use int2dds::xtypes::{
            CompleteStructMember, CompleteStructType, CompleteTypeObject, ExtensibilityKind,
            MemberFlag, TryConstructKind, TypeFlag, TypeIdentifier, TypeObject,
        };

        let flags = TypeFlag::new(ExtensibilityKind::Appendable, false, false);
        let mut st = CompleteStructType::new(flags, "S".to_string(), None);
        let fields: &[(u32, &str, TypeIdentifier, bool)] = &[
            (0, "sensor_id", TypeIdentifier::Int32, true),
            (1, "temperature", TypeIdentifier::Float64, false),
            (2, "humidity", TypeIdentifier::Float64, false),
            (3, "location", TypeIdentifier::String8, false),
        ];
        for (id, name, ty, is_key) in fields {
            let mf =
                MemberFlag::new(TryConstructKind::Discard, false, false, false, *is_key, false);
            st.add_member(CompleteStructMember::new(*id, mf, ty.clone(), name.to_string()));
        }
        let to = TypeObject::Complete(CompleteTypeObject::Struct(st));
        let h = Box::into_raw(Box::new(Int2DdsTypeObject { inner: to }));

        let mut count = 0u32;
        assert_eq!(unsafe { int2dds_type_object_member_count(h, &mut count) }, INT2DDS_RET_OK);
        assert_eq!(count, 4);

        for (i, (id, name, _ty, is_key)) in fields.iter().enumerate() {
            let mut info = Int2DdsMemberInfo { member_id: 0, kind: -1, flags: 0 };
            assert_eq!(
                unsafe { int2dds_type_object_member_info(h, i as u32, &mut info) },
                INT2DDS_RET_OK
            );
            assert_eq!(info.member_id, *id);
            assert_eq!((info.flags & INT2DDS_MEMBER_KEY) != 0, *is_key);

            let mut buf = [0i8; 64];
            let mut nlen = 0usize;
            assert_eq!(
                unsafe {
                    int2dds_type_object_member_name(
                        h,
                        i as u32,
                        buf.as_mut_ptr(),
                        buf.len(),
                        &mut nlen,
                    )
                },
                INT2DDS_RET_OK
            );
            let actual = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap();
            assert_eq!(actual, *name);
            assert_eq!(nlen, name.len());
        }

        let cname = std::ffi::CString::new("humidity").unwrap();
        let mut idx = 0u32;
        assert_eq!(
            unsafe { int2dds_type_object_find_member(h, cname.as_ptr(), &mut idx) },
            INT2DDS_RET_OK
        );
        assert_eq!(idx, 2);

        let missing = std::ffi::CString::new("nope").unwrap();
        assert_eq!(
            unsafe { int2dds_type_object_find_member(h, missing.as_ptr(), &mut idx) },
            INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND
        );

        unsafe { int2dds_type_object_destroy(h) };
    }

    // ========================================================================
    // Task 9 tests — position_reader_at_field + skip_member_by_kind
    //
    // Strategy: use `#[derive(DdsType)]` to produce both (a) the CDR bytes via
    // the generated XcdrSerialize impl and (b) the CompleteTypeObject via the
    // generated HasTypeObject impl. We then feed those bytes + type object into
    // our helper and verify it lands on the expected value.
    // ========================================================================

    use int2dds::serialize::cdr::{
        ExtensibilityKind as CdrExtKind, XcdrDeserialize, XcdrSerialize, XcdrSerializer,
    };
    use int2dds::serialize::BufferManager;
    use int2dds::xtypes::{CompleteTypeObject as XtCompleteTypeObject, HasTypeObject, TypeObject};
    use int2dds_derive::DdsType;

    fn serialize_with_header<T: XcdrSerialize>(value: &T, ext: CdrExtKind) -> Vec<u8> {
        let mut ser = XcdrSerializer::new(true, ext);
        ser.write_encapsulation_header().unwrap();
        value.serialize_xcdr(&mut ser).unwrap();
        ser.into_bytes()
    }

    fn type_object_of<T: HasTypeObject>() -> *mut Int2DdsTypeObject {
        let cto: XtCompleteTypeObject = T::complete_type_object();
        let to = TypeObject::Complete(cto);
        Box::into_raw(Box::new(Int2DdsTypeObject { inner: to }))
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct Phase1Final {
        pub a: i32,
        pub b: f64,
        pub c: String,
        pub d: u16,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
    struct Phase1Appendable {
        pub a: i32,
        pub b: String,
        pub c: u8,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Mutable")]
    struct Phase1Mutable {
        pub a: i32,
        pub b: String,
        pub c: f64,
    }

    #[test]
    fn position_reader_final_struct() {
        let v = Phase1Final { a: 0x1122_3344, b: 3.5, c: "hello".to_string(), d: 0xBEEF };
        let bytes = serialize_with_header(&v, CdrExtKind::Final);
        let handle = type_object_of::<Phase1Final>();
        let to_ref = unsafe { &*handle };

        // Field "a": first member -> positioned immediately.
        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "a").unwrap();
        assert_eq!(k, INT2DDS_FIELD_INT32);
        assert_eq!(i32::deserialize_xcdr(&mut r).unwrap(), 0x1122_3344);

        // Field "b": after skipping i32.
        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "b").unwrap();
        assert_eq!(k, INT2DDS_FIELD_FLOAT64);
        assert_eq!(f64::deserialize_xcdr(&mut r).unwrap(), 3.5);

        // Field "c": after skipping i32 + f64.
        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "c").unwrap();
        assert_eq!(k, INT2DDS_FIELD_STRING);
        assert_eq!(String::deserialize_xcdr(&mut r).unwrap(), "hello");

        // Field "d": after skipping i32 + f64 + string.
        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "d").unwrap();
        assert_eq!(k, INT2DDS_FIELD_UINT16);
        assert_eq!(u16::deserialize_xcdr(&mut r).unwrap(), 0xBEEF);

        // Missing field.
        assert_eq!(
            position_reader_at_field(&bytes, to_ref, "zzz").err().unwrap(),
            INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND
        );

        unsafe { int2dds_type_object_destroy(handle) };
    }

    #[test]
    fn position_reader_appendable_struct() {
        let v = Phase1Appendable { a: -42, b: "world".to_string(), c: 7 };
        let bytes = serialize_with_header(&v, CdrExtKind::Appendable);
        let handle = type_object_of::<Phase1Appendable>();
        let to_ref = unsafe { &*handle };

        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "a").unwrap();
        assert_eq!(k, INT2DDS_FIELD_INT32);
        assert_eq!(i32::deserialize_xcdr(&mut r).unwrap(), -42);

        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "b").unwrap();
        assert_eq!(k, INT2DDS_FIELD_STRING);
        assert_eq!(String::deserialize_xcdr(&mut r).unwrap(), "world");

        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "c").unwrap();
        assert_eq!(k, INT2DDS_FIELD_BYTE);
        assert_eq!(u8::deserialize_xcdr(&mut r).unwrap(), 7);

        unsafe { int2dds_type_object_destroy(handle) };
    }

    #[test]
    fn position_reader_mutable_struct() {
        let v = Phase1Mutable { a: 99, b: "dyn".to_string(), c: 1.25 };
        let bytes = serialize_with_header(&v, CdrExtKind::Mutable);
        let handle = type_object_of::<Phase1Mutable>();
        let to_ref = unsafe { &*handle };

        // Exercise all three members — mutable walk is order-independent, so
        // fetch them in reverse order to stress the EMHEADER search.
        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "c").unwrap();
        assert_eq!(k, INT2DDS_FIELD_FLOAT64);
        assert_eq!(f64::deserialize_xcdr(&mut r).unwrap(), 1.25);

        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "b").unwrap();
        assert_eq!(k, INT2DDS_FIELD_STRING);
        assert_eq!(String::deserialize_xcdr(&mut r).unwrap(), "dyn");

        let (mut r, k) = position_reader_at_field(&bytes, to_ref, "a").unwrap();
        assert_eq!(k, INT2DDS_FIELD_INT32);
        assert_eq!(i32::deserialize_xcdr(&mut r).unwrap(), 99);

        assert_eq!(
            position_reader_at_field(&bytes, to_ref, "missing").err().unwrap(),
            INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND
        );

        unsafe { int2dds_type_object_destroy(handle) };
    }

    // ========================================================================
    // Task 10/11 tests — primitive + string convenience getters
    // ========================================================================

    #[derive(DdsType)]
    #[dds_type(crate_path = "int2dds", extensibility = "Final")]
    struct PrimAll {
        pub b: bool,
        pub i8_v: i8,
        pub u8_v: u8,
        pub i16_v: i16,
        pub u16_v: u16,
        pub i32_v: i32,
        pub u32_v: u32,
        pub i64_v: i64,
        pub u64_v: u64,
        pub f32_v: f32,
        pub f64_v: f64,
        pub s: String,
    }

    fn prim_all_bytes_and_handle() -> (Vec<u8>, *mut Int2DdsTypeObject) {
        let v = PrimAll {
            b: true,
            i8_v: -8,
            u8_v: 200,
            i16_v: -1234,
            u16_v: 50000,
            i32_v: -7,
            u32_v: 0xDEAD_BEEF,
            i64_v: -1_000_000_000_000,
            u64_v: 9_000_000_000_000_000_000,
            f32_v: 1.5,
            f64_v: 2.25,
            s: "hello".to_string(),
        };
        let bytes = serialize_with_header(&v, CdrExtKind::Final);
        let h = type_object_of::<PrimAll>();
        (bytes, h)
    }

    #[test]
    fn primitive_getters_round_trip() {
        let (bytes, h) = prim_all_bytes_and_handle();

        macro_rules! check {
            ($getter:ident, $field:literal, $ty:ty, $expected:expr) => {{
                let cn = std::ffi::CString::new($field).unwrap();
                let mut out: $ty = Default::default();
                let ret = unsafe { $getter(bytes.as_ptr(), bytes.len(), h, cn.as_ptr(), &mut out) };
                assert_eq!(ret, INT2DDS_RET_OK, "getter for {} failed", $field);
                assert_eq!(out, $expected, "value mismatch for {}", $field);
            }};
        }

        check!(int2dds_dynamic_sample_get_bool, "b", bool, true);
        check!(int2dds_dynamic_sample_get_i8, "i8_v", i8, -8);
        check!(int2dds_dynamic_sample_get_u8, "u8_v", u8, 200);
        // u8 field has CDR kind BYTE — verify byte alias also works.
        check!(int2dds_dynamic_sample_get_byte, "u8_v", u8, 200);
        check!(int2dds_dynamic_sample_get_i16, "i16_v", i16, -1234);
        check!(int2dds_dynamic_sample_get_u16, "u16_v", u16, 50000);
        check!(int2dds_dynamic_sample_get_i32, "i32_v", i32, -7);
        check!(int2dds_dynamic_sample_get_u32, "u32_v", u32, 0xDEAD_BEEF);
        check!(int2dds_dynamic_sample_get_i64, "i64_v", i64, -1_000_000_000_000);
        check!(int2dds_dynamic_sample_get_u64, "u64_v", u64, 9_000_000_000_000_000_000);
        check!(int2dds_dynamic_sample_get_f32, "f32_v", f32, 1.5);
        check!(int2dds_dynamic_sample_get_f64, "f64_v", f64, 2.25);

        unsafe { int2dds_type_object_destroy(h) };
    }

    #[test]
    fn primitive_getter_type_mismatch() {
        let (bytes, h) = prim_all_bytes_and_handle();
        // Reading f64 field as i32 must report type mismatch.
        let cn = std::ffi::CString::new("f64_v").unwrap();
        let mut out: i32 = 0;
        let ret = unsafe {
            int2dds_dynamic_sample_get_i32(bytes.as_ptr(), bytes.len(), h, cn.as_ptr(), &mut out)
        };
        assert_eq!(ret, INT2DDS_RET_DYNAMIC_TYPE_MISMATCH);
        unsafe { int2dds_type_object_destroy(h) };
    }

    #[test]
    fn primitive_getter_field_not_found() {
        let (bytes, h) = prim_all_bytes_and_handle();
        let cn = std::ffi::CString::new("nope").unwrap();
        let mut out: i32 = 0;
        let ret = unsafe {
            int2dds_dynamic_sample_get_i32(bytes.as_ptr(), bytes.len(), h, cn.as_ptr(), &mut out)
        };
        assert_eq!(ret, INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND);
        unsafe { int2dds_type_object_destroy(h) };
    }

    #[test]
    fn string_getter_round_trip() {
        let (bytes, h) = prim_all_bytes_and_handle();
        let cn = std::ffi::CString::new("s").unwrap();
        let mut buf = [0i8; 64];
        let mut out_len: usize = 0;
        let ret = unsafe {
            int2dds_dynamic_sample_get_string(
                bytes.as_ptr(),
                bytes.len(),
                h,
                cn.as_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_len,
            )
        };
        assert_eq!(ret, INT2DDS_RET_OK);
        assert_eq!(out_len, 5);
        let actual = unsafe { CStr::from_ptr(buf.as_ptr()) }.to_str().unwrap();
        assert_eq!(actual, "hello");
        unsafe { int2dds_type_object_destroy(h) };
    }

    #[test]
    fn string_getter_buffer_too_small() {
        let (bytes, h) = prim_all_bytes_and_handle();
        let cn = std::ffi::CString::new("s").unwrap();
        let mut buf = [0i8; 3]; // too small for "hello\0"
        let mut out_len: usize = 0;
        let ret = unsafe {
            int2dds_dynamic_sample_get_string(
                bytes.as_ptr(),
                bytes.len(),
                h,
                cn.as_ptr(),
                buf.as_mut_ptr(),
                buf.len(),
                &mut out_len,
            )
        };
        assert_eq!(ret, INT2DDS_RET_DYNAMIC_DECODE_ERROR);
        assert_eq!(out_len, 5);
        unsafe { int2dds_type_object_destroy(h) };
    }
}
