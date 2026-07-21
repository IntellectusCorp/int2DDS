//! # Dynamic XTypes
//!
//! This module exposes a C-callable surface that lets a subscriber:
//!  - Discover a publisher's TypeObject at runtime
//!  - Introspect it (extensibility, members, kinds, member_ids, flags)
//!  - Register a topic for it via `RawTypeSupport`
//!  - Decode received CDR sample bytes into a `DynamicData` handle and read
//!    fields by dotted/indexed path, including nested structs and
//!    sequence/array elements (delegating to the core dynamic-type machinery)

use int2dds::common::builtin::topic::publication_builtin_topic_data::PublicationBuiltinTopicData;
use int2dds::xtypes::{
    deserialize_dynamic_data, CompleteStructType, CompleteTypeObject, DynamicData,
    DynamicTypeSupport, DynamicValue, ExtensibilityKind, FromDynamicValue, TypeIdentifier,
    TypeObject,
};
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Arc;

use crate::error::*;
use crate::type_info::{
    INT2DDS_FIELD_ARRAY, INT2DDS_FIELD_BOOL, INT2DDS_FIELD_BYTE, INT2DDS_FIELD_CHAR16,
    INT2DDS_FIELD_CHAR8, INT2DDS_FIELD_FLOAT32, INT2DDS_FIELD_FLOAT64, INT2DDS_FIELD_INT16,
    INT2DDS_FIELD_INT32, INT2DDS_FIELD_INT64, INT2DDS_FIELD_INT8, INT2DDS_FIELD_MAP,
    INT2DDS_FIELD_NESTED, INT2DDS_FIELD_SEQUENCE, INT2DDS_FIELD_STRING, INT2DDS_FIELD_UINT16,
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
        TypeIdentifier::MinimalTypeId(..) | TypeIdentifier::CompleteTypeId(..) => {
            INT2DDS_FIELD_NESTED
        }
        TypeIdentifier::PlainSequenceSmall { .. } | TypeIdentifier::PlainSequenceLarge { .. } => {
            INT2DDS_FIELD_SEQUENCE
        }
        TypeIdentifier::PlainArraySmall { .. } | TypeIdentifier::PlainArrayLarge { .. } => {
            INT2DDS_FIELD_ARRAY
        }
        TypeIdentifier::PlainMapSmall { .. } | TypeIdentifier::PlainMapLarge { .. } => {
            INT2DDS_FIELD_MAP
        }
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
pub(crate) unsafe fn copy_str_to_c(
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

    let cond = ffi_try!(reader.get_statuscondition()).clone();
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

use int2dds::common::instance_handle::InstanceHandle;
use int2dds::infrastructure::status::StatusMask;
use int2dds::publication::{data_writer::DataWriter, qos::DataWriterQos};
use int2dds::serialize::cdr::ExtensibilityKind as CdrExtensibilityKind;
use int2dds::subscription::sample_info::{InstanceStateKind, SampleStateKind, ViewStateKind};
use int2dds::subscription::{data_reader::DataReader, qos::DataReaderQos};
use int2dds::topic::{qos::TopicQos, TypeSupport};

use crate::data::Int2DdsData;
use crate::qos::{Int2DdsDataReaderQos, Int2DdsDataWriterQos, Int2DdsTopicQos};
use crate::raw_type_support::RawTypeSupport;
use crate::types::{Int2DdsPublisher, Int2DdsSampleInfo, Int2DdsTopic};

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

fn decode_flat(bytes: &[u8], type_obj: &Int2DdsTypeObject) -> Result<DynamicData, Int2DdsRet> {
    let support = DynamicTypeSupport::from_type_object(type_obj.inner.clone())
        .map_err(|_| INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE)?;
    deserialize_dynamic_data(bytes, support.dynamic_type())
        .map_err(|_| INT2DDS_RET_DYNAMIC_DECODE_ERROR)
}

fn split_index(seg: &str) -> (&str, Option<usize>) {
    match seg.split_once('[') {
        Some((name, rest)) => (name, rest.strip_suffix(']').and_then(|n| n.parse().ok())),
        None => (seg, None),
    }
}

/// Resolve a dotted/indexed path (e.g. `"pos.x"` or `"items[2].name"`) to a value.
pub(crate) fn resolve_path<'a>(data: &'a DynamicData, path: &str) -> Option<&'a DynamicValue> {
    let mut current: Option<&DynamicValue> = None;
    for seg in path.split('.') {
        let (name, index) = split_index(seg);
        current = Some(match current {
            None => data.get_value(name)?,
            Some(DynamicValue::Struct(inner)) => inner.get_value(name)?,
            _ => return None,
        });
        if let Some(i) = index {
            current = Some(match current? {
                DynamicValue::Sequence(items) | DynamicValue::Array(items) => items.get(i)?,
                _ => return None,
            });
        }
    }
    current
}

