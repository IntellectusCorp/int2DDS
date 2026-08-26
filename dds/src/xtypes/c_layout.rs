//! C offset-descriptor codec — CDR encode/decode straight against C struct
//! memory, driven by the compiled codec plan.
//!
//! The FFI hands in a [`CStructSpec`] describing where each member sits in the
//! caller's C struct (offsets, sizes, string mode, collection shape). [`BoundCLayout::bind`]
//! zips that against the type's [`TypePlans`], validating name-by-name, and the
//! result encodes/decodes without materializing `DynamicData`: reads reuse the
//! plan's wire walk, writes emit through the same serializer helpers the dynamic
//! codec uses, so the framing rules stay owned where they already live.
//!
//! Decode ownership: the caller supplies one buffer. `buf[0..struct_size]` is the
//! struct; variable-sized content (pointer-mode strings, unbounded sequences) is
//! appended behind it and the struct's pointer members are fixed up to point into
//! the same buffer. One allocation, one free, by the caller's allocator — which is
//! also why `{T}_cleanup` must never run on a decoded sample.
//!
//! Coverage is the plan's minus C representation limits: structs of scalars,
//! enums, bitmasks, strings (inline or pointer), inline wstrings, nested structs,
//! and arrays/sequences whose element is one of those. Not covered (bind fails,
//! caller keeps the inline codec): optional members, collections whose element is
//! itself a collection, and every shape the plan itself declines.

use std::sync::Arc;

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::serialize::cdr::{
    CdrDeserializer, CdrError, CdrSerializer, ExtensibilityKind, PrimitiveSerialize,
    StringSerialize, Xcdr2Deserializer, Xcdr2Serializer,
};
use crate::serialize::BufferManager;

use super::codec_plan::{
    is_xcdr2, locate_members, Framing, Node, PlanReader, StructNode, TypePlans,
};
use super::dynamic_type::PrimitiveKind;

// ============================================================================
// The FFI-facing layout description
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CScalarKind {
    Bool,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Char8,
    /// C enum: memory width is `size` (implementation defined), wire width is the
    /// plan's `bit_bound`.
    Enum,
    /// Bitmask: an unsigned integer of `size` bytes in memory.
    Packed,
}

#[derive(Debug, Clone)]
pub enum CElemSpec {
    Scalar {
        kind: CScalarKind,
        size: u32,
    },
    /// `char [capacity]` per element, NUL-terminated.
    StrInline {
        capacity: u32,
    },
    /// `char *` per element.
    StrPtr,
    /// `uint16_t [capacity]` per element, NUL-terminated UTF-16.
    WStrInline {
        capacity: u32,
    },
    Struct(Arc<CStructSpec>),
}

#[derive(Debug, Clone)]
pub enum CNodeSpec {
    Scalar {
        kind: CScalarKind,
        size: u32,
    },
    StrInline {
        capacity: u32,
    },
    StrPtr,
    WStrInline {
        capacity: u32,
    },
    Struct(Arc<CStructSpec>),
    /// Inline elements at the field offset, multi-dimensional counts flattened.
    Array {
        elem: CElemSpec,
        count: u32,
    },
    /// `bound: Some(n)` — `{ T data[n]; uint32_t length; }`, data inline at the
    /// field offset. `bound: None` — `{ T *data; uint32_t length; }`, pointer at
    /// the field offset.
    Seq {
        elem: CElemSpec,
        length_offset: u32,
        bound: Option<u32>,
    },
}

#[derive(Debug, Clone)]
pub struct CFieldSpec {
    pub name: String,
    pub offset: u32,
    pub node: CNodeSpec,
}

#[derive(Debug, Clone)]
pub struct CStructSpec {
    pub type_name: String,
    pub size: u32,
    pub fields: Vec<CFieldSpec>,
}

// ============================================================================
// Bound layout
// ============================================================================

struct CStruct {
    size: u32,
    /// Parallel to the plan `StructNode`'s members, index for index.
    members: Vec<CMember>,
}

struct CMember {
    offset: u32,
    op: COp,
}

/// Scalars keep only their memory width past binding: the kind is fully
/// validated there and the wire form always comes from the plan node.
enum CElemShape {
    Scalar(u32),
    StrInline(u32),
    StrPtr,
    WStrInline(u32),
    Struct(CStruct),
}

struct CElem {
    shape: CElemShape,
    stride: u32,
}

enum COp {
    Scalar(u32),
    StrInline(u32),
    StrPtr,
    WStrInline(u32),
    Struct(CStruct),
    Array { elem: CElem, count: u32 },
    Seq { elem: CElem, length_offset: u32, bound: Option<u32> },
}

/// A C layout validated against — and paired with — a type's codec plans.
pub struct BoundCLayout {
    x1: Arc<StructNode>,
    x2: Arc<StructNode>,
    c: CStruct,
}

fn err(message: impl Into<String>) -> DdsError {
    DdsError::Error(message.into())
}

fn cdr_error(error: CdrError) -> DdsError {
    DdsError::Error(error.to_string())
}

const PTR_SIZE: u32 = std::mem::size_of::<usize>() as u32;

impl BoundCLayout {
    pub fn bind(plans: &TypePlans, spec: &CStructSpec) -> DdsResult<BoundCLayout> {
        let (x1, x2) = match (&plans.xcdr1, &plans.xcdr2) {
            (Some(x1), Some(x2)) => (x1.root.clone(), x2.root.clone()),
            _ => return Err(err("no codec plan for this type; the C layout path cannot cover it")),
        };
        // Both plans compile the same member structure; validating against one
        // validates the other.
        let c = zip_struct(&x2, spec)?;
        Ok(BoundCLayout { x1, x2, c })
    }

    pub fn struct_size(&self) -> u32 {
        self.c.size
    }

    /// Serialize the C value at `src` (with encapsulation header), byte-identical
    /// to the type's canonical wire form.
    ///
    /// # Safety
    /// `src` must point to a live, initialized value of the exact C layout this
    /// was bound with; every pointer member (pointer-mode strings, unbounded
    /// sequence data) must be valid for its stated length.
    pub unsafe fn encode(&self, src: *const u8, xcdr2: bool) -> DdsResult<Vec<u8>> {
        if xcdr2 {
            let extensibility = match self.x2.framing {
                Framing::TaggedDelimited => ExtensibilityKind::Mutable,
                Framing::Delimited => ExtensibilityKind::Appendable,
                _ => ExtensibilityKind::Final,
            };
            let mut ser = Xcdr2Serializer::with_capacity(true, extensibility, 128);
            ser.write_encapsulation_header().map_err(cdr_error)?;
            encode_struct_x2(&mut ser, &self.x2, &self.c, src)?;
            Ok(ser.into_bytes())
        } else {
            // Mutable XCDR1 is PL_CDR (0x0003), matching the derive and generated-C
            // writers; everything else is plain CDR (0x0001).
            let extensibility = if self.x1.framing == Framing::TaggedSentinel {
                ExtensibilityKind::Mutable
            } else {
                ExtensibilityKind::Appendable
            };
            let mut ser = CdrSerializer::with_extensibility(true, extensibility);
            ser.write_encapsulation_header().map_err(cdr_error)?;
            encode_struct_x1(&mut ser, &self.x1, &self.c, src)?;
            Ok(ser.into_bytes())
        }
    }

    /// Deserialize `bytes` into `out`: the struct at `out[0..struct_size]`,
    /// variable-sized content appended behind it, pointer members fixed up into
    /// `out` itself. Returns the total size required; when that exceeds
    /// `out.len()` the content is unspecified and the caller retries with a
    /// buffer of the returned size.
    pub fn decode(&self, bytes: &[u8], out: &mut [u8]) -> DdsResult<usize> {
        out.fill(0);
        let mut ob = OutBuf { buf: out, needed: self.c.size as usize };
        if is_xcdr2(bytes) {
            let mut reader = Xcdr2Deserializer::new(bytes).map_err(cdr_error)?;
            decode_struct(&mut reader, &self.x2, &self.c, &mut ob, 0)?;
        } else {
            let mut reader = CdrDeserializer::new(bytes).map_err(cdr_error)?;
            decode_struct(&mut reader, &self.x1, &self.c, &mut ob, 0)?;
        }
        Ok(ob.needed)
    }
}

// ============================================================================
// Binding (zip + validation)
// ============================================================================

