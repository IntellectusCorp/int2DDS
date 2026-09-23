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
    spec_hash, CompleteStructMember, CompleteStructType, EquivalenceHash, ExtensibilityKind,
    MinimalStructMember, MinimalStructType, TypeIdentifier, TypeObject,
};

/// Error type for type compatibility checking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeCompatibilityError {
    /// TypeIdentifier hash mismatch in DisallowTypeCoercion mode.
    HashMismatch { writer_hash: EquivalenceHash, reader_hash: EquivalenceHash },

    /// Type kinds are fundamentally incompatible (e.g., struct vs enum).
    IncompatibleKind { writer: String, reader: String },

    /// Required member in reader is missing from writer.
    MissingRequiredMember { member_name: String, member_id: u32 },

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
                write!(
                    f,
                    "Missing required member '{}' (id={}) in writer type",
                    member_name, member_id
                )
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
    visited: std::cell::RefCell<std::collections::HashSet<(EquivalenceHash, EquivalenceHash)>>,
}

impl<'a> CompatCtx<'a> {
    fn new(resolver: &'a dyn TypeResolver, tce: &'a TypeConsistencyEnforcementQosPolicy) -> Self {
        Self { resolver, tce, visited: std::cell::RefCell::new(std::collections::HashSet::new()) }
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

        let w_obj = writer_type_obj.cloned().or_else(|| ctx.resolver.resolve(writer_id));
        let r_obj = reader_type_obj.cloned().or_else(|| ctx.resolver.resolve(reader_id));
        return match (w_obj, r_obj) {
            (Some(w), Some(r)) if same_object_kind(&w, &r) => {
                check_type_object_compatibility(&w, &r, ctx)
            }
            (Some(_), Some(_)) => {
                match (ctx.resolver.resolve(writer_id), ctx.resolver.resolve(reader_id)) {
                    (Some(w), Some(r)) => check_type_object_compatibility(&w, &r, ctx),
                    _ => TypeCompatibility::Indeterminate,
                }
            }
            _ => TypeCompatibility::Indeterminate,
        };
    }

    if writer_id.is_primitive() && reader_id.is_primitive() {
        return from_result(check_primitive_coercion(writer_id, reader_id));
    }

    if writer_id.is_string() && reader_id.is_string() {
        return from_result(check_string_compatibility(
            writer_id,
            reader_id,
            ctx.tce.ignore_string_bounds,
        ));
    }

    if writer_id.is_collection() && reader_id.is_collection() {
        return check_collection_compatibility(writer_id, reader_id, ctx);
    }

    TypeCompatibility::Incompatible(TypeCompatibilityError::IncompatibleKind {
        writer: format!("{:?}", writer_id),
        reader: format!("{:?}", reader_id),
    })
}

fn same_object_kind(a: &TypeObject, b: &TypeObject) -> bool {
    matches!(
        (a, b),
        (TypeObject::Minimal(_), TypeObject::Minimal(_))
            | (TypeObject::Complete(_), TypeObject::Complete(_))
    )
}

fn check_type_object_compatibility(
    writer_obj: &TypeObject,
    reader_obj: &TypeObject,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    match (writer_obj, reader_obj) {
        (TypeObject::Minimal(w), TypeObject::Minimal(r)) => {
            check_minimal_type_object_compatibility(w, r, ctx)
        }
        (TypeObject::Complete(w), TypeObject::Complete(r)) => {
            check_complete_type_object_compatibility(w, r, ctx)
        }
        // Mixed Minimal/Complete cannot be structurally compared: defer instead of trusting.
        _ => TypeCompatibility::Indeterminate,
    }
}

fn check_minimal_type_object_compatibility(
    writer_obj: &crate::xtypes::MinimalTypeObject,
    reader_obj: &crate::xtypes::MinimalTypeObject,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    use crate::xtypes::MinimalTypeObject;

    match (writer_obj, reader_obj) {
        (MinimalTypeObject::Struct(w_struct), MinimalTypeObject::Struct(r_struct)) => {
            check_minimal_struct_compatibility(w_struct, r_struct, ctx)
        }
        (MinimalTypeObject::Enum(w_enum), MinimalTypeObject::Enum(r_enum)) => {
            from_result(check_minimal_enum_compatibility(w_enum, r_enum))
        }
        _ => TypeCompatibility::Incompatible(TypeCompatibilityError::IncompatibleKind {
            writer: format!("{:?}", std::mem::discriminant(writer_obj)),
            reader: format!("{:?}", std::mem::discriminant(reader_obj)),
        }),
    }
}

