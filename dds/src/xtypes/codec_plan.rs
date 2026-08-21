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
//! Coverage is structs of every extensibility — Final, Appendable and Mutable,
//! the last in both its XCDR2 EMHEADER and XCDR1 PL_CDR framings — holding
//! primitives, strings, enums, bitmask/bitset, optional members, nested structs,
//! arrays and sequences. Unions, maps, `char16` and `float128` compile to `None`
//! and the caller keeps the dynamic path: the last two because the dynamic codec
//! reads them asymmetrically, so matching it here would mean reproducing that
//! rather than a wire rule, and maps because their count field is an open spec
//! question (OMG DDSXTY14-72) that should not be answered twice.

use std::sync::Arc;

use crate::common::instance_handle::InstanceHandle;
use crate::dcps::core::error::{DdsError, DdsResult};
use crate::serialize::cdr::{
    CdrDeserializer, CdrError, ExtensibilityKind, PlCdrMemberHeader, PrimitiveSerialize,
    StringSerialize, Xcdr2Deserializer, Xcdr2Serializer,
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
    member_id: u32,
    /// Only meaningful under `Framing::Plain`/`Delimited`, where an optional member
    /// spends a presence marker. Tagged framings express absence by omitting the
    /// member's header entirely.
    optional: bool,
    node: Node,
}

/// How a struct's members are laid out, which is the one place representation and
/// extensibility meet.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Framing {
    /// Members inline in declaration order: XCDR1 Final/Appendable, XCDR2 Final.
    Plain,
    /// XCDR2 Appendable: a DHEADER, then members inline.
    Delimited,
    /// XCDR2 Mutable: a DHEADER, then EMHEADER-tagged members in any order.
    TaggedDelimited,
    /// XCDR1 Mutable (XTypes PL_CDR): PID-tagged members closed by PID_SENTINEL.
    TaggedSentinel,
}

#[derive(Debug)]
struct StructNode {
    /// Members in declaration order, which is also wire order unless `framing` is
    /// tagged.
    members: Vec<MemberNode>,
    /// Indices to project into the key holder. The `@key` members in member_id
    /// order, or — when the struct has none — every member in declaration order,
    /// which is the RTPS key-holder rule for a nested aggregate (DDSI-RTPS 9.6.4.8).
    key_order: Vec<usize>,
    framing: Framing,
}

impl StructNode {
    fn index_of_id(&self, member_id: u32) -> Option<usize> {
        self.members.iter().position(|member| member.member_id == member_id)
    }
}

/// One tagged member's header.
struct MemberTag {
    id: u32,
    length: u32,
    must_understand: bool,
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
        Some(plan.handle_for(&key))
    }

    /// Both key answers from a single projection, for the write path that needs
    /// the key CDR and its handle together.
    pub fn key_info(&self, bytes: &[u8]) -> Option<DdsResult<(Vec<u8>, InstanceHandle)>> {
        if !self.has_key {
            return Some(Ok((Vec::new(), InstanceHandle::NIL)));
        }
        let plan = self.plan_for(bytes)?;
        Some(plan.project_key(bytes).map(|(key, _)| {
            let handle = plan.handle_for(&key);
            (key, handle)
        }))
    }

    /// Read one field of `bytes` by name, `.`-separated for nested structs.
    pub fn field_value(&self, bytes: &[u8], field_path: &str) -> Option<DdsResult<Parameter>> {
        let plan = self.plan_for(bytes)?;
        Some(plan.field_value(bytes, field_path))
    }

    /// Whether `field_path` names a readable field. Answered from either plan,
    /// since the two describe the same members.
    pub fn has_field(&self, field_path: &str) -> bool {
        self.filter_has_field(field_path).unwrap_or(false)
    }

    /// `has_field` distinguishing "no plan compiled" (`None`) from "the plan does
    /// not know this field" (`Some(false)`), for creation-time filter validation.
    pub fn filter_has_field(&self, field_path: &str) -> Option<bool> {
        self.xcdr2.as_ref().or(self.xcdr1.as_ref()).map(|plan| plan.has_field(field_path))
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

    /// XTypes 7.6.8 step 5: the key holder goes in raw when its *maximum* size
    /// fits a KeyHash, else MD5.
    fn handle_for(&self, key: &[u8]) -> InstanceHandle {
        if key.is_empty() {
            return InstanceHandle::NIL;
        }
        match self.key_holder_max {
            Some(n) if n <= 16 => InstanceHandle::from_key_cdr(key),
            _ => InstanceHandle::from_key_cdr_hashed(key),
        }
    }
}