fn scalar_matches(prim: PrimitiveKind, kind: CScalarKind, size: u32) -> bool {
    use CScalarKind as K;
    use PrimitiveKind as P;
    let want = match prim {
        P::Boolean => (K::Bool, 1),
        P::Int8 => (K::I8, 1),
        P::Uint8 | P::Byte => (K::U8, 1),
        P::Char8 => (K::Char8, 1),
        P::Int16 => (K::I16, 2),
        P::Uint16 => (K::U16, 2),
        P::Int32 => (K::I32, 4),
        P::Uint32 => (K::U32, 4),
        P::Int64 => (K::I64, 8),
        P::Uint64 => (K::U64, 8),
        P::Float32 => (K::F32, 4),
        P::Float64 => (K::F64, 8),
        P::Char16 | P::Float128 => return false,
    };
    (kind, size) == want
}

fn integer_width_ok(size: u32, wire_width: u8) -> bool {
    matches!(size, 1 | 2 | 4 | 8) && size >= wire_width as u32
}

fn zip_struct(plan: &StructNode, spec: &CStructSpec) -> DdsResult<CStruct> {
    if plan.members.len() != spec.fields.len() {
        return Err(err(format!(
            "'{}': plan has {} members, C layout describes {}",
            spec.type_name,
            plan.members.len(),
            spec.fields.len()
        )));
    }
    let mut members = Vec::with_capacity(spec.fields.len());
    for (member, field) in plan.members.iter().zip(&spec.fields) {
        if &*member.name != field.name.as_str() {
            return Err(err(format!(
                "'{}': member '{}' does not match C field '{}' (order and names must agree)",
                spec.type_name, member.name, field.name
            )));
        }
        if member.optional {
            return Err(err(format!(
                "'{}.{}': optional members have no C representation",
                spec.type_name, member.name
            )));
        }
        let op = zip_node(&member.node, &field.node)
            .map_err(|e| err(format!("'{}.{}': {}", spec.type_name, member.name, e)))?;
        check_extent(&op, field.offset, spec.size)
            .map_err(|e| err(format!("'{}.{}': {}", spec.type_name, member.name, e)))?;
        members.push(CMember { offset: field.offset, op });
    }
    Ok(CStruct { size: spec.size, members })
}

fn zip_node(plan: &Node, spec: &CNodeSpec) -> DdsResult<COp> {
    Ok(match (plan, spec) {
        (Node::Prim(prim), CNodeSpec::Scalar { kind, size }) => {
            if !scalar_matches(*prim, *kind, *size) {
                return Err(err(format!("C scalar {kind:?}/{size} does not match {prim:?}")));
            }
            COp::Scalar(*size)
        }
        (Node::Enum { width }, CNodeSpec::Scalar { kind: CScalarKind::Enum, size }) => {
            if !integer_width_ok(*size, *width) {
                return Err(err(format!(
                    "enum memory width {size} cannot hold wire width {width}"
                )));
            }
            COp::Scalar(*size)
        }
        (Node::Packed { width }, CNodeSpec::Scalar { kind: CScalarKind::Packed, size }) => {
            if !integer_width_ok(*size, *width) {
                return Err(err(format!(
                    "bitmask memory width {size} cannot hold wire width {width}"
                )));
            }
            COp::Scalar(*size)
        }
        (Node::Str, CNodeSpec::StrInline { capacity }) if *capacity >= 1 => {
            COp::StrInline(*capacity)
        }
        (Node::Str, CNodeSpec::StrPtr) => COp::StrPtr,
        (Node::WStr, CNodeSpec::WStrInline { capacity }) if *capacity >= 1 => {
            COp::WStrInline(*capacity)
        }
        (Node::Struct(child), CNodeSpec::Struct(nested)) => COp::Struct(zip_struct(child, nested)?),
        (Node::Array { elem, count, .. }, CNodeSpec::Array { elem: celem, count: ccount }) => {
            if count != ccount {
                return Err(err(format!("array count {ccount} does not match the type's {count}")));
            }
            COp::Array { elem: zip_elem(elem, celem)?, count: *count }
        }
        (Node::Seq { elem, .. }, CNodeSpec::Seq { elem: celem, length_offset, bound }) => {
            COp::Seq { elem: zip_elem(elem, celem)?, length_offset: *length_offset, bound: *bound }
        }
        _ => return Err(err("C field shape does not match the type's member")),
    })
}

fn zip_elem(plan: &Node, spec: &CElemSpec) -> DdsResult<CElem> {
    let shape = match (plan, spec) {
        (Node::Prim(prim), CElemSpec::Scalar { kind, size }) => {
            if !scalar_matches(*prim, *kind, *size) {
                return Err(err(format!("C element {kind:?}/{size} does not match {prim:?}")));
            }
            CElemShape::Scalar(*size)
        }
        (Node::Enum { width }, CElemSpec::Scalar { kind: CScalarKind::Enum, size }) => {
            if !integer_width_ok(*size, *width) {
                return Err(err(format!(
                    "enum memory width {size} cannot hold wire width {width}"
                )));
            }
            CElemShape::Scalar(*size)
        }
        (Node::Packed { width }, CElemSpec::Scalar { kind: CScalarKind::Packed, size }) => {
            if !integer_width_ok(*size, *width) {
                return Err(err(format!(
                    "bitmask memory width {size} cannot hold wire width {width}"
                )));
            }
            CElemShape::Scalar(*size)
        }
        (Node::Str, CElemSpec::StrInline { capacity }) if *capacity >= 1 => {
            CElemShape::StrInline(*capacity)
        }
        (Node::Str, CElemSpec::StrPtr) => CElemShape::StrPtr,
        (Node::WStr, CElemSpec::WStrInline { capacity }) if *capacity >= 1 => {
            CElemShape::WStrInline(*capacity)
        }
        (Node::Struct(child), CElemSpec::Struct(nested)) => {
            CElemShape::Struct(zip_struct(child, nested)?)
        }
        (Node::Seq { .. } | Node::Array { .. }, _) => {
            return Err(err(
                "collection elements that are themselves collections are not covered; \
                 keep the inline codec for this type",
            ))
        }
        _ => return Err(err("C element shape does not match the type's element")),
    };
    let stride = match &shape {
        CElemShape::Scalar(size) => *size,
        CElemShape::StrInline(capacity) => *capacity,
        CElemShape::StrPtr => PTR_SIZE,
        CElemShape::WStrInline(capacity) => 2 * *capacity,
        CElemShape::Struct(nested) => nested.size,
    };
    if stride == 0 {
        return Err(err("element stride resolves to zero"));
    }
    Ok(CElem { shape, stride })
}

/// Bytes the op occupies at its field offset, so every inline access below is
/// provably inside `struct_size`.
fn check_extent(op: &COp, offset: u32, struct_size: u32) -> DdsResult<()> {
    let extent: u64 = match op {
        COp::Scalar(size) => *size as u64,
        COp::StrInline(capacity) => *capacity as u64,
        COp::StrPtr => PTR_SIZE as u64,
        COp::WStrInline(capacity) => 2 * *capacity as u64,
        COp::Struct(nested) => nested.size as u64,
        COp::Array { elem, count } => elem.stride as u64 * *count as u64,
        COp::Seq { elem, length_offset, bound } => {
            if *length_offset as u64 + 4 > struct_size as u64 {
                return Err(err("sequence length member lies outside the struct"));
            }
            match bound {
                Some(bound) => elem.stride as u64 * *bound as u64,
                None => PTR_SIZE as u64,
            }
        }
    };
    if offset as u64 + extent > struct_size as u64 {
        return Err(err(format!(
            "field at offset {offset} with extent {extent} lies outside struct size {struct_size}"
        )));
    }
    Ok(())
}

// ============================================================================
// Reading C memory (encode direction)
// ============================================================================

fn read_raw(p: *const u8, len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    unsafe { std::ptr::copy_nonoverlapping(p, out.as_mut_ptr(), len) };
    out
}

fn read_array<const N: usize>(p: *const u8) -> [u8; N] {
    let mut out = [0u8; N];
    unsafe { std::ptr::copy_nonoverlapping(p, out.as_mut_ptr(), N) };
    out
}

fn read_u32_native(p: *const u8) -> u32 {
    u32::from_ne_bytes(read_array(p))
}

fn read_uint(p: *const u8, size: u32) -> u64 {
    match size {
        1 => read_array::<1>(p)[0] as u64,
        2 => u16::from_ne_bytes(read_array(p)) as u64,
        4 => u32::from_ne_bytes(read_array(p)) as u64,
        _ => u64::from_ne_bytes(read_array(p)),
    }
}

fn read_int(p: *const u8, size: u32) -> i64 {
    match size {
        1 => read_array::<1>(p)[0] as i8 as i64,
        2 => i16::from_ne_bytes(read_array(p)) as i64,
        4 => i32::from_ne_bytes(read_array(p)) as i64,
        _ => i64::from_ne_bytes(read_array(p)),
    }
}

fn read_ptr(p: *const u8) -> *const u8 {
    usize::from_ne_bytes(read_array(p)) as *const u8
}