/// Check structural compatibility between MinimalStructTypes.
fn check_minimal_struct_compatibility(
    writer: &MinimalStructType,
    reader: &MinimalStructType,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let tce_policy = ctx.tce;
    let w_ext = writer.struct_flags.extensibility();
    let r_ext = reader.struct_flags.extensibility();

    if (w_ext == ExtensibilityKind::Final || r_ext == ExtensibilityKind::Final) && w_ext != r_ext {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::ExtensibilityMismatch {
            writer: format!("{:?}", w_ext),
            reader: format!("{:?}", r_ext),
        });
    }

    let writer_members = build_member_map(&writer.member_seq, tce_policy.ignore_member_names);

    let mut indeterminate = false;
    for r_member in &reader.member_seq {
        let key = get_member_key(r_member, tce_policy.ignore_member_names);
        let member_name = format!("hash:{:08x}", r_member.name_hash);

        match writer_members.get(&key) {
            Some(w_member) => {
                match check_member_type_compatibility(
                    &w_member.common.member_type_id,
                    &r_member.common.member_type_id,
                    &member_name,
                    ctx,
                ) {
                    TypeCompatibility::Compatible => {}
                    TypeCompatibility::Incompatible(e) => {
                        return TypeCompatibility::Incompatible(e)
                    }
                    TypeCompatibility::Indeterminate => indeterminate = true,
                }

                if w_member.common.member_flags.is_key() != r_member.common.member_flags.is_key() {
                    return TypeCompatibility::Incompatible(
                        TypeCompatibilityError::KeyMemberMismatch { member_name },
                    );
                }
            }
            None => {
                if !r_member.common.member_flags.is_optional() {
                    return TypeCompatibility::Incompatible(
                        TypeCompatibilityError::MissingRequiredMember {
                            member_name,
                            member_id: r_member.common.member_id,
                        },
                    );
                }
            }
        }
    }

    if tce_policy.prevent_type_widening {
        let reader_members = build_member_map(&reader.member_seq, tce_policy.ignore_member_names);

        let extra_members: Vec<String> = writer
            .member_seq
            .iter()
            .filter(|w_m| {
                let key = get_member_key(w_m, tce_policy.ignore_member_names);
                !reader_members.contains_key(&key)
            })
            .map(|m| format!("hash:{:08x}", m.name_hash))
            .collect();

        if !extra_members.is_empty() {
            return TypeCompatibility::Incompatible(
                TypeCompatibilityError::TypeWideningNotAllowed { extra_members },
            );
        }
    }

    if indeterminate {
        TypeCompatibility::Indeterminate
    } else {
        TypeCompatibility::Compatible
    }
}

/// Build a HashMap from member sequence for O(1) lookup.
fn build_member_map(
    members: &[MinimalStructMember],
    ignore_member_names: bool,
) -> HashMap<u32, &MinimalStructMember> {
    members.iter().map(|m| (get_member_key(m, ignore_member_names), m)).collect()
}

/// Get the key used for member matching.
fn get_member_key(member: &MinimalStructMember, ignore_member_names: bool) -> u32 {
    if ignore_member_names {
        member.common.member_id
    } else {
        member.name_hash
    }
}

fn check_complete_type_object_compatibility(
    writer_obj: &crate::xtypes::CompleteTypeObject,
    reader_obj: &crate::xtypes::CompleteTypeObject,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    use crate::xtypes::CompleteTypeObject;

    match (writer_obj, reader_obj) {
        (CompleteTypeObject::Struct(w), CompleteTypeObject::Struct(r)) => {
            check_complete_struct_compatibility(w, r, ctx)
        }
        (CompleteTypeObject::Enum(w), CompleteTypeObject::Enum(r)) => {
            from_result(check_complete_enum_compatibility(w, r))
        }
        (CompleteTypeObject::Union(w), CompleteTypeObject::Union(r)) => {
            check_complete_union_compatibility(w, r, ctx)
        }
        _ => TypeCompatibility::Incompatible(TypeCompatibilityError::IncompatibleKind {
            writer: format!("{:?}", std::mem::discriminant(writer_obj)),
            reader: format!("{:?}", std::mem::discriminant(reader_obj)),
        }),
    }
}

