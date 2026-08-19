//! Compiled codec plans — a type's CDR layout resolved once, then walked directly
//! over sample bytes.
//!
//! The dynamic path answers two byte-oriented questions (what is this sample's
//! key, what is the value of one field) by materializing the whole `DynamicData`
//! tree first. A plan answers both from the wire, allocating only for the values
//! it actually returns.
//!
//! A plan is compiled per representation, because XCDR1 and XCDR2 frame the same
//! type differently. Endianness is *not* part of the plan: it changes how bytes
//! are read, never where they sit, and it varies per sample.
//!
//! Coverage is the subset the raw FFI path uses: Final/Appendable structs of
//! primitives, strings, enums, bitmask/bitset, nested structs, arrays and
//! sequences. Mutable framing, unions, maps, optional members, `char16` and
//! `float128` compile to `None` and the caller keeps the dynamic path — the last
//! two because the dynamic codec reads them asymmetrically, and matching it here
//! would mean reproducing that rather than a wire rule.

use std::sync::Arc;

use crate::common::instance_handle::InstanceHandle;
use crate::dcps::core::error::{DdsError, DdsResult};
use crate::serialize::cdr::{
    CdrDeserializer, CdrError, ExtensibilityKind, PrimitiveSerialize, StringSerialize,
    Xcdr2Deserializer, Xcdr2Serializer,
};
use crate::serialize::{BufferManager, DeserializerReader};
use crate::topic::sql::ast::Parameter;

use super::dynamic_serialization::{
    enum_wire_width, is_primitive_kind, key_holder_max_size, packed_wire_width, ValueDeserializer,
};
use super::dynamic_type::{DynamicType, DynamicTypeKind, PrimitiveKind};

/// A member's wire layout.
#[derive(Debug)]
enum Node {
    Prim(PrimitiveKind),
    /// Enum, whose wire width follows its `bit_bound`.
    Enum {
        width: u8,
    },
    /// Bitmask or bitset: an unsigned integer of `width` bytes.
    Packed {
        width: u8,
    },
    Str,
    WStr,
    Struct(Arc<StructNode>),
    /// `framed_in` is the sample's DHEADER, `framed_out` the key holder's; they
    /// differ because the key holder is always XCDR2 while the sample may be XCDR1.
    Seq {
        elem: Box<Node>,
        framed_in: bool,
        framed_out: bool,
    },
    Array {
        elem: Box<Node>,
        count: u32,
        framed_in: bool,
        framed_out: bool,
    },
}

#[derive(Debug)]
struct MemberNode {
    name: Arc<str>,
    node: Node,
}

#[derive(Debug)]
struct StructNode {
    /// Members in wire (declaration) order.
    members: Vec<MemberNode>,
    /// Indices to project into the key holder. The `@key` members in member_id
    /// order, or — when the struct has none — every member in declaration order,
    /// which is the RTPS key-holder rule for a nested aggregate (DDSI-RTPS 9.6.4.8).
    key_order: Vec<usize>,
    /// The sample precedes this struct's members with a DHEADER.
    framed: bool,
}

/// One type's layout for one representation.
pub struct CodecPlan {
    root: Arc<StructNode>,
    /// XTypes 7.6.8 step 5 decides raw-vs-MD5 on the key holder's *maximum* size,
    /// which is a property of the type, not of the sample.
    key_holder_max: Option<usize>,
    /// The sole key member is a string, so the key CDR drops its trailing NULs.
    single_string_key: bool,
}

/// The plans for one type, indexed by the representation of the sample at hand.
pub struct TypePlans {
    xcdr1: Option<CodecPlan>,
    xcdr2: Option<CodecPlan>,
    has_key: bool,
}

impl std::fmt::Debug for TypePlans {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypePlans")
            .field("xcdr1", &self.xcdr1.is_some())
            .field("xcdr2", &self.xcdr2.is_some())
            .field("has_key", &self.has_key)
            .finish()
    }
}

impl TypePlans {
    /// Compile both representations. Either may come out `None`; a type outside
    /// the supported subset yields two `None`s and every entry point below then
    /// reports "no plan" so the caller stays on the dynamic path.
    pub fn compile(dynamic_type: &DynamicType) -> Self {
        Self {
            xcdr1: CodecPlan::compile(dynamic_type, false),
            xcdr2: CodecPlan::compile(dynamic_type, true),
            has_key: !dynamic_type.key_members().is_empty(),
        }
    }

    fn plan_for(&self, bytes: &[u8]) -> Option<&CodecPlan> {
        if bytes.len() < 4 {
            return None;
        }
        if is_xcdr2(bytes) {
            self.xcdr2.as_ref()
        } else {
            self.xcdr1.as_ref()
        }
    }

    /// Canonical RTPS KeyHash CDR for `bytes` (headerless, big-endian, §9.6.4.8),
    /// byte-identical to the dynamic path's `serialize_key_cdr`. `None` means no
    /// plan covers this sample.
    pub fn serialize_key(&self, bytes: &[u8]) -> Option<DdsResult<Vec<u8>>> {
        if !self.has_key {
            return Some(Ok(Vec::new()));
        }
        let plan = self.plan_for(bytes)?;
        Some(plan.project_key(bytes).map(|(key, _)| key))
    }

    /// The InstanceHandle of `bytes`, following the same raw-vs-MD5 rule as
    /// `DynamicTypeSupport::compute_key`.
    pub fn compute_key(&self, bytes: &[u8]) -> Option<InstanceHandle> {
        if !self.has_key {
            return Some(InstanceHandle::NIL);
        }
        let plan = self.plan_for(bytes)?;
        let (key, _) = plan.project_key(bytes).ok()?;
        if key.is_empty() {
            return Some(InstanceHandle::NIL);
        }
        Some(match plan.key_holder_max {
            Some(n) if n <= 16 => InstanceHandle::from_key_cdr(&key),
            _ => InstanceHandle::from_key_cdr_hashed(&key),
        })
    }