fn read_inline_str(p: *const u8, capacity: u32) -> DdsResult<String> {
    let bytes = read_raw(p, capacity as usize);
    let nul = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| err("inline string is not NUL-terminated within its capacity"))?;
    String::from_utf8(bytes[..nul].to_vec()).map_err(|_| err("string member is not valid UTF-8"))
}

fn read_ptr_str(p: *const u8) -> DdsResult<String> {
    let target = read_ptr(p);
    if target.is_null() {
        // A NULL string pointer writes as the empty string, the C convention the
        // generated cleanup leaves behind.
        return Ok(String::new());
    }
    let cstr = unsafe { std::ffi::CStr::from_ptr(target as *const std::os::raw::c_char) };
    cstr.to_str().map(str::to_owned).map_err(|_| err("string member is not valid UTF-8"))
}

fn read_inline_wstr(p: *const u8, capacity: u32) -> DdsResult<String> {
    let mut units = Vec::new();
    for i in 0..capacity {
        let unit = u16::from_ne_bytes(read_array(p.wrapping_add(2 * i as usize)));
        if unit == 0 {
            return String::from_utf16(&units)
                .map_err(|_| err("wstring member is not valid UTF-16"));
        }
        units.push(unit);
    }
    Err(err("inline wstring is not NUL-terminated within its capacity"))
}

// ============================================================================
// Encode
// ============================================================================

fn dds_to_cdr(error: DdsError) -> CdrError {
    CdrError::SerializationError(error.to_string())
}

fn encode_struct_x2(
    ser: &mut Xcdr2Serializer,
    plan: &StructNode,
    c: &CStruct,
    base: *const u8,
) -> DdsResult<()> {
    match plan.framing {
        Framing::Plain => encode_members_x2(ser, plan, c, base),
        Framing::Delimited => {
            let pos = ser.begin_struct().map_err(cdr_error)?;
            encode_members_x2(ser, plan, c, base)?;
            ser.end_struct(pos).map_err(cdr_error)
        }
        Framing::TaggedDelimited => {
            let pos = ser.begin_struct().map_err(cdr_error)?;
            for (member, cm) in plan.members.iter().zip(&c.members) {
                ser.write_member_with(member.member_id, member.must_understand, |s| {
                    encode_node_x2(s, &member.node, &cm.op, base, cm.offset).map_err(dds_to_cdr)
                })
                .map_err(cdr_error)?;
            }
            ser.end_struct(pos).map_err(cdr_error)
        }
        Framing::TaggedSentinel => Err(err("XCDR1 framing reached the XCDR2 encoder")),
    }
}

fn encode_members_x2(
    ser: &mut Xcdr2Serializer,
    plan: &StructNode,
    c: &CStruct,
    base: *const u8,
) -> DdsResult<()> {
    for (member, cm) in plan.members.iter().zip(&c.members) {
        encode_node_x2(ser, &member.node, &cm.op, base, cm.offset)?;
    }
    Ok(())
}

fn encode_struct_x1(
    ser: &mut CdrSerializer,
    plan: &StructNode,
    c: &CStruct,
    base: *const u8,
) -> DdsResult<()> {
    match plan.framing {
        Framing::Plain => {
            for (member, cm) in plan.members.iter().zip(&c.members) {
                encode_node_x1(ser, &member.node, &cm.op, base, cm.offset)?;
            }
            Ok(())
        }
        Framing::TaggedSentinel => {
            for (member, cm) in plan.members.iter().zip(&c.members) {
                ser.write_member_with_v1(member.member_id, member.must_understand, |s| {
                    encode_node_x1(s, &member.node, &cm.op, base, cm.offset).map_err(dds_to_cdr)
                })
                .map_err(cdr_error)?;
            }
            ser.end_mutable_struct().map_err(cdr_error)
        }
        _ => Err(err("XCDR2 framing reached the XCDR1 encoder")),
    }
}

/// The framing-free member shapes, shared by both representations. Scalars carry
/// only their memory width: the wire form comes from the plan node.
enum Atom<'a> {
    Scalar(u32),
    StrInline(u32),
    StrPtr,
    WStrInline(u32),
    Struct(&'a CStruct),
}

fn atom_of_op(op: &COp) -> Option<Atom<'_>> {
    Some(match op {
        COp::Scalar(size) => Atom::Scalar(*size),
        COp::StrInline(capacity) => Atom::StrInline(*capacity),
        COp::StrPtr => Atom::StrPtr,
        COp::WStrInline(capacity) => Atom::WStrInline(*capacity),
        COp::Struct(nested) => Atom::Struct(nested),
        COp::Array { .. } | COp::Seq { .. } => return None,
    })
}

fn atom_of_elem(elem: &CElemShape) -> Atom<'_> {
    match elem {
        CElemShape::Scalar(size) => Atom::Scalar(*size),
        CElemShape::StrInline(capacity) => Atom::StrInline(*capacity),
        CElemShape::StrPtr => Atom::StrPtr,
        CElemShape::WStrInline(capacity) => Atom::WStrInline(*capacity),
        CElemShape::Struct(nested) => Atom::Struct(nested),
    }
}

fn emit_prim<S: PrimitiveSerialize>(
    ser: &mut S,
    kind: PrimitiveKind,
    p: *const u8,
) -> DdsResult<()> {
    use PrimitiveKind as P;
    match kind {
        P::Boolean => ser.serialize_bool(read_array::<1>(p)[0] != 0),
        P::Int8 => ser.serialize_i8(read_array::<1>(p)[0] as i8),
        P::Uint8 | P::Byte | P::Char8 => ser.serialize_u8(read_array::<1>(p)[0]),
        P::Int16 => ser.serialize_i16(i16::from_ne_bytes(read_array(p))),
        P::Uint16 => ser.serialize_u16(u16::from_ne_bytes(read_array(p))),
        P::Int32 => ser.serialize_i32(i32::from_ne_bytes(read_array(p))),
        P::Uint32 => ser.serialize_u32(u32::from_ne_bytes(read_array(p))),
        P::Int64 => ser.serialize_i64(i64::from_ne_bytes(read_array(p))),
        P::Uint64 => ser.serialize_u64(u64::from_ne_bytes(read_array(p))),
        P::Float32 => ser.serialize_f32(f32::from_ne_bytes(read_array(p))),
        P::Float64 => ser.serialize_f64(f64::from_ne_bytes(read_array(p))),
        P::Char16 | P::Float128 => {
            return Err(err(format!("unsupported primitive in plan: {kind:?}")))
        }
    }
    .map_err(cdr_error)
}

/// Emit one framing-free value. `plan` decides the wire form (widths), `atom`
/// where it sits in C memory; `recurse` closes over the representation-specific
/// struct encoder.
fn encode_atom<S, F>(
    ser: &mut S,
    plan: &Node,
    atom: Atom<'_>,
    p: *const u8,
    recurse: F,
) -> DdsResult<()>
where
    S: PrimitiveSerialize + StringSerialize,
    F: FnOnce(&mut S, &StructNode, &CStruct, *const u8) -> DdsResult<()>,
{
    match (plan, atom) {
        (Node::Prim(kind), Atom::Scalar(_)) => emit_prim(ser, *kind, p),
        (Node::Enum { width }, Atom::Scalar(size)) => {
            let value = read_int(p, size);
            match width {
                1 => ser.serialize_i8(value as i8),
                2 => ser.serialize_i16(value as i16),
                _ => ser.serialize_i32(value as i32),
            }
            .map_err(cdr_error)
        }
        (Node::Packed { width }, Atom::Scalar(size)) => {
            let value = read_uint(p, size);
            match width {
                1 => ser.serialize_u8(value as u8),
                2 => ser.serialize_u16(value as u16),
                4 => ser.serialize_u32(value as u32),
                _ => ser.serialize_u64(value),
            }
            .map_err(cdr_error)
        }
        (Node::Str, Atom::StrInline(capacity)) => {
            ser.serialize_string(&read_inline_str(p, capacity)?).map_err(cdr_error)
        }
        (Node::Str, Atom::StrPtr) => ser.serialize_string(&read_ptr_str(p)?).map_err(cdr_error),
        (Node::WStr, Atom::WStrInline(capacity)) => {
            ser.serialize_wstring16(&read_inline_wstr(p, capacity)?).map_err(cdr_error)
        }
        (Node::Struct(child), Atom::Struct(nested)) => recurse(ser, child, nested, p),
        _ => Err(err("bound layout does not match the plan")),
    }
}