fn check_complete_struct_compatibility(
    writer: &crate::xtypes::CompleteStructType,
    reader: &crate::xtypes::CompleteStructType,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let tce_policy = ctx.tce;
    let w_ext = writer.struct_flags.extensibility();
    let r_ext = reader.struct_flags.extensibility();

    if (w_ext == ExtensibilityKind::Final || r_ext == ExtensibilityKind::Final) && w_ext != r_ext {
        return TypeCompatibility::Incompatible(TypeCompatibilityError::ExtensibilityMismatch {
            writer: format!("{:?}", w_ext),
            reader: format!("{:?}", r_ext),
        });
    }

    let writer_members: HashMap<String, &crate::xtypes::CompleteStructMember> = writer
        .member_seq
        .iter()
        .map(|m| {
            let key = if tce_policy.ignore_member_names {
                m.common.member_id.to_string()
            } else {
                m.detail.name.clone()
            };
            (key, m)
        })
        .collect();

    let mut indeterminate = false;
    for r_member in &reader.member_seq {
        let key = if tce_policy.ignore_member_names {
            r_member.common.member_id.to_string()
        } else {
            r_member.detail.name.clone()
        };

        match writer_members.get(&key) {
            Some(w_member) => {
                match check_member_type_compatibility(
                    &w_member.common.member_type_id,
                    &r_member.common.member_type_id,
                    &r_member.detail.name,
                    ctx,
                ) {
                    TypeCompatibility::Compatible => {}
                    TypeCompatibility::Incompatible(e) => {
                        return TypeCompatibility::Incompatible(e)
                    }
                    TypeCompatibility::Indeterminate => indeterminate = true,
                }

                if w_member.common.member_flags.is_key() != r_member.common.member_flags.is_key() {
                    return TypeCompatibility::Incompatible(
                        TypeCompatibilityError::KeyMemberMismatch {
                            member_name: r_member.detail.name.clone(),
                        },
                    );
                }
            }
            None => {
                if !r_member.common.member_flags.is_optional() {
                    return TypeCompatibility::Incompatible(
                        TypeCompatibilityError::MissingRequiredMember {
                            member_name: r_member.detail.name.clone(),
                            member_id: r_member.common.member_id,
                        },
                    );
                }
            }
        }
    }

    if tce_policy.prevent_type_widening {
        let reader_members: HashMap<String, &crate::xtypes::CompleteStructMember> = reader
            .member_seq
            .iter()
            .map(|m| {
                let key = if tce_policy.ignore_member_names {
                    m.common.member_id.to_string()
                } else {
                    m.detail.name.clone()
                };
                (key, m)
            })
            .collect();

        let extra_members: Vec<String> = writer
            .member_seq
            .iter()
            .filter(|w_m| {
                let key = if tce_policy.ignore_member_names {
                    w_m.common.member_id.to_string()
                } else {
                    w_m.detail.name.clone()
                };
                !reader_members.contains_key(&key)
            })
            .map(|m| m.detail.name.clone())
            .collect();

        if !extra_members.is_empty() {
            return TypeCompatibility::Incompatible(
                TypeCompatibilityError::TypeWideningNotAllowed { extra_members },
            );
        }
    }

    if indeterminate {
        TypeCompatibility::Indeterminate
    } else {
        TypeCompatibility::Compatible
    }
}

fn check_complete_enum_compatibility(
    writer: &crate::xtypes::CompleteEnumeratedType,
    reader: &crate::xtypes::CompleteEnumeratedType,
) -> TypeCompatibilityResult {
    let writer_literals: HashMap<&str, i32> =
        writer.literal_seq.iter().map(|l| (l.detail.name.as_str(), l.common.value)).collect();

    for r_literal in &reader.literal_seq {
        if r_literal.common.flags.is_default() {
            continue;
        }
        match writer_literals.get(r_literal.detail.name.as_str()) {
            Some(&w_value) => {
                if w_value != r_literal.common.value {
                    return Err(TypeCompatibilityError::MemberTypeMismatch {
                        member_name: r_literal.detail.name.clone(),
                        writer_type: format!("enum value {}", w_value),
                        reader_type: format!("enum value {}", r_literal.common.value),
                    });
                }
            }
            None => {
                return Err(TypeCompatibilityError::MissingRequiredMember {
                    member_name: r_literal.detail.name.clone(),
                    member_id: r_literal.common.value as u32,
                });
            }
        }
    }

    Ok(())
}

