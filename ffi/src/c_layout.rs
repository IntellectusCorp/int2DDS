//! C offset-layout descriptors: the topic-anchored codec that serializes C
//! struct memory straight to CDR and back, without generated per-field code.
//!
//! Generated headers describe each type once as a static [`Int2DdsCTypeLayout`]
//! table (offsets from `offsetof`, sizes from `sizeof`) and bind it with
//! [`int2dds_topic_bind_c_layout`]. The kernel validates the table against the
//! type's compiled codec plan; after that, [`int2dds_topic_c_encode`] /
//! [`int2dds_topic_c_decode`] move whole samples across the FFI in one call each.

use std::os::raw::{c_char, c_void};
use std::ptr::addr_of;
use std::sync::Arc;

use int2dds::xtypes::{BoundCLayout, CElemSpec, CFieldSpec, CNodeSpec, CScalarKind, CStructSpec};

use super::error::*;
use super::types::Int2DdsTopic;

/// Field kinds for [`Int2DdsCFieldLayout`].
///
/// `i32` for the same reason as `Int2DdsQosPolicyId`: the width is part of a
/// struct layout and must not move under `-fshort-enums`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Int2DdsCFieldKind {
    CFieldNone = 0,
    CFieldBool = 1,
    CFieldInt8 = 2,
    CFieldUInt8 = 3,
    CFieldInt16 = 4,
    CFieldUInt16 = 5,
    CFieldInt32 = 6,
    CFieldUInt32 = 7,
    CFieldInt64 = 8,
    CFieldUInt64 = 9,
    CFieldFloat32 = 10,
    CFieldFloat64 = 11,
    CFieldChar8 = 12,
    /// C enum member: `size` is its `sizeof` (implementation defined); the wire
    /// width comes from the type's `bit_bound`.
    CFieldEnum = 13,
    /// Bitmask member: an unsigned integer of `size` bytes.
    CFieldBitmask = 14,
    /// Inline `char [size]`, NUL-terminated (`size` includes the NUL).
    CFieldString = 15,
    /// Pointer-mode `char *`.
    CFieldStringPtr = 16,
    /// Inline `uint16_t [size]`, NUL-terminated UTF-16.
    CFieldWString = 17,
    /// Nested struct; `nested` points at its table.
    CFieldStruct = 18,
    /// Inline array; `count` is the flattened element count.
    CFieldArray = 19,
    /// Bounded `{ T data[count]; uint32_t length; }`: `offset` is `data`,
    /// `length_offset` is `length`, `count` is the bound.
    CFieldSequence = 20,
    /// Unbounded `{ T *data; uint32_t length; }`: `offset` is the pointer.
    CFieldSequencePtr = 21,
}

fn kind_from_raw(raw: i32) -> Option<Int2DdsCFieldKind> {
    use Int2DdsCFieldKind::*;
    Some(match raw {
        0 => CFieldNone,
        1 => CFieldBool,
        2 => CFieldInt8,
        3 => CFieldUInt8,
        4 => CFieldInt16,
        5 => CFieldUInt16,
        6 => CFieldInt32,
        7 => CFieldUInt32,
        8 => CFieldInt64,
        9 => CFieldUInt64,
        10 => CFieldFloat32,
        11 => CFieldFloat64,
        12 => CFieldChar8,
        13 => CFieldEnum,
        14 => CFieldBitmask,
        15 => CFieldString,
        16 => CFieldStringPtr,
        17 => CFieldWString,
        18 => CFieldStruct,
        19 => CFieldArray,
        20 => CFieldSequence,
        21 => CFieldSequencePtr,
        _ => return None,
    })
}

fn scalar_kind(kind: Int2DdsCFieldKind) -> Option<CScalarKind> {
    use Int2DdsCFieldKind as K;
    Some(match kind {
        K::CFieldBool => CScalarKind::Bool,
        K::CFieldInt8 => CScalarKind::I8,
        K::CFieldUInt8 => CScalarKind::U8,
        K::CFieldInt16 => CScalarKind::I16,
        K::CFieldUInt16 => CScalarKind::U16,
        K::CFieldInt32 => CScalarKind::I32,
        K::CFieldUInt32 => CScalarKind::U32,
        K::CFieldInt64 => CScalarKind::I64,
        K::CFieldUInt64 => CScalarKind::U64,
        K::CFieldFloat32 => CScalarKind::F32,
        K::CFieldFloat64 => CScalarKind::F64,
        K::CFieldChar8 => CScalarKind::Char8,
        K::CFieldEnum => CScalarKind::Enum,
        K::CFieldBitmask => CScalarKind::Packed,
        _ => return None,
    })
}

/// One member of a C struct, located by `offsetof`/`sizeof`.
///
/// Which of the other fields are meaningful depends on `kind`; see the
/// [`Int2DdsCFieldKind`] variants. `elem_kind`/`elem_size` describe an
/// array/sequence element the same way `kind`/`size` describe a member, and
/// `nested` carries the table of a struct member or struct element.
#[repr(C)]
pub struct Int2DdsCFieldLayout {
    pub name: *const c_char,
    pub kind: Int2DdsCFieldKind,
    pub offset: u32,
    pub length_offset: u32,
    pub size: u32,
    pub count: u32,
    pub elem_kind: Int2DdsCFieldKind,
    pub elem_size: u32,
    pub nested: *const Int2DdsCTypeLayout,
}