/// Read a typed value at `path` from already-decoded `DynamicData`.
fn get_as<T: FromDynamicValue>(data: &DynamicData, path: &str) -> Result<T, Int2DdsRet> {
    let value = resolve_path(data, path).ok_or(INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND)?;
    T::from_dynamic(value).map_err(|_| INT2DDS_RET_DYNAMIC_TYPE_MISMATCH)
}

macro_rules! define_flat_sample_getter {
    ($fn_name:ident, $rust_ty:ty) => {
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
            let data = match decode_flat(slice, &*type_obj) {
                Ok(d) => d,
                Err(e) => return e,
            };
            match get_as::<$rust_ty>(&data, name) {
                Ok(v) => {
                    *out = v;
                    INT2DDS_RET_OK
                }
                Err(e) => e,
            }
        }
    };
}

define_flat_sample_getter!(int2dds_dynamic_sample_get_bool, bool);
define_flat_sample_getter!(int2dds_dynamic_sample_get_i8, i8);
define_flat_sample_getter!(int2dds_dynamic_sample_get_u8, u8);
define_flat_sample_getter!(int2dds_dynamic_sample_get_byte, u8);
define_flat_sample_getter!(int2dds_dynamic_sample_get_i16, i16);
define_flat_sample_getter!(int2dds_dynamic_sample_get_u16, u16);
define_flat_sample_getter!(int2dds_dynamic_sample_get_i32, i32);
define_flat_sample_getter!(int2dds_dynamic_sample_get_u32, u32);
define_flat_sample_getter!(int2dds_dynamic_sample_get_i64, i64);
define_flat_sample_getter!(int2dds_dynamic_sample_get_u64, u64);
define_flat_sample_getter!(int2dds_dynamic_sample_get_f32, f32);
define_flat_sample_getter!(int2dds_dynamic_sample_get_f64, f64);

/// Read a char8 field (returned as its byte value) from a flat sample.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_sample_get_char8(
    bytes: *const u8,
    len: usize,
    type_obj: *const Int2DdsTypeObject,
    field_name: *const c_char,
    out: *mut u8,
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
    let data = match decode_flat(slice, &*type_obj) {
        Ok(d) => d,
        Err(e) => return e,
    };
    match get_as::<char>(&data, name) {
        Ok(c) => {
            *out = c as u8;
            INT2DDS_RET_OK
        }
        Err(e) => e,
    }
}

/// Read a string field by name from a flat sample.
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
    let data = match decode_flat(slice, &*type_obj) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let s: String = match get_as(&data, name) {
        Ok(v) => v,
        Err(e) => return e,
    };
    copy_str_to_c(&s, out_buf, buf_cap, out_len)
}

pub struct Int2DdsDynamicData {
    pub(crate) inner: DynamicData,
}

#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_from_sample(
    participant: *const Int2DdsParticipant,
    bytes: *const u8,
    len: usize,
    type_obj: *const Int2DdsTypeObject,
    out: *mut *mut Int2DdsDynamicData,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(bytes);
    check_null!(type_obj);
    check_null!(out);
    let slice = std::slice::from_raw_parts(bytes, len);
    let support = ffi_try!((*participant)
        .inner
        .create_dynamic_type_from_type_object((*type_obj).inner.clone()));
    let data = match deserialize_dynamic_data(slice, support.dynamic_type()) {
        Ok(d) => d,
        Err(_) => return INT2DDS_RET_DYNAMIC_DECODE_ERROR,
    };
    *out = Box::into_raw(Box::new(Int2DdsDynamicData { inner: data }));
    INT2DDS_RET_OK
}

/// Destroy a DynamicData handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_destroy(d: *mut Int2DdsDynamicData) {
    if !d.is_null() {
        drop(Box::from_raw(d));
    }
}

