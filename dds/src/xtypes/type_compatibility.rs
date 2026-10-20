//! Type compatibility checking for DDS-XTypes.
//!
//! This module implements structural type compatibility checking according to
//! DDS-XTypes 1.3 specification section 7.2.4.
//!
//! # Overview
//!
//! Type compatibility checking determines whether a DataWriter and DataReader
//! can communicate based on their type definitions. The checking is controlled
//! by the [`TypeConsistencyEnforcementQosPolicy`].
//!
//! # Performance
//!
//! - Hash comparison (fast path): O(1), 14 bytes comparison
//! - Structural comparison: O(n) where n = number of members
//! - Only performed during discovery, not on data transmission path

use std::collections::HashMap;
use std::fmt;

use crate::dcps::infrastructure::qos_policy::{
    TypeConsistencyEnforcementQosPolicy, TypeConsistencyKind,
};
use crate::xtypes::{
    compute_name_hash, spec_hash, CommonDiscriminatorMember, CompleteStructMember,
    CompleteStructType, CompleteTypeObject, EquivalenceHash, ExtensibilityKind, MemberFlag,
    MinimalStructMember, MinimalStructType, MinimalTypeObject, TypeIdentifier, TypeObject,
};

/// Error type for type compatibility checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCompatibilityError {
    /// TypeIdentifier hash mismatch in DisallowTypeCoercion mode.
    HashMismatch { writer_hash: EquivalenceHash, reader_hash: EquivalenceHash },

    /// Type kinds are fundamentally incompatible (e.g., struct vs enum).
    IncompatibleKind { writer: String, reader: String },

    /// A key or must_understand member exists on one side only.
    MissingRequiredMember { member_name: String, member_id: u32 },

    /// Same name with different ids, or same id with different names.
    MemberIdentityMismatch { writer: String, reader: String },

    /// No member id (struct) or non-default label (union) in common.
    NoCommonMember,

    /// Appendable/final member order, final member set or final union label set differs.
    LayoutMismatch { detail: String },

    /// Member types are incompatible.
    MemberTypeMismatch { member_name: String, writer_type: String, reader_type: String },

    /// Sequence bound exceeded (writer bound > reader bound).
    SequenceBoundExceeded { member_name: String, writer_bound: u32, reader_bound: u32 },

    /// String bound exceeded (writer bound > reader bound).
    StringBoundExceeded { member_name: String, writer_bound: u32, reader_bound: u32 },

    /// Type widening not allowed (writer has extra members).
    TypeWideningNotAllowed { extra_members: Vec<String> },

    /// Key member mismatch between writer and reader.
    KeyMemberMismatch { member_name: String },

    /// Extensibility kind mismatch.
    ExtensibilityMismatch { writer: String, reader: String },

    /// TypeIdentifier required but not present.
    TypeIdentifierRequired,

    /// TypeObject required for structural check but not available.
    TypeObjectRequired,
}

impl fmt::Display for TypeCompatibilityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeCompatibilityError::HashMismatch { writer_hash, reader_hash } => {
                write!(f, "Type hash mismatch: writer={}, reader={}", writer_hash, reader_hash)
            }
            TypeCompatibilityError::IncompatibleKind { writer, reader } => {
                write!(f, "Incompatible type kinds: writer={}, reader={}", writer, reader)
            }
            TypeCompatibilityError::MissingRequiredMember { member_name, member_id } => {
                write!(f, "Member '{}' (id={}) must exist on both sides", member_name, member_id)
            }
            TypeCompatibilityError::MemberIdentityMismatch { writer, reader } => {
                write!(f, "Member name/id mismatch: writer={}, reader={}", writer, reader)
            }
            TypeCompatibilityError::NoCommonMember => write!(f, "No member in common"),
            TypeCompatibilityError::LayoutMismatch { detail } => {
                write!(f, "Member layout mismatch: {}", detail)
            }
            TypeCompatibilityError::MemberTypeMismatch {
                member_name,
                writer_type,
                reader_type,
            } => {
                write!(
                    f,
                    "Member '{}' type mismatch: writer={}, reader={}",
                    member_name, writer_type, reader_type
                )
            }
            TypeCompatibilityError::SequenceBoundExceeded {
                member_name,
                writer_bound,
                reader_bound,
            } => {
                write!(
                    f,
                    "Sequence bound exceeded for '{}': writer={}, reader={}",
                    member_name, writer_bound, reader_bound
                )
            }
            TypeCompatibilityError::StringBoundExceeded {
                member_name,
                writer_bound,
                reader_bound,
            } => {
                write!(
                    f,
                    "String bound exceeded for '{}': writer={}, reader={}",
                    member_name, writer_bound, reader_bound
                )
            }
            TypeCompatibilityError::TypeWideningNotAllowed { extra_members } => {
                write!(f, "Type widening not allowed, extra members: {:?}", extra_members)
            }
            TypeCompatibilityError::KeyMemberMismatch { member_name } => {
                write!(f, "Key member mismatch for '{}'", member_name)
            }
            TypeCompatibilityError::ExtensibilityMismatch { writer, reader } => {
                write!(f, "Extensibility mismatch: writer={}, reader={}", writer, reader)
            }
            TypeCompatibilityError::TypeIdentifierRequired => {
                write!(f, "TypeIdentifier required but not present")
            }
            TypeCompatibilityError::TypeObjectRequired => {
                write!(f, "TypeObject required for structural check but not available")
            }
        }
    }
}

impl std::error::Error for TypeCompatibilityError {}

/// Result type for type compatibility checking.
pub type TypeCompatibilityResult = Result<(), TypeCompatibilityError>;

/// Tri-state structural compatibility result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCompatibility {
    Compatible,
    Incompatible(TypeCompatibilityError),
    Indeterminate,
}

pub trait TypeResolver {
    fn resolve(&self, id: &TypeIdentifier) -> Option<TypeObject>;

    fn resolve_complete(&self, id: &TypeIdentifier) -> Option<TypeObject> {
        self.resolve(id)
    }
}

struct NullResolver;

impl TypeResolver for NullResolver {
    fn resolve(&self, _id: &TypeIdentifier) -> Option<TypeObject> {
        None
    }
}

/// The equivalence-kind discriminant of a hash `TypeIdentifier` (Complete vs Minimal),
/// or `None` for non-hash ids.
fn ek_discriminant(id: &TypeIdentifier) -> Option<u8> {
    match id {
        TypeIdentifier::CompleteTypeId(_) => Some(0),
        TypeIdentifier::MinimalTypeId(_) => Some(1),
        _ => None,
    }
}

/// Two hash ids of differing equivalence kind (e.g. Complete vs Minimal).
fn is_cross_ek(a: &TypeIdentifier, b: &TypeIdentifier) -> bool {
    matches!((ek_discriminant(a), ek_discriminant(b)), (Some(x), Some(y)) if x != y)
}

/// Compare two cross-EK hash ids by resolving BOTH to their Complete objects and
/// comparing content hashes; fall back to structural comparison, or Indeterminate
/// when either side cannot be resolved.
fn cross_ek_via_complete(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    match (ctx.resolver.resolve_complete(writer_id), ctx.resolver.resolve_complete(reader_id)) {
        (Some(w), Some(r)) => {
            if spec_hash(&w) == spec_hash(&r) {
                TypeCompatibility::Compatible
            } else {
                check_type_object_compatibility(&w, &r, ctx)
            }
        }
        _ => TypeCompatibility::Indeterminate,
    }
}

struct CompatCtx<'a> {
    resolver: &'a dyn TypeResolver,
    tce: &'a TypeConsistencyEnforcementQosPolicy,
    /// Table 19 key member rules; off in `INT2DDS_XTYPES_RTI_KEY_COMPAT` mode.
    key_rules: bool,
    visited: std::cell::RefCell<std::collections::HashSet<(EquivalenceHash, EquivalenceHash)>>,
}

impl<'a> CompatCtx<'a> {
    fn new(resolver: &'a dyn TypeResolver, tce: &'a TypeConsistencyEnforcementQosPolicy) -> Self {
        Self {
            resolver,
            tce,
            key_rules: !crate::common::env::get_xtypes_rti_key_compat(),
            visited: std::cell::RefCell::new(std::collections::HashSet::new()),
        }
    }

    fn enter(&self, pair: (EquivalenceHash, EquivalenceHash)) -> bool {
        self.visited.borrow_mut().insert(pair)
    }
}

fn from_result(r: TypeCompatibilityResult) -> TypeCompatibility {
    match r {
        Ok(()) => TypeCompatibility::Compatible,
        Err(e) => TypeCompatibility::Incompatible(e),
    }
}

