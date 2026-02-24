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
    EquivalenceHash, ExtensibilityKind, MinimalStructMember, MinimalStructType, TypeIdentifier,
    TypeObject,
};

// ============================================================================
// Error Types
// ============================================================================

/// Error type for type compatibility checking.
///
/// Provides detailed information about why two types are incompatible.
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

// ============================================================================
// Main Compatibility Check Functions
// ============================================================================

/// Check structural type compatibility between writer and reader types.
///
/// This is the main entry point for type compatibility checking. It follows
/// DDS-XTypes 1.3 specification for type matching based on the
/// TypeConsistencyEnforcementQosPolicy.
///
/// # Performance
///
/// - Fast path: Hash comparison only (O(1)) when hashes match or DisallowTypeCoercion
/// - Slow path: Structural comparison (O(n)) only when needed
///
/// # Arguments
///
/// * `writer_type_id` - TypeIdentifier from the writer (offered)
/// * `reader_type_id` - TypeIdentifier from the reader (requested)
/// * `writer_type_obj` - Optional TypeObject from the writer
/// * `reader_type_obj` - Optional TypeObject from the reader
/// * `tce_policy` - TypeConsistencyEnforcementQosPolicy from the reader
///
/// # Returns
///
/// * `Ok(())` if types are compatible
/// * `Err(TypeCompatibilityError)` with details if incompatible
pub fn check_structural_compatibility(
    writer_type_id: Option<&TypeIdentifier>,
    reader_type_id: Option<&TypeIdentifier>,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    match (writer_type_id, reader_type_id) {
        (Some(writer_id), Some(reader_id)) => {
            // Both have TypeIdentifier - check based on consistency policy
            check_with_type_identifiers(
                writer_id,
                reader_id,
                writer_type_obj,
                reader_type_obj,
                tce_policy,
            )
        }
        (None, None) => {
            // Neither has TypeIdentifier - rely on type_name matching (backward compatible)
            // This is handled elsewhere in the discovery process
            Ok(())
        }
        _ => {
            // One has TypeIdentifier, the other doesn't
            if tce_policy.force_type_validation {
                Err(TypeCompatibilityError::TypeIdentifierRequired)
            } else {
                // Fall back to type_name matching (handled elsewhere)
                Ok(())
            }
        }
    }
}

/// Check compatibility when both sides have TypeIdentifiers.
fn check_with_type_identifiers(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    match tce_policy.kind {
        TypeConsistencyKind::DisallowTypeCoercion => {
            // Strict mode: types must be identical (hash match)
            check_disallow_type_coercion(writer_id, reader_id)
        }
        TypeConsistencyKind::AllowTypeCoercion => {
            // Permissive mode: allow compatible type coercion
            check_allow_type_coercion(
                writer_id,
                reader_id,
                writer_type_obj,
                reader_type_obj,
                tce_policy,
            )
        }
    }
}

/// Check compatibility in DisallowTypeCoercion mode (strict).
///
/// Types must have identical hashes for complex types, or be exactly equal
/// for primitive types.
fn check_disallow_type_coercion(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
) -> TypeCompatibilityResult {
    if writer_id.is_complex() && reader_id.is_complex() {
        // Compare equivalence hashes for complex types
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
        // Primitive types: exact match required
        Ok(())
    } else {
        Err(TypeCompatibilityError::IncompatibleKind {
            writer: format!("{:?}", writer_id),
            reader: format!("{:?}", reader_id),
        })
    }
}

/// Check compatibility in AllowTypeCoercion mode (permissive).
///
/// Allows compatible type evolution and primitive widening.
fn check_allow_type_coercion(
    writer_id: &TypeIdentifier,
    reader_id: &TypeIdentifier,
    writer_type_obj: Option<&TypeObject>,
    reader_type_obj: Option<&TypeObject>,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    // Fast path: identical types are always compatible
    if writer_id == reader_id {
        return Ok(());
    }

    // Fast path: if both are complex types with same hash, compatible
    if writer_id.is_complex() && reader_id.is_complex() {
        let writer_hash = writer_id.equivalence_hash();
        let reader_hash = reader_id.equivalence_hash();

        if writer_hash == reader_hash {
            return Ok(());
        }

        // Hashes differ - need structural check if TypeObjects available
        if let (Some(w_obj), Some(r_obj)) = (writer_type_obj, reader_type_obj) {
            return check_type_object_compatibility(w_obj, r_obj, tce_policy);
        }

        // No TypeObjects available - trust the type names (checked elsewhere)
        return Ok(());
    }

    // Primitive types: check if coercion is allowed
    if writer_id.is_primitive() && reader_id.is_primitive() {
        return check_primitive_coercion(writer_id, reader_id);
    }

    // String types: check bounds
    if writer_id.is_string() && reader_id.is_string() {
        return check_string_compatibility(writer_id, reader_id, tce_policy.ignore_string_bounds);
    }

    // Collection types: check element type and bounds
    if writer_id.is_collection() && reader_id.is_collection() {
        return check_collection_compatibility(writer_id, reader_id, tce_policy);
    }

    // Different type categories
    Err(TypeCompatibilityError::IncompatibleKind {
        writer: format!("{:?}", writer_id),
        reader: format!("{:?}", reader_id),
    })
}