/// Handle-based getter reading a value at a dotted/indexed `field_path`.
macro_rules! define_handle_getter {
    ($fn_name:ident, $rust_ty:ty) => {
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(
            data: *const Int2DdsDynamicData,
            field_path: *const c_char,
            out: *mut $rust_ty,
        ) -> Int2DdsRet {
            check_null!(data);
            check_null!(field_path);
            check_null!(out);
            let path = match CStr::from_ptr(field_path).to_str() {
                Ok(s) => s,
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            };
            match get_as::<$rust_ty>(&(*data).inner, path) {
                Ok(v) => {
                    *out = v;
                    INT2DDS_RET_OK
                }
                Err(e) => e,
            }
        }
    };
}

define_handle_getter!(int2dds_dynamic_data_get_bool, bool);
define_handle_getter!(int2dds_dynamic_data_get_i8, i8);
define_handle_getter!(int2dds_dynamic_data_get_u8, u8);
define_handle_getter!(int2dds_dynamic_data_get_i16, i16);
define_handle_getter!(int2dds_dynamic_data_get_u16, u16);
define_handle_getter!(int2dds_dynamic_data_get_i32, i32);
define_handle_getter!(int2dds_dynamic_data_get_u32, u32);
define_handle_getter!(int2dds_dynamic_data_get_i64, i64);
define_handle_getter!(int2dds_dynamic_data_get_u64, u64);
define_handle_getter!(int2dds_dynamic_data_get_f32, f32);
define_handle_getter!(int2dds_dynamic_data_get_f64, f64);

/// Read a char8 field (as its byte value) at `field_path`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_get_char8(
    data: *const Int2DdsDynamicData,
    field_path: *const c_char,
    out: *mut u8,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field_path);
    check_null!(out);
    let path = match CStr::from_ptr(field_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    match get_as::<char>(&(*data).inner, path) {
        Ok(c) => {
            *out = c as u8;
            INT2DDS_RET_OK
        }
        Err(e) => e,
    }
}

/// Read a string field at `field_path` into a caller-supplied buffer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_get_string(
    data: *const Int2DdsDynamicData,
    field_path: *const c_char,
    out_buf: *mut c_char,
    buf_cap: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field_path);
    check_null!(out_buf);
    check_null!(out_len);
    let path = match CStr::from_ptr(field_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let s: String = match get_as(&(*data).inner, path) {
        Ok(v) => v,
        Err(e) => return e,
    };
    copy_str_to_c(&s, out_buf, buf_cap, out_len)
}

