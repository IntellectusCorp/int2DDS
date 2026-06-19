//! # Dynamic Value tree
//!
//! A C-callable, owning value type mirroring [`DynamicValue`]. It lets callers
//! build arbitrarily nested write payloads (structs, sequences, arrays, maps,
//! unions, enums, bitmask/bitset, wide strings) and inspect read-back values of
//! the same shapes — the pieces the flat `int2dds_dynamic_data_set_*` setters
//! and path getters cannot express.
//!
//! ## Ownership
//!
//! Constructors return a newly-allocated handle the caller owns. The handle is
//! *consumed* (must not be destroyed or reused) when passed to
//! `int2dds_dynamic_value_push`, `_map_insert`, `_union`, or
//! `int2dds_dynamic_data_set_value` **on success**. On error those calls do not
//! consume their inputs, so the caller still owns and must destroy them.
//! Anything never handed off must be released with `int2dds_dynamic_value_destroy`.

use std::ffi::CStr;
use std::os::raw::c_char;

use int2dds::xtypes::{DynamicValue, FromDynamicValue};

use crate::dynamic::{copy_str_to_c, resolve_path, Int2DdsDynamicData};
use crate::error::*;

/// Value-kind discriminators returned by `int2dds_dynamic_value_kind`.
pub const INT2DDS_VALUE_KIND_BOOLEAN: i32 = 0;
pub const INT2DDS_VALUE_KIND_INT8: i32 = 1;
pub const INT2DDS_VALUE_KIND_INT16: i32 = 2;
pub const INT2DDS_VALUE_KIND_INT32: i32 = 3;
pub const INT2DDS_VALUE_KIND_INT64: i32 = 4;
pub const INT2DDS_VALUE_KIND_UINT8: i32 = 5;
pub const INT2DDS_VALUE_KIND_UINT16: i32 = 6;
pub const INT2DDS_VALUE_KIND_UINT32: i32 = 7;
pub const INT2DDS_VALUE_KIND_UINT64: i32 = 8;
pub const INT2DDS_VALUE_KIND_FLOAT32: i32 = 9;
pub const INT2DDS_VALUE_KIND_FLOAT64: i32 = 10;
pub const INT2DDS_VALUE_KIND_CHAR8: i32 = 11;
pub const INT2DDS_VALUE_KIND_BYTE: i32 = 12;
pub const INT2DDS_VALUE_KIND_STRING: i32 = 13;
pub const INT2DDS_VALUE_KIND_WSTRING: i32 = 14;
pub const INT2DDS_VALUE_KIND_ENUM: i32 = 15;
pub const INT2DDS_VALUE_KIND_UNION: i32 = 16;
pub const INT2DDS_VALUE_KIND_BITMASK: i32 = 17;
pub const INT2DDS_VALUE_KIND_BITSET: i32 = 18;
pub const INT2DDS_VALUE_KIND_STRUCT: i32 = 19;
pub const INT2DDS_VALUE_KIND_SEQUENCE: i32 = 20;
pub const INT2DDS_VALUE_KIND_ARRAY: i32 = 21;
pub const INT2DDS_VALUE_KIND_MAP: i32 = 22;
pub const INT2DDS_VALUE_KIND_OPTIONAL: i32 = 23;
pub const INT2DDS_VALUE_KIND_NULL: i32 = 24;

/// Opaque, owning handle wrapping a [`DynamicValue`].
pub struct Int2DdsDynamicValue {
    pub(crate) inner: DynamicValue,
}

fn into_handle(value: DynamicValue) -> *mut Int2DdsDynamicValue {
    Box::into_raw(Box::new(Int2DdsDynamicValue { inner: value }))
}

// ----------------------------------------------------------------------------
// Scalar constructors
// ----------------------------------------------------------------------------

macro_rules! value_ctor {
    ($fn_name:ident, $rust_ty:ty, $variant:expr) => {
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(
            value: $rust_ty,
            out: *mut *mut Int2DdsDynamicValue,
        ) -> Int2DdsRet {
            check_null!(out);
            *out = into_handle($variant(value));
            INT2DDS_RET_OK
        }
    };
}