    /// Read one field of `bytes` by name, `.`-separated for nested structs.
    pub fn field_value(&self, bytes: &[u8], field_path: &str) -> Option<DdsResult<Parameter>> {
        let plan = self.plan_for(bytes)?;
        Some(plan.field_value(bytes, field_path))
    }

    /// Whether `field_path` names a readable field. Answered from either plan,
    /// since the two describe the same members.
    pub fn has_field(&self, field_path: &str) -> bool {
        let Some(plan) = self.xcdr2.as_ref().or(self.xcdr1.as_ref()) else {
            return false;
        };
        plan.has_field(field_path)
    }
}

/// Matches the encapsulation ids the dynamic path treats as XCDR2. A payload too
/// short to hold one reads as XCDR1, whose first read then fails the bounds check —
/// the projection must degrade to an error, never panic on the receive path.
fn is_xcdr2(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && matches!(u16::from_be_bytes([bytes[0], bytes[1]]), 0x0006..=0x000B)
}

// ============================================================================
// Compilation
// ============================================================================

impl CodecPlan {
    fn compile(dynamic_type: &DynamicType, xcdr2: bool) -> Option<Self> {
        let root = compile_struct(dynamic_type, xcdr2)?;
        // `key_order` stands for every member when the struct has no `@key`, and
        // the single-string rule is about a real key, so ask the type.
        let single_string_key = !dynamic_type.key_members().is_empty()
            && root.key_order.len() == 1
            && matches!(root.members[root.key_order[0]].node, Node::Str);
        Some(Self { root, key_holder_max: key_holder_max_size(dynamic_type), single_string_key })
    }
}

fn compile_struct(dynamic_type: &DynamicType, xcdr2: bool) -> Option<Arc<StructNode>> {
    let extensibility = dynamic_type.extensibility();
    if matches!(extensibility, ExtensibilityKind::Mutable) {
        return None;
    }
    let struct_desc = dynamic_type.as_struct()?;

    let mut members = Vec::with_capacity(struct_desc.member_count());
    let mut keys: Vec<(u32, usize)> = Vec::new();
    for (index, member) in struct_desc.members().iter().enumerate() {
        if member.is_optional {
            return None;
        }
        members.push(MemberNode {
            name: member.name.clone(),
            node: compile_node(&member.member_type, xcdr2)?,
        });
        if member.is_key {
            keys.push((member.member_id, index));
        }
    }

    keys.sort_by_key(|(member_id, _)| *member_id);
    let key_order = if keys.is_empty() {
        (0..members.len()).collect()
    } else {
        keys.into_iter().map(|(_, index)| index).collect()
    };

    Some(Arc::new(StructNode {
        members,
        key_order,
        framed: xcdr2 && !matches!(extensibility, ExtensibilityKind::Final),
    }))
}

fn compile_node(kind: &DynamicTypeKind, xcdr2: bool) -> Option<Node> {
    match kind {
        DynamicTypeKind::Primitive(PrimitiveKind::Char16)
        | DynamicTypeKind::Primitive(PrimitiveKind::Float128) => None,
        DynamicTypeKind::Primitive(prim) => Some(Node::Prim(*prim)),
        DynamicTypeKind::String { .. } => Some(Node::Str),
        DynamicTypeKind::WString { .. } => Some(Node::WStr),
        DynamicTypeKind::Enum(desc) => {
            Some(Node::Enum { width: enum_wire_width(desc.bit_bound()) })
        }
        DynamicTypeKind::Bitmask(desc) => {
            Some(Node::Packed { width: packed_wire_width(desc.bit_bound) })
        }
        DynamicTypeKind::Bitset(desc) => {
            Some(Node::Packed { width: packed_wire_width(desc.total_bits()) })
        }
        DynamicTypeKind::Sequence { element_type, .. } => {
            let primitive = is_primitive_kind(element_type);
            Some(Node::Seq {
                elem: Box::new(compile_node(element_type, xcdr2)?),
                framed_in: xcdr2 && !primitive,
                framed_out: !primitive,
            })
        }
        DynamicTypeKind::Array { element_type, dimensions } => {
            let primitive = is_primitive_kind(element_type);
            let count = dimensions.iter().try_fold(1u32, |acc, &d| acc.checked_mul(d))?;
            Some(Node::Array {
                elem: Box::new(compile_node(element_type, xcdr2)?),
                count,
                framed_in: xcdr2 && !primitive,
                framed_out: !primitive,
            })
        }
        DynamicTypeKind::TypeRef(inner) => match inner.kind() {
            DynamicTypeKind::Struct(_) => Some(Node::Struct(compile_struct(inner, xcdr2)?)),
            other => compile_node(other, xcdr2),
        },
        // A struct kind reached without a resolved `TypeRef` carries no
        // extensibility of its own, and an unresolved external type carries no
        // layout at all.
        DynamicTypeKind::Struct(_)
        | DynamicTypeKind::Union(_)
        | DynamicTypeKind::Map { .. }
        | DynamicTypeKind::ExternalType { .. } => None,
    }
}

// ============================================================================
// Reading
// ============================================================================

/// The two deserializers, unified for the plan walk. `read_dheader` is the only
/// place they genuinely differ; XCDR1 never reaches it, because no XCDR1 plan
/// sets a `framed` flag.
trait PlanReader: ValueDeserializer + DeserializerReader<Error = CdrError> {
    fn read_dheader(&mut self) -> Result<u32, CdrError>;
}