/// Get the element count of a sequence/array field at `field_path`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_get_len(
    data: *const Int2DdsDynamicData,
    field_path: *const c_char,
    out: *mut usize,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field_path);
    check_null!(out);
    let path = match CStr::from_ptr(field_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    match resolve_path(&(*data).inner, path) {
        Some(DynamicValue::Sequence(items)) | Some(DynamicValue::Array(items)) => {
            *out = items.len();
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
        None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Extract a nested struct value at `field_path` as a new DynamicData handle.
/// Destroy it with `int2dds_dynamic_data_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_get_member(
    data: *const Int2DdsDynamicData,
    field_path: *const c_char,
    out: *mut *mut Int2DdsDynamicData,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field_path);
    check_null!(out);
    let path = match CStr::from_ptr(field_path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    match resolve_path(&(*data).inner, path) {
        Some(DynamicValue::Struct(inner)) => {
            *out = Box::into_raw(Box::new(Int2DdsDynamicData { inner: (**inner).clone() }));
            INT2DDS_RET_OK
        }
        Some(_) => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
        None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

// ============================================================================
// Dynamic pub/sub: build, write, and take DynamicData via DynamicTypeSupport.
// Mirrors the core create_topic_dynamic / create_datawriter_dynamic /
// create_datareader_dynamic path so XML- or discovery-sourced types can be
// published and subscribed without compile-time IDL.
// ============================================================================

/// Opaque handle wrapping an `Arc<DynamicTypeSupport>` (full dependency closure
/// resolved). Obtain one from an XML registry or a discovered TypeObject.
pub struct Int2DdsDynamicTypeSupport {
    pub(crate) inner: Arc<DynamicTypeSupport>,
}
unsafe impl Send for Int2DdsDynamicTypeSupport {}
unsafe impl Sync for Int2DdsDynamicTypeSupport {}

/// Opaque handle to a dynamic DataWriter (`DataWriter<DynamicData>`).
pub struct Int2DdsDynamicDataWriter {
    pub(crate) inner: DataWriter<DynamicData>,
}
unsafe impl Send for Int2DdsDynamicDataWriter {}
unsafe impl Sync for Int2DdsDynamicDataWriter {}

/// Opaque handle to a dynamic DataReader (`DataReader<DynamicData>`).
pub struct Int2DdsDynamicDataReader {
    pub(crate) inner: DataReader<DynamicData>,
}
unsafe impl Send for Int2DdsDynamicDataReader {}
unsafe impl Sync for Int2DdsDynamicDataReader {}

/// Destroy a dynamic type support handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_type_support_destroy(s: *mut Int2DdsDynamicTypeSupport) {
    if !s.is_null() {
        drop(Box::from_raw(s));
    }
}

/// Register a topic backed by a dynamic type support. The support's full type
/// closure is advertised during discovery.
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_topic_dynamic(
    participant: *const Int2DdsParticipant,
    topic_name: *const c_char,
    type_support: *const Int2DdsDynamicTypeSupport,
    qos: *const Int2DdsTopicQos,
    out: *mut *mut Int2DdsTopic,
) -> Int2DdsRet {
    check_null!(participant);
    check_null!(topic_name);
    check_null!(type_support);
    check_null!(out);

    let p = &*participant;
    let topic_name_str = match CStr::from_ptr(topic_name).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let support = (*type_support).inner.clone();
    let type_name = support.get_type_name().to_string();
    let topic_qos = if qos.is_null() { TopicQos::default() } else { (*qos).inner.clone() };

    let topic = ffi_try!(p.inner.create_topic_dynamic(
        topic_name_str,
        support,
        topic_qos,
        None,
        StatusMask::default(),
    ));
    *out = Box::into_raw(Box::new(Int2DdsTopic { inner: Arc::new(topic), type_name }));
    INT2DDS_RET_OK
}

/// Create a dynamic DataWriter. Pass null `qos` to use the default.
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datawriter_dynamic(
    publisher: *const Int2DdsPublisher,
    topic: *const Int2DdsTopic,
    type_support: *const Int2DdsDynamicTypeSupport,
    qos: *const Int2DdsDataWriterQos,
    out: *mut *mut Int2DdsDynamicDataWriter,
) -> Int2DdsRet {
    check_null!(publisher);
    check_null!(topic);
    check_null!(type_support);
    check_null!(out);

    let support = (*type_support).inner.clone();
    let writer_qos = if qos.is_null() { DataWriterQos::default() } else { (*qos).inner.clone() };
    let writer = ffi_try!((*publisher).inner.create_datawriter_dynamic(
        (*topic).inner.as_ref(),
        support,
        writer_qos,
        None,
        StatusMask::default(),
    ));
    *out = Box::into_raw(Box::new(Int2DdsDynamicDataWriter { inner: writer }));
    INT2DDS_RET_OK
}

/// Create a dynamic DataReader. Pass null `qos` to use the default.
#[no_mangle]
pub unsafe extern "C" fn int2dds_create_datareader_dynamic(
    subscriber: *const Int2DdsSubscriber,
    topic: *const Int2DdsTopic,
    type_support: *const Int2DdsDynamicTypeSupport,
    qos: *const Int2DdsDataReaderQos,
    out: *mut *mut Int2DdsDynamicDataReader,
) -> Int2DdsRet {
    check_null!(subscriber);
    check_null!(topic);
    check_null!(type_support);
    check_null!(out);

    let support = (*type_support).inner.clone();
    let reader_qos = if qos.is_null() { DataReaderQos::default() } else { (*qos).inner.clone() };
    let reader = ffi_try!((*subscriber).inner.create_datareader_dynamic(
        (*topic).inner.as_ref(),
        support,
        reader_qos,
        None,
        StatusMask::default(),
    ));
    *out = Box::into_raw(Box::new(Int2DdsDynamicDataReader { inner: reader }));
    INT2DDS_RET_OK
}

/// Destroy a dynamic DataWriter handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_writer_destroy(w: *mut Int2DdsDynamicDataWriter) {
    if !w.is_null() {
        drop(Box::from_raw(w));
    }
}

/// Destroy a dynamic DataReader handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_reader_destroy(r: *mut Int2DdsDynamicDataReader) {
    if !r.is_null() {
        drop(Box::from_raw(r));
    }
}

/// Current number of DataReaders matched to this dynamic writer.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_writer_publication_matched_count(
    writer: *const Int2DdsDynamicDataWriter,
    out: *mut i32,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(out);
    let status = ffi_try!((*writer).inner.get_publication_matched_status());
    *out = status.current_count();
    INT2DDS_RET_OK
}