fn check_minimal_enum_compatibility(
    writer: &crate::xtypes::MinimalEnumeratedType,
    reader: &crate::xtypes::MinimalEnumeratedType,
) -> TypeCompatibilityResult {
    let writer_literals: HashMap<u32, i32> =
        writer.literal_seq.iter().map(|l| (l.name_hash, l.common.value)).collect();

    for r_literal in &reader.literal_seq {
        if r_literal.common.flags.is_default() {
            continue;
        }
        match writer_literals.get(&r_literal.name_hash) {
            Some(&w_value) => {
                if w_value != r_literal.common.value {
                    return Err(TypeCompatibilityError::MemberTypeMismatch {
                        member_name: format!("hash:{:08x}", r_literal.name_hash),
                        writer_type: format!("enum value {}", w_value),
                        reader_type: format!("enum value {}", r_literal.common.value),
                    });
                }
            }
            None => {
                return Err(TypeCompatibilityError::MissingRequiredMember {
                    member_name: format!("hash:{:08x}", r_literal.name_hash),
                    member_id: r_literal.common.value as u32,
                });
            }
        }
    }

    Ok(())
}

fn check_complete_union_compatibility(
    writer: &crate::xtypes::CompleteUnionType,
    reader: &crate::xtypes::CompleteUnionType,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let mut indeterminate = false;
    match check_member_type_compatibility(
        &writer.discriminator.type_id,
        &reader.discriminator.type_id,
        "__discriminator",
        ctx,
    ) {
        TypeCompatibility::Compatible => {}
        TypeCompatibility::Incompatible(e) => return TypeCompatibility::Incompatible(e),
        TypeCompatibility::Indeterminate => indeterminate = true,
    }

    let writer_members: HashMap<&str, &crate::xtypes::CompleteUnionMember> =
        writer.member_seq.iter().map(|m| (m.detail.name.as_str(), m)).collect();

    for r_member in &reader.member_seq {
        if r_member.common.member_flags.is_default() {
            continue;
        }
        match writer_members.get(r_member.detail.name.as_str()) {
            Some(w_member) => {
                match check_member_type_compatibility(
                    &w_member.common.member_type_id,
                    &r_member.common.member_type_id,
                    &r_member.detail.name,
                    ctx,
                ) {
                    TypeCompatibility::Compatible => {}
                    TypeCompatibility::Incompatible(e) => {
                        return TypeCompatibility::Incompatible(e)
                    }
                    TypeCompatibility::Indeterminate => indeterminate = true,
                }
            }
            None => {
                return TypeCompatibility::Incompatible(
                    TypeCompatibilityError::MissingRequiredMember {
                        member_name: r_member.detail.name.clone(),
                        member_id: r_member.common.member_id,
                    },
                );
            }
        }
    }

    if indeterminate {
        TypeCompatibility::Indeterminate
    } else {
        TypeCompatibility::Compatible
    }
}