pub fn check_structural_compatibility(
    writer_type_id: Option<&TypeIdentifier>,
    reader_type_id: Option<&TypeIdentifier>,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    let ctx = CompatCtx::new(&NullResolver, tce_policy);
    match evaluate_compat(writer_type_id, reader_type_id, writer_type_obj, reader_type_obj, &ctx) {
        TypeCompatibility::Compatible => Ok(()),
        TypeCompatibility::Incompatible(e) => Err(e),
        TypeCompatibility::Indeterminate => {
            if tce_policy.force_type_validation {
                Err(TypeCompatibilityError::TypeObjectRequired)
            } else {
                Ok(())
            }
        }
    }
}

pub fn evaluate_structural_compatibility(
    writer_type_id: Option<&TypeIdentifier>,
    reader_type_id: Option<&TypeIdentifier>,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
    resolver: &dyn TypeResolver,
) -> TypeCompatibility {
    let ctx = CompatCtx::new(resolver, tce_policy);
    evaluate_compat(writer_type_id, reader_type_id, writer_type_obj, reader_type_obj, &ctx)
}

fn evaluate_compat(
    writer_type_id: Option<&TypeIdentifier>,
    reader_type_id: Option<&TypeIdentifier>,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    match (writer_type_id, reader_type_id) {
        (Some(writer_id), Some(reader_id)) => match ctx.tce.kind {
            TypeConsistencyKind::DisallowTypeCoercion => {
                disallow_type_coercion_with_ctx(writer_id, reader_id, ctx)
            }
            TypeConsistencyKind::AllowTypeCoercion => {
                compat_allow_coercion(writer_id, reader_id, writer_type_obj, reader_type_obj, ctx)
            }
        },
        (None, None) => TypeCompatibility::Compatible,
        _ => {
            if ctx.tce.force_type_validation {
                TypeCompatibility::Incompatible(TypeCompatibilityError::TypeIdentifierRequired)
            } else {
                TypeCompatibility::Compatible
            }
        }
    }
}

/// DisallowTypeCoercion with resolver access: same-EK requires hash equality; two
/// complex ids of differing EK are reconciled by translating both to their Complete
/// content hashes (equal -> Ok, both resolved but different -> HashMismatch,
/// unresolvable -> Indeterminate). Non-complex ids defer to the pure check.
fn disallow_type_coercion_with_ctx(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    if writer_id.is_complex() && reader_id.is_complex() {
        if writer_id.equivalence_hash() == reader_id.equivalence_hash() {
            return TypeCompatibility::Compatible;
        }
        let mismatch = || {
            TypeCompatibility::Incompatible(TypeCompatibilityError::HashMismatch {
                writer_hash: writer_id.equivalence_hash().cloned().unwrap_or_default(),
                reader_hash: reader_id.equivalence_hash().cloned().unwrap_or_default(),
            })
        };
        if !is_cross_ek(writer_id, reader_id) {
            return mismatch();
        }
        return match (
            ctx.resolver.resolve_complete(writer_id),
            ctx.resolver.resolve_complete(reader_id),
        ) {
            (Some(w), Some(r)) => {
                if spec_hash(&w) == spec_hash(&r) {
                    TypeCompatibility::Compatible
                } else {
                    mismatch()
                }
            }
            _ => TypeCompatibility::Indeterminate,
        };
    }
    from_result(check_disallow_type_coercion(writer_id, reader_id))
}

fn check_disallow_type_coercion(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
) -> TypeCompatibilityResult {
    if writer_id.is_complex() && reader_id.is_complex() {
        let writer_hash = writer_id.equivalence_hash();
        let reader_hash = reader_id.equivalence_hash();

        if writer_hash == reader_hash {
            Ok(())
        } else {
            Err(TypeCompatibilityError::HashMismatch {
                writer_hash: writer_hash.cloned().unwrap_or_default(),
                reader_hash: reader_hash.cloned().unwrap_or_default(),
            })
        }
    } else if writer_id == reader_id {
        Ok(())
    } else {
        Err(TypeCompatibilityError::IncompatibleKind {
            writer: format!("{:?}", writer_id),
            reader: format!("{:?}", reader_id),
        })
    }
}

fn compat_allow_coercion(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    if writer_id == reader_id {
        return TypeCompatibility::Compatible;
    }

    if writer_id.is_complex() && reader_id.is_complex() {
        if writer_id.equivalence_hash() == reader_id.equivalence_hash() {
            return TypeCompatibility::Compatible;
        }
        if is_cross_ek(writer_id, reader_id) {
            return cross_ek_via_complete(writer_id, reader_id, ctx);
        }
    }

    let w = node_from_id(writer_id, writer_type_obj, ctx);
    let r = node_from_id(reader_id, reader_type_obj, ctx);
    assignable_nodes(w, r, "", ctx)
}

fn check_type_object_compatibility(
    writer_obj: &TypeObject,
    reader_obj: &TypeObject,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let w = unalias(Some(writer_obj.clone()), ctx);
    let r = unalias(Some(reader_obj.clone()), ctx);
    assignable_nodes(w, r, "", ctx)
}

/// A type with aliases resolved (XTypes 1.3 Table 14): either a plain `TypeIdentifier`
/// (primitive, string, collection) or a resolved aggregated/enumerated `TypeObject`.
enum Node {
    Plain(TypeIdentifier),
    Object(TypeObject),
    Unresolved,
}

fn node_from_id(id: &TypeIdentifier, obj: Option<&TypeObject>, ctx: &CompatCtx) -> Node {
    if !id.is_complex() {
        return Node::Plain(id.clone());
    }
    unalias(obj.cloned().or_else(|| ctx.resolver.resolve(id)), ctx)
}

fn unalias(mut obj: Option<TypeObject>, ctx: &CompatCtx) -> Node {
    for _ in 0..32 {
        let Some(current) = obj else { return Node::Unresolved };
        let target = match &current {
            TypeObject::Minimal(MinimalTypeObject::Alias(a)) => a.body.related_type.clone(),
            TypeObject::Complete(CompleteTypeObject::Alias(a)) => a.body.related_type.clone(),
            _ => return Node::Object(current),
        };
        if !target.is_complex() {
            return Node::Plain(target);
        }
        obj = ctx.resolver.resolve(&target);
    }
    Node::Unresolved
}

/// `reader` is-assignable-from `writer` for arbitrary type identifiers (recursive entry).
fn assignable_ids(
    writer: &TypeIdentifier,
    reader: &TypeIdentifier,
    member: &str,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    if writer == reader {
        return TypeCompatibility::Compatible;
    }
    if writer.is_complex() && reader.is_complex() {
        if writer.equivalence_hash() == reader.equivalence_hash() {
            return TypeCompatibility::Compatible;
        }
        if let (Some(wh), Some(rh)) =
            (writer.equivalence_hash().copied(), reader.equivalence_hash().copied())
        {
            if !ctx.enter((wh, rh)) {
                return TypeCompatibility::Compatible;
            }
        }
        if is_cross_ek(writer, reader) {
            return cross_ek_via_complete(writer, reader, ctx);
        }
    }
    let w = node_from_id(writer, None, ctx);
    let r = node_from_id(reader, None, ctx);
    assignable_nodes(w, r, member, ctx)
}

fn assignable_nodes(
    writer: Node,
    reader: Node,
    member: &str,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    match (writer, reader) {
        (Node::Unresolved, _) | (_, Node::Unresolved) => TypeCompatibility::Indeterminate,
        (Node::Plain(w), Node::Plain(r)) => assignable_plain(&w, &r, member, ctx),
        (Node::Object(w), Node::Object(r)) => assignable_objects(&w, &r, member, ctx),
        // Table 15 / Table 18: unsigned integers and bitmasks of a matching bit bound.
        (Node::Object(w), Node::Plain(r)) => match bitmask_bound(&w) {
            Some(bound) if uint_fits_bitmask(&r, bound) => TypeCompatibility::Compatible,
            _ => kind_mismatch(member, &object_kind(&w), &format!("{:?}", r)),
        },
        (Node::Plain(w), Node::Object(r)) => match bitmask_bound(&r) {
            Some(bound) if uint_fits_bitmask(&w, bound) => TypeCompatibility::Compatible,
            _ => kind_mismatch(member, &format!("{:?}", w), &object_kind(&r)),
        },
    }
}

fn kind_mismatch(member: &str, writer: &str, reader: &str) -> TypeCompatibility {
    TypeCompatibility::Incompatible(if member.is_empty() {
        TypeCompatibilityError::IncompatibleKind {
            writer: writer.to_string(),
            reader: reader.to_string(),
        }
    } else {
        TypeCompatibilityError::MemberTypeMismatch {
            member_name: member.to_string(),
            writer_type: writer.to_string(),
            reader_type: reader.to_string(),
        }
    })
}