/// Current number of DataWriters matched to this dynamic reader.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_reader_subscription_matched_count(
    reader: *const Int2DdsDynamicDataReader,
    out: *mut i32,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(out);
    let status = ffi_try!((*reader).inner.get_subscription_matched_status());
    *out = status.current_count();
    INT2DDS_RET_OK
}

/// Create an empty, writable DynamicData for the given type support.
/// Populate it with the `int2dds_dynamic_data_set_*` setters, then publish via
/// `int2dds_dynamic_writer_write`. Destroy with `int2dds_dynamic_data_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_create(
    type_support: *const Int2DdsDynamicTypeSupport,
    out: *mut *mut Int2DdsDynamicData,
) -> Int2DdsRet {
    check_null!(type_support);
    check_null!(out);
    let data = (*type_support).inner.create_data();
    *out = Box::into_raw(Box::new(Int2DdsDynamicData { inner: data }));
    INT2DDS_RET_OK
}

/// Setter writing a primitive value to a top-level field by name.
macro_rules! define_handle_setter {
    ($fn_name:ident, $rust_ty:ty) => {
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(
            data: *mut Int2DdsDynamicData,
            field: *const c_char,
            value: $rust_ty,
        ) -> Int2DdsRet {
            check_null!(data);
            check_null!(field);
            let name = match CStr::from_ptr(field).to_str() {
                Ok(s) => s,
                Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
            };
            match (*data).inner.set(name, value) {
                Ok(()) => INT2DDS_RET_OK,
                Err(_) => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
            }
        }
    };
}

define_handle_setter!(int2dds_dynamic_data_set_bool, bool);
define_handle_setter!(int2dds_dynamic_data_set_i8, i8);
define_handle_setter!(int2dds_dynamic_data_set_u8, u8);
define_handle_setter!(int2dds_dynamic_data_set_i16, i16);
define_handle_setter!(int2dds_dynamic_data_set_u16, u16);
define_handle_setter!(int2dds_dynamic_data_set_i32, i32);
define_handle_setter!(int2dds_dynamic_data_set_u32, u32);
define_handle_setter!(int2dds_dynamic_data_set_i64, i64);
define_handle_setter!(int2dds_dynamic_data_set_u64, u64);
define_handle_setter!(int2dds_dynamic_data_set_f32, f32);
define_handle_setter!(int2dds_dynamic_data_set_f64, f64);