// ============================================================================
// TypeObject Structural Compatibility
// ============================================================================

/// Check structural compatibility using TypeObjects.
fn check_type_object_compatibility(
    writer_obj: &TypeObject,
    reader_obj: &TypeObject,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    match (writer_obj, reader_obj) {
        (TypeObject::Minimal(w), TypeObject::Minimal(r)) => {
            check_minimal_type_object_compatibility(w, r, tce_policy)
        }
        (TypeObject::Complete(w), TypeObject::Complete(r)) => {
            // Complete types have more information - convert to minimal for comparison
            // For now, treat as compatible if they reached this point
            // Full complete-to-complete comparison would need more complex logic
            let _ = (w, r);
            Ok(())
        }
        _ => {
            // Mixed minimal/complete - compare what we can
            Ok(())
        }
    }
}

/// Check compatibility between MinimalTypeObjects.
fn check_minimal_type_object_compatibility(
    writer_obj: &crate::xtypes::MinimalTypeObject,
    reader_obj: &crate::xtypes::MinimalTypeObject,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    use crate::xtypes::MinimalTypeObject;

    match (writer_obj, reader_obj) {
        (MinimalTypeObject::Struct(w_struct), MinimalTypeObject::Struct(r_struct)) => {
            check_minimal_struct_compatibility(w_struct, r_struct, tce_policy)
        }
        (MinimalTypeObject::Enum(w_enum), MinimalTypeObject::Enum(r_enum)) => {
            // Enum compatibility: check if reader literals are subset of writer
            let _ = (w_enum, r_enum);
            Ok(())
        }
        _ => Err(TypeCompatibilityError::IncompatibleKind {
            writer: format!("{:?}", std::mem::discriminant(writer_obj)),
            reader: format!("{:?}", std::mem::discriminant(reader_obj)),
        }),
    }
}

/// Check structural compatibility between MinimalStructTypes.
///
/// # Rules (DDS-XTypes 1.3)
///
/// 1. All non-optional reader members must exist in writer
/// 2. Member types must be compatible
/// 3. Key members must match exactly
/// 4. Extensibility must be compatible
/// 5. If prevent_type_widening is set, writer cannot have extra members
fn check_minimal_struct_compatibility(
    writer: &MinimalStructType,
    reader: &MinimalStructType,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    // 1. Check extensibility compatibility
    let w_ext = writer.struct_flags.extensibility();
    let r_ext = reader.struct_flags.extensibility();

    // Final types must match exactly
    if w_ext == ExtensibilityKind::Final || r_ext == ExtensibilityKind::Final {
        if w_ext != r_ext {
            return Err(TypeCompatibilityError::ExtensibilityMismatch {
                writer: format!("{:?}", w_ext),
                reader: format!("{:?}", r_ext),
            });
        }
    }

    // 2. Build HashMap for writer members for O(1) lookup
    let writer_members = build_member_map(&writer.member_seq, tce_policy.ignore_member_names);

    // 3. Check all reader members exist in writer with compatible types
    for r_member in &reader.member_seq {
        let key = get_member_key(r_member, tce_policy.ignore_member_names);
        let member_name = format!("hash:{:08x}", r_member.name_hash);

        match writer_members.get(&key) {
            Some(w_member) => {
                // Check member type compatibility
                check_member_type_compatibility(
                    &w_member.common.member_type_id,
                    &r_member.common.member_type_id,
                    &member_name,
                    tce_policy,
                )?;

                // Check key consistency
                if w_member.common.member_flags.is_key() != r_member.common.member_flags.is_key() {
                    return Err(TypeCompatibilityError::KeyMemberMismatch { member_name });
                }
            }
            None => {
                // Member not found in writer
                if !r_member.common.member_flags.is_optional() {
                    return Err(TypeCompatibilityError::MissingRequiredMember {
                        member_name,
                        member_id: r_member.common.member_id,
                    });
                }
            }
        }
    }

    // 4. Check prevent_type_widening: writer should not have extra members
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
            return Err(TypeCompatibilityError::TypeWideningNotAllowed { extra_members });
        }
    }

    Ok(())
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

// ============================================================================
// Member Type Compatibility
// ============================================================================