fn uint_fits_bitmask(uint: &TypeIdentifier, bound: u16) -> bool {
    matches!(
        (uint, bound),
        (TypeIdentifier::Uint8, 1..=8)
            | (TypeIdentifier::Uint16, 9..=16)
            | (TypeIdentifier::Uint32, 17..=32)
            | (TypeIdentifier::Uint64, 33..=64)
    )
}

fn bitmask_bound(obj: &TypeObject) -> Option<u16> {
    match obj {
        TypeObject::Minimal(MinimalTypeObject::Bitmask(b)) => Some(b.header.bit_bound),
        TypeObject::Complete(CompleteTypeObject::Bitmask(b)) => Some(b.header.common.bit_bound),
        _ => None,
    }
}

fn object_kind(obj: &TypeObject) -> String {
    let kind = match obj {
        TypeObject::Minimal(m) => match m {
            MinimalTypeObject::Struct(_) => "struct",
            MinimalTypeObject::Enum(_) => "enum",
            MinimalTypeObject::Union(_) => "union",
            MinimalTypeObject::Alias(_) => "alias",
            MinimalTypeObject::Bitmask(_) => "bitmask",
            MinimalTypeObject::Bitset(_) => "bitset",
        },
        TypeObject::Complete(c) => match c {
            CompleteTypeObject::Struct(_) => "struct",
            CompleteTypeObject::Enum(_) => "enum",
            CompleteTypeObject::Union(_) => "union",
            CompleteTypeObject::Alias(_) => "alias",
            CompleteTypeObject::Bitmask(_) => "bitmask",
            CompleteTypeObject::Bitset(_) => "bitset",
        },
    };
    kind.to_string()
}

/// Tables 15-17: primitives, strings and collections. Unbounded (0) means no limit.
fn assignable_plain(
    writer: &TypeIdentifier,
    reader: &TypeIdentifier,
    member: &str,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    if writer == reader {
        return TypeCompatibility::Compatible;
    }
    let mismatch = || kind_mismatch(member, &format!("{:?}", writer), &format!("{:?}", reader));

    if writer.is_string() && reader.is_string() {
        if is_wide_string(writer) != is_wide_string(reader) {
            return mismatch();
        }
        let (w, r) = (get_string_bound(writer), get_string_bound(reader));
        if !ctx.tce.ignore_string_bounds && !bound_fits(w, r) {
            return TypeCompatibility::Incompatible(TypeCompatibilityError::StringBoundExceeded {
                member_name: member.to_string(),
                writer_bound: w,
                reader_bound: r,
            });
        }
        return TypeCompatibility::Compatible;
    }

    if let (Some((w_elem, w_bound)), Some((r_elem, r_bound))) =
        (sequence_info(writer), sequence_info(reader))
    {
        if !ctx.tce.ignore_sequence_bounds && !bound_fits(w_bound, r_bound) {
            return TypeCompatibility::Incompatible(
                TypeCompatibilityError::SequenceBoundExceeded {
                    member_name: member.to_string(),
                    writer_bound: w_bound,
                    reader_bound: r_bound,
                },
            );
        }
        return assignable_ids(w_elem, r_elem, member, ctx);
    }

    if let (Some((w_elem, w_dims)), Some((r_elem, r_dims))) =
        (array_info(writer), array_info(reader))
    {
        if w_dims != r_dims {
            return kind_mismatch(
                member,
                &format!("array{:?}", w_dims),
                &format!("array{:?}", r_dims),
            );
        }
        return assignable_ids(w_elem, r_elem, member, ctx);
    }

    if let (Some((w_key, w_elem, w_bound)), Some((r_key, r_elem, r_bound))) =
        (map_info(writer), map_info(reader))
    {
        if !ctx.tce.ignore_sequence_bounds && !bound_fits(w_bound, r_bound) {
            return TypeCompatibility::Incompatible(
                TypeCompatibilityError::SequenceBoundExceeded {
                    member_name: member.to_string(),
                    writer_bound: w_bound,
                    reader_bound: r_bound,
                },
            );
        }
        return combine(assignable_ids(w_key, r_key, member, ctx), || {
            assignable_ids(w_elem, r_elem, member, ctx)
        });
    }

    mismatch()
}

fn combine(
    first: TypeCompatibility,
    second: impl FnOnce() -> TypeCompatibility,
) -> TypeCompatibility {
    match first {
        TypeCompatibility::Incompatible(e) => TypeCompatibility::Incompatible(e),
        TypeCompatibility::Indeterminate => match second() {
            TypeCompatibility::Incompatible(e) => TypeCompatibility::Incompatible(e),
            _ => TypeCompatibility::Indeterminate,
        },
        TypeCompatibility::Compatible => second(),
    }
}

/// L1 >= L2 where 0 is unbounded (XTypes 1.3 7.6.3.3).
fn bound_fits(writer: u32, reader: u32) -> bool {
    reader == 0 || (writer != 0 && writer <= reader)
}

fn get_string_bound(type_id: &TypeIdentifier) -> u32 {
    match type_id {
        TypeIdentifier::String8Small { bound } | TypeIdentifier::String16Small { bound } => {
            *bound as u32
        }
        TypeIdentifier::String8Large { bound } | TypeIdentifier::String16Large { bound } => *bound,
        _ => 0,
    }
}

fn is_wide_string(type_id: &TypeIdentifier) -> bool {
    matches!(
        type_id,
        TypeIdentifier::String16
            | TypeIdentifier::String16Small { .. }
            | TypeIdentifier::String16Large { .. }
    )
}

fn sequence_info(type_id: &TypeIdentifier) -> Option<(&TypeIdentifier, u32)> {
    match type_id {
        TypeIdentifier::PlainSequenceSmall { element_identifier, bound, .. } => {
            Some((element_identifier, *bound as u32))
        }
        TypeIdentifier::PlainSequenceLarge { element_identifier, bound, .. } => {
            Some((element_identifier, *bound))
        }
        _ => None,
    }
}

fn array_info(type_id: &TypeIdentifier) -> Option<(&TypeIdentifier, Vec<u32>)> {
    match type_id {
        TypeIdentifier::PlainArraySmall { element_identifier, array_bound_seq, .. } => {
            Some((element_identifier, array_bound_seq.iter().map(|b| *b as u32).collect()))
        }
        TypeIdentifier::PlainArrayLarge { element_identifier, array_bound_seq, .. } => {
            Some((element_identifier, array_bound_seq.clone()))
        }
        _ => None,
    }
}

fn map_info(type_id: &TypeIdentifier) -> Option<(&TypeIdentifier, &TypeIdentifier, u32)> {
    match type_id {
        TypeIdentifier::PlainMapSmall { key_identifier, element_identifier, bound, .. } => {
            Some((key_identifier, element_identifier, *bound as u32))
        }
        TypeIdentifier::PlainMapLarge { key_identifier, element_identifier, bound, .. } => {
            Some((key_identifier, element_identifier, *bound))
        }
        _ => None,
    }
}

/// Minimal/Complete-agnostic member view used by the Table 18/19 rules.
struct Member {
    id: u32,
    flags: MemberFlag,
    ty: TypeIdentifier,
    name_hash: u32,
    name: String,
    labels: Vec<i32>,
}

struct Aggregate {
    ext: ExtensibilityKind,
    members: Vec<Member>,
    discriminator: Option<CommonDiscriminatorMember>,
}

struct Literal {
    value: i32,
    name_hash: u32,
    name: String,
}

enum View {
    Struct(Aggregate),
    Union(Aggregate),
    Enum(Vec<Literal>),
    Bitmask(u16),
    Bitset(Vec<(u16, u8, TypeIdentifier)>),
}

fn hash_name(name_hash: u32) -> String {
    format!("hash:{:08x}", name_hash)
}