/// A C struct's complete offset layout, members in the type's declaration order
/// (ancestors first when the IDL type inherits).
#[repr(C)]
pub struct Int2DdsCTypeLayout {
    pub type_name: *const c_char,
    pub struct_size: u32,
    pub field_count: u32,
    pub fields: *const Int2DdsCFieldLayout,
}

const MAX_NESTING: u32 = 32;
const MAX_FIELDS: u32 = 0x10000;

unsafe fn read_name(name: *const c_char) -> Result<String, Int2DdsRet> {
    if name.is_null() {
        return Err(INT2DDS_RET_NULL_POINTER);
    }
    std::ffi::CStr::from_ptr(name)
        .to_str()
        .map(str::to_owned)
        .map_err(|_| INT2DDS_RET_INVALID_ARGUMENT)
}

/// The kind slots are read as raw `i32` (never as the enum type) so an
/// out-of-range value from C is a clean error instead of undefined behavior.
unsafe fn read_kind(slot: *const Int2DdsCFieldKind) -> Result<Int2DdsCFieldKind, Int2DdsRet> {
    kind_from_raw(slot.cast::<i32>().read()).ok_or(INT2DDS_RET_INVALID_ARGUMENT)
}

unsafe fn parse_elem(
    kind_slot: *const Int2DdsCFieldKind,
    elem_size: u32,
    nested: *const Int2DdsCTypeLayout,
    depth: u32,
) -> Result<CElemSpec, Int2DdsRet> {
    let kind = read_kind(kind_slot)?;
    if kind == Int2DdsCFieldKind::CFieldStruct {
        return Ok(CElemSpec::Struct(Arc::new(parse_struct(nested, depth + 1)?)));
    }
    Ok(match kind {
        Int2DdsCFieldKind::CFieldString => CElemSpec::StrInline { capacity: elem_size },
        Int2DdsCFieldKind::CFieldStringPtr => CElemSpec::StrPtr,
        Int2DdsCFieldKind::CFieldWString => CElemSpec::WStrInline { capacity: elem_size },
        other => match scalar_kind(other) {
            Some(kind) => CElemSpec::Scalar { kind, size: elem_size },
            None => return Err(INT2DDS_RET_INVALID_ARGUMENT),
        },
    })
}

unsafe fn parse_struct(
    layout: *const Int2DdsCTypeLayout,
    depth: u32,
) -> Result<CStructSpec, Int2DdsRet> {
    if layout.is_null() {
        return Err(INT2DDS_RET_NULL_POINTER);
    }
    if depth > MAX_NESTING {
        return Err(INT2DDS_RET_INVALID_ARGUMENT);
    }
    let type_name = read_name(addr_of!((*layout).type_name).read())?;
    let struct_size = addr_of!((*layout).struct_size).read();
    let field_count = addr_of!((*layout).field_count).read();
    let fields_ptr = addr_of!((*layout).fields).read();
    if field_count > MAX_FIELDS {
        return Err(INT2DDS_RET_INVALID_ARGUMENT);
    }
    if field_count > 0 && fields_ptr.is_null() {
        return Err(INT2DDS_RET_NULL_POINTER);
    }

    let mut fields = Vec::with_capacity(field_count as usize);
    for i in 0..field_count as usize {
        let f = fields_ptr.add(i);
        let name = read_name(addr_of!((*f).name).read())?;
        let kind = read_kind(addr_of!((*f).kind))?;
        let offset = addr_of!((*f).offset).read();
        let length_offset = addr_of!((*f).length_offset).read();
        let size = addr_of!((*f).size).read();
        let count = addr_of!((*f).count).read();
        let elem_size = addr_of!((*f).elem_size).read();
        let nested = addr_of!((*f).nested).read();

        use Int2DdsCFieldKind as K;
        let node = match kind {
            K::CFieldString => CNodeSpec::StrInline { capacity: size },
            K::CFieldStringPtr => CNodeSpec::StrPtr,
            K::CFieldWString => CNodeSpec::WStrInline { capacity: size },
            K::CFieldStruct => CNodeSpec::Struct(Arc::new(parse_struct(nested, depth + 1)?)),
            K::CFieldArray => CNodeSpec::Array {
                elem: parse_elem(addr_of!((*f).elem_kind), elem_size, nested, depth)?,
                count,
            },
            K::CFieldSequence => {
                if count == 0 {
                    return Err(INT2DDS_RET_INVALID_ARGUMENT);
                }
                CNodeSpec::Seq {
                    elem: parse_elem(addr_of!((*f).elem_kind), elem_size, nested, depth)?,
                    length_offset,
                    bound: Some(count),
                }
            }
            K::CFieldSequencePtr => CNodeSpec::Seq {
                elem: parse_elem(addr_of!((*f).elem_kind), elem_size, nested, depth)?,
                length_offset,
                bound: None,
            },
            other => match scalar_kind(other) {
                Some(kind) => CNodeSpec::Scalar { kind, size },
                None => return Err(INT2DDS_RET_INVALID_ARGUMENT),
            },
        };
        fields.push(CFieldSpec { name, offset, node });
    }
    Ok(CStructSpec { type_name, size: struct_size, fields })
}