/// The sequence's length and element base, validated against the C capacity —
/// the write-side counterpart of the generated decoder's bound check.
fn seq_source(
    base: *const u8,
    offset: u32,
    length_offset: u32,
    bound: Option<u32>,
) -> DdsResult<(u32, *const u8)> {
    let length = read_u32_native(base.wrapping_add(length_offset as usize));
    let data = match bound {
        Some(bound) => {
            if length > bound {
                return Err(err(format!("sequence length {length} exceeds its bound {bound}")));
            }
            base.wrapping_add(offset as usize)
        }
        None => {
            let data = read_ptr(base.wrapping_add(offset as usize));
            if data.is_null() && length > 0 {
                return Err(err("sequence data pointer is NULL with a non-zero length"));
            }
            data
        }
    };
    Ok((length, data))
}

fn framed_x2<F>(ser: &mut Xcdr2Serializer, framed: bool, write: F) -> DdsResult<()>
where
    F: FnOnce(&mut Xcdr2Serializer) -> DdsResult<()>,
{
    if !framed {
        return write(ser);
    }
    let slot = ser.reserve_dheader();
    let start = ser.position();
    write(ser)?;
    let size = (ser.position() - start) as u32;
    ser.write_dheader_at(slot, size);
    Ok(())
}

fn encode_node_x2(
    ser: &mut Xcdr2Serializer,
    plan: &Node,
    op: &COp,
    base: *const u8,
    offset: u32,
) -> DdsResult<()> {
    let p = base.wrapping_add(offset as usize);
    match (plan, op) {
        (Node::Seq { elem, framed_in, .. }, COp::Seq { elem: celem, length_offset, bound }) => {
            let (length, data) = seq_source(base, offset, *length_offset, *bound)?;
            framed_x2(ser, *framed_in, |s| {
                s.serialize_u32(length).map_err(cdr_error)?;
                for i in 0..length {
                    let ep = data.wrapping_add(i as usize * celem.stride as usize);
                    encode_atom(s, elem, atom_of_elem(&celem.shape), ep, encode_struct_x2)?;
                }
                Ok(())
            })
        }
        (Node::Array { elem, count, framed_in, .. }, COp::Array { elem: celem, .. }) => {
            framed_x2(ser, *framed_in, |s| {
                for i in 0..*count {
                    let ep = p.wrapping_add(i as usize * celem.stride as usize);
                    encode_atom(s, elem, atom_of_elem(&celem.shape), ep, encode_struct_x2)?;
                }
                Ok(())
            })
        }
        _ => match atom_of_op(op) {
            Some(atom) => encode_atom(ser, plan, atom, p, encode_struct_x2),
            None => Err(err("bound layout does not match the plan")),
        },
    }
}

fn encode_node_x1(
    ser: &mut CdrSerializer,
    plan: &Node,
    op: &COp,
    base: *const u8,
    offset: u32,
) -> DdsResult<()> {
    let p = base.wrapping_add(offset as usize);
    match (plan, op) {
        (Node::Seq { elem, .. }, COp::Seq { elem: celem, length_offset, bound }) => {
            let (length, data) = seq_source(base, offset, *length_offset, *bound)?;
            ser.serialize_u32(length).map_err(cdr_error)?;
            for i in 0..length {
                let ep = data.wrapping_add(i as usize * celem.stride as usize);
                encode_atom(ser, elem, atom_of_elem(&celem.shape), ep, encode_struct_x1)?;
            }
            Ok(())
        }
        (Node::Array { elem, count, .. }, COp::Array { elem: celem, .. }) => {
            for i in 0..*count {
                let ep = p.wrapping_add(i as usize * celem.stride as usize);
                encode_atom(ser, elem, atom_of_elem(&celem.shape), ep, encode_struct_x1)?;
            }
            Ok(())
        }
        _ => match atom_of_op(op) {
            Some(atom) => encode_atom(ser, plan, atom, p, encode_struct_x1),
            None => Err(err("bound layout does not match the plan")),
        },
    }
}

// ============================================================================
// Decode
// ============================================================================

/// The caller's single output buffer. Writes beyond its capacity are dropped
/// while `needed` keeps counting, so a too-small call still reports the exact
/// retry size.
struct OutBuf<'a> {
    buf: &'a mut [u8],
    needed: usize,
}

fn align_up(pos: usize, align: usize) -> usize {
    (pos + align - 1) & !(align - 1)
}

impl OutBuf<'_> {
    fn write(&mut self, offset: usize, bytes: &[u8]) {
        if let Some(dst) = self.buf.get_mut(offset..offset + bytes.len()) {
            dst.copy_from_slice(bytes);
        }
    }

    fn alloc(&mut self, len: usize, align: usize) -> DdsResult<usize> {
        self.needed = align_up(self.needed, align);
        let offset = self.needed;
        self.needed = self.needed.checked_add(len).ok_or_else(|| err("decoded size overflows"))?;
        Ok(offset)
    }

    fn write_ptr(&mut self, offset: usize, target: usize) {
        let address = (self.buf.as_ptr() as usize).wrapping_add(target);
        self.write(offset, &address.to_ne_bytes());
    }
}

fn write_int(ob: &mut OutBuf<'_>, offset: usize, value: i64, size: u32) {
    match size {
        1 => ob.write(offset, &(value as i8).to_ne_bytes()),
        2 => ob.write(offset, &(value as i16).to_ne_bytes()),
        4 => ob.write(offset, &(value as i32).to_ne_bytes()),
        _ => ob.write(offset, &value.to_ne_bytes()),
    }
}

fn write_uint(ob: &mut OutBuf<'_>, offset: usize, value: u64, size: u32) {
    match size {
        1 => ob.write(offset, &(value as u8).to_ne_bytes()),
        2 => ob.write(offset, &(value as u16).to_ne_bytes()),
        4 => ob.write(offset, &(value as u32).to_ne_bytes()),
        _ => ob.write(offset, &value.to_ne_bytes()),
    }
}

fn decode_struct<R: PlanReader>(
    reader: &mut R,
    plan: &StructNode,
    c: &CStruct,
    ob: &mut OutBuf<'_>,
    at: usize,
) -> DdsResult<()> {
    let (starts, end) = locate_members(reader, plan)?;
    for ((member, cm), start) in plan.members.iter().zip(&c.members).zip(&starts) {
        match start {
            Some(position) => {
                reader.set_position(*position);
                decode_node(reader, &member.node, &cm.op, ob, at, cm.offset)?;
            }
            // A member the sample omits (a Mutable writer's choice) decodes as
            // the type default, which the zero-filled buffer already is — only
            // pointer strings need a target.
            None => default_op(&cm.op, ob, at + cm.offset as usize)?,
        }
    }
    reader.set_position(end);
    Ok(())
}

fn default_op(op: &COp, ob: &mut OutBuf<'_>, offset: usize) -> DdsResult<()> {
    match op {
        COp::StrPtr => {
            let target = ob.alloc(1, 1)?;
            ob.write_ptr(offset, target);
        }
        COp::Struct(nested) => {
            for member in &nested.members {
                default_op(&member.op, ob, offset + member.offset as usize)?;
            }
        }
        COp::Array { elem, count } => {
            if let CElemShape::StrPtr | CElemShape::Struct(_) = &elem.shape {
                for i in 0..*count {
                    default_elem(&elem.shape, ob, offset + i as usize * elem.stride as usize)?;
                }
            }
        }
        // Zero length and a NULL data pointer are already the empty sequence.
        _ => {}
    }
    Ok(())
}