fn view_of(obj: &TypeObject, ctx: &CompatCtx) -> Result<View, TypeCompatibility> {
    Ok(match obj {
        TypeObject::Minimal(m) => match m {
            MinimalTypeObject::Struct(s) => View::Struct(Aggregate {
                ext: s.struct_flags.extensibility(),
                members: struct_members(obj, ctx, 0)?,
                discriminator: None,
            }),
            MinimalTypeObject::Union(u) => View::Union(Aggregate {
                ext: u.union_flags.extensibility(),
                members: u
                    .member_seq
                    .iter()
                    .map(|m| Member {
                        id: m.common.member_id,
                        flags: m.common.member_flags,
                        ty: m.common.member_type_id.clone(),
                        name_hash: m.name_hash,
                        name: hash_name(m.name_hash),
                        labels: m.common.label_seq.clone(),
                    })
                    .collect(),
                discriminator: Some(u.discriminator.clone()),
            }),
            MinimalTypeObject::Enum(e) => View::Enum(
                e.literal_seq
                    .iter()
                    .map(|l| Literal {
                        value: l.common.value,
                        name_hash: l.name_hash,
                        name: hash_name(l.name_hash),
                    })
                    .collect(),
            ),
            MinimalTypeObject::Bitmask(b) => View::Bitmask(b.header.bit_bound),
            MinimalTypeObject::Bitset(b) => View::Bitset(
                b.field_seq
                    .iter()
                    .map(|f| (f.common.position, f.common.bitcount, f.common.holder_type.clone()))
                    .collect(),
            ),
            MinimalTypeObject::Alias(_) => return Err(TypeCompatibility::Indeterminate),
        },
        TypeObject::Complete(c) => match c {
            CompleteTypeObject::Struct(s) => View::Struct(Aggregate {
                ext: s.struct_flags.extensibility(),
                members: struct_members(obj, ctx, 0)?,
                discriminator: None,
            }),
            CompleteTypeObject::Union(u) => View::Union(Aggregate {
                ext: u.union_flags.extensibility(),
                members: u
                    .member_seq
                    .iter()
                    .map(|m| Member {
                        id: m.common.member_id,
                        flags: m.common.member_flags,
                        ty: m.common.member_type_id.clone(),
                        name_hash: compute_name_hash(&m.detail.name),
                        name: m.detail.name.clone(),
                        labels: m.common.label_seq.clone(),
                    })
                    .collect(),
                discriminator: Some(u.discriminator.clone()),
            }),
            CompleteTypeObject::Enum(e) => View::Enum(
                e.literal_seq
                    .iter()
                    .map(|l| Literal {
                        value: l.common.value,
                        name_hash: compute_name_hash(&l.detail.name),
                        name: l.detail.name.clone(),
                    })
                    .collect(),
            ),
            CompleteTypeObject::Bitmask(b) => View::Bitmask(b.header.common.bit_bound),
            CompleteTypeObject::Bitset(b) => View::Bitset(
                b.field_seq
                    .iter()
                    .map(|f| (f.common.position, f.common.bitcount, f.common.holder_type.clone()))
                    .collect(),
            ),
            CompleteTypeObject::Alias(_) => return Err(TypeCompatibility::Indeterminate),
        },
    })
}

/// Struct members with inherited members flattened in front (base first), as the
/// member_index rules of Table 19 see them.
fn struct_members(
    obj: &TypeObject,
    ctx: &CompatCtx,
    depth: usize,
) -> Result<Vec<Member>, TypeCompatibility> {
    if depth > 32 {
        return Err(TypeCompatibility::Indeterminate);
    }
    let (base, own): (Option<&TypeIdentifier>, Vec<Member>) = match obj {
        TypeObject::Minimal(MinimalTypeObject::Struct(s)) => (
            s.header.base_type.as_ref(),
            s.member_seq
                .iter()
                .map(|m| Member {
                    id: m.common.member_id,
                    flags: m.common.member_flags,
                    ty: m.common.member_type_id.clone(),
                    name_hash: m.name_hash,
                    name: hash_name(m.name_hash),
                    labels: Vec::new(),
                })
                .collect(),
        ),
        TypeObject::Complete(CompleteTypeObject::Struct(s)) => (
            s.header.base_type.as_ref(),
            s.member_seq
                .iter()
                .map(|m| Member {
                    id: m.common.member_id,
                    flags: m.common.member_flags,
                    ty: m.common.member_type_id.clone(),
                    name_hash: compute_name_hash(&m.detail.name),
                    name: m.detail.name.clone(),
                    labels: Vec::new(),
                })
                .collect(),
        ),
        _ => {
            return Err(TypeCompatibility::Incompatible(TypeCompatibilityError::IncompatibleKind {
                writer: "struct".to_string(),
                reader: object_kind(obj),
            }))
        }
    };
    let mut members = match base {
        Some(base_id) => match unalias(ctx.resolver.resolve(base_id), ctx) {
            Node::Object(base_obj) => struct_members(&base_obj, ctx, depth + 1)?,
            _ => return Err(TypeCompatibility::Indeterminate),
        },
        None => Vec::new(),
    };
    members.extend(own);
    Ok(members)
}

fn assignable_objects(
    writer: &TypeObject,
    reader: &TypeObject,
    member: &str,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let (w, r) = match (view_of(writer, ctx), view_of(reader, ctx)) {
        (Ok(w), Ok(r)) => (w, r),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    match (w, r) {
        (View::Struct(w), View::Struct(r)) => assignable_structs(&w, &r, ctx),
        (View::Union(w), View::Union(r)) => assignable_unions(&w, &r, ctx),
        (View::Enum(w), View::Enum(r)) => assignable_enums(&w, &r),
        (View::Bitmask(w), View::Bitmask(r)) if w == r => TypeCompatibility::Compatible,
        (View::Bitset(w), View::Bitset(r)) if w == r => TypeCompatibility::Compatible,
        _ => kind_mismatch(member, &object_kind(writer), &object_kind(reader)),
    }
}

/// Table 19, STRUCTURE_TYPE. `reader` is T1, `writer` is T2.
fn assignable_structs(
    writer: &Aggregate,
    reader: &Aggregate,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let tce = ctx.tce;
    if writer.ext != reader.ext {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::ExtensibilityMismatch {
            writer: format!("{:?}", writer.ext),
            reader: format!("{:?}", reader.ext),
        });
    }
    if !tce.ignore_member_names {
        if let Some(e) = name_id_inconsistency(&writer.members, &reader.members) {
            return TypeCompatibility::Incompatible(e);
        }
    }

    let writer_by_id: HashMap<u32, &Member> = writer.members.iter().map(|m| (m.id, m)).collect();
    let reader_by_id: HashMap<u32, &Member> = reader.members.iter().map(|m| (m.id, m)).collect();
    let required =
        |m: &Member| m.flags.is_key() || (m.flags.is_must_understand() && !m.flags.is_optional());

    let mut common = 0;
    let mut indeterminate = false;
    let mut widening = Vec::new();
    for m1 in &reader.members {
        match writer_by_id.get(&m1.id) {
            Some(m2) => {
                common += 1;
                let mut result = assignable_ids(&m2.ty, &m1.ty, &m1.name, ctx);
                if ctx.key_rules && (m1.flags.is_key() || m2.flags.is_key()) {
                    result = combine(result, || assignable_key(&m2.ty, &m1.ty, &m1.name, ctx, 0));
                }
                match result {
                    TypeCompatibility::Compatible => {}
                    TypeCompatibility::Incompatible(e) => {
                        return TypeCompatibility::Incompatible(e)
                    }
                    TypeCompatibility::Indeterminate => indeterminate = true,
                }
            }
            None if required(m1) => return missing(m1),
            None if tce.prevent_type_widening && !m1.flags.is_optional() => {
                widening.push(m1.name.clone())
            }
            None => {}
        }
    }
    if let Some(m2) =
        writer.members.iter().find(|m| required(m) && !reader_by_id.contains_key(&m.id))
    {
        return missing(m2);
    }
    if common == 0 {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::NoCommonMember);
    }
    if !widening.is_empty() {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::TypeWideningNotAllowed {
            extra_members: widening,
        });
    }

    if reader.ext != ExtensibilityKind::Mutable {
        for (index, (m1, m2)) in reader.members.iter().zip(&writer.members).enumerate() {
            if m1.id != m2.id || m1.flags.is_optional() != m2.flags.is_optional() {
                return layout_mismatch(format!(
                    "member index {index}: writer '{}' (id={}), reader '{}' (id={})",
                    m2.name, m2.id, m1.name, m1.id
                ));
            }
        }
        if reader.ext == ExtensibilityKind::Final && reader.members.len() != writer.members.len() {
            return layout_mismatch(format!(
                "final struct member count: writer={}, reader={}",
                writer.members.len(),
                reader.members.len()
            ));
        }
    }

    if indeterminate {
        TypeCompatibility::Indeterminate
    } else {
        TypeCompatibility::Compatible
    }
}

fn missing(m: &Member) -> TypeCompatibility {
    TypeCompatibility::Incompatible(TypeCompatibilityError::MissingRequiredMember {
        member_name: m.name.clone(),
        member_id: m.id,
    })
}