impl PlanReader for CdrDeserializer<'_> {
    fn read_dheader(&mut self) -> Result<u32, CdrError> {
        Err(CdrError::DeserializationError("XCDR1 has no DHEADER".to_string()))
    }
}

impl PlanReader for Xcdr2Deserializer<'_> {
    fn read_dheader(&mut self) -> Result<u32, CdrError> {
        Xcdr2Deserializer::read_dheader(self)
    }
}

#[inline]
fn advance<R: PlanReader>(reader: &mut R, bytes: usize) -> DdsResult<()> {
    reader.check_available(bytes).map_err(cdr_error)?;
    reader.set_position(reader.get_position() + bytes);
    Ok(())
}

#[inline]
fn skip_scalar<R: PlanReader>(reader: &mut R, size: usize) -> DdsResult<()> {
    reader.align(size);
    advance(reader, size)
}

fn cdr_error(error: CdrError) -> DdsError {
    DdsError::Error(error.to_string())
}

/// Step over one value without decoding it.
fn skip_node<R: PlanReader>(reader: &mut R, node: &Node) -> DdsResult<()> {
    match node {
        Node::Prim(kind) => skip_scalar(reader, kind.size()),
        Node::Enum { width } | Node::Packed { width } => skip_scalar(reader, *width as usize),
        Node::Str => {
            let length = reader.deserialize_u32().map_err(cdr_error)? as usize;
            advance(reader, length)
        }
        Node::WStr => {
            // The length counts UTF-16 code units. Reading it left the position
            // 4-aligned, so the write path's align(2) moved nothing.
            let length = reader.deserialize_u32().map_err(cdr_error)? as usize;
            advance(reader, length.saturating_mul(2))
        }
        Node::Struct(node) => skip_struct(reader, node),
        Node::Seq { elem, framed_in, .. } => {
            if *framed_in {
                let size = reader.read_dheader().map_err(cdr_error)? as usize;
                return advance(reader, size);
            }
            let count = reader.deserialize_u32().map_err(cdr_error)?;
            skip_elements(reader, elem, count)
        }
        Node::Array { elem, count, framed_in, .. } => {
            if *framed_in {
                let size = reader.read_dheader().map_err(cdr_error)? as usize;
                return advance(reader, size);
            }
            skip_elements(reader, elem, *count)
        }
    }
}

fn skip_elements<R: PlanReader>(reader: &mut R, elem: &Node, count: u32) -> DdsResult<()> {
    // A run of same-width scalars is one alignment and one jump. The write path
    // skips the alignment on an empty run, so this must too.
    if let Some(size) = scalar_width(elem) {
        if count == 0 {
            return Ok(());
        }
        reader.align(size);
        return advance(reader, size.saturating_mul(count as usize));
    }
    for _ in 0..count {
        skip_node(reader, elem)?;
    }
    Ok(())
}

fn scalar_width(node: &Node) -> Option<usize> {
    match node {
        Node::Prim(kind) => Some(kind.size()),
        Node::Enum { width } | Node::Packed { width } => Some(*width as usize),
        _ => None,
    }
}

fn skip_struct<R: PlanReader>(reader: &mut R, node: &StructNode) -> DdsResult<()> {
    if node.framed {
        let size = reader.read_dheader().map_err(cdr_error)? as usize;
        return advance(reader, size);
    }
    for member in &node.members {
        skip_node(reader, &member.node)?;
    }
    Ok(())
}

// ============================================================================
// Key projection
// ============================================================================

impl CodecPlan {
    /// Project the key members of `bytes` into canonical key CDR. Returns the
    /// bytes and whether the single-string rule applied, mirroring the dynamic
    /// path's `serialize_key_cdr`.
    fn project_key(&self, bytes: &[u8]) -> DdsResult<(Vec<u8>, bool)> {
        // The key holder is always PLAIN_CDR2 big-endian with no encapsulation
        // header (XTypes 7.6.8 step 4). Writing the header keeps the alignment
        // math relative to it, then it is stripped.
        let mut out = Xcdr2Serializer::with_capacity(false, ExtensibilityKind::Final, 64);
        out.write_encapsulation_header().map_err(cdr_error)?;

        if is_xcdr2(bytes) {
            let mut reader = Xcdr2Deserializer::new(bytes).map_err(cdr_error)?;
            project_struct(&mut reader, &self.root, &mut out)?;
        } else {
            let mut reader = CdrDeserializer::new(bytes).map_err(cdr_error)?;
            project_struct(&mut reader, &self.root, &mut out)?;
        }

        let mut key = out.into_bytes();
        key.drain(..4);
        if self.single_string_key {
            while key.len() > 1 && key[key.len() - 1] == 0 && key[key.len() - 2] == 0 {
                key.pop();
            }
        }
        Ok((key, self.single_string_key))
    }
}

/// Walk a struct in wire order, then re-emit its projected members in key order.
///
/// The two orders differ whenever member ids are not declaration order, so the
/// first pass records where each member starts and the second seeks back to the
/// ones the key holder wants.
fn project_struct<R: PlanReader>(
    reader: &mut R,
    node: &StructNode,
    out: &mut Xcdr2Serializer,
) -> DdsResult<()> {
    let framed_end = if node.framed {
        let size = reader.read_dheader().map_err(cdr_error)? as usize;
        Some(reader.get_position() + size)
    } else {
        None
    };

    let mut starts = Vec::with_capacity(node.members.len());
    for member in &node.members {
        starts.push(reader.get_position());
        skip_node(reader, &member.node)?;
    }

    // An Appendable sample may carry members this plan does not know; the DHEADER
    // says where they end.
    let end = match framed_end {
        Some(framed_end) => {
            if reader.get_position() > framed_end {
                return Err(DdsError::Error(
                    "struct DHEADER is shorter than its known members".to_string(),
                ));
            }
            framed_end
        }
        None => reader.get_position(),
    };

    for &index in &node.key_order {
        reader.set_position(starts[index]);
        emit_node(reader, &node.members[index].node, out)?;
    }
    reader.set_position(end);
    Ok(())
}