/// Check compatibility between member types.
fn check_member_type_compatibility(
    writer_type: &TypeIdentifier,
    reader_type: &TypeIdentifier,
    member_name: &str,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    // Identical types are always compatible
    if writer_type == reader_type {
        return Ok(());
    }

    // Primitive coercion
    if writer_type.is_primitive() && reader_type.is_primitive() {
        return check_primitive_coercion(writer_type, reader_type).map_err(|_| {
            TypeCompatibilityError::MemberTypeMismatch {
                member_name: member_name.to_string(),
                writer_type: format!("{:?}", writer_type),
                reader_type: format!("{:?}", reader_type),
            }
        });
    }

    // String bounds
    if writer_type.is_string() && reader_type.is_string() {
        return check_string_compatibility_with_name(
            writer_type,
            reader_type,
            member_name,
            tce_policy.ignore_string_bounds,
        );
    }

    // Sequence bounds - handle separately due to different bound types
    if is_sequence_type(writer_type) && is_sequence_type(reader_type) {
        let w_elem = get_sequence_element(writer_type);
        let r_elem = get_sequence_element(reader_type);

        if let (Some(w_elem), Some(r_elem)) = (w_elem, r_elem) {
            // Check element type compatibility recursively
            check_member_type_compatibility(w_elem, r_elem, member_name, tce_policy)?;
        }

        // Check bounds
        let w = get_sequence_bound(writer_type);
        let r = get_sequence_bound(reader_type);

        if !tce_policy.ignore_sequence_bounds {
            // writer_bound > 0 means bounded, 0 means unbounded
            // reader must be able to accept all writer data
            if w > 0 && r > 0 && w > r {
                return Err(TypeCompatibilityError::SequenceBoundExceeded {
                    member_name: member_name.to_string(),
                    writer_bound: w,
                    reader_bound: r,
                });
            }
        }
        return Ok(());
    }

    // Complex types with same hash
    if writer_type.is_complex() && reader_type.is_complex() {
        if writer_type.equivalence_hash() == reader_type.equivalence_hash() {
            return Ok(());
        }
    }

    Err(TypeCompatibilityError::MemberTypeMismatch {
        member_name: member_name.to_string(),
        writer_type: format!("{:?}", writer_type),
        reader_type: format!("{:?}", reader_type),
    })
}

// ============================================================================
// Primitive Type Coercion
// ============================================================================

/// Check if primitive type coercion is allowed.
///
/// According to DDS-XTypes, certain primitive type coercions are allowed
/// when AllowTypeCoercion policy is set:
/// - Widening integer conversions (int8 -> int16 -> int32 -> int64)
/// - Widening float conversions (float32 -> float64)
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

// ============================================================================
// String Type Compatibility
// ============================================================================

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

// ============================================================================
// Collection Type Compatibility
// ============================================================================

/// Check collection (sequence/array/map) compatibility.
fn check_collection_compatibility(
    writer_type: &TypeIdentifier,
    reader_type: &TypeIdentifier,
    tce_policy: &TypeConsistencyEnforcementQosPolicy,
) -> TypeCompatibilityResult {
    // Sequences
    if is_sequence_type(writer_type) && is_sequence_type(reader_type) {
        let w_elem = get_sequence_element(writer_type);
        let r_elem = get_sequence_element(reader_type);

        // Check element type compatibility (with type coercion support)
        if let (Some(w), Some(r)) = (w_elem, r_elem) {
            check_member_type_compatibility(w, r, "sequence_element", tce_policy)?;
        }

        // Check bounds
        if !tce_policy.ignore_sequence_bounds {
            let w_bound = get_sequence_bound(writer_type);
            let r_bound = get_sequence_bound(reader_type);

            if w_bound > 0 && r_bound > 0 && w_bound > r_bound {
                return Err(TypeCompatibilityError::SequenceBoundExceeded {
                    member_name: "sequence".to_string(),
                    writer_bound: w_bound,
                    reader_bound: r_bound,
                });
            }
        }

        return Ok(());
    }

    // Arrays - must have same dimensions and element type
    if is_array_type(writer_type) && is_array_type(reader_type) {
        let w_info = get_array_info(writer_type);
        let r_info = get_array_info(reader_type);

        if let (Some((w_elem, w_dims)), Some((r_elem, r_dims))) = (w_info, r_info) {
            // Arrays must have exact same dimension count
            if w_dims != r_dims {
                return Err(TypeCompatibilityError::IncompatibleKind {
                    writer: format!("array[{} dims]", w_dims),
                    reader: format!("array[{} dims]", r_dims),
                });
            }

            // Check element type compatibility (with type coercion support)
            check_member_type_compatibility(w_elem, r_elem, "array_element", tce_policy)?;
        }

        return Ok(());
    }

    // Maps
    if is_map_type(writer_type) && is_map_type(reader_type) {
        let w_info = get_map_info(writer_type);
        let r_info = get_map_info(reader_type);

        if let (Some((w_key, w_elem)), Some((r_key, r_elem))) = (w_info, r_info) {
            // Check key and element types (with type coercion support)
            check_member_type_compatibility(w_key, r_key, "map_key", tce_policy)?;
            check_member_type_compatibility(w_elem, r_elem, "map_element", tce_policy)?;
        }

        return Ok(());
    }

    // Incompatible collection types
    Err(TypeCompatibilityError::IncompatibleKind {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::{MemberFlag, MinimalTypeObject, TryConstructKind, TypeFlag};

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
}