fn layout_mismatch(detail: String) -> TypeCompatibility {
    TypeCompatibility::Incompatible(TypeCompatibilityError::LayoutMismatch { detail })
}

/// Same name => same id and same id => same name, across the two member lists.
fn name_id_inconsistency(writer: &[Member], reader: &[Member]) -> Option<TypeCompatibilityError> {
    let by_id: HashMap<u32, &Member> = writer.iter().map(|m| (m.id, m)).collect();
    let by_name: HashMap<u32, &Member> = writer.iter().map(|m| (m.name_hash, m)).collect();
    for m1 in reader {
        let clash = by_id
            .get(&m1.id)
            .filter(|m2| m2.name_hash != m1.name_hash)
            .or_else(|| by_name.get(&m1.name_hash).filter(|m2| m2.id != m1.id));
        if let Some(m2) = clash {
            return Some(TypeCompatibilityError::MemberIdentityMismatch {
                writer: format!("'{}' (id={})", m2.name, m2.id),
                reader: format!("'{}' (id={})", m1.name, m1.id),
            });
        }
    }
    None
}

/// Table 19, UNION_TYPE. Members correspond by label; a default member counts as the
/// label `default` for the final-union rule.
fn assignable_unions(writer: &Aggregate, reader: &Aggregate, ctx: &CompatCtx) -> TypeCompatibility {
    let tce = ctx.tce;
    if writer.ext != reader.ext {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::ExtensibilityMismatch {
            writer: format!("{:?}", writer.ext),
            reader: format!("{:?}", reader.ext),
        });
    }
    let (Some(w_disc), Some(r_disc)) = (&writer.discriminator, &reader.discriminator) else {
        return TypeCompatibility::Indeterminate;
    };
    let mut indeterminate = false;
    match assignable_ids(&w_disc.type_id, &r_disc.type_id, "discriminator", ctx) {
        TypeCompatibility::Compatible => {}
        TypeCompatibility::Incompatible(e) => return TypeCompatibility::Incompatible(e),
        TypeCompatibility::Indeterminate => indeterminate = true,
    }
    if w_disc.member_flags.is_key() != r_disc.member_flags.is_key() {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::KeyMemberMismatch {
            member_name: "discriminator".to_string(),
        });
    }
    if !tce.ignore_member_names {
        if let Some(e) = name_id_inconsistency(&writer.members, &reader.members) {
            return TypeCompatibility::Incompatible(e);
        }
    }

    let w_default = writer.members.iter().find(|m| m.flags.is_default());
    let r_default = reader.members.iter().find(|m| m.flags.is_default());
    let labels = |members: &[Member]| -> HashMap<i32, usize> {
        members
            .iter()
            .enumerate()
            .flat_map(|(i, m)| m.labels.iter().map(move |l| (*l, i)))
            .collect()
    };
    let w_labels = labels(&writer.members);
    let r_labels = labels(&reader.members);

    let mut check = |m2: &Member, m1: &Member| -> Option<TypeCompatibility> {
        match assignable_ids(&m2.ty, &m1.ty, &m1.name, ctx) {
            TypeCompatibility::Compatible => None,
            TypeCompatibility::Incompatible(e) => Some(TypeCompatibility::Incompatible(e)),
            TypeCompatibility::Indeterminate => {
                indeterminate = true;
                None
            }
        }
    };
    for m2 in &writer.members {
        for label in &m2.labels {
            let m1 = r_labels.get(label).map(|&i| &reader.members[i]).or(r_default);
            if let Some(m1) = m1 {
                if let Some(e) = check(m2, m1) {
                    return e;
                }
            }
        }
    }
    if let Some(wd) = w_default {
        for m1 in &reader.members {
            if m1.labels.iter().any(|l| !w_labels.contains_key(l)) {
                if let Some(e) = check(wd, m1) {
                    return e;
                }
            }
        }
        if let Some(rd) = r_default {
            if let Some(e) = check(wd, rd) {
                return e;
            }
        }
    }

    let common = w_labels.keys().any(|l| r_labels.contains_key(l));
    if reader.ext == ExtensibilityKind::Final {
        let same_labels = w_labels.len() == r_labels.len()
            && w_labels.keys().all(|l| r_labels.contains_key(l))
            && w_default.is_some() == r_default.is_some();
        if !same_labels {
            return layout_mismatch("final union label sets differ".to_string());
        }
    } else if !common {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::NoCommonMember);
    }

    if indeterminate {
        TypeCompatibility::Indeterminate
    } else {
        TypeCompatibility::Compatible
    }
}

/// Table 18, ENUMERATION_TYPE. Extensibility is not compared: enum flags are unused on
/// the wire, so both sides always decode as the default.
fn assignable_enums(writer: &[Literal], reader: &[Literal]) -> TypeCompatibility {
    let by_name: HashMap<u32, &Literal> = writer.iter().map(|l| (l.name_hash, l)).collect();
    let by_value: HashMap<i32, &Literal> = writer.iter().map(|l| (l.value, l)).collect();
    for l1 in reader {
        let clash = by_name
            .get(&l1.name_hash)
            .filter(|l2| l2.value != l1.value)
            .or_else(|| by_value.get(&l1.value).filter(|l2| l2.name_hash != l1.name_hash));
        if let Some(l2) = clash {
            return TypeCompatibility::Incompatible(TypeCompatibilityError::MemberTypeMismatch {
                member_name: l1.name.clone(),
                writer_type: format!("literal {}={}", l2.name, l2.value),
                reader_type: format!("literal {}={}", l1.name, l1.value),
            });
        }
    }
    TypeCompatibility::Compatible
}