fn emit_node<R: PlanReader>(
    reader: &mut R,
    node: &Node,
    out: &mut Xcdr2Serializer,
) -> DdsResult<()> {
    match node {
        Node::Prim(kind) => emit_primitive(reader, *kind, out),
        Node::Enum { width } => match width {
            1 => out.serialize_i8(reader.deserialize_i8().map_err(cdr_error)?),
            2 => out.serialize_i16(reader.deserialize_i16().map_err(cdr_error)?),
            _ => out.serialize_i32(reader.deserialize_i32().map_err(cdr_error)?),
        }
        .map_err(cdr_error),
        Node::Packed { width } => match width {
            1 => out.serialize_u8(reader.deserialize_u8().map_err(cdr_error)?),
            2 => out.serialize_u16(reader.deserialize_u16().map_err(cdr_error)?),
            4 => out.serialize_u32(reader.deserialize_u32().map_err(cdr_error)?),
            _ => out.serialize_u64(reader.deserialize_u64().map_err(cdr_error)?),
        }
        .map_err(cdr_error),
        Node::Str => {
            let value = reader.deserialize_string().map_err(cdr_error)?;
            out.serialize_string(&value).map_err(cdr_error)
        }
        Node::WStr => {
            let value = reader.deserialize_wstring16().map_err(cdr_error)?;
            out.serialize_wstring16(&value).map_err(cdr_error)
        }
        // A nested aggregate inside the key is itself a key holder: FINAL framing,
        // and only its own key members if it has any.
        Node::Struct(node) => project_struct(reader, node, out),
        Node::Seq { elem, framed_in, framed_out } => {
            if *framed_in {
                let _ = reader.read_dheader().map_err(cdr_error)?;
            }
            let count = reader.deserialize_u32().map_err(cdr_error)?;
            emit_framed(out, *framed_out, |out| {
                out.serialize_u32(count).map_err(cdr_error)?;
                for _ in 0..count {
                    emit_node(reader, elem, out)?;
                }
                Ok(())
            })
        }
        Node::Array { elem, count, framed_in, framed_out } => {
            if *framed_in {
                let _ = reader.read_dheader().map_err(cdr_error)?;
            }
            emit_framed(out, *framed_out, |out| {
                for _ in 0..*count {
                    emit_node(reader, elem, out)?;
                }
                Ok(())
            })
        }
    }
}

fn emit_framed<F>(out: &mut Xcdr2Serializer, framed: bool, write: F) -> DdsResult<()>
where
    F: FnOnce(&mut Xcdr2Serializer) -> DdsResult<()>,
{
    if !framed {
        return write(out);
    }
    let slot = out.reserve_dheader();
    let start = out.position();
    write(out)?;
    let size = (out.position() - start) as u32;
    out.write_dheader_at(slot, size);
    Ok(())
}

fn emit_primitive<R: PlanReader>(
    reader: &mut R,
    kind: PrimitiveKind,
    out: &mut Xcdr2Serializer,
) -> DdsResult<()> {
    match kind {
        PrimitiveKind::Boolean => out.serialize_bool(reader.deserialize_bool().map_err(cdr_error)?),
        PrimitiveKind::Byte | PrimitiveKind::Uint8 | PrimitiveKind::Char8 => {
            out.serialize_u8(reader.deserialize_u8().map_err(cdr_error)?)
        }
        PrimitiveKind::Int8 => out.serialize_i8(reader.deserialize_i8().map_err(cdr_error)?),
        PrimitiveKind::Int16 => out.serialize_i16(reader.deserialize_i16().map_err(cdr_error)?),
        PrimitiveKind::Uint16 => out.serialize_u16(reader.deserialize_u16().map_err(cdr_error)?),
        PrimitiveKind::Int32 => out.serialize_i32(reader.deserialize_i32().map_err(cdr_error)?),
        PrimitiveKind::Uint32 => out.serialize_u32(reader.deserialize_u32().map_err(cdr_error)?),
        PrimitiveKind::Int64 => out.serialize_i64(reader.deserialize_i64().map_err(cdr_error)?),
        PrimitiveKind::Uint64 => out.serialize_u64(reader.deserialize_u64().map_err(cdr_error)?),
        PrimitiveKind::Float32 => out.serialize_f32(reader.deserialize_f32().map_err(cdr_error)?),
        PrimitiveKind::Float64 => out.serialize_f64(reader.deserialize_f64().map_err(cdr_error)?),
        // Excluded at compile time.
        PrimitiveKind::Char16 | PrimitiveKind::Float128 => {
            return Err(DdsError::Error(format!("unsupported primitive in plan: {:?}", kind)))
        }
    }
    .map_err(cdr_error)
}

// ============================================================================
// Field access
// ============================================================================

impl CodecPlan {
    fn field_value(&self, bytes: &[u8], field_path: &str) -> DdsResult<Parameter> {
        let path: Vec<&str> = field_path.split('.').collect();
        if is_xcdr2(bytes) {
            let mut reader = Xcdr2Deserializer::new(bytes).map_err(cdr_error)?;
            read_field(&mut reader, &self.root, &path)
        } else {
            let mut reader = CdrDeserializer::new(bytes).map_err(cdr_error)?;
            read_field(&mut reader, &self.root, &path)
        }
    }