fn check_member_type_compatibility(
    writer_type: &TypeIdentifier,
    reader_type: &TypeIdentifier,
    member_name: &str,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let tce_policy = ctx.tce;
    if writer_type == reader_type {
        return TypeCompatibility::Compatible;
    }

    if writer_type.is_primitive() && reader_type.is_primitive() {
        return from_result(check_primitive_coercion(writer_type, reader_type).map_err(|_| {
            TypeCompatibilityError::MemberTypeMismatch {
                member_name: member_name.to_string(),
                writer_type: format!("{:?}", writer_type),
                reader_type: format!("{:?}", reader_type),
            }
        }));
    }

    if writer_type.is_string() && reader_type.is_string() {
        return from_result(check_string_compatibility_with_name(
            writer_type,
            reader_type,
            member_name,
            tce_policy.ignore_string_bounds,
        ));
    }

    if is_sequence_type(writer_type) && is_sequence_type(reader_type) {
        let w_elem = get_sequence_element(writer_type);
        let r_elem = get_sequence_element(reader_type);

        let mut indeterminate = false;
        if let (Some(w_elem), Some(r_elem)) = (w_elem, r_elem) {
            match check_member_type_compatibility(w_elem, r_elem, member_name, ctx) {
                TypeCompatibility::Compatible => {}
                TypeCompatibility::Incompatible(e) => return TypeCompatibility::Incompatible(e),
                TypeCompatibility::Indeterminate => indeterminate = true,
            }
        }

        let w = get_sequence_bound(writer_type);
        let r = get_sequence_bound(reader_type);

        if !tce_policy.ignore_sequence_bounds && w > 0 && r > 0 && w > r {
            return TypeCompatibility::Incompatible(
                TypeCompatibilityError::SequenceBoundExceeded {
                    member_name: member_name.to_string(),
                    writer_bound: w,
                    reader_bound: r,
                },
            );
        }

        return if indeterminate {
            TypeCompatibility::Indeterminate
        } else {
            TypeCompatibility::Compatible
        };
    }

    if writer_type.is_complex() && reader_type.is_complex() {
        if writer_type.equivalence_hash() == reader_type.equivalence_hash() {
            return TypeCompatibility::Compatible;
        }

        if let (Some(wh), Some(rh)) =
            (writer_type.equivalence_hash().copied(), reader_type.equivalence_hash().copied())
        {
            if !ctx.enter((wh, rh)) {
                return TypeCompatibility::Compatible;
            }
        }

        // Same type advertised under different equivalence kinds: reconcile via Complete.
        if is_cross_ek(writer_type, reader_type) {
            return cross_ek_via_complete(writer_type, reader_type, ctx);
        }

        let w_obj = ctx.resolver.resolve(writer_type);
        let r_obj = ctx.resolver.resolve(reader_type);
        return match (w_obj, r_obj) {
            (Some(w), Some(r)) => check_type_object_compatibility(&w, &r, ctx),
            _ => TypeCompatibility::Indeterminate,
        };
    }

    TypeCompatibility::Incompatible(TypeCompatibilityError::MemberTypeMismatch {
        member_name: member_name.to_string(),
        writer_type: format!("{:?}", writer_type),
        reader_type: format!("{:?}", reader_type),
    })
}

/// Check if primitive type coercion is allowed.
fn check_primitive_coercion(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
) -> TypeCompatibilityResult {
    if writer_id == reader_id {
        return Ok(());
    }

    let is_coercion_allowed = match (writer_id, reader_id) {
        // Integer widening (signed)
        (TypeIdentifier::Int8, TypeIdentifier::Int16)
        | (TypeIdentifier::Int8, TypeIdentifier::Int32)
        | (TypeIdentifier::Int8, TypeIdentifier::Int64)
        | (TypeIdentifier::Int16, TypeIdentifier::Int32)
        | (TypeIdentifier::Int16, TypeIdentifier::Int64)
        | (TypeIdentifier::Int32, TypeIdentifier::Int64) => true,

        // Integer widening (unsigned)
        (TypeIdentifier::Uint8, TypeIdentifier::Uint16)
        | (TypeIdentifier::Uint8, TypeIdentifier::Uint32)
        | (TypeIdentifier::Uint8, TypeIdentifier::Uint64)
        | (TypeIdentifier::Uint16, TypeIdentifier::Uint32)
        | (TypeIdentifier::Uint16, TypeIdentifier::Uint64)
        | (TypeIdentifier::Uint32, TypeIdentifier::Uint64) => true,

        // Float widening
        (TypeIdentifier::Float32, TypeIdentifier::Float64) => true,

        // Char widening
        (TypeIdentifier::Char8, TypeIdentifier::Char16) => true,

        // All other cases are not allowed
        _ => false,
    };

    if is_coercion_allowed {
        Ok(())
    } else {
        Err(TypeCompatibilityError::IncompatibleKind {
            writer: format!("{:?}", writer_id),
            reader: format!("{:?}", reader_id),
        })
    }
}

/// Check string type compatibility.
fn check_string_compatibility(
    writer_type: &TypeIdentifier,
    reader_type: &TypeIdentifier,
    ignore_string_bounds: bool,
) -> TypeCompatibilityResult {
    check_string_compatibility_with_name(writer_type, reader_type, "string", ignore_string_bounds)
}