/// Table 19 key member rules (writer key m2 → reader key m1), on top of plain assignability:
/// the writer key must map onto the reader key without aliasing or loss. Bounds are
/// checked even when the ignore_*_bounds flags are set.
fn assignable_key(
    writer: &TypeIdentifier,
    reader: &TypeIdentifier,
    member: &str,
    ctx: &CompatCtx,
    depth: usize,
) -> TypeCompatibility {
    if depth > 32 {
        return TypeCompatibility::Indeterminate;
    }
    let (w, r) = match (node_from_id(writer, None, ctx), node_from_id(reader, None, ctx)) {
        (Node::Unresolved, _) | (_, Node::Unresolved) => return TypeCompatibility::Indeterminate,
        (Node::Plain(w), Node::Plain(r)) => return key_bounds(&w, &r, member),
        (Node::Object(w), Node::Object(r)) => (w, r),
        _ => return TypeCompatibility::Compatible,
    };
    let (w, r) = match (view_of(&w, ctx), view_of(&r, ctx)) {
        (Ok(w), Ok(r)) => (w, r),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    match (w, r) {
        (View::Enum(w), View::Enum(r)) => {
            if w.iter().all(|l2| r.iter().any(|l1| l1.value == l2.value)) {
                TypeCompatibility::Compatible
            } else {
                key_mismatch(member)
            }
        }
        (View::Struct(w), View::Struct(r)) => key_holder_members(&w, &r, member, ctx, depth),
        (View::Union(w), View::Union(r)) => key_union_labels(&w, &r, member, ctx, depth),
        _ => TypeCompatibility::Compatible,
    }
}

fn key_mismatch(member: &str) -> TypeCompatibility {
    TypeCompatibility::Incompatible(TypeCompatibilityError::KeyMemberMismatch {
        member_name: member.to_string(),
    })
}

/// String, sequence and map key members: L1 >= L2.
fn key_bounds(writer: &TypeIdentifier, reader: &TypeIdentifier, member: &str) -> TypeCompatibility {
    if writer.is_string() && reader.is_string() {
        let (w, r) = (get_string_bound(writer), get_string_bound(reader));
        if !bound_fits(w, r) {
            return TypeCompatibility::Incompatible(TypeCompatibilityError::StringBoundExceeded {
                member_name: member.to_string(),
                writer_bound: w,
                reader_bound: r,
            });
        }
        return TypeCompatibility::Compatible;
    }
    let bounds = match (sequence_info(writer), sequence_info(reader)) {
        (Some((_, w)), Some((_, r))) => Some((w, r)),
        _ => match (map_info(writer), map_info(reader)) {
            (Some((_, _, w)), Some((_, _, r))) => Some((w, r)),
            _ => None,
        },
    };
    match bounds {
        Some((w, r)) if !bound_fits(w, r) => {
            TypeCompatibility::Incompatible(TypeCompatibilityError::SequenceBoundExceeded {
                member_name: member.to_string(),
                writer_bound: w,
                reader_bound: r,
            })
        }
        _ => TypeCompatibility::Compatible,
    }
}

/// KeyHolder(T1) is-assignable-from KeyHolder(T2) for structs: the key members, or every
/// member when there is none, appear on both sides and follow the key rules.
fn key_holder_members(
    writer: &Aggregate,
    reader: &Aggregate,
    member: &str,
    ctx: &CompatCtx,
    depth: usize,
) -> TypeCompatibility {
    let holder = |a: &Aggregate| -> Vec<(u32, TypeIdentifier, String)> {
        let any_key = a.members.iter().any(|m| m.flags.is_key());
        a.members
            .iter()
            .filter(|m| !any_key || m.flags.is_key())
            .map(|m| (m.id, m.ty.clone(), m.name.clone()))
            .collect()
    };
    let (w_keys, r_keys) = (holder(writer), holder(reader));
    if r_keys.iter().any(|(id, ..)| !w_keys.iter().any(|(w_id, ..)| w_id == id)) {
        return key_mismatch(member);
    }
    let mut result = TypeCompatibility::Compatible;
    for (id, w_ty, _) in &w_keys {
        let Some((_, r_ty, r_name)) = r_keys.iter().find(|(r_id, ..)| r_id == id) else {
            return key_mismatch(member);
        };
        result = combine(result, || assignable_key(w_ty, r_ty, r_name, ctx, depth + 1));
        if matches!(result, TypeCompatibility::Incompatible(_)) {
            return result;
        }
    }
    result
}

/// Union key members: every discriminator value that selects a writer member m22 selects a
/// reader member m11 whose key accepts m22's.
fn key_union_labels(
    writer: &Aggregate,
    reader: &Aggregate,
    member: &str,
    ctx: &CompatCtx,
    depth: usize,
) -> TypeCompatibility {
    let r_default = reader.members.iter().find(|m| m.flags.is_default());
    let w_labels: Vec<i32> = writer.members.iter().flat_map(|m| m.labels.iter().copied()).collect();
    let mut result = TypeCompatibility::Compatible;
    for m22 in &writer.members {
        let mut targets = Vec::new();
        for label in &m22.labels {
            match reader.members.iter().find(|m| m.labels.contains(label)).or(r_default) {
                Some(m11) => targets.push(m11),
                None => return key_mismatch(member),
            }
        }
        // The writer default selects every value outside the writer labels; only a reader
        // default is guaranteed to select all of them.
        if m22.flags.is_default() {
            let Some(rd) = r_default else { return key_mismatch(member) };
            targets.push(rd);
            targets.extend(reader.members.iter().filter(|m| {
                !m.flags.is_default() && m.labels.iter().any(|l| !w_labels.contains(l))
            }));
        }
        for m11 in targets {
            result =
                combine(result, || assignable_key(&m22.ty, &m11.ty, &m11.name, ctx, depth + 1));
            if matches!(result, TypeCompatibility::Incompatible(_)) {
                return result;
            }
        }
    }
    result
}

/// Project a `MinimalStructType` by retaining only members whose flags lack the `@key` bit.
pub fn minimal_key_erased(ty: &MinimalStructType) -> MinimalStructType {
    MinimalStructType {
        struct_flags: ty.struct_flags,
        header: ty.header.clone(),
        member_seq: filter_minimal_members(&ty.member_seq, false),
    }
}

/// Project a `MinimalStructType` by retaining only members whose flags carry the `@key` bit.
pub fn minimal_key_holder(ty: &MinimalStructType) -> MinimalStructType {
    MinimalStructType {
        struct_flags: ty.struct_flags,
        header: ty.header.clone(),
        member_seq: filter_minimal_members(&ty.member_seq, true),
    }
}

/// Project a `CompleteStructType` by retaining only members whose flags lack the `@key` bit.
pub fn complete_key_erased(ty: &CompleteStructType) -> CompleteStructType {
    CompleteStructType {
        struct_flags: ty.struct_flags,
        header: ty.header.clone(),
        member_seq: filter_complete_members(&ty.member_seq, false),
    }
}

/// Project a `CompleteStructType` by retaining only members whose flags carry the `@key` bit.
pub fn complete_key_holder(ty: &CompleteStructType) -> CompleteStructType {
    CompleteStructType {
        struct_flags: ty.struct_flags,
        header: ty.header.clone(),
        member_seq: filter_complete_members(&ty.member_seq, true),
    }
}

fn filter_minimal_members(
    members: &[MinimalStructMember],
    keep_keys: bool,
) -> Vec<MinimalStructMember> {
    members.iter().filter(|m| m.common.member_flags.is_key() == keep_keys).cloned().collect()
}

fn filter_complete_members(
    members: &[CompleteStructMember],
    keep_keys: bool,
) -> Vec<CompleteStructMember> {
    members.iter().filter(|m| m.common.member_flags.is_key() == keep_keys).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::{
        CompleteTypeObject, MemberFlag, MinimalTypeObject, TryConstructKind, TypeFlag,
    };

    fn default_tce_policy() -> TypeConsistencyEnforcementQosPolicy {
        TypeConsistencyEnforcementQosPolicy::default()
    }

    fn disallow_coercion_policy() -> TypeConsistencyEnforcementQosPolicy {
        TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::DisallowTypeCoercion,
            ignore_sequence_bounds: false,
            ignore_string_bounds: false,
            ignore_member_names: false,
            prevent_type_widening: false,
            force_type_validation: false,
        }
    }

    fn allow_coercion_policy() -> TypeConsistencyEnforcementQosPolicy {
        TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ..Default::default()
        }
    }

    #[test]
    fn test_identical_types_compatible() {
        let hash = EquivalenceHash::compute(b"test");
        let writer_id = TypeIdentifier::MinimalTypeId(hash);
        let reader_id = TypeIdentifier::MinimalTypeId(hash);
        let policy = default_tce_policy();

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy
        )
        .is_ok());
    }

    #[test]
    fn test_hash_mismatch_disallow_coercion() {
        let writer_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(b"type1"));
        let reader_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(b"type2"));
        let policy = disallow_coercion_policy();

        let result =
            check_structural_compatibility(Some(&writer_id), Some(&reader_id), None, None, &policy);

        assert!(matches!(result, Err(TypeCompatibilityError::HashMismatch { .. })));
    }

    #[test]
    fn test_primitive_widening_rejected() {
        // XTypes 1.3 Table 15: a primitive is assignable only from the same primitive.
        let writer_id = TypeIdentifier::Int16;
        let reader_id = TypeIdentifier::Int32;
        let policy = allow_coercion_policy();

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy
        )
        .is_err());
    }

    #[test]
    fn test_primitive_widening_not_allowed_in_strict_mode() {
        let writer_id = TypeIdentifier::Int16;
        let reader_id = TypeIdentifier::Int32;
        let policy = disallow_coercion_policy();

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy
        )
        .is_err());
    }

    #[test]
    fn test_ignore_sequence_bounds() {
        let writer_id = TypeIdentifier::PlainSequenceLarge {
            header: Default::default(),
            bound: 100,
            element_identifier: Box::new(TypeIdentifier::Int32),
        };
        let reader_id = TypeIdentifier::PlainSequenceLarge {
            header: Default::default(),
            bound: 50,
            element_identifier: Box::new(TypeIdentifier::Int32),
        };

        // Without ignore_sequence_bounds - should fail
        let policy_strict = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ignore_sequence_bounds: false,
            ..Default::default()
        };

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy_strict
        )
        .is_err());

        // With ignore_sequence_bounds - should succeed
        let policy_lenient = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ignore_sequence_bounds: true,
            ..Default::default()
        };

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy_lenient
        )
        .is_ok());
    }

    #[test]
    fn test_ignore_string_bounds() {
        let writer_id = TypeIdentifier::String8Large { bound: 1000 };
        let reader_id = TypeIdentifier::String8Large { bound: 100 };

        // Without ignore_string_bounds - should fail
        let policy_strict = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ignore_string_bounds: false,
            ..Default::default()
        };

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy_strict
        )
        .is_err());

        // With ignore_string_bounds - should succeed
        let policy_lenient = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ignore_string_bounds: true,
            ..Default::default()
        };

        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            None,
            None,
            &policy_lenient
        )
        .is_ok());
    }

    #[test]
    fn test_force_type_validation() {
        let writer_id = Some(TypeIdentifier::Int32);
        let reader_id: Option<&TypeIdentifier> = None;

        let policy_normal = default_tce_policy();
        assert!(check_structural_compatibility(
            writer_id.as_ref(),
            reader_id,
            None,
            None,
            &policy_normal
        )
        .is_ok());

        let policy_forced = TypeConsistencyEnforcementQosPolicy {
            force_type_validation: true,
            ..Default::default()
        };
        assert!(check_structural_compatibility(
            writer_id.as_ref(),
            reader_id,
            None,
            None,
            &policy_forced
        )
        .is_err());
    }

    #[test]
    fn test_prevent_type_widening() {
        // The reader is the wider type: its non-optional "field2" is absent from the writer.
        let mut writer_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        writer_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));

        let mut reader_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        reader_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));
        reader_struct.add_member(MinimalStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field2",
        ));

        let writer_obj = TypeObject::Minimal(MinimalTypeObject::Struct(writer_struct));
        let reader_obj = TypeObject::Minimal(MinimalTypeObject::Struct(reader_struct));

        let writer_id = TypeIdentifier::MinimalTypeId(writer_obj.compute_hash());
        let reader_id = TypeIdentifier::MinimalTypeId(reader_obj.compute_hash());

        // With prevent_type_widening = true, should fail
        let policy = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            prevent_type_widening: true,
            ..Default::default()
        };

        let result = check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            Some(&writer_obj),
            Some(&reader_obj),
            &policy,
        );

        assert!(matches!(result, Err(TypeCompatibilityError::TypeWideningNotAllowed { .. })));
    }

    #[test]
    fn test_missing_required_member() {
        // A must_understand member has to exist on both sides (Table 19).
        let mut writer_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        writer_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));

        let mut reader_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        reader_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));
        reader_struct.add_member(MinimalStructMember::new(
            1,
            MemberFlag::new(TryConstructKind::Discard, false, false, true, false, false),
            TypeIdentifier::Int32,
            "field2",
        ));

        let writer_obj = TypeObject::Minimal(MinimalTypeObject::Struct(writer_struct));
        let reader_obj = TypeObject::Minimal(MinimalTypeObject::Struct(reader_struct));

        let writer_id = TypeIdentifier::MinimalTypeId(writer_obj.compute_hash());
        let reader_id = TypeIdentifier::MinimalTypeId(reader_obj.compute_hash());

        let policy = allow_coercion_policy();

        let result = check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            Some(&writer_obj),
            Some(&reader_obj),
            &policy,
        );

        assert!(matches!(result, Err(TypeCompatibilityError::MissingRequiredMember { .. })));
    }

    #[test]
    fn test_optional_member_can_be_missing() {
        // Writer missing an optional member
        let mut writer_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        writer_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));

        let mut reader_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        reader_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));
        reader_struct.add_member(MinimalStructMember::new(
            1,
            MemberFlag::new(TryConstructKind::Discard, false, true, false, false, false), // Optional
            TypeIdentifier::Int32,
            "field2",
        ));

        let writer_obj = TypeObject::Minimal(MinimalTypeObject::Struct(writer_struct));
        let reader_obj = TypeObject::Minimal(MinimalTypeObject::Struct(reader_struct));

        let writer_id = TypeIdentifier::MinimalTypeId(writer_obj.compute_hash());
        let reader_id = TypeIdentifier::MinimalTypeId(reader_obj.compute_hash());

        let policy = allow_coercion_policy();

        let result = check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            Some(&writer_obj),
            Some(&reader_obj),
            &policy,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_no_type_identifiers_backward_compatible() {
        let policy = default_tce_policy();

        assert!(check_structural_compatibility(None, None, None, None, &policy).is_ok());
    }

    #[test]
    fn test_ignore_member_names_matches_by_id() {
        let mut writer_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        writer_struct.add_member(MinimalStructMember::new(
            7,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "alpha",
        ));

        let mut reader_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        reader_struct.add_member(MinimalStructMember::new(
            7,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "beta",
        ));

        let writer_obj = TypeObject::Minimal(MinimalTypeObject::Struct(writer_struct));
        let reader_obj = TypeObject::Minimal(MinimalTypeObject::Struct(reader_struct));
        let writer_id = TypeIdentifier::MinimalTypeId(writer_obj.compute_hash());
        let reader_id = TypeIdentifier::MinimalTypeId(reader_obj.compute_hash());

        let strict = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ignore_member_names: false,
            ..Default::default()
        };
        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            Some(&writer_obj),
            Some(&reader_obj),
            &strict,
        )
        .is_err());

        let lenient = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            ignore_member_names: true,
            ..Default::default()
        };
        assert!(check_structural_compatibility(
            Some(&writer_id),
            Some(&reader_id),
            Some(&writer_obj),
            Some(&reader_obj),
            &lenient,
        )
        .is_ok());
    }

    #[test]
    fn test_minimal_key_projections_split_members() {
        let mut ty = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        ty.add_member(MinimalStructMember::new(
            1,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, true, false),
            TypeIdentifier::Int32,
            "id",
        ));
        ty.add_member(MinimalStructMember::new(
            2,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "value",
        ));

        let erased = minimal_key_erased(&ty);
        assert_eq!(erased.member_seq.len(), 1);
        assert_eq!(erased.member_seq[0].common.member_id, 2);

        let holder = minimal_key_holder(&ty);
        assert_eq!(holder.member_seq.len(), 1);
        assert_eq!(holder.member_seq[0].common.member_id, 1);
        assert!(holder.member_seq[0].common.member_flags.is_key());
    }

    #[test]
    fn test_complete_key_projections_split_members() {
        use crate::xtypes::CompleteStructMember;

        let mut ty = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            "Sample".to_string(),
            None,
        );
        ty.add_member(CompleteStructMember::new(
            1,
            MemberFlag::new(TryConstructKind::Discard, false, false, false, true, false),
            TypeIdentifier::Int32,
            "id".to_string(),
        ));
        ty.add_member(CompleteStructMember::new(
            2,
            MemberFlag::default(),
            TypeIdentifier::String8,
            "value".to_string(),
        ));

        let erased = complete_key_erased(&ty);
        assert_eq!(erased.member_seq.len(), 1);
        assert_eq!(erased.member_seq[0].detail.name, "value");

        let holder = complete_key_holder(&ty);
        assert_eq!(holder.member_seq.len(), 1);
        assert_eq!(holder.member_seq[0].detail.name, "id");
        assert!(holder.member_seq[0].common.member_flags.is_key());
    }

    #[test]
    fn test_evaluate_indeterminate_without_type_objects() {
        // AllowTypeCoercion, two complex types with different hashes and no
        // TypeObjects available -> needs resolution, so Indeterminate.
        let writer_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(b"a"));
        let reader_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(b"b"));
        let policy = allow_coercion_policy();

        assert_eq!(
            evaluate_structural_compatibility(
                Some(&writer_id),
                Some(&reader_id),
                None,
                None,
                &policy,
                &NullResolver
            ),
            TypeCompatibility::Indeterminate
        );
    }

    #[test]
    fn test_evaluate_compatible_same_hash() {
        let hash = EquivalenceHash::compute(b"same");
        let writer_id = TypeIdentifier::MinimalTypeId(hash);
        let reader_id = TypeIdentifier::MinimalTypeId(hash);

        assert_eq!(
            evaluate_structural_compatibility(
                Some(&writer_id),
                Some(&reader_id),
                None,
                None,
                &allow_coercion_policy(),
                &NullResolver
            ),
            TypeCompatibility::Compatible
        );
    }

    #[test]
    fn test_evaluate_disallow_hash_mismatch_is_incompatible() {
        // DisallowTypeCoercion never defers: a hash mismatch is Incompatible.
        let writer_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(b"a"));
        let reader_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(b"b"));

        assert!(matches!(
            evaluate_structural_compatibility(
                Some(&writer_id),
                Some(&reader_id),
                None,
                None,
                &disallow_coercion_policy(),
                &NullResolver
            ),
            TypeCompatibility::Incompatible(_)
        ));
    }

    #[test]
    fn test_evaluate_both_type_objects_present_not_indeterminate() {
        // With both TypeObjects available, the structural check runs to a verdict
        // (here: reader widens writer with prevent_type_widening -> Incompatible),
        // never Indeterminate.
        let mut writer_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        writer_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));

        let mut reader_struct = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        reader_struct.add_member(MinimalStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field1",
        ));
        reader_struct.add_member(MinimalStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field2",
        ));

        let writer_obj = TypeObject::Minimal(MinimalTypeObject::Struct(writer_struct));
        let reader_obj = TypeObject::Minimal(MinimalTypeObject::Struct(reader_struct));
        let writer_id = TypeIdentifier::MinimalTypeId(writer_obj.compute_hash());
        let reader_id = TypeIdentifier::MinimalTypeId(reader_obj.compute_hash());

        let policy = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            prevent_type_widening: true,
            ..Default::default()
        };

        assert!(matches!(
            evaluate_structural_compatibility(
                Some(&writer_id),
                Some(&reader_id),
                Some(&writer_obj),
                Some(&reader_obj),
                &policy,
                &NullResolver
            ),
            TypeCompatibility::Incompatible(_)
        ));
    }

    struct MapResolver(std::collections::HashMap<EquivalenceHash, TypeObject>);

    impl TypeResolver for MapResolver {
        fn resolve(&self, id: &TypeIdentifier) -> Option<TypeObject> {
            id.equivalence_hash().and_then(|h| self.0.get(h).cloned())
        }
    }

    fn appendable_struct(members: &[(u32, &str, TypeIdentifier)]) -> MinimalStructType {
        let mut s = MinimalStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            None,
        );
        for (id, name, ty) in members {
            s.add_member(MinimalStructMember::new(*id, MemberFlag::default(), ty.clone(), name));
        }
        s
    }

    #[test]
    fn test_nested_member_incompatibility_detected_via_resolver() {
        // Reader.inner has an extra required member that writer.inner lacks under
        // prevent_type_widening -> deep comparison must reject, not pass.
        let w_inner = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "a",
            TypeIdentifier::Int32,
        )])));
        let r_inner = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[
            (0, "a", TypeIdentifier::Int32),
            (1, "b", TypeIdentifier::Int32),
        ])));
        let w_inner_hash = w_inner.compute_hash();
        let r_inner_hash = r_inner.compute_hash();

        let w_outer = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "child",
            TypeIdentifier::MinimalTypeId(w_inner_hash),
        )])));
        let r_outer = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "child",
            TypeIdentifier::MinimalTypeId(r_inner_hash),
        )])));
        let w_id = TypeIdentifier::MinimalTypeId(w_outer.compute_hash());
        let r_id = TypeIdentifier::MinimalTypeId(r_outer.compute_hash());

        let mut map = std::collections::HashMap::new();
        map.insert(w_inner_hash, w_inner);
        map.insert(r_inner_hash, r_inner);
        let resolver = MapResolver(map);

        let policy = TypeConsistencyEnforcementQosPolicy {
            kind: TypeConsistencyKind::AllowTypeCoercion,
            prevent_type_widening: true,
            ..Default::default()
        };

        assert!(matches!(
            evaluate_structural_compatibility(
                Some(&w_id),
                Some(&r_id),
                Some(&w_outer),
                Some(&r_outer),
                &policy,
                &resolver
            ),
            TypeCompatibility::Incompatible(_)
        ));
    }

    #[test]
    fn test_nested_member_unresolved_is_indeterminate() {
        // Same nested shapes, but the resolver cannot supply the inner types ->
        // Indeterminate (needs TypeLookup), not an optimistic Compatible.
        let w_inner_hash = EquivalenceHash::compute(b"inner_w");
        let r_inner_hash = EquivalenceHash::compute(b"inner_r");

        let w_outer = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "child",
            TypeIdentifier::MinimalTypeId(w_inner_hash),
        )])));
        let r_outer = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "child",
            TypeIdentifier::MinimalTypeId(r_inner_hash),
        )])));
        let w_id = TypeIdentifier::MinimalTypeId(w_outer.compute_hash());
        let r_id = TypeIdentifier::MinimalTypeId(r_outer.compute_hash());

        assert_eq!(
            evaluate_structural_compatibility(
                Some(&w_id),
                Some(&r_id),
                Some(&w_outer),
                Some(&r_outer),
                &allow_coercion_policy(),
                &NullResolver
            ),
            TypeCompatibility::Indeterminate
        );
    }

    #[test]
    fn test_mixed_minimal_complete_is_not_trusted() {
        // Writer inline is Minimal, reader inline is Complete, differing hashes and no
        // resolver to normalize -> must be Indeterminate, never an optimistic Compatible.
        let w_obj = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "a",
            TypeIdentifier::Int32,
        )])));
        let mut r_struct = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            "R".to_string(),
            None,
        );
        r_struct.add_member(CompleteStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "a".to_string(),
        ));
        let r_obj = TypeObject::Complete(CompleteTypeObject::Struct(r_struct));
        let w_id = TypeIdentifier::MinimalTypeId(w_obj.compute_hash());
        let r_id = TypeIdentifier::CompleteTypeId(r_obj.compute_hash());

        assert_eq!(
            evaluate_structural_compatibility(
                Some(&w_id),
                Some(&r_id),
                Some(&w_obj),
                Some(&r_obj),
                &allow_coercion_policy(),
                &NullResolver
            ),
            TypeCompatibility::Indeterminate
        );
    }

    // ---- Cross-EK (Complete vs Minimal ids for the same/other type) ----

    struct CompleteMapResolver {
        completes: HashMap<EquivalenceHash, crate::xtypes::CompleteTypeObject>,
    }

    impl TypeResolver for CompleteMapResolver {
        fn resolve(&self, id: &TypeIdentifier) -> Option<TypeObject> {
            self.resolve_complete(id)
        }
        fn resolve_complete(&self, id: &TypeIdentifier) -> Option<TypeObject> {
            id.equivalence_hash()
                .and_then(|h| self.completes.get(h))
                .map(|c| TypeObject::Complete(c.clone()))
        }
    }

    fn complete_struct_obj(
        name: &str,
        members: &[(u32, &str)],
    ) -> crate::xtypes::CompleteTypeObject {
        let mut s = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            name.to_string(),
            None,
        );
        for (id, mname) in members {
            s.add_member(CompleteStructMember::new(
                *id,
                MemberFlag::default(),
                TypeIdentifier::Int32,
                mname.to_string(),
            ));
        }
        crate::xtypes::CompleteTypeObject::Struct(s)
    }

    #[test]
    fn cross_ek_same_type_is_compatible() {
        let obj = complete_struct_obj("Same", &[(0, "a")]);
        let hc = EquivalenceHash::new([1; 14]);
        let hm = EquivalenceHash::new([2; 14]);
        let w_id = TypeIdentifier::CompleteTypeId(hc);
        let r_id = TypeIdentifier::MinimalTypeId(hm);
        // Both hash ids resolve to the SAME complete object.
        let mut completes = HashMap::new();
        completes.insert(hc, obj.clone());
        completes.insert(hm, obj);
        let resolver = CompleteMapResolver { completes };

        for policy in [allow_coercion_policy(), disallow_coercion_policy()] {
            assert_eq!(
                evaluate_structural_compatibility(
                    Some(&w_id),
                    Some(&r_id),
                    None,
                    None,
                    &policy,
                    &resolver
                ),
                TypeCompatibility::Compatible,
                "cross-EK same type must be Compatible"
            );
        }
    }

    #[test]
    fn cross_ek_different_types_incompatible() {
        // No member id in common -> structurally incompatible.
        let w_obj = complete_struct_obj("W", &[(0, "a")]);
        let r_obj = complete_struct_obj("R", &[(1, "b")]);
        let hc = EquivalenceHash::new([3; 14]);
        let hm = EquivalenceHash::new([4; 14]);
        let w_id = TypeIdentifier::CompleteTypeId(hc);
        let r_id = TypeIdentifier::MinimalTypeId(hm);
        let mut completes = HashMap::new();
        completes.insert(hc, w_obj);
        completes.insert(hm, r_obj);
        let resolver = CompleteMapResolver { completes };

        // AllowTypeCoercion: structural comparison reports the incompatibility.
        assert!(matches!(
            evaluate_structural_compatibility(
                Some(&w_id),
                Some(&r_id),
                None,
                None,
                &allow_coercion_policy(),
                &resolver
            ),
            TypeCompatibility::Incompatible(_)
        ));
        // DisallowTypeCoercion: differing content hashes report a hash mismatch.
        assert!(matches!(
            evaluate_structural_compatibility(
                Some(&w_id),
                Some(&r_id),
                None,
                None,
                &disallow_coercion_policy(),
                &resolver
            ),
            TypeCompatibility::Incompatible(_)
        ));
    }

    #[test]
    fn cross_ek_unresolvable_is_indeterminate() {
        let w_id = TypeIdentifier::CompleteTypeId(EquivalenceHash::new([5; 14]));
        let r_id = TypeIdentifier::MinimalTypeId(EquivalenceHash::new([6; 14]));
        for policy in [allow_coercion_policy(), disallow_coercion_policy()] {
            assert_eq!(
                evaluate_structural_compatibility(
                    Some(&w_id),
                    Some(&r_id),
                    None,
                    None,
                    &policy,
                    &NullResolver
                ),
                TypeCompatibility::Indeterminate,
                "cross-EK unresolvable must be Indeterminate"
            );
        }
    }
}