    fn has_field(&self, field_path: &str) -> bool {
        let mut node = &self.root;
        let mut segments = field_path.split('.').peekable();
        while let Some(name) = segments.next() {
            let Some(member) = node.members.iter().find(|m| &*m.name == name) else {
                return false;
            };
            if segments.peek().is_none() {
                return readable_as_parameter(&member.node);
            }
            match &member.node {
                Node::Struct(nested) => node = nested,
                _ => return false,
            }
        }
        false
    }
}

fn readable_as_parameter(node: &Node) -> bool {
    matches!(node, Node::Prim(_) | Node::Enum { .. } | Node::Packed { .. } | Node::Str | Node::WStr)
}

fn read_field<R: PlanReader>(
    reader: &mut R,
    node: &StructNode,
    path: &[&str],
) -> DdsResult<Parameter> {
    if node.framed {
        let _ = reader.read_dheader().map_err(cdr_error)?;
    }
    let target = node
        .members
        .iter()
        .position(|member| &*member.name == path[0])
        .ok_or_else(|| DdsError::Error(format!("Field '{}' not found", path[0])))?;

    for member in &node.members[..target] {
        skip_node(reader, &member.node)?;
    }

    let member = &node.members[target];
    if path.len() == 1 {
        return read_value(reader, &member.node);
    }
    match &member.node {
        Node::Struct(nested) => read_field(reader, nested, &path[1..]),
        _ => Err(DdsError::Error(format!("Field '{}' is not a struct", path[0]))),
    }
}