/// Check string type compatibility with member name for error reporting.
fn check_string_compatibility_with_name(
    writer_type: &TypeIdentifier,
    reader_type: &TypeIdentifier,
    member_name: &str,
    ignore_string_bounds: bool,
) -> TypeCompatibilityResult {
    // Get bounds (0 = unbounded)
    let writer_bound = get_string_bound(writer_type);
    let reader_bound = get_string_bound(reader_type);

    // Check string type compatibility (String8 vs String16)
    let writer_is_wide = is_wide_string(writer_type);
    let reader_is_wide = is_wide_string(reader_type);

    if writer_is_wide != reader_is_wide {
        return Err(TypeCompatibilityError::MemberTypeMismatch {
            member_name: member_name.to_string(),
            writer_type: format!("{:?}", writer_type),
            reader_type: format!("{:?}", reader_type),
        });
    }

    if ignore_string_bounds {
        return Ok(());
    }

    // Check bounds: reader must accept all writer data
    // writer_bound > 0 means bounded, 0 means unbounded
    if writer_bound > 0 && reader_bound > 0 && writer_bound > reader_bound {
        return Err(TypeCompatibilityError::StringBoundExceeded {
            member_name: member_name.to_string(),
            writer_bound,
            reader_bound,
        });
    }

    Ok(())
}

/// Get string bound from TypeIdentifier (0 = unbounded).
fn get_string_bound(type_id: &TypeIdentifier) -> u32 {
    match type_id {
        TypeIdentifier::String8 | TypeIdentifier::String16 => 0, // unbounded
        TypeIdentifier::String8Small { bound } | TypeIdentifier::String16Small { bound } => {
            *bound as u32
        }
        TypeIdentifier::String8Large { bound } | TypeIdentifier::String16Large { bound } => *bound,
        _ => 0,
    }
}

/// Check if string type is wide (String16).
fn is_wide_string(type_id: &TypeIdentifier) -> bool {
    matches!(
        type_id,
        TypeIdentifier::String16
            | TypeIdentifier::String16Small { .. }
            | TypeIdentifier::String16Large { .. }
    )
}

/// Check collection (sequence/array/map) compatibility.
fn check_collection_compatibility(
    writer_type: &TypeIdentifier,
    reader_type: &TypeIdentifier,
    ctx: &CompatCtx,
) -> TypeCompatibility {
    let tce_policy = ctx.tce;
    if is_sequence_type(writer_type) && is_sequence_type(reader_type) {
        let w_elem = get_sequence_element(writer_type);
        let r_elem = get_sequence_element(reader_type);

        let mut indeterminate = false;
        if let (Some(w), Some(r)) = (w_elem, r_elem) {
            match check_member_type_compatibility(w, r, "sequence_element", ctx) {
                TypeCompatibility::Compatible => {}
                TypeCompatibility::Incompatible(e) => return TypeCompatibility::Incompatible(e),
                TypeCompatibility::Indeterminate => indeterminate = true,
            }
        }

        if !tce_policy.ignore_sequence_bounds {
            let w_bound = get_sequence_bound(writer_type);
            let r_bound = get_sequence_bound(reader_type);

            if w_bound > 0 && r_bound > 0 && w_bound > r_bound {
                return TypeCompatibility::Incompatible(
                    TypeCompatibilityError::SequenceBoundExceeded {
                        member_name: "sequence".to_string(),
                        writer_bound: w_bound,
                        reader_bound: r_bound,
                    },
                );
            }
        }

        return if indeterminate {
            TypeCompatibility::Indeterminate
        } else {
            TypeCompatibility::Compatible
        };
    }

    if is_array_type(writer_type) && is_array_type(reader_type) {
        let w_info = get_array_info(writer_type);
        let r_info = get_array_info(reader_type);

        if let (Some((w_elem, w_dims)), Some((r_elem, r_dims))) = (w_info, r_info) {
            if w_dims != r_dims {
                return TypeCompatibility::Incompatible(TypeCompatibilityError::IncompatibleKind {
                    writer: format!("array[{} dims]", w_dims),
                    reader: format!("array[{} dims]", r_dims),
                });
            }

            return check_member_type_compatibility(w_elem, r_elem, "array_element", ctx);
        }

        return TypeCompatibility::Compatible;
    }

    if is_map_type(writer_type) && is_map_type(reader_type) {
        let w_info = get_map_info(writer_type);
        let r_info = get_map_info(reader_type);

        if let (Some((w_key, w_elem)), Some((r_key, r_elem))) = (w_info, r_info) {
            let mut indeterminate = false;
            match check_member_type_compatibility(w_key, r_key, "map_key", ctx) {
                TypeCompatibility::Compatible => {}
                TypeCompatibility::Incompatible(e) => return TypeCompatibility::Incompatible(e),
                TypeCompatibility::Indeterminate => indeterminate = true,
            }
            match check_member_type_compatibility(w_elem, r_elem, "map_element", ctx) {
                TypeCompatibility::Compatible => {}
                TypeCompatibility::Incompatible(e) => return TypeCompatibility::Incompatible(e),
                TypeCompatibility::Indeterminate => indeterminate = true,
            }
            return if indeterminate {
                TypeCompatibility::Indeterminate
            } else {
                TypeCompatibility::Compatible
            };
        }

        return TypeCompatibility::Compatible;
    }

    TypeCompatibility::Incompatible(TypeCompatibilityError::IncompatibleKind {
        writer: format!("{:?}", writer_type),
        reader: format!("{:?}", reader_type),
    })
}

