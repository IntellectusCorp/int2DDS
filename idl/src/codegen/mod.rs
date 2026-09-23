pub mod c;
pub mod csharp;
pub mod java;
pub mod python;
pub mod rpc;
pub mod rust;
pub mod xml;

use crate::naming;
use crate::types::{
    ConstValue, ExtensibilityKind, IdlModel, ResolvedMember, ResolvedType, ResolvedUnion,
    ResolvedUnionCaseMember, ResolvedUnionLabel,
};

/// The `INT2DDS_MEMBER_DEFAULT` flag marking a union's `default:` member.
pub(crate) const MEMBER_DEFAULT: i32 = 1 << 4;

/// A union case member as the `ResolvedMember` the type_info field emitters consume:
/// PascalCase name like the generated Rust variant, no member annotations.
pub(crate) fn union_case_member(cm: &ResolvedUnionCaseMember) -> ResolvedMember {
    ResolvedMember {
        name: naming::to_pascal_case(&cm.name),
        resolved_type: cm.resolved_type.clone(),
        is_key: false,
        member_id: None,
        is_optional: false,
        must_understand: false,
        is_external: false,
        default_value: None,
        hashid: None,
    }
}

/// The implicit discriminator value of a union's `default:` member (spec §7.14.2): for a
/// signed discriminator (signed integers and enums) the negative value closest to zero
/// that no case label uses, otherwise the non-negative value closest to zero that no case
/// label uses. Shared by the Rust generator (variant discriminant) and the type_info
/// generators (advertised label).
pub(crate) fn union_default_label(u: &ResolvedUnion) -> i64 {
    let signed = matches!(
        u.discriminant_type,
        ResolvedType::I8
            | ResolvedType::I16
            | ResolvedType::I32
            | ResolvedType::I64
            | ResolvedType::Enum(_)
    );
    let used: std::collections::HashSet<i64> = u
        .cases
        .iter()
        .flat_map(|c| &c.labels)
        .filter_map(|l| match l {
            ResolvedUnionLabel::Int(v) => Some(*v),
            ResolvedUnionLabel::Bool(b) => Some(*b as i64),
            ResolvedUnionLabel::Ident(_) => None,
        })
        .collect();
    let (mut candidate, step) = if signed { (-1i64, -1i64) } else { (0i64, 1i64) };
    while used.contains(&candidate) {
        candidate += step;
    }
    candidate
}

/// The numeric value a case label advertises in the TypeObject: booleans as `1`/`0`,
/// identifiers resolved through the enum literals and integer constants in scope.
/// `None` when an identifier cannot be resolved.
pub(crate) fn union_label_value(model: &IdlModel, label: &ResolvedUnionLabel) -> Option<i64> {
    match label {
        ResolvedUnionLabel::Int(v) => Some(*v),
        ResolvedUnionLabel::Bool(v) => Some(i64::from(*v)),
        ResolvedUnionLabel::Ident(name) => {
            let simple = name.rsplit("::").next().unwrap_or(name);
            let literal = model
                .enums
                .iter()
                .chain(model.imported.enums.iter())
                .flat_map(|e| e.variants.iter())
                .find(|v| v.name == simple)
                .map(|v| i64::from(v.value));
            literal.or_else(|| {
                model.constants.iter().find(|c| c.name == simple).and_then(|c| match c.value {
                    ConstValue::Int(v) => Some(v),
                    ConstValue::Bool(b) => Some(i64::from(b)),
                    _ => None,
                })
            })
        }
    }
}

/// The scalar kind a union discriminator is advertised as: byte-width discriminators
/// (boolean/char/octet/uint8) as `octet`, matching the `#[repr(u8)]` the Rust generator
/// emits for them, and enums as their `i32` storage.
pub(crate) fn discriminator_kind(ty: &ResolvedType) -> ResolvedType {
    match ty {
        ResolvedType::Bool | ResolvedType::Char | ResolvedType::U8 | ResolvedType::UInt8 => {
            ResolvedType::U8
        }
        ResolvedType::I8 => ResolvedType::I8,
        ResolvedType::I16 => ResolvedType::I16,
        ResolvedType::U16 => ResolvedType::U16,
        ResolvedType::U32 => ResolvedType::U32,
        ResolvedType::I64 => ResolvedType::I64,
        ResolvedType::U64 => ResolvedType::U64,
        _ => ResolvedType::I32,
    }
}

/// The unsigned kind that holds a `bitfield<bit_width>` in the generated Rust bitset
/// (`u8` up to 8 bits, then `u16`/`u32`/`u64`), which the derive advertises.
pub(crate) fn bitfield_holder_kind(bit_width: u32) -> ResolvedType {
    match bit_width {
        0..=8 => ResolvedType::U8,
        9..=16 => ResolvedType::U16,
        17..=32 => ResolvedType::U32,
        _ => ResolvedType::U64,
    }
}

/// The `INT2DDS_FIELD_*` code of a scalar, string or char type; `None` for composites.
pub(crate) fn field_type_code(ty: &ResolvedType) -> Option<i32> {
    Some(match ty {
        ResolvedType::Bool => 0,
        ResolvedType::U8 => 1,
        ResolvedType::Char => 2,
        ResolvedType::I8 => 3,
        ResolvedType::I16 => 4,
        ResolvedType::I32 => 5,
        ResolvedType::I64 => 6,
        ResolvedType::UInt8 => 7,
        ResolvedType::U16 => 8,
        ResolvedType::U32 => 9,
        ResolvedType::U64 => 10,
        ResolvedType::F32 => 11,
        ResolvedType::F64 => 12,
        ResolvedType::String { .. } => 13,
        ResolvedType::WChar => 14,
        ResolvedType::WString { .. } => 15,
        _ => return None,
    })
}

/// The string bound carried by a scalar map key/value (`0` when unbounded or not a
/// string), as the `int2dds_type_info_add_map*` builders expect.
pub(crate) fn scalar_string_bound(ty: &ResolvedType) -> u32 {
    match ty {
        ResolvedType::String { bound } | ResolvedType::WString { bound } => bound.unwrap_or(0),
        _ => 0,
    }
}

/// The integer code `int2dds_type_info_create*` takes for an extensibility kind.
pub(crate) fn extensibility_code(kind: ExtensibilityKind) -> i32 {
    match kind {
        ExtensibilityKind::Final => 0,
        ExtensibilityKind::Appendable => 1,
        ExtensibilityKind::Mutable => 2,
    }
}