value_ctor!(int2dds_dynamic_value_bool, bool, DynamicValue::Boolean);
value_ctor!(int2dds_dynamic_value_i8, i8, DynamicValue::Int8);
value_ctor!(int2dds_dynamic_value_i16, i16, DynamicValue::Int16);
value_ctor!(int2dds_dynamic_value_i32, i32, DynamicValue::Int32);
value_ctor!(int2dds_dynamic_value_i64, i64, DynamicValue::Int64);
value_ctor!(int2dds_dynamic_value_u8, u8, DynamicValue::Uint8);
value_ctor!(int2dds_dynamic_value_u16, u16, DynamicValue::Uint16);
value_ctor!(int2dds_dynamic_value_u32, u32, DynamicValue::Uint32);
value_ctor!(int2dds_dynamic_value_u64, u64, DynamicValue::Uint64);
value_ctor!(int2dds_dynamic_value_f32, f32, DynamicValue::Float32);
value_ctor!(int2dds_dynamic_value_f64, f64, DynamicValue::Float64);
value_ctor!(int2dds_dynamic_value_byte, u8, DynamicValue::Byte);
value_ctor!(int2dds_dynamic_value_bitmask, u64, DynamicValue::Bitmask);
value_ctor!(int2dds_dynamic_value_bitset, u64, DynamicValue::Bitset);

/// Construct a char8 value from its byte value.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_char8(
    value: u8,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(out);
    *out = into_handle(DynamicValue::Char8(char::from(value)));
    INT2DDS_RET_OK
}

unsafe fn cstr_value(ptr: *const c_char) -> Result<String, Int2DdsRet> {
    match CStr::from_ptr(ptr).to_str() {
        Ok(s) => Ok(s.to_string()),
        Err(_) => Err(INT2DDS_RET_INVALID_ARGUMENT),
    }
}

/// Construct a UTF-8 string value. `value` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_string(
    value: *const c_char,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    let s = match cstr_value(value) {
        Ok(s) => s,
        Err(e) => return e,
    };
    *out = into_handle(DynamicValue::String(s));
    INT2DDS_RET_OK
}

/// Construct a wide-string value. `value` must be null-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_wstring(
    value: *const c_char,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    let s = match cstr_value(value) {
        Ok(s) => s,
        Err(e) => return e,
    };
    *out = into_handle(DynamicValue::WString(s));
    INT2DDS_RET_OK
}

/// Construct an enum value from its literal name and numeric value. The name may
/// be empty when only the numeric value is known.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_enum(
    name: *const c_char,
    value: i32,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(name);
    check_null!(out);
    let name = match cstr_value(name) {
        Ok(s) => s,
        Err(e) => return e,
    };
    *out = into_handle(DynamicValue::Enum { name, value });
    INT2DDS_RET_OK
}

/// Construct a nested struct value by cloning a DynamicData instance.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_struct(
    data: *const Int2DdsDynamicData,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(out);
    *out = into_handle(DynamicValue::Struct(Box::new((*data).inner.clone())));
    INT2DDS_RET_OK
}

/// Construct an empty sequence value. Append elements with
/// `int2dds_dynamic_value_push`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_sequence(
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(out);
    *out = into_handle(DynamicValue::Sequence(Vec::new()));
    INT2DDS_RET_OK
}

/// Construct an empty array value. Append elements with
/// `int2dds_dynamic_value_push`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_array(
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(out);
    *out = into_handle(DynamicValue::Array(Vec::new()));
    INT2DDS_RET_OK
}

/// Construct an empty map value. Add entries with
/// `int2dds_dynamic_value_map_insert`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_map(
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(out);
    *out = into_handle(DynamicValue::Map(Vec::new()));
    INT2DDS_RET_OK
}

/// Construct a union value from a discriminator and the selected branch value.
/// Both inputs are consumed on success.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_union(
    discriminator: *mut Int2DdsDynamicValue,
    value: *mut Int2DdsDynamicValue,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(discriminator);
    check_null!(value);
    check_null!(out);
    let disc = Box::from_raw(discriminator);
    let val = Box::from_raw(value);
    *out = into_handle(DynamicValue::Union {
        discriminator: Box::new(disc.inner),
        value: Box::new(val.inner),
    });
    INT2DDS_RET_OK
}