fn compile_struct(dynamic_type: &DynamicType, xcdr2: bool) -> Option<Arc<StructNode>> {
    let struct_desc = dynamic_type.as_struct()?;

    // Whether a shape compiles must not depend on the representation: `has_field`
    // answers from whichever plan exists, so a member present in one and absent in
    // the other would make a filter claim a field it then cannot read.
    let framing = match (dynamic_type.extensibility(), xcdr2) {
        (ExtensibilityKind::Mutable, true) => Framing::TaggedDelimited,
        (ExtensibilityKind::Mutable, false) => Framing::TaggedSentinel,
        (ExtensibilityKind::Appendable, true) => Framing::Delimited,
        _ => Framing::Plain,
    };

    let mut members = Vec::with_capacity(struct_desc.member_count());
    let mut keys: Vec<(u32, usize)> = Vec::new();
    for (index, member) in struct_desc.members().iter().enumerate() {
        members.push(MemberNode {
            name: member.name.clone(),
            member_id: member.member_id,
            optional: member.is_optional,
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

    Some(Arc::new(StructNode { members, key_order, framing }))
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

/// The two deserializers, unified for the plan walk. Each method is reached only
/// from the framing that uses it, so the other side's arm is unreachable rather
/// than approximate.
trait PlanReader: ValueDeserializer + DeserializerReader<Error = CdrError> {
    fn read_dheader(&mut self) -> Result<u32, CdrError>;

    /// The next tagged member, or `None` at a terminator the framing carries
    /// itself — the PL_CDR sentinel. XCDR2 is bounded by its DHEADER instead and
    /// always yields a member.
    fn read_member_tag(&mut self) -> Result<Option<MemberTag>, CdrError>;

    /// The presence marker an optional member spends in a non-tagged struct.
    fn read_optional_presence(&mut self) -> Result<bool, CdrError>;
}

impl PlanReader for CdrDeserializer<'_> {
    fn read_dheader(&mut self) -> Result<u32, CdrError> {
        Err(CdrError::DeserializationError("XCDR1 has no DHEADER".to_string()))
    }

    fn read_member_tag(&mut self) -> Result<Option<MemberTag>, CdrError> {
        Ok(match self.read_parameter_header()? {
            PlCdrMemberHeader::Sentinel => None,
            PlCdrMemberHeader::Short { pid, length, must_understand } => {
                Some(MemberTag { id: pid as u32, length: length as u32, must_understand })
            }
            PlCdrMemberHeader::Long { member_id, length, must_understand } => {
                Some(MemberTag { id: member_id, length, must_understand })
            }
        })
    }

    fn read_optional_presence(&mut self) -> Result<bool, CdrError> {
        match self.read_parameter_header()? {
            PlCdrMemberHeader::Short { length: 0, .. }
            | PlCdrMemberHeader::Long { length: 0, .. } => Ok(false),
            PlCdrMemberHeader::Short { .. } | PlCdrMemberHeader::Long { .. } => Ok(true),
            PlCdrMemberHeader::Sentinel => Err(CdrError::DeserializationError(
                "PID_SENTINEL where an optional member header was expected".to_string(),
            )),
        }
    }
}

impl PlanReader for Xcdr2Deserializer<'_> {
    fn read_dheader(&mut self) -> Result<u32, CdrError> {
        Xcdr2Deserializer::read_dheader(self)
    }

    fn read_member_tag(&mut self) -> Result<Option<MemberTag>, CdrError> {
        let (id, length, must_understand) = self.read_member_header_full()?;
        Ok(Some(MemberTag { id, length, must_understand }))
    }

    fn read_optional_presence(&mut self) -> Result<bool, CdrError> {
        self.deserialize_bool()
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
        // A collection's DHEADER is read and discarded rather than jumped, which
        // is what the dynamic path does with it: the elements state their own
        // extent, and jumping would swallow every error inside them.
        Node::Seq { elem, framed_in, .. } => {
            if *framed_in {
                let _ = reader.read_dheader().map_err(cdr_error)?;
            }
            let count = reader.deserialize_u32().map_err(cdr_error)?;
            skip_elements(reader, elem, count)
        }
        Node::Array { elem, count, framed_in, .. } => {
            if *framed_in {
                let _ = reader.read_dheader().map_err(cdr_error)?;
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

/// Step over one struct without decoding it — the same walk, discarding where
/// the members sat.
fn skip_struct<R: PlanReader>(reader: &mut R, node: &StructNode) -> DdsResult<()> {
    let end = walk_struct(reader, node, None)?;
    reader.set_position(end);
    Ok(())
}

/// Where each member's value begins in this sample, `None` for one the sample
/// omits, plus the position just past the struct.
///
/// This is the single wire walk every consumer shares: key projection seeks back
/// into it in member_id order, field access seeks to one entry.
fn locate_members<R: PlanReader>(
    reader: &mut R,
    node: &StructNode,
) -> DdsResult<(Vec<Option<usize>>, usize)> {
    let mut starts = vec![None; node.members.len()];
    let end = walk_struct(reader, node, Some(&mut starts))?;
    Ok((starts, end))
}

/// Walk one struct's members — recording where each begins when `starts` is
/// given — and return the position just past it.
///
/// Every framing is walked, never jumped, even when nothing here is read: a
/// DHEADER states an extent but says nothing about what is inside it, and the
/// dynamic path refuses an unknown must-understand member, a truncated string or
/// an overlong sequence wherever it sits. Jumping to the extent would hand back a
/// key for a sample the fallback rejects.
///
/// The extent is still what closes the struct, because an Appendable or Mutable
/// sample may carry trailing members this plan cannot name — refusing those would
/// be the opposite divergence. A collection's DHEADER (`skip_node`) is the other
/// case and is *not* an extent to resume from: the dynamic path discards it.
fn walk_struct<R: PlanReader>(
    reader: &mut R,
    node: &StructNode,
    mut starts: Option<&mut [Option<usize>]>,
) -> DdsResult<usize> {
    let declared_end = match node.framing {
        Framing::Delimited | Framing::TaggedDelimited => {
            let size = reader.read_dheader().map_err(cdr_error)? as usize;
            Some(reader.get_position() + size)
        }
        _ => None,
    };

    match node.framing {
        Framing::Plain | Framing::Delimited => {
            for (index, member) in node.members.iter().enumerate() {
                if member.optional && !reader.read_optional_presence().map_err(cdr_error)? {
                    continue;
                }
                if let Some(starts) = starts.as_deref_mut() {
                    starts[index] = Some(reader.get_position());
                }
                skip_node(reader, &member.node)?;
            }
        }
        Framing::TaggedDelimited | Framing::TaggedSentinel => {
            let end = declared_end.unwrap_or(usize::MAX);
            while reader.get_position() < end {
                let Some(tag) = reader.read_member_tag().map_err(cdr_error)? else {
                    break;
                };
                let start = reader.get_position();
                match node.index_of_id(tag.id) {
                    Some(index) => {
                        if let Some(starts) = starts.as_deref_mut() {
                            starts[index] = Some(start);
                        }
                        skip_node(reader, &node.members[index].node)?;
                    }
                    // Refusing an unknown must-understand member is what the
                    // dynamic path does, and a plan that silently skipped one
                    // would hand back a key the fallback would have refused.
                    None if tag.must_understand => {
                        return Err(DdsError::Error(format!(
                            "Unknown required member with id {}",
                            tag.id
                        )))
                    }
                    None => advance(reader, tag.length as usize)?,
                }
                // A PL_CDR parameter's declared length includes padding the value
                // itself does not read. An EMHEADER's does not, and the dynamic
                // path reconciles only the former — following it matters more
                // than the rule, since a divergence here moves every key.
                if node.framing == Framing::TaggedSentinel {
                    let consumed = reader.get_position() - start;
                    if consumed < tag.length as usize {
                        advance(reader, tag.length as usize - consumed)?;
                    }
                }
            }
        }
    }

    // An Appendable or Mutable sample may carry members this plan does not know;
    // the DHEADER says where they end.
    let end = match declared_end {
        Some(declared_end) => {
            if reader.get_position() > declared_end {
                return Err(DdsError::Error(
                    "struct DHEADER is shorter than its known members".to_string(),
                ));
            }
            declared_end
        }
        None => reader.get_position(),
    };
    Ok(end)
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

/// Locate the struct's members, then re-emit the projected ones in key order.
///
/// Wire order and key order differ whenever member ids are not declaration order
/// — and under a tagged framing the wire order is the writer's choice entirely —
/// so the walk records where each member starts and this seeks back to the ones
/// the key holder wants.
fn project_struct<R: PlanReader>(
    reader: &mut R,
    node: &StructNode,
    out: &mut Xcdr2Serializer,
) -> DdsResult<()> {
    let (starts, end) = locate_members(reader, node)?;

    for &index in &node.key_order {
        // The dynamic path substitutes the type's default for an absent non-optional
        // member and refuses an absent optional one. Refusing both sends the caller
        // back to it, which is where those two answers already live — reproducing
        // default emission here would be a second copy of them.
        let start = starts[index].ok_or_else(|| {
            DdsError::Error(format!("Missing key member value for '{}'", node.members[index].name))
        })?;
        reader.set_position(start);
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
    let target = node
        .members
        .iter()
        .position(|member| &*member.name == path[0])
        .ok_or_else(|| DdsError::Error(format!("Field '{}' not found", path[0])))?;

    // The same walk the key projection uses: under a tagged framing the target's
    // offset is not derivable from the members before it, and an optional member
    // may not be in the sample at all.
    let (starts, _) = locate_members(reader, node)?;
    // An absent member (unset optional, or one this writer's type lacks) is a
    // value-level fact, not an error: comparisons involving it evaluate false.
    let Some(start) = starts[target] else {
        return Ok(Parameter::Unset);
    };
    reader.set_position(start);

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
        member_flag_mu(is_optional, is_key, false)
    }

    fn member_flag_mu(is_optional: bool, is_key: bool, must_understand: bool) -> MemberFlag {
        MemberFlag::new(
            TryConstructKind::Discard,
            false,
            is_optional,
            must_understand,
            is_key,
            false,
        )
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
        must_understand: bool,
    }

    fn field(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { name, id, type_id, is_key: false, is_optional: false, must_understand: false }
    }

    fn key(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { is_key: true, ..field(name, id, type_id) }
    }

    fn optional(name: &'static str, id: u32, type_id: TypeIdentifier) -> Field {
        Field { is_optional: true, ..field(name, id, type_id) }
    }

    fn struct_object(name: &str, ext: ExtensibilityKind, fields: Vec<Field>) -> CompleteTypeObject {
        let mut desc = CompleteStructType::new(tf(ext), name.into(), None);
        for f in fields {
            desc.add_member(CompleteStructMember::new(
                f.id,
                member_flag_mu(f.is_optional, f.is_key, f.must_understand),
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
                Field { is_key: inner_has_key, ..field("a", 0, TypeIdentifier::Int32) },
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
            for ext in [
                ExtensibilityKind::Final,
                ExtensibilityKind::Appendable,
                ExtensibilityKind::Mutable,
            ] {
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
                key("names", 0, sequence_of(TypeIdentifier::String8Small { bound: 0 })),
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

    fn mutable_type(fields: Vec<Field>) -> Arc<DynamicType> {
        build(struct_object("Mut", ExtensibilityKind::Mutable, fields))
    }

    /// Mutable members carry their own id and length, so wire order is the
    /// writer's choice and the projection has to find its keys by id.
    #[test]
    fn mutable_struct_key_matches_dynamic_path() {
        let dynamic_type = mutable_type(vec![
            key("second", 5, TypeIdentifier::Int32),
            field("name", 6, TypeIdentifier::String8Small { bound: 0 }),
            key("first", 1, TypeIdentifier::Int32),
            field("ratio", 7, TypeIdentifier::Float64),
        ]);
        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("second", 0x2222_2222i32).unwrap();
        data.set("name", "hello".to_string()).unwrap();
        data.set("first", 0x1111_1111i32).unwrap();
        data.set("ratio", 1.5f64).unwrap();
        check_all_formats("mutable", &data, &dynamic_type);

        // Member id order, not the order the writer chose.
        let bytes = serialize_dynamic_data(&data, &SerializationFormat::Cdr).unwrap();
        let projected = TypePlans::compile(&dynamic_type).serialize_key(&bytes).unwrap().unwrap();
        assert_eq!(projected, vec![0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x22, 0x22]);
    }

    /// A member the reader's type does not declare is stepped over by the length
    /// its own header states — unless it demands to be understood, which the
    /// dynamic path refuses and so must this.
    #[test]
    fn mutable_sample_with_members_the_plan_cannot_name() {
        let reader_type = mutable_type(vec![
            key("id", 1, TypeIdentifier::Int32),
            field("name", 6, TypeIdentifier::String8Small { bound: 0 }),
        ]);

        for must_understand in [false, true] {
            let writer_type = mutable_type(vec![
                key("id", 1, TypeIdentifier::Int32),
                Field { must_understand, ..field("extra", 4, TypeIdentifier::Float64) },
                field("name", 6, TypeIdentifier::String8Small { bound: 0 }),
            ]);
            let mut data = DynamicData::new(writer_type.clone());
            data.set("id", 7i32).unwrap();
            data.set("extra", 2.5f64).unwrap();
            data.set("name", "hello".to_string()).unwrap();

            for (format_name, format) in formats(ExtensibilityKind::Mutable) {
                let label = format!("mutable-unknown/mu={must_understand}/{format_name}");
                let bytes = serialize_dynamic_data(&data, &format).unwrap();
                if must_understand {
                    let plans = TypePlans::compile(&reader_type);
                    assert!(plans.serialize_key(&bytes).unwrap().is_err(), "{label}");
                    assert!(deserialize_dynamic_data(&bytes, &reader_type).is_err(), "{label}");
                } else {
                    assert_matches_dynamic(&label, &bytes, &reader_type);
                }
            }
        }
    }

    /// A tagged struct nobody reads still has to refuse a member that demands to
    /// be understood: the dynamic path refuses it wherever it sits, so skipping
    /// past one would hand back a key the fallback would never have produced.
    #[test]
    fn unknown_must_understand_member_inside_a_skipped_nested_struct() {
        fn outer_with(inner: CompleteTypeObject) -> Arc<DynamicType> {
            let hash = EquivalenceHash::compute(&inner.serialize());
            let mut registry = TypeRegistry::new();
            registry.register_complete(hash, "Inner".into(), inner);
            build_with(
                struct_object(
                    "Outer",
                    ExtensibilityKind::Final,
                    vec![
                        field("child", 0, TypeIdentifier::CompleteTypeId(hash)),
                        key("id", 1, TypeIdentifier::Int32),
                    ],
                ),
                &registry,
            )
        }

        let writer_type = outer_with(struct_object(
            "Inner",
            ExtensibilityKind::Mutable,
            vec![
                field("a", 0, TypeIdentifier::Int32),
                Field { must_understand: true, ..field("secret", 9, TypeIdentifier::Int32) },
            ],
        ));
        let reader_type = outer_with(struct_object(
            "Inner",
            ExtensibilityKind::Mutable,
            vec![field("a", 0, TypeIdentifier::Int32)],
        ));

        let inner_type = match &writer_type.get_member("child").unwrap().member_type {
            DynamicTypeKind::TypeRef(inner) => inner.clone(),
            other => panic!("nested member did not resolve: {other:?}"),
        };
        let mut inner = DynamicData::new(inner_type);
        inner.set("a", 11i32).unwrap();
        inner.set("secret", 99i32).unwrap();
        let mut data = DynamicData::new(writer_type.clone());
        data.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();
        data.set("id", 7i32).unwrap();

        for (format_name, format) in formats(ExtensibilityKind::Final) {
            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let plans = TypePlans::compile(&reader_type);
            assert!(plans.serialize_key(&bytes).unwrap().is_err(), "{format_name}");
            assert!(deserialize_dynamic_data(&bytes, &reader_type).is_err(), "{format_name}");
        }
    }

    fn array_of(element: TypeIdentifier, count: u8) -> TypeIdentifier {
        TypeIdentifier::PlainArraySmall {
            header: PlainCollectionHeader {
                equiv_kind: plain_collection_equiv_kind(&element),
                element_flags: CollectionElementFlag::default(),
            },
            array_bound_seq: vec![count],
            element_identifier: Box::new(element),
        }
    }

    /// The three ways a nested type can sit behind a DHEADER the key must step
    /// over.
    #[derive(Clone, Copy, Debug)]
    enum Skipped {
        /// Wrapped in an Appendable struct, whose own DHEADER states the extent.
        Delimited,
        Sequence,
        Array,
    }

    /// `Outer { skipped: <shape of Inner>, id: @key }` — the key lives past the
    /// nested type, so reaching it steps over one.
    fn outer_over(inner: CompleteTypeObject, shape: Skipped) -> Arc<DynamicType> {
        let inner_hash = EquivalenceHash::compute(&inner.serialize());
        let mut registry = TypeRegistry::new();
        registry.register_complete(inner_hash, "Inner".into(), inner);
        let inner_id = TypeIdentifier::CompleteTypeId(inner_hash);

        let skipped = match shape {
            Skipped::Delimited => {
                let mid = struct_object(
                    "Mid",
                    ExtensibilityKind::Appendable,
                    vec![field("child", 0, inner_id)],
                );
                let mid_hash = EquivalenceHash::compute(&mid.serialize());
                registry.register_complete(mid_hash, "Mid".into(), mid);
                TypeIdentifier::CompleteTypeId(mid_hash)
            }
            Skipped::Sequence => sequence_of(inner_id),
            Skipped::Array => array_of(inner_id, 2),
        };

        build_with(
            struct_object(
                "Outer",
                ExtensibilityKind::Final,
                vec![field("skipped", 0, skipped), key("id", 1, TypeIdentifier::Int32)],
            ),
            &registry,
        )
    }

    /// Populate that `Outer`, applying `fill` to every `Inner` it holds.
    fn outer_data(
        outer: &Arc<DynamicType>,
        shape: Skipped,
        fill: impl Fn(&mut DynamicData),
    ) -> DynamicData {
        let member = &outer.get_member("skipped").unwrap().member_type;
        let value = match shape {
            Skipped::Delimited => {
                let mid_type = member.as_type_ref().expect("Mid unresolved").clone();
                let inner_type = mid_type
                    .get_member("child")
                    .unwrap()
                    .member_type
                    .as_type_ref()
                    .expect("Inner unresolved")
                    .clone();
                let mut inner = DynamicData::new(inner_type);
                fill(&mut inner);
                let mut mid = DynamicData::new(mid_type);
                mid.set_value("child", DynamicValue::Struct(Box::new(inner))).unwrap();
                DynamicValue::Struct(Box::new(mid))
            }
            Skipped::Sequence | Skipped::Array => {
                let element_type = match member {
                    DynamicTypeKind::Sequence { element_type, .. }
                    | DynamicTypeKind::Array { element_type, .. } => {
                        element_type.as_type_ref().expect("Inner unresolved").clone()
                    }
                    other => panic!("collection member did not resolve: {other:?}"),
                };
                let items = (0..2)
                    .map(|_| {
                        let mut inner = DynamicData::new(element_type.clone());
                        fill(&mut inner);
                        DynamicValue::Struct(Box::new(inner))
                    })
                    .collect();
                match shape {
                    Skipped::Array => DynamicValue::Array(items),
                    _ => DynamicValue::Sequence(items),
                }
            }
        };

        let mut data = DynamicData::new(outer.clone());
        data.set_value("skipped", value).unwrap();
        data.set("id", 7i32).unwrap();
        data
    }

    /// The same rule one frame further down. An Appendable struct's DHEADER and a
    /// collection's both state an extent, and taking either as a jump would hide
    /// the member demanding to be understood inside it — the shape the tagged-only
    /// fix above still let through.
    #[test]
    fn unknown_must_understand_member_below_a_framed_skip() {
        // The wstring is here because its skip rule derives the byte count by hand
        // (code units x 2) rather than delegating to the decoder, and widening the
        // walk is what starts exercising it.
        fn inner_type(must_understand: bool, with_secret: bool) -> CompleteTypeObject {
            let mut members = vec![
                field("a", 0, TypeIdentifier::Int32),
                field("w", 2, TypeIdentifier::String16Small { bound: 0 }),
            ];
            if with_secret {
                members
                    .push(Field { must_understand, ..field("secret", 9, TypeIdentifier::Int32) });
            }
            struct_object("Inner", ExtensibilityKind::Mutable, members)
        }

        for shape in [Skipped::Delimited, Skipped::Sequence, Skipped::Array] {
            for must_understand in [false, true] {
                let writer = outer_over(inner_type(must_understand, true), shape);
                let reader = outer_over(inner_type(false, false), shape);
                let data = outer_data(&writer, shape, |inner| {
                    inner.set("a", 11i32).unwrap();
                    // Non-BMP: the code-unit count is neither the byte count nor
                    // the character count.
                    inner.set_value("w", DynamicValue::WString("wide\u{1F600}".into())).unwrap();
                    inner.set("secret", 99i32).unwrap();
                });

                for (format_name, format) in formats(ExtensibilityKind::Final) {
                    let label = format!("framed-skip/{shape:?}/mu={must_understand}/{format_name}");
                    let bytes = serialize_dynamic_data(&data, &format).unwrap();
                    if must_understand {
                        let plans = TypePlans::compile(&reader);
                        assert!(plans.serialize_key(&bytes).unwrap().is_err(), "{label}");
                        assert!(deserialize_dynamic_data(&bytes, &reader).is_err(), "{label}");
                    } else {
                        // Walking must not become refusing what the fallback accepts.
                        assert_matches_dynamic(&label, &bytes, &reader);
                    }
                }
            }
        }
    }

    /// The opposite direction, and the reason the walk still ends at the DHEADER:
    /// an Appendable writer may append members below a skip too, and those have to
    /// be accepted. XCDR2 only — XCDR1 frames Appendable inline, with no extent to
    /// resume from.
    #[test]
    fn appendable_trailing_members_survive_a_framed_skip() {
        fn inner_type(with_appended: bool) -> CompleteTypeObject {
            let mut members = vec![
                field("a", 0, TypeIdentifier::Int32),
                field("w", 2, TypeIdentifier::String16Small { bound: 0 }),
            ];
            if with_appended {
                members.push(field("appended", 1, TypeIdentifier::Int64));
            }
            struct_object("Inner", ExtensibilityKind::Appendable, members)
        }

        let format = SerializationFormat::Xcdr {
            extensibility_kind: ExtensibilityKind::Final,
            use_delimiters: false,
        };
        for shape in [Skipped::Delimited, Skipped::Sequence, Skipped::Array] {
            let writer = outer_over(inner_type(true), shape);
            let reader = outer_over(inner_type(false), shape);
            let data = outer_data(&writer, shape, |inner| {
                inner.set("a", 11i32).unwrap();
                inner.set_value("w", DynamicValue::WString("wide\u{1F600}".into())).unwrap();
                inner.set("appended", 22i64).unwrap();
            });

            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            assert_matches_dynamic(&format!("appendable-below/{shape:?}"), &bytes, &reader);
        }
    }

    /// A Mutable sample can omit a member its type declares. The dynamic path
    /// substitutes the type's default there; the plan declines the sample, which
    /// sends the caller back to exactly that answer rather than copying it.
    #[test]
    fn mutable_sample_missing_a_key_member_defers_to_the_dynamic_path() {
        let reader_type = mutable_type(vec![
            key("id", 1, TypeIdentifier::Int32),
            field("name", 6, TypeIdentifier::String8Small { bound: 0 }),
        ]);
        let writer_type =
            mutable_type(vec![field("name", 6, TypeIdentifier::String8Small { bound: 0 })]);
        let mut data = DynamicData::new(writer_type.clone());
        data.set("name", "hello".to_string()).unwrap();

        for (format_name, format) in formats(ExtensibilityKind::Mutable) {
            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let plans = TypePlans::compile(&reader_type);
            assert!(plans.serialize_key(&bytes).unwrap().is_err(), "{format_name}");
            assert!(plans.compute_key(&bytes).is_none(), "{format_name}");
            assert!(plans.key_info(&bytes).unwrap().is_err(), "{format_name}");

            let (key, _) = oracle(&bytes, &reader_type);
            assert_eq!(key, vec![0, 0, 0, 0], "{format_name}: default int32 key");
        }
    }

    /// An optional member spends a presence marker in a non-tagged struct — an
    /// XCDR2 bool, an XCDR1 PL_CDR header whose zero length means absent — and
    /// simply omits its header when the struct is Mutable.
    #[test]
    fn optional_members_match_dynamic_path() {
        for ext in
            [ExtensibilityKind::Final, ExtensibilityKind::Appendable, ExtensibilityKind::Mutable]
        {
            let dynamic_type = build(struct_object(
                "WithOptional",
                ext,
                vec![
                    optional("maybe", 0, TypeIdentifier::Int64),
                    key("id", 1, TypeIdentifier::Int32),
                    optional("note", 2, TypeIdentifier::String8Small { bound: 0 }),
                ],
            ));

            let mut present = DynamicData::new(dynamic_type.clone());
            present.set("maybe", 5i64).unwrap();
            present.set("id", 7i32).unwrap();
            present.set("note", "hi".to_string()).unwrap();
            check_all_formats(&format!("optional-present/{ext:?}"), &present, &dynamic_type);

            let mut absent = DynamicData::new(dynamic_type.clone());
            absent.set("id", 7i32).unwrap();
            check_all_formats(&format!("optional-absent/{ext:?}"), &absent, &dynamic_type);
        }
    }

    #[test]
    fn field_values_read_from_tagged_and_optional_members() {
        let dynamic_type = build(struct_object(
            "MutFields",
            ExtensibilityKind::Mutable,
            vec![
                key("id", 3, TypeIdentifier::Int32),
                optional("note", 1, TypeIdentifier::String8Small { bound: 0 }),
                field("ratio", 2, TypeIdentifier::Float64),
            ],
        ));
        let plans = TypePlans::compile(&dynamic_type);
        assert!(plans.has_field("note"));

        let mut data = DynamicData::new(dynamic_type.clone());
        data.set("id", 7i32).unwrap();
        data.set("note", "hi".to_string()).unwrap();
        data.set("ratio", 1.5f64).unwrap();

        let mut absent = DynamicData::new(dynamic_type.clone());
        absent.set("id", 7i32).unwrap();
        absent.set("ratio", 1.5f64).unwrap();

        for (format_name, format) in formats(ExtensibilityKind::Mutable) {
            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let read = |name: &str| plans.field_value(&bytes, name).unwrap().unwrap();
            assert_eq!(read("id"), Parameter::IntegerValue(7), "{format_name}");
            assert_eq!(read("note"), Parameter::String("hi".into()), "{format_name}");
            assert_eq!(read("ratio"), Parameter::FloatValue(1.5), "{format_name}");

            // An absent optional reads as Unset, and the members after it are
            // still reachable.
            let bytes = serialize_dynamic_data(&absent, &format).unwrap();
            assert_eq!(
                plans.field_value(&bytes, "note").unwrap().unwrap(),
                Parameter::Unset,
                "{format_name}"
            );
            assert_eq!(
                plans.field_value(&bytes, "ratio").unwrap().unwrap(),
                Parameter::FloatValue(1.5),
                "{format_name}"
            );
        }
    }

    /// `key_info` is the one-walk form of `serialize_key` followed by
    /// `compute_key`, and must not drift from either.
    #[test]
    fn key_info_agrees_with_the_separate_entry_points() {
        let dynamic_type = flat_type(ExtensibilityKind::Appendable);
        let data = flat_data(&dynamic_type);
        let plans = TypePlans::compile(&dynamic_type);

        for (format_name, format) in formats(ExtensibilityKind::Appendable) {
            let bytes = serialize_dynamic_data(&data, &format).unwrap();
            let (key, handle) = plans.key_info(&bytes).unwrap().unwrap();
            assert_eq!(key, plans.serialize_key(&bytes).unwrap().unwrap(), "{format_name}");
            assert_eq!(handle, plans.compute_key(&bytes).unwrap(), "{format_name}");
        }
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