/// Bind a C offset layout to the topic's type.
///
/// Validates `layout` member by member against the type's compiled codec plan
/// (names, order, widths, collection shapes) and stores the bound result on the
/// topic. Binding is once per topic: later calls on an already-bound topic
/// return `INT2DDS_RET_OK` without re-validating, so a generated writer and
/// reader may both bind the same table.
///
/// Returns `INT2DDS_RET_UNSUPPORTED` when the topic has no compiled plan
/// (created without a full TypeObject) or the type/layout has a shape this path
/// does not cover — the caller then keeps its inline generated codec.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `layout` must point to a fully initialized layout table whose `name`,
///   `fields`, and `nested` pointers stay valid for the duration of this call
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_bind_c_layout(
    topic: *const Int2DdsTopic,
    layout: *const Int2DdsCTypeLayout,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(layout);

    let topic_ref = &*topic;
    if topic_ref.c_layout.get().is_some() {
        return INT2DDS_RET_OK;
    }
    let plans = match &topic_ref.plans {
        Some(plans) => plans,
        None => return INT2DDS_RET_UNSUPPORTED,
    };
    let spec = match parse_struct(layout, 0) {
        Ok(spec) => spec,
        Err(code) => return code,
    };
    match BoundCLayout::bind(plans, &spec) {
        Ok(bound) => {
            let _ = topic_ref.c_layout.set(Arc::new(bound));
            INT2DDS_RET_OK
        }
        Err(_) => INT2DDS_RET_UNSUPPORTED,
    }
}

/// Serialize the C struct at `sample` into CDR bytes (with encapsulation
/// header), byte-identical to the generated inline codec for the same value.
/// `xcdr2` selects the representation; pass the writer's effective one
/// (`int2dds_datawriter_data_representation`).
///
/// On success copies the bytes into `buffer` and sets `actual_size_out`. When
/// the buffer is too small, returns `INT2DDS_RET_BUFFER_TOO_SMALL` with the
/// required size in `actual_size_out` so a retry can succeed. Returns
/// `INT2DDS_RET_PRECONDITION_NOT_MET` before `int2dds_topic_bind_c_layout`.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `sample` must point to a live, initialized value of the bound C layout;
///   its pointer members must be valid for their stated lengths
/// - `buffer` must point to at least `buffer_capacity` writable bytes
/// - `actual_size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_c_encode(
    topic: *const Int2DdsTopic,
    sample: *const c_void,
    xcdr2: bool,
    buffer: *mut u8,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(sample);
    check_null!(buffer);
    check_null!(actual_size_out);

    let bound = match (*topic).c_layout.get() {
        Some(bound) => bound,
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };
    let bytes = ffi_try!(bound.encode(sample as *const u8, xcdr2));
    *actual_size_out = bytes.len();
    if bytes.len() > buffer_capacity {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
    INT2DDS_RET_OK
}

/// Deserialize CDR sample bytes into the caller's buffer as a C struct.
///
/// `buffer[0..struct_size]` receives the struct; variable-sized content
/// (pointer-mode strings, unbounded sequence data) is appended behind it and the
/// struct's pointer members point into `buffer` itself. One buffer, freed once,
/// by the caller — never call the generated `{T}_cleanup` on a sample decoded
/// this way, its pointers do not come from `malloc`.
///
/// `actual_size_out` always receives the total size required; when it exceeds
/// `buffer_capacity` the content is unspecified and the return is
/// `INT2DDS_RET_BUFFER_TOO_SMALL` so a retry can succeed. Returns
/// `INT2DDS_RET_PRECONDITION_NOT_MET` before `int2dds_topic_bind_c_layout`.
///
/// # Safety
/// - `topic` must be a valid topic
/// - `data` must point to at least `data_len` readable bytes
/// - `buffer` must point to at least `buffer_capacity` writable bytes
/// - `actual_size_out` must be a valid pointer
#[no_mangle]
pub unsafe extern "C" fn int2dds_topic_c_decode(
    topic: *const Int2DdsTopic,
    data: *const u8,
    data_len: usize,
    buffer: *mut c_void,
    buffer_capacity: usize,
    actual_size_out: *mut usize,
) -> Int2DdsRet {
    check_null!(topic);
    check_null!(data);
    check_null!(buffer);
    check_null!(actual_size_out);

    let bound = match (*topic).c_layout.get() {
        Some(bound) => bound,
        None => return INT2DDS_RET_PRECONDITION_NOT_MET,
    };
    let bytes = std::slice::from_raw_parts(data, data_len);
    let out = std::slice::from_raw_parts_mut(buffer as *mut u8, buffer_capacity);
    let needed = ffi_try!(bound.decode(bytes, out));
    *actual_size_out = needed;
    if needed > buffer_capacity {
        return INT2DDS_RET_BUFFER_TOO_SMALL;
    }
    INT2DDS_RET_OK
}