/// Get sequence bound from TypeIdentifier (0 = unbounded).
fn get_sequence_bound(type_id: &TypeIdentifier) -> u32 {
    match type_id {
        TypeIdentifier::PlainSequenceSmall { bound, .. } => *bound as u32,
        TypeIdentifier::PlainSequenceLarge { bound, .. } => *bound,
        _ => 0,
    }
}

/// Check if type is a sequence type.
fn is_sequence_type(type_id: &TypeIdentifier) -> bool {
    matches!(
        type_id,
        TypeIdentifier::PlainSequenceSmall { .. } | TypeIdentifier::PlainSequenceLarge { .. }
    )
}

/// Get sequence element type.
fn get_sequence_element(type_id: &TypeIdentifier) -> Option<&TypeIdentifier> {
    match type_id {
        TypeIdentifier::PlainSequenceSmall { element_identifier, .. }
        | TypeIdentifier::PlainSequenceLarge { element_identifier, .. } => Some(element_identifier),
        _ => None,
    }
}

/// Check if type is an array type.
fn is_array_type(type_id: &TypeIdentifier) -> bool {
    matches!(
        type_id,
        TypeIdentifier::PlainArraySmall { .. } | TypeIdentifier::PlainArrayLarge { .. }
    )
}

/// Get array element type and dimension count.
fn get_array_info(type_id: &TypeIdentifier) -> Option<(&TypeIdentifier, usize)> {
    match type_id {
        TypeIdentifier::PlainArraySmall { element_identifier, array_bound_seq, .. } => {
            Some((element_identifier, array_bound_seq.len()))
        }
        TypeIdentifier::PlainArrayLarge { element_identifier, array_bound_seq, .. } => {
            Some((element_identifier, array_bound_seq.len()))
        }
        _ => None,
    }
}

/// Check if type is a map type.
fn is_map_type(type_id: &TypeIdentifier) -> bool {
    matches!(type_id, TypeIdentifier::PlainMapSmall { .. } | TypeIdentifier::PlainMapLarge { .. })
}

/// Get map key and element types.
fn get_map_info(type_id: &TypeIdentifier) -> Option<(&TypeIdentifier, &TypeIdentifier)> {
    match type_id {
        TypeIdentifier::PlainMapSmall { key_identifier, element_identifier, .. }
        | TypeIdentifier::PlainMapLarge { key_identifier, element_identifier, .. } => {
            Some((key_identifier, element_identifier))
        }
        _ => None,
    }
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
    fn test_primitive_widening_allowed() {
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
        .is_ok());
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
        // Create struct types for TypeObject
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
        writer_struct.add_member(MinimalStructMember::new(
            1,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "field2", // Extra field
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
        // Writer missing a required member that reader needs
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
            MemberFlag::default(), // Not optional
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
        // Writer.inner has an extra required member that reader.inner lacks under
        // prevent_type_widening -> deep comparison must reject, not pass.
        let w_inner = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[
            (0, "a", TypeIdentifier::Int32),
            (1, "b", TypeIdentifier::Int32),
        ])));
        let r_inner = TypeObject::Minimal(MinimalTypeObject::Struct(appendable_struct(&[(
            0,
            "a",
            TypeIdentifier::Int32,
        )])));
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
        // Reader requires a member the writer lacks -> structurally incompatible.
        let w_obj = complete_struct_obj("W", &[(0, "a")]);
        let r_obj = complete_struct_obj("R", &[(0, "a"), (1, "b")]);
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