fn read_value<R: PlanReader>(reader: &mut R, node: &Node) -> DdsResult<Parameter> {
    let value = match node {
        Node::Prim(PrimitiveKind::Boolean) => {
            Parameter::IntegerValue(reader.deserialize_bool().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Int8) => {
            Parameter::IntegerValue(reader.deserialize_i8().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Int16) => {
            Parameter::IntegerValue(reader.deserialize_i16().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Int32) => {
            Parameter::IntegerValue(reader.deserialize_i32().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Int64) => {
            Parameter::IntegerValue(reader.deserialize_i64().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Uint8) | Node::Prim(PrimitiveKind::Byte) => {
            Parameter::IntegerValue(reader.deserialize_u8().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Uint16) => {
            Parameter::IntegerValue(reader.deserialize_u16().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Uint32) => {
            Parameter::IntegerValue(reader.deserialize_u32().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Uint64) => {
            Parameter::IntegerValue(reader.deserialize_u64().map_err(cdr_error)? as i128)
        }
        Node::Prim(PrimitiveKind::Float32) => {
            Parameter::FloatValue(reader.deserialize_f32().map_err(cdr_error)? as f64)
        }
        Node::Prim(PrimitiveKind::Float64) => {
            Parameter::FloatValue(reader.deserialize_f64().map_err(cdr_error)?)
        }
        Node::Prim(PrimitiveKind::Char8) => {
            Parameter::CharValue(reader.deserialize_u8().map_err(cdr_error)? as char)
        }
        Node::Prim(kind) => {
            return Err(DdsError::Error(format!("unsupported primitive in plan: {:?}", kind)))
        }
        // The wire value, not the literal name: the typed `FieldAccessor` the
        // derive macro emits has no enum arm at all, so there is nothing to match.
        Node::Enum { width } => Parameter::IntegerValue(match width {
            1 => reader.deserialize_i8().map_err(cdr_error)? as i128,
            2 => reader.deserialize_i16().map_err(cdr_error)? as i128,
            _ => reader.deserialize_i32().map_err(cdr_error)? as i128,
        }),
        Node::Packed { width } => Parameter::IntegerValue(match width {
            1 => reader.deserialize_u8().map_err(cdr_error)? as i128,
            2 => reader.deserialize_u16().map_err(cdr_error)? as i128,
            4 => reader.deserialize_u32().map_err(cdr_error)? as i128,
            _ => reader.deserialize_u64().map_err(cdr_error)? as i128,
        }),
        Node::Str => Parameter::String(reader.deserialize_string().map_err(cdr_error)?),
        Node::WStr => Parameter::String(reader.deserialize_wstring16().map_err(cdr_error)?),
        Node::Struct(_) | Node::Seq { .. } | Node::Array { .. } => {
            return Err(DdsError::Error("Aggregate fields are not filterable".to_string()))
        }
    };
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dcps::topic::type_support::SerializationFormat;
    use crate::xtypes::{
        plain_collection_equiv_kind, CollectionElementFlag, CompleteStructMember,
        CompleteStructType, CompleteTypeObject, CompleteUnionMember, CompleteUnionType,
        DynamicData, DynamicValue, EquivalenceHash, MemberFlag, PlainCollectionHeader,
        TryConstructKind, TypeFlag, TypeIdentifier, TypeRegistry,
    };

    use super::super::dynamic_serialization::{
        deserialize_dynamic_data, serialize_dynamic_data, serialize_key_cdr,
    };

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
        is_optional: bool,
    }

    fn field(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { name, id, type_id, is_key: false, is_optional: false }
    }

    fn key(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { name, id, type_id, is_key: true, is_optional: false }
    }

    fn struct_object(name: &str, ext: ExtensibilityKind, fields: Vec<Field>) -> CompleteTypeObject {
        let mut desc = CompleteStructType::new(tf(ext), name.into(), None);
        for f in fields {
            desc.add_member(CompleteStructMember::new(
                f.id,
                member_flag(f.is_optional, f.is_key),
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

    fn formats(ext: ExtensibilityKind) -> Vec<(&'static str, SerializationFormat)> {
        vec![
            ("xcdr1", SerializationFormat::Cdr),
            ("xcdr2", SerializationFormat::Xcdr { extensibility_kind: ext, use_delimiters: false }),
        ]
    }

    /// What the dynamic path produces for the same bytes: full deserialize, then
    /// the canonical key projection and the RTPS raw-vs-MD5 rule.
    fn oracle(bytes: &[u8], dynamic_type: &Arc<DynamicType>) -> (Vec<u8>, InstanceHandle) {
        let data = deserialize_dynamic_data(bytes, dynamic_type).expect("dynamic deserialize");
        let (key, _) = serialize_key_cdr(&data).expect("dynamic key");
        let handle = if key.is_empty() {
            InstanceHandle::NIL
        } else {
            match key_holder_max_size(dynamic_type) {
                Some(n) if n <= 16 => InstanceHandle::from_key_cdr(&key),
                _ => InstanceHandle::from_key_cdr_hashed(&key),
            }
        };
        (key, handle)
    }

    fn assert_matches_dynamic(label: &str, bytes: &[u8], dynamic_type: &Arc<DynamicType>) {
        let plans = TypePlans::compile(dynamic_type);
        let (want_key, want_handle) = oracle(bytes, dynamic_type);

        let got_key = plans
            .serialize_key(bytes)
            .unwrap_or_else(|| panic!("{label}: no plan compiled"))
            .unwrap_or_else(|e| panic!("{label}: plan key failed: {e:?}"));
        assert_eq!(got_key, want_key, "{label}: key CDR differs");

        let got_handle =
            plans.compute_key(bytes).unwrap_or_else(|| panic!("{label}: no plan compiled"));
        assert_eq!(got_handle, want_handle, "{label}: InstanceHandle differs");
    }

    /// Serialize `data` in every format the type supports and assert the plan
    /// reproduces the dynamic path byte for byte.
    fn check_all_formats(label: &str, data: &DynamicData, dynamic_type: &Arc<DynamicType>) {
        for (format_name, format) in formats(dynamic_type.extensibility()) {
            let bytes = serialize_dynamic_data(data, &format).expect("dynamic serialize");
            assert_matches_dynamic(&format!("{label}/{format_name}"), &bytes, dynamic_type);
        }
    }

    fn flat_type(ext: ExtensibilityKind) -> Arc<DynamicType> {
        build(struct_object(
            "Flat",
            ext,
            vec![
                key("id", 0, TypeIdentifier::Int32),
                field("count", 1, TypeIdentifier::Int64),
                field("name", 2, TypeIdentifier::String8Small { bound: 0 }),
                field("ratio", 3, TypeIdentifier::Float64),
            ],
        ))
    }

    fn flat_data(dynamic_type: &Arc<DynamicType>) -> DynamicData {
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 7i32).unwrap();
        data.set("count", -9_000_000_000i64).unwrap();
        data.set("name", "hello".to_string()).unwrap();
        data.set("ratio", 1.5f64).unwrap();
        data
    }

    #[test]
    fn flat_struct_key_matches_dynamic_path() {
        for ext in [ExtensibilityKind::Final, ExtensibilityKind::Appendable] {
            let dynamic_type = flat_type(ext);
            let data = flat_data(&dynamic_type);
            check_all_formats(&format!("flat/{ext:?}"), &data, &dynamic_type);
        }
    }

    #[test]
    fn string_key_matches_dynamic_path() {
        for ext in [ExtensibilityKind::Final, ExtensibilityKind::Appendable] {
            let dynamic_type = build(struct_object(
                "StringKey",
                ext,
                vec![
                    key("name", 0, TypeIdentifier::String8Small { bound: 0 }),
                    field("count", 1, TypeIdentifier::Int32),
                ],
            ));
            let mut data = DynamicData::new(dynamic_type.clone());
            data.set("name", "BLUE".to_string()).unwrap();
            data.set("count", 3i32).unwrap();
            check_all_formats(&format!("string-key/{ext:?}"), &data, &dynamic_type);
        }
    }

    /// The key holder writes members in member_id order while the wire carries
    /// them in declaration order; here the two disagree.
    #[test]
    fn key_order_follows_member_id_not_declaration_order() {
        let dynamic_type = build(struct_object(
            "Reordered",
            ExtensibilityKind::Final,
            vec![
                key("second", 5, TypeIdentifier::Int32),
                field("filler", 6, TypeIdentifier::Int16),
                key("first", 1, TypeIdentifier::Int32),
            ],
        ));
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("second", 0x2222_2222i32).unwrap();
        data.set("filler", 9i16).unwrap();
        data.set("first", 0x1111_1111i32).unwrap();

        check_all_formats("reordered", &data, &dynamic_type);

        // And the ordering is observable: `first` (id 1) precedes `second` (id 5).
        let bytes =
            serialize_dynamic_data(&data, &SerializationFormat::Cdr).expect("dynamic serialize");
        let plans = TypePlans::compile(&dynamic_type);
        let projected = plans.serialize_key(&bytes).unwrap().unwrap();
        assert_eq!(projected, vec![0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x22, 0x22]);
    }

    #[test]
    fn keyless_type_projects_no_key() {
        let dynamic_type = build(struct_object(
            "Keyless",
            ExtensibilityKind::Final,
            vec![field("a", 0, TypeIdentifier::Int32)],
        ));
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("a", 1i32).unwrap();
        let bytes =
            serialize_dynamic_data(&data, &SerializationFormat::Cdr).expect("dynamic serialize");

        let plans = TypePlans::compile(&dynamic_type);
        assert_eq!(plans.serialize_key(&bytes).unwrap().unwrap(), Vec::<u8>::new());
        assert_eq!(plans.compute_key(&bytes).unwrap(), InstanceHandle::NIL);
    }

    fn nested_registry(
        inner_ext: ExtensibilityKind,
        inner_has_key: bool,
    ) -> (TypeRegistry, EquivalenceHash) {
        let inner = struct_object(
            "Inner",
            inner_ext,
            vec![
                Field {
                    name: "a",
                    id: 0,
                    type_id: TypeIdentifier::Int32,
                    is_key: inner_has_key,
                    is_optional: false,
                },
                field("b", 1, TypeIdentifier::Int32),
            ],
        );
        let hash = EquivalenceHash::compute(&inner.serialize());
        let mut registry = TypeRegistry::new();
        registry.register_complete(hash, "Inner".into(), inner);
        (registry, hash)
    }

    /// A nested aggregate inside the key is projected as its own key holder:
    /// only its `@key` members when it has any, all of them when it has none.
    #[test]
    fn nested_struct_key_matches_dynamic_path() {
        for inner_has_key in [true, false] {
            for ext in [ExtensibilityKind::Final, ExtensibilityKind::Appendable] {
                let (registry, hash) = nested_registry(ext, inner_has_key);
                let dynamic_type = build_with(
                    struct_object(
                        "Outer",
                        ext,
                        vec![
                            field("id", 0, TypeIdentifier::Int32),
                            key("child", 1, TypeIdentifier::CompleteTypeId(hash)),
                        ],
                    ),
                    &registry,
                );

                let inner_type = match &dynamic_type.get_member("child").unwrap().member_type {
                    DynamicTypeKind::TypeRef(inner) => inner.clone(),
                    other => panic!("nested member did not resolve: {other:?}"),
                };
                let mut inner = DynamicData::new(inner_type);
                inner.set("a", 11i32).unwrap();
                inner.set("b", 22i32).unwrap();

                let mut data = DynamicData::new(dynamic_type.clone());
                data.set("id", 7i32).unwrap();
                data.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();

                check_all_formats(
                    &format!("nested/{ext:?}/inner_key={inner_has_key}"),
                    &data,
                    &dynamic_type,
                );
            }
        }
    }

    /// Sequences sit between the key members and must be stepped over exactly:
    /// a primitive element carries no DHEADER, a string element does (XCDR2).
    #[test]
    fn sequences_are_skipped_to_reach_a_later_key() {
        for ext in [ExtensibilityKind::Final, ExtensibilityKind::Appendable] {
            let dynamic_type = build(struct_object(
                "WithSequences",
                ext,
                vec![
                    field("values", 0, sequence_of(TypeIdentifier::Int64)),
                    field("names", 1, sequence_of(TypeIdentifier::String8Small { bound: 0 })),
                    key("id", 2, TypeIdentifier::Int32),
                ],
            ));
            let mut data = DynamicData::new(dynamic_type.clone());
            data.set_value(
                "values",
                DynamicValue::Sequence(vec![
                    DynamicValue::Int64(1),
                    DynamicValue::Int64(2),
                    DynamicValue::Int64(3),
                ]),
            )
            .unwrap();
            data.set_value(
                "names",
                DynamicValue::Sequence(vec![
                    DynamicValue::String("a".into()),
                    DynamicValue::String("bb".into()),
                ]),
            )
            .unwrap();
            data.set("id", 42i32).unwrap();
            check_all_formats(&format!("sequences/{ext:?}"), &data, &dynamic_type);

            // An empty run must not consume the alignment padding it never wrote.
            let mut empty = DynamicData::new(dynamic_type.clone());
            empty.set_value("values", DynamicValue::Sequence(Vec::new())).unwrap();
            empty.set_value("names", DynamicValue::Sequence(Vec::new())).unwrap();
            empty.set("id", 42i32).unwrap();
            check_all_formats(&format!("empty-sequences/{ext:?}"), &empty, &dynamic_type);
        }
    }

    /// A key member that is itself a sequence keeps the collection DHEADER inside
    /// the key holder (XTypes 7.6.8 rules 9/12 hold for any extensibility).
    #[test]
    fn sequence_key_member_matches_dynamic_path() {
        let dynamic_type = build(struct_object(
            "SeqKey",
            ExtensibilityKind::Final,
            vec![
                Field {
                    name: "names",
                    id: 0,
                    type_id: sequence_of(TypeIdentifier::String8Small { bound: 0 }),
                    is_key: true,
                    is_optional: false,
                },
                field("filler", 1, TypeIdentifier::Int32),
            ],
        ));
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set_value(
            "names",
            DynamicValue::Sequence(vec![
                DynamicValue::String("a".into()),
                DynamicValue::String("bb".into()),
            ]),
        )
        .unwrap();
        data.set("filler", 1i32).unwrap();
        check_all_formats("sequence-key", &data, &dynamic_type);
    }

    /// Endianness is a runtime input, not part of the plan. `serialize_dynamic_data`
    /// only emits little-endian, so this vector is written by hand.
    #[test]
    fn big_endian_sample_matches_dynamic_path() {
        let dynamic_type = flat_type(ExtensibilityKind::Final);

        let mut bytes = vec![0x00, 0x00, 0x00, 0x00]; // CDR_BE
        bytes.extend_from_slice(&7i32.to_be_bytes());
        bytes.extend_from_slice(&[0u8; 4]); // pad to 8 for the i64
        bytes.extend_from_slice(&(-9_000_000_000i64).to_be_bytes());
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(b"hello\0");
        bytes.extend_from_slice(&[0u8; 6]); // pad to 8 for the f64
        bytes.extend_from_slice(&1.5f64.to_be_bytes());
        assert_matches_dynamic("flat/xcdr1-be", &bytes, &dynamic_type);

        let mut bytes = vec![0x00, 0x06, 0x00, 0x00]; // PLAIN_CDR2_BE
        bytes.extend_from_slice(&7i32.to_be_bytes());
        bytes.extend_from_slice(&(-9_000_000_000i64).to_be_bytes());
        bytes.extend_from_slice(&6u32.to_be_bytes());
        bytes.extend_from_slice(b"hello\0");
        bytes.extend_from_slice(&[0u8; 2]); // pad to 4 for the f64
        bytes.extend_from_slice(&1.5f64.to_be_bytes());
        assert_matches_dynamic("flat/xcdr2-be", &bytes, &dynamic_type);
    }

    /// An Appendable writer may append members this reader does not know; the
    /// DHEADER says where they end, and the walk must land there.
    #[test]
    fn appendable_sample_with_unknown_trailing_members() {
        let dynamic_type = build(struct_object(
            "Appended",
            ExtensibilityKind::Appendable,
            vec![
                key("id", 0, TypeIdentifier::Int32),
                field("name", 1, TypeIdentifier::String8Small { bound: 0 }),
            ],
        ));

        let mut body = Vec::new();
        body.extend_from_slice(&7i32.to_le_bytes());
        body.extend_from_slice(&6u32.to_le_bytes());
        body.extend_from_slice(b"hello\0");
        body.extend_from_slice(&[0u8; 2]); // pad
        body.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes()); // member this plan cannot see

        let mut bytes = vec![0x00, 0x09, 0x00, 0x00]; // DELIMITED_CDR2_LE
        bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&body);

        assert_matches_dynamic("appendable-trailing", &bytes, &dynamic_type);
    }

    #[test]
    fn shapes_outside_the_subset_compile_to_no_plan() {
        let mutable = build(struct_object(
            "Mutable",
            ExtensibilityKind::Mutable,
            vec![key("id", 0, TypeIdentifier::Int32)],
        ));
        assert!(TypePlans::compile(&mutable).xcdr2.is_none());
        assert!(TypePlans::compile(&mutable).xcdr1.is_none());

        let optional = build(struct_object(
            "Optional",
            ExtensibilityKind::Final,
            vec![
                key("id", 0, TypeIdentifier::Int32),
                Field {
                    name: "maybe",
                    id: 1,
                    type_id: TypeIdentifier::Int32,
                    is_key: false,
                    is_optional: true,
                },
            ],
        ));
        assert!(TypePlans::compile(&optional).xcdr2.is_none());

        let wide = build(struct_object(
            "Wide",
            ExtensibilityKind::Final,
            vec![key("c", 0, TypeIdentifier::Char16)],
        ));
        assert!(TypePlans::compile(&wide).xcdr2.is_none());

        let mut union_desc = CompleteUnionType::new(
            tf(ExtensibilityKind::Final),
            member_flag(false, false),
            TypeIdentifier::Int32,
            "U".to_string(),
        );
        union_desc.add_member(CompleteUnionMember::new(
            0,
            member_flag(false, false),
            TypeIdentifier::Int32,
            vec![0],
            "a".to_string(),
        ));
        let union_object = CompleteTypeObject::Union(union_desc);
        let hash = EquivalenceHash::compute(&union_object.serialize());
        let mut registry = TypeRegistry::new();
        registry.register_complete(hash, "U".into(), union_object);
        let with_union = build_with(
            struct_object(
                "WithUnion",
                ExtensibilityKind::Final,
                vec![
                    key("id", 0, TypeIdentifier::Int32),
                    field("choice", 1, TypeIdentifier::CompleteTypeId(hash)),
                ],
            ),
            &registry,
        );
        assert!(TypePlans::compile(&with_union).xcdr2.is_none());
    }

    #[test]
    fn field_values_read_from_the_wire() {
        let dynamic_type = flat_type(ExtensibilityKind::Appendable);
        let data = flat_data(&dynamic_type);
        let plans = TypePlans::compile(&dynamic_type);

        for (format_name, format) in formats(ExtensibilityKind::Appendable) {
            let bytes = serialize_dynamic_data(&data, &format).expect("dynamic serialize");
            let read = |name: &str| plans.field_value(&bytes, name).unwrap().unwrap();
            assert_eq!(read("id"), Parameter::IntegerValue(7), "{format_name}");
            assert_eq!(read("count"), Parameter::IntegerValue(-9_000_000_000), "{format_name}");
            assert_eq!(read("name"), Parameter::String("hello".into()), "{format_name}");
            assert_eq!(read("ratio"), Parameter::FloatValue(1.5), "{format_name}");
            assert!(plans.field_value(&bytes, "missing").unwrap().is_err(), "{format_name}");
        }

        assert!(plans.has_field("id"));
        assert!(plans.has_field("ratio"));
        assert!(!plans.has_field("missing"));
    }

    /// The flat descriptor path this replaces could not reach a nested member.
    #[test]
    fn nested_field_values_are_reachable_by_path() {
        let (registry, hash) = nested_registry(ExtensibilityKind::Final, false);
        let dynamic_type = build_with(
            struct_object(
                "Outer",
                ExtensibilityKind::Final,
                vec![
                    key("id", 0, TypeIdentifier::Int32),
                    field("child", 1, TypeIdentifier::CompleteTypeId(hash)),
                ],
            ),
            &registry,
        );

        let inner_type = match &dynamic_type.get_member("child").unwrap().member_type {
            DynamicTypeKind::TypeRef(inner) => inner.clone(),
            other => panic!("nested member did not resolve: {other:?}"),
        };
        let mut inner = DynamicData::new(inner_type);
        inner.set("a", 11i32).unwrap();
        inner.set("b", 22i32).unwrap();

        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 7i32).unwrap();
        data.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();

        let bytes =
            serialize_dynamic_data(&data, &SerializationFormat::Cdr).expect("dynamic serialize");
        let plans = TypePlans::compile(&dynamic_type);
        assert_eq!(
            plans.field_value(&bytes, "child.b").unwrap().unwrap(),
            Parameter::IntegerValue(22)
        );
        assert!(plans.has_field("child.a"));
        assert!(!plans.has_field("child"));
    }
}