/// Set a char8 field (given as its byte value) on a top-level field.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_set_char8(
    data: *mut Int2DdsDynamicData,
    field: *const c_char,
    value: u8,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    let name = match CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    match (*data).inner.set(name, char::from(value)) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Set a string field on a top-level field. `value` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_set_string(
    data: *mut Int2DdsDynamicData,
    field: *const c_char,
    value: *const c_char,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(value);
    let name = match CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let v = match CStr::from_ptr(value).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    match (*data).inner.set(name, v) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Publish a populated DynamicData sample.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_writer_write(
    writer: *const Int2DdsDynamicDataWriter,
    data: *const Int2DdsDynamicData,
) -> Int2DdsRet {
    check_null!(writer);
    check_null!(data);
    match (*writer).inner.write(&(*data).inner, InstanceHandle::NIL) {
        Ok(()) => INT2DDS_RET_OK,
        Err(e) => dds_error_to_code(&e),
    }
}

/// Take the next available DynamicData sample. On success `out_data` receives a
/// new DynamicData handle (destroy with `int2dds_dynamic_data_destroy`) and, if
/// non-null, `out_info` receives the sample info. Returns INT2DDS_RET_NO_DATA
/// when no valid sample is available.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_reader_take(
    reader: *const Int2DdsDynamicDataReader,
    out_data: *mut *mut Int2DdsDynamicData,
    out_info: *mut Int2DdsSampleInfo,
) -> Int2DdsRet {
    check_null!(reader);
    check_null!(out_data);

    let samples = match (*reader).inner.take(
        1,
        &[SampleStateKind::NOT_READ_SAMPLE_STATE],
        &[ViewStateKind::ANY_VIEW_STATE],
        &[InstanceStateKind::ANY_INSTANCE_STATE],
    ) {
        Ok(s) => s,
        Err(int2dds::dcps::core::error::DdsError::NoData) => return INT2DDS_RET_NO_DATA,
        Err(e) => return dds_error_to_code(&e),
    };

    let sample = match samples.into_iter().next() {
        Some(s) => s,
        None => return INT2DDS_RET_NO_DATA,
    };

    let info = sample.sample_info();
    if !out_info.is_null() {
        *out_info = Int2DdsSampleInfo::from(&info);
    }
    if !info.valid_data {
        return INT2DDS_RET_NO_DATA;
    }

    let dynamic_data = ffi_try!(sample.data());
    *out_data = Box::into_raw(Box::new(Int2DdsDynamicData { inner: dynamic_data }));
    INT2DDS_RET_OK
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

            let mut buf = [0 as c_char; 64];
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

    use int2dds::serialize::cdr::{ExtensibilityKind as CdrExtKind, XcdrSerialize, XcdrSerializer};
    use int2dds::serialize::{BufferManager, DeserializerReader};
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

    fn flat_i32(bytes: &[u8], h: *const Int2DdsTypeObject, field: &str) -> (Int2DdsRet, i32) {
        let c = std::ffi::CString::new(field).unwrap();
        let mut out = 0i32;
        let r = unsafe {
            int2dds_dynamic_sample_get_i32(bytes.as_ptr(), bytes.len(), h, c.as_ptr(), &mut out)
        };
        (r, out)
    }

    #[test]
    fn dynamic_sample_final_struct() {
        let v = Phase1Final { a: 0x1122_3344, b: 3.5, c: "hello".to_string(), d: 0xBEEF };
        let bytes = serialize_with_header(&v, CdrExtKind::Final);
        let h = type_object_of::<Phase1Final>();

        assert_eq!(flat_i32(&bytes, h, "a"), (INT2DDS_RET_OK, 0x1122_3344));

        let mut b = 0f64;
        let cn = std::ffi::CString::new("b").unwrap();
        assert_eq!(
            unsafe {
                int2dds_dynamic_sample_get_f64(bytes.as_ptr(), bytes.len(), h, cn.as_ptr(), &mut b)
            },
            INT2DDS_RET_OK
        );
        assert_eq!(b, 3.5);

        let mut d = 0u16;
        let cn = std::ffi::CString::new("d").unwrap();
        assert_eq!(
            unsafe {
                int2dds_dynamic_sample_get_u16(bytes.as_ptr(), bytes.len(), h, cn.as_ptr(), &mut d)
            },
            INT2DDS_RET_OK
        );
        assert_eq!(d, 0xBEEF);

        assert_eq!(flat_i32(&bytes, h, "zzz").0, INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND);

        unsafe { int2dds_type_object_destroy(h) };
    }

    #[test]
    fn dynamic_sample_appendable_struct() {
        let v = Phase1Appendable { a: -42, b: "world".to_string(), c: 7 };
        let bytes = serialize_with_header(&v, CdrExtKind::Appendable);
        let h = type_object_of::<Phase1Appendable>();

        assert_eq!(flat_i32(&bytes, h, "a"), (INT2DDS_RET_OK, -42));

        let mut cc = 0u8;
        let cn = std::ffi::CString::new("c").unwrap();
        assert_eq!(
            unsafe {
                int2dds_dynamic_sample_get_u8(bytes.as_ptr(), bytes.len(), h, cn.as_ptr(), &mut cc)
            },
            INT2DDS_RET_OK
        );
        assert_eq!(cc, 7);

        unsafe { int2dds_type_object_destroy(h) };
    }

    #[test]
    fn dynamic_sample_mutable_struct() {
        let v = Phase1Mutable { a: 99, b: "dyn".to_string(), c: 1.25 };
        let bytes = serialize_with_header(&v, CdrExtKind::Mutable);
        let h = type_object_of::<Phase1Mutable>();

        let mut cval = 0f64;
        let cn = std::ffi::CString::new("c").unwrap();
        assert_eq!(
            unsafe {
                int2dds_dynamic_sample_get_f64(
                    bytes.as_ptr(),
                    bytes.len(),
                    h,
                    cn.as_ptr(),
                    &mut cval,
                )
            },
            INT2DDS_RET_OK
        );
        assert_eq!(cval, 1.25);

        assert_eq!(flat_i32(&bytes, h, "a"), (INT2DDS_RET_OK, 99));
        assert_eq!(flat_i32(&bytes, h, "missing").0, INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND);

        unsafe { int2dds_type_object_destroy(h) };
    }

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
        let mut buf = [0 as c_char; 64];
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
        let mut buf = [0 as c_char; 3]; // too small for "hello\0"
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

    #[test]
    fn dynamic_data_nested_and_sequence_getters() {
        use int2dds::topic::type_support::DdsType;
        use int2dds::xtypes::{DynamicType, HasTypeObject};
        use std::collections::HashMap;
        use std::ffi::CString;
        use std::sync::Arc;

        #[derive(DdsType)]
        #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
        struct Inner {
            x: i32,
            name: String,
        }
        #[derive(DdsType)]
        #[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
        struct Outer {
            id: i32,
            inner: Inner,
            tags: Vec<i32>,
        }

        let inner_dt = Arc::new(
            DynamicType::from_type_object(Inner::complete_type_object(), Inner::type_identifier())
                .unwrap(),
        );
        let outer_dt = Arc::new(
            DynamicType::from_type_object(Outer::complete_type_object(), Outer::type_identifier())
                .unwrap(),
        );

        let mut inner_vals = HashMap::new();
        inner_vals.insert(Arc::from("x"), DynamicValue::Int32(11));
        inner_vals.insert(Arc::from("name"), DynamicValue::String("leaf".to_string()));
        let inner_data = DynamicData::with_values(inner_dt, inner_vals);

        let mut outer_vals = HashMap::new();
        outer_vals.insert(Arc::from("id"), DynamicValue::Int32(7));
        outer_vals.insert(Arc::from("inner"), DynamicValue::Struct(Box::new(inner_data)));
        outer_vals.insert(
            Arc::from("tags"),
            DynamicValue::Sequence(vec![DynamicValue::Int32(100), DynamicValue::Int32(200)]),
        );
        let outer_data = DynamicData::with_values(outer_dt, outer_vals);

        let h = Box::into_raw(Box::new(Int2DdsDynamicData { inner: outer_data }));

        let mut id = 0i32;
        let p = CString::new("id").unwrap();
        assert_eq!(unsafe { int2dds_dynamic_data_get_i32(h, p.as_ptr(), &mut id) }, INT2DDS_RET_OK);
        assert_eq!(id, 7);

        let mut x = 0i32;
        let p = CString::new("inner.x").unwrap();
        assert_eq!(unsafe { int2dds_dynamic_data_get_i32(h, p.as_ptr(), &mut x) }, INT2DDS_RET_OK);
        assert_eq!(x, 11);

        let p = CString::new("inner.name").unwrap();
        let mut buf = [0 as c_char; 16];
        let mut nlen = 0usize;
        assert_eq!(
            unsafe {
                int2dds_dynamic_data_get_string(
                    h,
                    p.as_ptr(),
                    buf.as_mut_ptr(),
                    buf.len(),
                    &mut nlen,
                )
            },
            INT2DDS_RET_OK
        );
        assert_eq!(nlen, 4);

        let mut len = 0usize;
        let p = CString::new("tags").unwrap();
        assert_eq!(
            unsafe { int2dds_dynamic_data_get_len(h, p.as_ptr(), &mut len) },
            INT2DDS_RET_OK
        );
        assert_eq!(len, 2);

        let mut t1 = 0i32;
        let p = CString::new("tags[1]").unwrap();
        assert_eq!(unsafe { int2dds_dynamic_data_get_i32(h, p.as_ptr(), &mut t1) }, INT2DDS_RET_OK);
        assert_eq!(t1, 200);

        let p = CString::new("inner").unwrap();
        let mut child: *mut Int2DdsDynamicData = std::ptr::null_mut();
        assert_eq!(
            unsafe { int2dds_dynamic_data_get_member(h, p.as_ptr(), &mut child) },
            INT2DDS_RET_OK
        );
        let mut cx = 0i32;
        let p = CString::new("x").unwrap();
        assert_eq!(
            unsafe { int2dds_dynamic_data_get_i32(child, p.as_ptr(), &mut cx) },
            INT2DDS_RET_OK
        );
        assert_eq!(cx, 11);

        unsafe { int2dds_dynamic_data_destroy(child) };
        unsafe { int2dds_dynamic_data_destroy(h) };
    }
}