/// Append `element` to a sequence/array value. Consumes `element` on success;
/// on error `element` is left owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_push(
    collection: *mut Int2DdsDynamicValue,
    element: *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(collection);
    check_null!(element);
    match &mut (*collection).inner {
        DynamicValue::Sequence(items) | DynamicValue::Array(items) => {
            items.push(Box::from_raw(element).inner);
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Insert a key/value pair into a map value. Consumes `key` and `value` on
/// success; on error both are left owned by the caller.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_map_insert(
    map: *mut Int2DdsDynamicValue,
    key: *mut Int2DdsDynamicValue,
    value: *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(map);
    check_null!(key);
    check_null!(value);
    match &mut (*map).inner {
        DynamicValue::Map(pairs) => {
            let k = Box::from_raw(key);
            let v = Box::from_raw(value);
            pairs.push((k.inner, v.inner));
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Destroy a value handle. Safe to call with null.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_destroy(value: *mut Int2DdsDynamicValue) {
    if !value.is_null() {
        drop(Box::from_raw(value));
    }
}

// ----------------------------------------------------------------------------
// Bridge to DynamicData
// ----------------------------------------------------------------------------

/// Set a top-level field to `value`. Consumes `value` on a successful parse of
/// `field` (regardless of whether the field exists); returns the field handle to
/// the caller only when `field` is not valid UTF-8.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_set_value(
    data: *mut Int2DdsDynamicData,
    field: *const c_char,
    value: *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(field);
    check_null!(value);
    let name = match CStr::from_ptr(field).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    let v = Box::from_raw(value);
    match (*data).inner.set_value(name, v.inner) {
        Ok(()) => INT2DDS_RET_OK,
        Err(_) => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Clone the value at a dotted/indexed `path` into a new value handle. Destroy
/// it with `int2dds_dynamic_value_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_data_get_value(
    data: *const Int2DdsDynamicData,
    path: *const c_char,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(data);
    check_null!(path);
    check_null!(out);
    let path = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return INT2DDS_RET_INVALID_ARGUMENT,
    };
    match resolve_path(&(*data).inner, path) {
        Some(v) => {
            *out = into_handle(v.clone());
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

// ----------------------------------------------------------------------------
// Inspection
// ----------------------------------------------------------------------------

/// Report the kind of a value (one of the `INT2DDS_VALUE_KIND_*` constants).
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_kind(
    value: *const Int2DdsDynamicValue,
    out: *mut i32,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    *out = match &(*value).inner {
        DynamicValue::Boolean(_) => INT2DDS_VALUE_KIND_BOOLEAN,
        DynamicValue::Int8(_) => INT2DDS_VALUE_KIND_INT8,
        DynamicValue::Int16(_) => INT2DDS_VALUE_KIND_INT16,
        DynamicValue::Int32(_) => INT2DDS_VALUE_KIND_INT32,
        DynamicValue::Int64(_) => INT2DDS_VALUE_KIND_INT64,
        DynamicValue::Uint8(_) => INT2DDS_VALUE_KIND_UINT8,
        DynamicValue::Uint16(_) => INT2DDS_VALUE_KIND_UINT16,
        DynamicValue::Uint32(_) => INT2DDS_VALUE_KIND_UINT32,
        DynamicValue::Uint64(_) => INT2DDS_VALUE_KIND_UINT64,
        DynamicValue::Float32(_) => INT2DDS_VALUE_KIND_FLOAT32,
        DynamicValue::Float64(_) => INT2DDS_VALUE_KIND_FLOAT64,
        DynamicValue::Char8(_) => INT2DDS_VALUE_KIND_CHAR8,
        DynamicValue::Byte(_) => INT2DDS_VALUE_KIND_BYTE,
        DynamicValue::String(_) => INT2DDS_VALUE_KIND_STRING,
        DynamicValue::WString(_) => INT2DDS_VALUE_KIND_WSTRING,
        DynamicValue::Enum { .. } => INT2DDS_VALUE_KIND_ENUM,
        DynamicValue::Union { .. } => INT2DDS_VALUE_KIND_UNION,
        DynamicValue::Bitmask(_) => INT2DDS_VALUE_KIND_BITMASK,
        DynamicValue::Bitset(_) => INT2DDS_VALUE_KIND_BITSET,
        DynamicValue::Struct(_) => INT2DDS_VALUE_KIND_STRUCT,
        DynamicValue::Sequence(_) => INT2DDS_VALUE_KIND_SEQUENCE,
        DynamicValue::Array(_) => INT2DDS_VALUE_KIND_ARRAY,
        DynamicValue::Map(_) => INT2DDS_VALUE_KIND_MAP,
        DynamicValue::Optional(_) => INT2DDS_VALUE_KIND_OPTIONAL,
        DynamicValue::Null => INT2DDS_VALUE_KIND_NULL,
    };
    INT2DDS_RET_OK
}

macro_rules! value_as {
    ($fn_name:ident, $rust_ty:ty) => {
        #[no_mangle]
        pub unsafe extern "C" fn $fn_name(
            value: *const Int2DdsDynamicValue,
            out: *mut $rust_ty,
        ) -> Int2DdsRet {
            check_null!(value);
            check_null!(out);
            match <$rust_ty as FromDynamicValue>::from_dynamic(&(*value).inner) {
                Ok(v) => {
                    *out = v;
                    INT2DDS_RET_OK
                }
                Err(_) => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
            }
        }
    };
}

value_as!(int2dds_dynamic_value_as_bool, bool);
value_as!(int2dds_dynamic_value_as_i8, i8);
value_as!(int2dds_dynamic_value_as_i16, i16);
value_as!(int2dds_dynamic_value_as_i32, i32);
value_as!(int2dds_dynamic_value_as_i64, i64);
value_as!(int2dds_dynamic_value_as_u8, u8);
value_as!(int2dds_dynamic_value_as_u16, u16);
value_as!(int2dds_dynamic_value_as_u32, u32);
value_as!(int2dds_dynamic_value_as_u64, u64);
value_as!(int2dds_dynamic_value_as_f32, f32);
value_as!(int2dds_dynamic_value_as_f64, f64);

/// Read a char8 value as its byte value.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_as_char8(
    value: *const Int2DdsDynamicValue,
    out: *mut u8,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match <char as FromDynamicValue>::from_dynamic(&(*value).inner) {
        Ok(c) => {
            *out = u8::try_from(u32::from(c)).unwrap_or(0);
            INT2DDS_RET_OK
        }
        Err(_) => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Read a string or wide-string value into `buf`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_as_string(
    value: *const Int2DdsDynamicValue,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
) -> Int2DdsRet {
    check_null!(value);
    match &(*value).inner {
        DynamicValue::String(s) | DynamicValue::WString(s) => {
            copy_str_to_c(s, buf, buf_len, out_len)
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Read an enum value's literal name into `buf` and its numeric value into
/// `out_value`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_as_enum(
    value: *const Int2DdsDynamicValue,
    buf: *mut c_char,
    buf_len: usize,
    out_len: *mut usize,
    out_value: *mut i32,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out_value);
    match &(*value).inner {
        DynamicValue::Enum { name, value: v } => {
            *out_value = *v;
            copy_str_to_c(name, buf, buf_len, out_len)
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Read a bitmask value's packed bits.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_as_bitmask(
    value: *const Int2DdsDynamicValue,
    out: *mut u64,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Bitmask(v) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Read a bitset value's packed bitfields.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_as_bitset(
    value: *const Int2DdsDynamicValue,
    out: *mut u64,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Bitset(v) => {
            *out = *v;
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Element count of a sequence/array/map value.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_len(
    value: *const Int2DdsDynamicValue,
    out: *mut usize,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Sequence(items) | DynamicValue::Array(items) => {
            *out = items.len();
            INT2DDS_RET_OK
        }
        DynamicValue::Map(pairs) => {
            *out = pairs.len();
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Clone the element at `index` of a sequence/array value.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_element(
    value: *const Int2DdsDynamicValue,
    index: usize,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    let items = match &(*value).inner {
        DynamicValue::Sequence(items) | DynamicValue::Array(items) => items,
        _ => return INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    };
    match items.get(index) {
        Some(v) => {
            *out = into_handle(v.clone());
            INT2DDS_RET_OK
        }
        None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
    }
}

/// Clone the key of the map entry at `index`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_map_key(
    value: *const Int2DdsDynamicValue,
    index: usize,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Map(pairs) => match pairs.get(index) {
            Some((k, _)) => {
                *out = into_handle(k.clone());
                INT2DDS_RET_OK
            }
            None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
        },
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Clone the value of the map entry at `index`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_map_value(
    value: *const Int2DdsDynamicValue,
    index: usize,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Map(pairs) => match pairs.get(index) {
            Some((_, v)) => {
                *out = into_handle(v.clone());
                INT2DDS_RET_OK
            }
            None => INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND,
        },
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Clone a nested struct value into a new DynamicData handle. Destroy it with
/// `int2dds_dynamic_data_destroy`.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_as_struct(
    value: *const Int2DdsDynamicValue,
    out: *mut *mut Int2DdsDynamicData,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Struct(inner) => {
            *out = Box::into_raw(Box::new(Int2DdsDynamicData { inner: (**inner).clone() }));
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Clone a union value's discriminator into a new value handle.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_union_discriminator(
    value: *const Int2DdsDynamicValue,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Union { discriminator, .. } => {
            *out = into_handle((**discriminator).clone());
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}

/// Clone a union value's selected branch value into a new value handle.
#[no_mangle]
pub unsafe extern "C" fn int2dds_dynamic_value_union_value(
    value: *const Int2DdsDynamicValue,
    out: *mut *mut Int2DdsDynamicValue,
) -> Int2DdsRet {
    check_null!(value);
    check_null!(out);
    match &(*value).inner {
        DynamicValue::Union { value: v, .. } => {
            *out = into_handle((**v).clone());
            INT2DDS_RET_OK
        }
        _ => INT2DDS_RET_DYNAMIC_TYPE_MISMATCH,
    }
}