fn default_elem(shape: &CElemShape, ob: &mut OutBuf<'_>, offset: usize) -> DdsResult<()> {
    match shape {
        CElemShape::StrPtr => {
            let target = ob.alloc(1, 1)?;
            ob.write_ptr(offset, target);
            Ok(())
        }
        CElemShape::Struct(nested) => {
            for member in &nested.members {
                default_op(&member.op, ob, offset + member.offset as usize)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn decode_atom<R: PlanReader>(
    reader: &mut R,
    plan: &Node,
    atom: Atom<'_>,
    ob: &mut OutBuf<'_>,
    offset: usize,
) -> DdsResult<()> {
    use PrimitiveKind as P;
    match (plan, atom) {
        (Node::Prim(kind), Atom::Scalar(_)) => {
            match kind {
                P::Boolean => {
                    let v = reader.deserialize_bool().map_err(cdr_error)?;
                    ob.write(offset, &[v as u8]);
                }
                P::Int8 => {
                    ob.write(offset, &reader.deserialize_i8().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Uint8 | P::Byte | P::Char8 => {
                    ob.write(offset, &[reader.deserialize_u8().map_err(cdr_error)?])
                }
                P::Int16 => {
                    ob.write(offset, &reader.deserialize_i16().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Uint16 => {
                    ob.write(offset, &reader.deserialize_u16().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Int32 => {
                    ob.write(offset, &reader.deserialize_i32().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Uint32 => {
                    ob.write(offset, &reader.deserialize_u32().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Int64 => {
                    ob.write(offset, &reader.deserialize_i64().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Uint64 => {
                    ob.write(offset, &reader.deserialize_u64().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Float32 => {
                    ob.write(offset, &reader.deserialize_f32().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Float64 => {
                    ob.write(offset, &reader.deserialize_f64().map_err(cdr_error)?.to_ne_bytes())
                }
                P::Char16 | P::Float128 => {
                    return Err(err(format!("unsupported primitive in plan: {kind:?}")))
                }
            }
            Ok(())
        }
        (Node::Enum { width }, Atom::Scalar(size)) => {
            let value = match width {
                1 => reader.deserialize_i8().map_err(cdr_error)? as i64,
                2 => reader.deserialize_i16().map_err(cdr_error)? as i64,
                _ => reader.deserialize_i32().map_err(cdr_error)? as i64,
            };
            write_int(ob, offset, value, size);
            Ok(())
        }
        (Node::Packed { width }, Atom::Scalar(size)) => {
            let value = match width {
                1 => reader.deserialize_u8().map_err(cdr_error)? as u64,
                2 => reader.deserialize_u16().map_err(cdr_error)? as u64,
                4 => reader.deserialize_u32().map_err(cdr_error)? as u64,
                _ => reader.deserialize_u64().map_err(cdr_error)?,
            };
            write_uint(ob, offset, value, size);
            Ok(())
        }
        (Node::Str, Atom::StrInline(capacity)) => {
            let value = reader.deserialize_string().map_err(cdr_error)?;
            if value.len() as u64 + 1 > capacity as u64 {
                return Err(err(format!(
                    "decoded string of {} bytes exceeds inline capacity {capacity}",
                    value.len()
                )));
            }
            ob.write(offset, value.as_bytes());
            Ok(())
        }
        (Node::Str, Atom::StrPtr) => {
            let value = reader.deserialize_string().map_err(cdr_error)?;
            let target = ob.alloc(value.len() + 1, 1)?;
            ob.write(target, value.as_bytes());
            // The retry path re-zeroes the buffer, so the NUL is already there.
            ob.write_ptr(offset, target);
            Ok(())
        }
        (Node::WStr, Atom::WStrInline(capacity)) => {
            let value = reader.deserialize_wstring16().map_err(cdr_error)?;
            let units: Vec<u16> = value.encode_utf16().collect();
            if units.len() as u64 + 1 > capacity as u64 {
                return Err(err(format!(
                    "decoded wstring of {} units exceeds inline capacity {capacity}",
                    units.len()
                )));
            }
            for (i, unit) in units.iter().enumerate() {
                ob.write(offset + 2 * i, &unit.to_ne_bytes());
            }
            Ok(())
        }
        (Node::Struct(child), Atom::Struct(nested)) => {
            decode_struct(reader, child, nested, ob, offset)
        }
        _ => Err(err("bound layout does not match the plan")),
    }
}

fn decode_node<R: PlanReader>(
    reader: &mut R,
    plan: &Node,
    op: &COp,
    ob: &mut OutBuf<'_>,
    at: usize,
    offset: u32,
) -> DdsResult<()> {
    let field = at + offset as usize;
    match (plan, op) {
        (Node::Seq { elem, framed_in, .. }, COp::Seq { elem: celem, length_offset, bound }) => {
            // The collection DHEADER is read and discarded, exactly as the plan
            // walk and the dynamic path treat it.
            if *framed_in {
                let _ = reader.read_dheader().map_err(cdr_error)?;
            }
            let count = reader.deserialize_u32().map_err(cdr_error)?;
            let data = match bound {
                Some(bound) => {
                    if count > *bound {
                        return Err(err(format!(
                            "decoded sequence length {count} exceeds its bound {bound}"
                        )));
                    }
                    field
                }
                None => {
                    if count == 0 {
                        ob.write(at + *length_offset as usize, &0u32.to_ne_bytes());
                        return Ok(());
                    }
                    let total = (count as usize)
                        .checked_mul(celem.stride as usize)
                        .ok_or_else(|| err("decoded sequence size overflows"))?;
                    let target = ob.alloc(total, 8)?;
                    ob.write_ptr(field, target);
                    target
                }
            };
            ob.write(at + *length_offset as usize, &count.to_ne_bytes());
            for i in 0..count {
                let ep = data + i as usize * celem.stride as usize;
                decode_atom(reader, elem, atom_of_elem(&celem.shape), ob, ep)?;
            }
            Ok(())
        }
        (Node::Array { elem, count, framed_in, .. }, COp::Array { elem: celem, .. }) => {
            if *framed_in {
                let _ = reader.read_dheader().map_err(cdr_error)?;
            }
            for i in 0..*count {
                let ep = field + i as usize * celem.stride as usize;
                decode_atom(reader, elem, atom_of_elem(&celem.shape), ob, ep)?;
            }
            Ok(())
        }
        _ => match atom_of_op(op) {
            Some(atom) => decode_atom(reader, plan, atom, ob, field),
            None => Err(err("bound layout does not match the plan")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::topic::type_support::SerializationFormat;
    use crate::xtypes::{
        plain_collection_equiv_kind, serialize_dynamic_data, CollectionElementFlag,
        CompleteBitflag, CompleteBitmaskType, CompleteEnumeratedLiteral, CompleteEnumeratedType,
        CompleteStructMember, CompleteStructType, CompleteTypeObject, DynamicData, DynamicType,
        DynamicValue, EnumeratedLiteralFlag, EquivalenceHash, MemberFlag, PlainCollectionHeader,
        TryConstructKind, TypeFlag, TypeIdentifier, TypeRegistry,
    };
    use std::mem::offset_of;

    fn member_flag(is_optional: bool, is_key: bool) -> MemberFlag {
        MemberFlag::new(TryConstructKind::Discard, false, is_optional, false, is_key, false)
    }

    fn tf(ext: ExtensibilityKind) -> TypeFlag {
        let kind = match ext {
            ExtensibilityKind::Final => crate::xtypes::ExtensibilityKind::Final,
            ExtensibilityKind::Appendable => crate::xtypes::ExtensibilityKind::Appendable,
            ExtensibilityKind::Mutable => crate::xtypes::ExtensibilityKind::Mutable,
        };
        TypeFlag::new(kind, false, false)
    }

    struct Field {
        name: &'static str,
        id: u32,
        type_id: TypeIdentifier,
        is_key: bool,
    }

    fn field(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { name, id, type_id, is_key: false }
    }

    fn key(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { is_key: true, ..field(name, id, type_id) }
    }

    fn struct_object(name: &str, ext: ExtensibilityKind, fields: Vec<Field>) -> CompleteTypeObject {
        let mut desc = CompleteStructType::new(tf(ext), name.into(), None);
        for f in fields {
            desc.add_member(CompleteStructMember::new(
                f.id,
                member_flag(false, f.is_key),
                f.type_id,
                f.name.to_string(),
            ));
        }
        CompleteTypeObject::Struct(desc)
    }

    fn build(object: CompleteTypeObject) -> Arc<DynamicType> {
        Arc::new(DynamicType::from_type_object(object, TypeIdentifier::None).unwrap())
    }

    fn build_with(object: CompleteTypeObject, registry: &TypeRegistry) -> Arc<DynamicType> {
        Arc::new(
            DynamicType::from_type_object_with_registry(
                Arc::new(object),
                TypeIdentifier::None,
                registry,
            )
            .unwrap(),
        )
    }

    fn register(
        registry: &mut TypeRegistry,
        name: &str,
        obj: CompleteTypeObject,
    ) -> TypeIdentifier {
        let hash = EquivalenceHash::compute(&obj.serialize());
        registry.register_complete(hash, name.into(), obj);
        TypeIdentifier::CompleteTypeId(hash)
    }

    fn sequence_of(element: TypeIdentifier) -> TypeIdentifier {
        TypeIdentifier::PlainSequenceSmall {
            header: PlainCollectionHeader {
                equiv_kind: plain_collection_equiv_kind(&element),
                element_flags: CollectionElementFlag::default(),
            },
            bound: 0,
            element_identifier: Box::new(element),
        }
    }

    fn array_of(element: TypeIdentifier, bounds: Vec<u8>) -> TypeIdentifier {
        TypeIdentifier::PlainArraySmall {
            header: PlainCollectionHeader {
                equiv_kind: plain_collection_equiv_kind(&element),
                element_flags: CollectionElementFlag::default(),
            },
            array_bound_seq: bounds,
            element_identifier: Box::new(element),
        }
    }

    fn formats(ext: ExtensibilityKind) -> Vec<(&'static str, SerializationFormat, bool)> {
        vec![
            ("xcdr1", SerializationFormat::Cdr, false),
            (
                "xcdr2",
                SerializationFormat::Xcdr { extensibility_kind: ext, use_delimiters: false },
                true,
            ),
        ]
    }

    fn scalar(kind: CScalarKind, size: u32) -> CNodeSpec {
        CNodeSpec::Scalar { kind, size }
    }

    fn spec_field(name: &str, offset: usize, node: CNodeSpec) -> CFieldSpec {
        CFieldSpec { name: name.to_string(), offset: offset as u32, node }
    }

    // ---- flat struct across every extensibility and representation ----

    #[repr(C)]
    struct CFlat {
        id: u32,
        a: i32,
        flag: u8,
        half: i16,
        big: i64,
        ratio: f32,
        x: f64,
        name: [u8; 24],
    }

    fn flat_type(ext: ExtensibilityKind) -> Arc<DynamicType> {
        build(struct_object(
            "Flat",
            ext,
            vec![
                key("id", 0, TypeIdentifier::Uint32),
                field("a", 1, TypeIdentifier::Int32),
                field("flag", 2, TypeIdentifier::Boolean),
                field("half", 3, TypeIdentifier::Int16),
                field("big", 4, TypeIdentifier::Int64),
                field("ratio", 5, TypeIdentifier::Float32),
                field("x", 6, TypeIdentifier::Float64),
                field("name", 7, TypeIdentifier::String8Small { bound: 23 }),
            ],
        ))
    }

    fn flat_spec() -> CStructSpec {
        CStructSpec {
            type_name: "Flat".into(),
            size: std::mem::size_of::<CFlat>() as u32,
            fields: vec![
                spec_field("id", offset_of!(CFlat, id), scalar(CScalarKind::U32, 4)),
                spec_field("a", offset_of!(CFlat, a), scalar(CScalarKind::I32, 4)),
                spec_field("flag", offset_of!(CFlat, flag), scalar(CScalarKind::Bool, 1)),
                spec_field("half", offset_of!(CFlat, half), scalar(CScalarKind::I16, 2)),
                spec_field("big", offset_of!(CFlat, big), scalar(CScalarKind::I64, 8)),
                spec_field("ratio", offset_of!(CFlat, ratio), scalar(CScalarKind::F32, 4)),
                spec_field("x", offset_of!(CFlat, x), scalar(CScalarKind::F64, 8)),
                spec_field("name", offset_of!(CFlat, name), CNodeSpec::StrInline { capacity: 24 }),
            ],
        }
    }

    fn flat_value() -> CFlat {
        let mut name = [0u8; 24];
        name[..5].copy_from_slice(b"hello");
        CFlat {
            id: 7,
            a: -3,
            flag: 1,
            half: 513,
            big: -1_234_567_890_123,
            ratio: 1.5,
            x: -2.25,
            name,
        }
    }

    fn flat_data(dynamic_type: &Arc<DynamicType>) -> DynamicData {
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 7u32).unwrap();
        data.set("a", -3i32).unwrap();
        data.set("flag", true).unwrap();
        data.set("half", 513i16).unwrap();
        data.set("big", -1_234_567_890_123i64).unwrap();
        data.set("ratio", 1.5f32).unwrap();
        data.set("x", -2.25f64).unwrap();
        data.set("name", "hello".to_string()).unwrap();
        data
    }

    /// Encode from C memory and compare against the dynamic serializer, the
    /// kernel's canonical wire form. Mutable XCDR1 is the one deliberate
    /// difference: this path stamps the PL_CDR encapsulation (0x0003) the derive
    /// and generated-C writers use, while the dynamic path stamps 0x0001 — the
    /// bodies must still be identical.
    fn assert_encodes_like_dynamic(
        label: &str,
        bound: &BoundCLayout,
        src: *const u8,
        data: &DynamicData,
    ) {
        for (name, format, xcdr2) in formats(data.dynamic_type().extensibility()) {
            let want = serialize_dynamic_data(data, &format).unwrap();
            let got = unsafe { bound.encode(src, xcdr2) }
                .unwrap_or_else(|e| panic!("{label}/{name}: encode failed: {e:?}"));
            if !xcdr2 && data.dynamic_type().extensibility() == ExtensibilityKind::Mutable {
                assert_eq!(&got[..2], &[0x00, 0x03], "{label}/{name}: PL_CDR encapsulation");
                assert_eq!(&got[4..], &want[4..], "{label}/{name}: body bytes differ");
            } else {
                assert_eq!(got, want.as_ref(), "{label}/{name}: bytes differ");
            }
        }
    }

    #[test]
    fn flat_struct_bytes_match_dynamic_path() {
        for ext in
            [ExtensibilityKind::Final, ExtensibilityKind::Appendable, ExtensibilityKind::Mutable]
        {
            let dynamic_type = flat_type(ext);
            let plans = TypePlans::compile(&dynamic_type);
            let bound = BoundCLayout::bind(&plans, &flat_spec()).unwrap();
            let value = flat_value();
            let data = flat_data(&dynamic_type);
            assert_encodes_like_dynamic(
                &format!("flat/{ext:?}"),
                &bound,
                &value as *const CFlat as *const u8,
                &data,
            );
        }
    }

    #[test]
    fn flat_struct_decodes_and_roundtrips() {
        for ext in
            [ExtensibilityKind::Final, ExtensibilityKind::Appendable, ExtensibilityKind::Mutable]
        {
            let dynamic_type = flat_type(ext);
            let plans = TypePlans::compile(&dynamic_type);
            let bound = BoundCLayout::bind(&plans, &flat_spec()).unwrap();
            let data = flat_data(&dynamic_type);

            for (name, format, xcdr2) in formats(ext) {
                let bytes = serialize_dynamic_data(&data, &format).unwrap();
                let mut buf = vec![0u8; bound.struct_size() as usize];
                let needed = bound.decode(&bytes, &mut buf).unwrap();
                assert_eq!(needed, bound.struct_size() as usize, "{ext:?}/{name}");

                let got: CFlat = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const CFlat) };
                assert_eq!(got.id, 7, "{ext:?}/{name}");
                assert_eq!(got.a, -3);
                assert_eq!(got.flag, 1);
                assert_eq!(got.half, 513);
                assert_eq!(got.big, -1_234_567_890_123);
                assert_eq!(got.ratio, 1.5);
                assert_eq!(got.x, -2.25);
                assert_eq!(&got.name[..6], b"hello\0");

                let again = unsafe { bound.encode(buf.as_ptr(), xcdr2) }.unwrap();
                if xcdr2 || ext != ExtensibilityKind::Mutable {
                    assert_eq!(again, bytes.as_ref(), "{ext:?}/{name}: roundtrip bytes");
                } else {
                    assert_eq!(&again[4..], &bytes[4..], "{ext:?}/{name}: roundtrip body");
                }
            }
        }
    }

    // ---- pointer strings and unbounded sequences (decode fixup) ----

    #[repr(C)]
    struct CPtrSeq {
        name: *const u8,
        values: CDoubleSeq,
        id: i32,
    }

    #[repr(C)]
    struct CDoubleSeq {
        data: *const f64,
        length: u32,
    }

    fn ptr_type() -> Arc<DynamicType> {
        build(struct_object(
            "Ptr",
            ExtensibilityKind::Appendable,
            vec![
                field("name", 0, TypeIdentifier::String8Small { bound: 0 }),
                field("values", 1, sequence_of(TypeIdentifier::Float64)),
                key("id", 2, TypeIdentifier::Int32),
            ],
        ))
    }

    fn ptr_spec() -> CStructSpec {
        CStructSpec {
            type_name: "Ptr".into(),
            size: std::mem::size_of::<CPtrSeq>() as u32,
            fields: vec![
                spec_field("name", offset_of!(CPtrSeq, name), CNodeSpec::StrPtr),
                CFieldSpec {
                    name: "values".into(),
                    offset: (offset_of!(CPtrSeq, values) + offset_of!(CDoubleSeq, data)) as u32,
                    node: CNodeSpec::Seq {
                        elem: CElemSpec::Scalar { kind: CScalarKind::F64, size: 8 },
                        length_offset: (offset_of!(CPtrSeq, values)
                            + offset_of!(CDoubleSeq, length))
                            as u32,
                        bound: None,
                    },
                },
                spec_field("id", offset_of!(CPtrSeq, id), scalar(CScalarKind::I32, 4)),
            ],
        }
    }

    #[test]
    fn pointer_members_fix_up_into_the_callers_buffer() {
        let dynamic_type = ptr_type();
        let plans = TypePlans::compile(&dynamic_type);
        let bound = BoundCLayout::bind(&plans, &ptr_spec()).unwrap();

        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("name", "fixup".to_string()).unwrap();
        data.set_value(
            "values",
            DynamicValue::Sequence((0..5).map(|i| DynamicValue::Float64(i as f64 * 0.5)).collect()),
        )
        .unwrap();
        data.set("id", 42i32).unwrap();
        let bytes = serialize_dynamic_data(&data, &SerializationFormat::Cdr).unwrap();

        // Too small: the struct alone cannot hold the tail, and the exact size
        // comes back for the retry.
        let mut small = vec![0u8; bound.struct_size() as usize];
        let needed = bound.decode(&bytes, &mut small).unwrap();
        assert!(needed > small.len());

        let mut buf = vec![0u8; needed];
        assert_eq!(bound.decode(&bytes, &mut buf).unwrap(), needed);
        let got: CPtrSeq = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const CPtrSeq) };
        assert_eq!(got.id, 42);

        let base = buf.as_ptr() as usize;
        let name_addr = got.name as usize;
        assert!(name_addr >= base && name_addr < base + needed, "name points into the buffer");
        let name = unsafe { std::ffi::CStr::from_ptr(got.name as *const std::os::raw::c_char) };
        assert_eq!(name.to_str().unwrap(), "fixup");

        assert_eq!(got.values.length, 5);
        let data_addr = got.values.data as usize;
        assert!(data_addr >= base && data_addr < base + needed, "data points into the buffer");
        let values = unsafe { std::slice::from_raw_parts(got.values.data, 5) };
        assert_eq!(values, &[0.0, 0.5, 1.0, 1.5, 2.0]);

        // And back out through the encoder.
        let again = unsafe { bound.encode(buf.as_ptr(), false) }.unwrap();
        assert_eq!(again, bytes.as_ref());
    }

    // ---- enum/bitmask memory width vs wire width ----

    #[repr(C)]
    struct CMixed {
        color: i32,
        flags: u64,
        grid: [i32; 6],
        palette: CColorSeq,
    }

    #[repr(C)]
    struct CColorSeq {
        data: [i32; 4],
        length: u32,
    }

    fn color_enum() -> CompleteTypeObject {
        let mut e = CompleteEnumeratedType::new(tf(ExtensibilityKind::Final), "Color".into(), 8);
        e.add_literal(CompleteEnumeratedLiteral::new(0, EnumeratedLiteralFlag(0), "RED".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(1, EnumeratedLiteralFlag(0), "GREEN".into()));
        e.add_literal(CompleteEnumeratedLiteral::new(2, EnumeratedLiteralFlag(0), "BLUE".into()));
        CompleteTypeObject::Enum(e)
    }

    fn flags_bitmask() -> CompleteTypeObject {
        let mut b = CompleteBitmaskType::new(tf(ExtensibilityKind::Final), "Flags".into(), 16);
        b.add_flag(CompleteBitflag::new(0, MemberFlag::default(), "A".into()));
        b.add_flag(CompleteBitflag::new(9, MemberFlag::default(), "B".into()));
        CompleteTypeObject::Bitmask(b)
    }

    fn mixed_type() -> Arc<DynamicType> {
        let mut registry = TypeRegistry::new();
        let enum_id = register(&mut registry, "Color", color_enum());
        let mask_id = register(&mut registry, "Flags", flags_bitmask());
        build_with(
            struct_object(
                "Mixed",
                ExtensibilityKind::Final,
                vec![
                    field("color", 0, enum_id.clone()),
                    field("flags", 1, mask_id),
                    field("grid", 2, array_of(TypeIdentifier::Int32, vec![2, 3])),
                    field("palette", 3, sequence_of(enum_id)),
                ],
            ),
            &registry,
        )
    }

    fn mixed_spec() -> CStructSpec {
        let palette = offset_of!(CMixed, palette);
        CStructSpec {
            type_name: "Mixed".into(),
            size: std::mem::size_of::<CMixed>() as u32,
            fields: vec![
                spec_field("color", offset_of!(CMixed, color), scalar(CScalarKind::Enum, 4)),
                spec_field("flags", offset_of!(CMixed, flags), scalar(CScalarKind::Packed, 8)),
                CFieldSpec {
                    name: "grid".into(),
                    offset: offset_of!(CMixed, grid) as u32,
                    node: CNodeSpec::Array {
                        elem: CElemSpec::Scalar { kind: CScalarKind::I32, size: 4 },
                        count: 6,
                    },
                },
                CFieldSpec {
                    name: "palette".into(),
                    offset: (palette + offset_of!(CColorSeq, data)) as u32,
                    node: CNodeSpec::Seq {
                        elem: CElemSpec::Scalar { kind: CScalarKind::Enum, size: 4 },
                        length_offset: (palette + offset_of!(CColorSeq, length)) as u32,
                        bound: Some(4),
                    },
                },
            ],
        }
    }

    #[test]
    fn enum_and_bitmask_convert_between_memory_and_wire_width() {
        let dynamic_type = mixed_type();
        let plans = TypePlans::compile(&dynamic_type);
        let bound = BoundCLayout::bind(&plans, &mixed_spec()).unwrap();

        let value = CMixed {
            color: 2,
            flags: 0b10_0000_0001,
            grid: [0, 1, 2, 3, 4, 5],
            palette: CColorSeq { data: [1, 0, 2, 0], length: 3 },
        };
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set_value("color", DynamicValue::Enum { name: "BLUE".into(), value: 2 }).unwrap();
        data.set_value("flags", DynamicValue::Bitmask(0b10_0000_0001)).unwrap();
        data.set_value("grid", DynamicValue::Array((0..6).map(DynamicValue::Int32).collect()))
            .unwrap();
        data.set_value(
            "palette",
            DynamicValue::Sequence(vec![
                DynamicValue::Enum { name: "GREEN".into(), value: 1 },
                DynamicValue::Enum { name: "RED".into(), value: 0 },
                DynamicValue::Enum { name: "BLUE".into(), value: 2 },
            ]),
        )
        .unwrap();

        assert_encodes_like_dynamic("mixed", &bound, &value as *const CMixed as *const u8, &data);

        for (name, format, _) in formats(ExtensibilityKind::Final) {
            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let mut buf = vec![0u8; bound.struct_size() as usize];
            bound.decode(&bytes, &mut buf).unwrap();
            let got: CMixed = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const CMixed) };
            assert_eq!(got.color, 2, "{name}");
            assert_eq!(got.flags, 0b10_0000_0001, "{name}");
            assert_eq!(got.grid, [0, 1, 2, 3, 4, 5], "{name}");
            assert_eq!(got.palette.length, 3, "{name}");
            assert_eq!(&got.palette.data[..3], &[1, 0, 2], "{name}");
        }
    }

    // ---- nested structs, inline strings in sequences ----

    #[repr(C)]
    struct CInner {
        a: i32,
        label: [u8; 16],
    }

    #[repr(C)]
    struct COuter {
        id: u32,
        child: CInner,
        items: CInnerSeq,
    }

    #[repr(C)]
    struct CInnerSeq {
        data: [CInner; 3],
        length: u32,
    }

    fn inner_object(ext: ExtensibilityKind) -> CompleteTypeObject {
        struct_object(
            "Inner",
            ext,
            vec![
                field("a", 0, TypeIdentifier::Int32),
                field("label", 1, TypeIdentifier::String8Small { bound: 15 }),
            ],
        )
    }

    fn inner_spec() -> CStructSpec {
        CStructSpec {
            type_name: "Inner".into(),
            size: std::mem::size_of::<CInner>() as u32,
            fields: vec![
                spec_field("a", offset_of!(CInner, a), scalar(CScalarKind::I32, 4)),
                spec_field(
                    "label",
                    offset_of!(CInner, label),
                    CNodeSpec::StrInline { capacity: 16 },
                ),
            ],
        }
    }

    fn outer_type(inner_ext: ExtensibilityKind) -> Arc<DynamicType> {
        let mut registry = TypeRegistry::new();
        let inner_id = register(&mut registry, "Inner", inner_object(inner_ext));
        build_with(
            struct_object(
                "Outer",
                ExtensibilityKind::Appendable,
                vec![
                    key("id", 0, TypeIdentifier::Uint32),
                    field("child", 1, inner_id.clone()),
                    field("items", 2, sequence_of(inner_id)),
                ],
            ),
            &registry,
        )
    }

    fn outer_spec() -> CStructSpec {
        let items = offset_of!(COuter, items);
        CStructSpec {
            type_name: "Outer".into(),
            size: std::mem::size_of::<COuter>() as u32,
            fields: vec![
                spec_field("id", offset_of!(COuter, id), scalar(CScalarKind::U32, 4)),
                CFieldSpec {
                    name: "child".into(),
                    offset: offset_of!(COuter, child) as u32,
                    node: CNodeSpec::Struct(Arc::new(inner_spec())),
                },
                CFieldSpec {
                    name: "items".into(),
                    offset: (items + offset_of!(CInnerSeq, data)) as u32,
                    node: CNodeSpec::Seq {
                        elem: CElemSpec::Struct(Arc::new(inner_spec())),
                        length_offset: (items + offset_of!(CInnerSeq, length)) as u32,
                        bound: Some(3),
                    },
                },
            ],
        }
    }

    fn inner(dynamic_type: &Arc<DynamicType>, a: i32, label: &str) -> DynamicValue {
        let inner_type = dynamic_type
            .get_member("child")
            .unwrap()
            .member_type
            .as_type_ref()
            .expect("Inner unresolved")
            .clone();
        let mut data = DynamicData::new(inner_type);
        data.set("a", a).unwrap();
        data.set("label", label.to_string()).unwrap();
        DynamicValue::Struct(Box::new(data))
    }

    fn c_inner(a: i32, label: &str) -> CInner {
        let mut buf = [0u8; 16];
        buf[..label.len()].copy_from_slice(label.as_bytes());
        CInner { a, label: buf }
    }

    #[test]
    fn nested_structs_inline_and_as_sequence_elements() {
        for inner_ext in
            [ExtensibilityKind::Final, ExtensibilityKind::Appendable, ExtensibilityKind::Mutable]
        {
            let dynamic_type = outer_type(inner_ext);
            let plans = TypePlans::compile(&dynamic_type);
            let bound = BoundCLayout::bind(&plans, &outer_spec()).unwrap();

            let value = COuter {
                id: 9,
                child: c_inner(11, "leaf"),
                items: CInnerSeq {
                    data: [c_inner(1, "one"), c_inner(2, "two"), c_inner(0, "")],
                    length: 2,
                },
            };
            let mut data = DynamicData::new(dynamic_type.clone());
            data.set("id", 9u32).unwrap();
            data.set_value("child", inner(&dynamic_type, 11, "leaf")).unwrap();
            data.set_value(
                "items",
                DynamicValue::Sequence(vec![
                    inner(&dynamic_type, 1, "one"),
                    inner(&dynamic_type, 2, "two"),
                ]),
            )
            .unwrap();

            let label = format!("outer/inner={inner_ext:?}");
            assert_encodes_like_dynamic(
                &label,
                &bound,
                &value as *const COuter as *const u8,
                &data,
            );

            for (name, format, _) in formats(ExtensibilityKind::Appendable) {
                let bytes = serialize_dynamic_data(&data, &format).unwrap();
                let mut buf = vec![0u8; bound.struct_size() as usize];
                bound.decode(&bytes, &mut buf).unwrap();
                let got: COuter =
                    unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const COuter) };
                assert_eq!(got.id, 9, "{label}/{name}");
                assert_eq!(got.child.a, 11);
                assert_eq!(&got.child.label[..5], b"leaf\0");
                assert_eq!(got.items.length, 2);
                assert_eq!(got.items.data[0].a, 1);
                assert_eq!(&got.items.data[1].label[..4], b"two\0");
            }
        }
    }

    // ---- a mutable sample that omits a member decodes as the default ----

    #[test]
    fn absent_mutable_member_decodes_as_default() {
        let reader_type = build(struct_object(
            "Mut",
            ExtensibilityKind::Mutable,
            vec![key("id", 1, TypeIdentifier::Uint32), field("a", 2, TypeIdentifier::Int32)],
        ));
        let writer_type = build(struct_object(
            "Mut",
            ExtensibilityKind::Mutable,
            vec![key("id", 1, TypeIdentifier::Uint32)],
        ));

        #[repr(C)]
        struct CMut {
            id: u32,
            a: i32,
        }
        let spec = CStructSpec {
            type_name: "Mut".into(),
            size: std::mem::size_of::<CMut>() as u32,
            fields: vec![
                spec_field("id", offset_of!(CMut, id), scalar(CScalarKind::U32, 4)),
                spec_field("a", offset_of!(CMut, a), scalar(CScalarKind::I32, 4)),
            ],
        };
        let plans = TypePlans::compile(&reader_type);
        let bound = BoundCLayout::bind(&plans, &spec).unwrap();

        let mut data = DynamicData::new(writer_type);
        data.set("id", 5u32).unwrap();
        for (name, format, _) in formats(ExtensibilityKind::Mutable) {
            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let mut buf = vec![0u8; bound.struct_size() as usize];
            bound.decode(&bytes, &mut buf).unwrap();
            let got: CMut = unsafe { std::ptr::read_unaligned(buf.as_ptr() as *const CMut) };
            assert_eq!(got.id, 5, "{name}");
            assert_eq!(got.a, 0, "{name}: absent member is the default");
        }
    }

    // ---- bind rejections ----

    #[test]
    fn bind_rejects_layouts_that_do_not_match() {
        let dynamic_type = flat_type(ExtensibilityKind::Final);
        let plans = TypePlans::compile(&dynamic_type);

        let mut renamed = flat_spec();
        renamed.fields[1].name = "wrong".into();
        assert!(BoundCLayout::bind(&plans, &renamed).is_err(), "name mismatch");

        let mut narrow = flat_spec();
        narrow.fields[4].node = scalar(CScalarKind::I64, 4);
        assert!(BoundCLayout::bind(&plans, &narrow).is_err(), "wrong scalar width");

        let mut fewer = flat_spec();
        fewer.fields.pop();
        assert!(BoundCLayout::bind(&plans, &fewer).is_err(), "member count mismatch");

        let mut outside = flat_spec();
        outside.fields[0].offset = outside.size;
        assert!(BoundCLayout::bind(&plans, &outside).is_err(), "offset outside the struct");

        // A shape the plan itself declines never binds.
        let with_wide = build(struct_object(
            "Wide",
            ExtensibilityKind::Final,
            vec![field("c", 0, TypeIdentifier::Char16)],
        ));
        let no_plans = TypePlans::compile(&with_wide);
        let spec = CStructSpec {
            type_name: "Wide".into(),
            size: 2,
            fields: vec![spec_field("c", 0, scalar(CScalarKind::U16, 2))],
        };
        assert!(BoundCLayout::bind(&no_plans, &spec).is_err(), "no plan");

        // A collection whose element is a collection stays on the inline codec.
        let seq_of_seq = build(struct_object(
            "SS",
            ExtensibilityKind::Final,
            vec![field("q", 0, sequence_of(sequence_of(TypeIdentifier::Int32)))],
        ));
        let plans = TypePlans::compile(&seq_of_seq);
        let spec = CStructSpec {
            type_name: "SS".into(),
            size: 16,
            fields: vec![CFieldSpec {
                name: "q".into(),
                offset: 0,
                node: CNodeSpec::Seq {
                    elem: CElemSpec::Scalar { kind: CScalarKind::I32, size: 4 },
                    length_offset: 8,
                    bound: None,
                },
            }],
        };
        assert!(BoundCLayout::bind(&plans, &spec).is_err(), "collection-of-collection element");
    }

    #[test]
    fn hostile_lengths_are_rejected_on_both_directions() {
        let dynamic_type = mixed_type();
        let plans = TypePlans::compile(&dynamic_type);
        let bound = BoundCLayout::bind(&plans, &mixed_spec()).unwrap();

        // Encode: a C length above the bound must not read out of the array.
        let value = CMixed {
            color: 0,
            flags: 0,
            grid: [0; 6],
            palette: CColorSeq { data: [0; 4], length: 9 },
        };
        assert!(unsafe { bound.encode(&value as *const CMixed as *const u8, true) }.is_err());

        // Decode: a wire length above the bound must not write out of the array.
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set_value("color", DynamicValue::Enum { name: "RED".into(), value: 0 }).unwrap();
        data.set_value("flags", DynamicValue::Bitmask(0)).unwrap();
        data.set_value("grid", DynamicValue::Array((0..6).map(DynamicValue::Int32).collect()))
            .unwrap();
        data.set_value(
            "palette",
            DynamicValue::Sequence(vec![DynamicValue::Enum { name: "RED".into(), value: 0 }]),
        )
        .unwrap();
        let mut bytes = serialize_dynamic_data(&data, &SerializationFormat::Cdr).unwrap().to_vec();
        // Patch the palette length (last u32-count then one 1-byte enum at the tail).
        let tail = bytes.len();
        bytes[tail - 5..tail - 1].copy_from_slice(&64u32.to_le_bytes());
        let mut buf = vec![0u8; bound.struct_size() as usize];
        assert!(bound.decode(&bytes, &mut buf).is_err());
    }
}
