use std::collections::HashSet;

use crate::dcps::core::error::{DdsError, DdsResult};
use std::collections::HashMap;

use crate::xtypes::{
    recompute_collection_kind, AppliedBuiltinMemberAnnotations, AppliedBuiltinTypeAnnotations,
    CollectionElementFlag, CompleteBitfield, CompleteBitflag, CompleteBitmaskType,
    CompleteBitsetType, CompleteEnumeratedLiteral, CompleteEnumeratedType, CompleteStructMember,
    CompleteStructType, CompleteTypeObject, CompleteUnionMember, CompleteUnionType,
    EnumeratedLiteralFlag, EquivalenceHash, MemberFlag, PlainCollectionHeader, TryConstructKind,
    TypeFlag, TypeIdentifier,
};

use super::ast::{
    XmlBitmask, XmlBitset, XmlCaseLabel, XmlEnum, XmlMember, XmlMemberType, XmlStruct, XmlTypeDecl,
    XmlUnion,
};

pub(crate) fn to_complete_type_object(decl: &XmlTypeDecl) -> DdsResult<CompleteTypeObject> {
    match decl {
        XmlTypeDecl::Struct(s) => struct_to_type_object(s),
        XmlTypeDecl::Enum(e) => enum_to_type_object(e),
        XmlTypeDecl::Union(u) => union_to_type_object(u),
        XmlTypeDecl::Bitmask(b) => bitmask_to_type_object(b),
        XmlTypeDecl::Bitset(b) => bitset_to_type_object(b),
        XmlTypeDecl::Typedef(t) => Err(DdsError::Error(format!(
            "XML types: typedef '{}' has no TypeObject; it is flattened at use sites",
            t.name
        ))),
    }
}

fn bitmask_to_type_object(b: &XmlBitmask) -> DdsResult<CompleteTypeObject> {
    let mut bitmask = CompleteBitmaskType::new(TypeFlag::default(), b.name.clone(), b.bit_bound);
    let mut seen_names = HashSet::new();
    let mut seen_positions = HashSet::new();
    for flag in &b.flags {
        if flag.position >= b.bit_bound {
            return Err(DdsError::Error(format!(
                "XML types: bitmask '{}': flag '{}' position {} exceeds bit_bound {}",
                b.name, flag.name, flag.position, b.bit_bound
            )));
        }
        if !seen_names.insert(flag.name.as_str()) {
            return Err(DdsError::Error(format!(
                "XML types: bitmask '{}': duplicate flag '{}'",
                b.name, flag.name
            )));
        }
        if !seen_positions.insert(flag.position) {
            return Err(DdsError::Error(format!(
                "XML types: bitmask '{}': duplicate position {}",
                b.name, flag.position
            )));
        }
        bitmask.add_flag(CompleteBitflag::new(
            flag.position,
            MemberFlag::default(),
            flag.name.clone(),
        ));
    }
    Ok(CompleteTypeObject::Bitmask(bitmask))
}

fn bitset_to_type_object(b: &XmlBitset) -> DdsResult<CompleteTypeObject> {
    let mut bitset = CompleteBitsetType::new(TypeFlag::default(), b.name.clone());
    let mut position: u16 = 0;
    let mut seen_names = HashSet::new();
    for field in &b.fields {
        let pos = position;
        position = position
            .checked_add(u16::from(field.bitcount))
            .filter(|p| *p <= 64)
            .ok_or_else(|| {
                DdsError::Error(format!(
                    "XML types: bitset '{}': total bit count exceeds 64",
                    b.name
                ))
            })?;
        // Anonymous bitfields advance the position but are not emitted (padding).
        if let Some(name) = &field.name {
            if !seen_names.insert(name.as_str()) {
                return Err(DdsError::Error(format!(
                    "XML types: bitset '{}': duplicate bitfield '{}'",
                    b.name, name
                )));
            }
            bitset.add_field(CompleteBitfield::new(
                pos,
                MemberFlag::default(),
                field.bitcount,
                field.holder.clone(),
                name.clone(),
            ));
        }
    }
    Ok(CompleteTypeObject::Bitset(bitset))
}

// Resolves each enumerator to its value (explicit or previous+1), with the same
// duplicate checks as enum_to_type_object. Shared with union label resolution.
pub(crate) fn enum_literal_values(e: &XmlEnum) -> DdsResult<Vec<i32>> {
    let mut values = Vec::with_capacity(e.literals.len());
    let mut seen_names = HashSet::new();
    let mut seen_values = HashSet::new();
    let mut next_implicit: i64 = 0;
    for lit in &e.literals {
        if !seen_names.insert(lit.name.as_str()) {
            return Err(DdsError::Error(format!(
                "XML types: enum '{}': duplicate enumerator '{}'",
                e.name, lit.name
            )));
        }
        let value = match lit.value {
            Some(v) => v,
            None => i32::try_from(next_implicit).map_err(|_| {
                DdsError::Error(format!(
                    "XML types: enum '{}': implicit value overflows i32 (enumerator '{}')",
                    e.name, lit.name
                ))
            })?,
        };
        if !seen_values.insert(value) {
            return Err(DdsError::Error(format!(
                "XML types: enum '{}': duplicate value {value} (enumerator '{}')",
                e.name, lit.name
            )));
        }
        next_implicit = i64::from(value) + 1;
        values.push(value);
    }
    Ok(values)
}

// Same name-based id convention as derive's collect_nested_type_objects. Used as a
// pre-rewrite placeholder: `resolve_content_ids` later swaps these for content ids.
pub(crate) fn name_based_type_id(name: &str) -> TypeIdentifier {
    TypeIdentifier::MinimalTypeId(EquivalenceHash::compute(name.as_bytes()))
}

/// Every name-based (`MinimalTypeId`) hash a complete object references — struct
/// members and base type, union discriminator and members, alias body — recursing
/// into plain collection element/key ids.
pub(crate) fn referenced_name_hashes(obj: &CompleteTypeObject) -> Vec<EquivalenceHash> {
    let mut out = Vec::new();
    let mut push = |id: &TypeIdentifier| collect_id_hashes(id, &mut out);
    match obj {
        CompleteTypeObject::Struct(s) => {
            if let Some(b) = &s.header.base_type {
                push(b);
            }
            for m in &s.member_seq {
                push(&m.common.member_type_id);
            }
        }
        CompleteTypeObject::Union(u) => {
            push(&u.discriminator.type_id);
            for m in &u.member_seq {
                push(&m.common.member_type_id);
            }
        }
        CompleteTypeObject::Alias(a) => push(&a.body.related_type),
        CompleteTypeObject::Enum(_)
        | CompleteTypeObject::Bitmask(_)
        | CompleteTypeObject::Bitset(_) => {}
    }
    out
}

fn collect_id_hashes(id: &TypeIdentifier, out: &mut Vec<EquivalenceHash>) {
    if let Some(h) = id.equivalence_hash() {
        out.push(*h);
        return;
    }
    match id {
        TypeIdentifier::PlainSequenceSmall { element_identifier, .. }
        | TypeIdentifier::PlainSequenceLarge { element_identifier, .. }
        | TypeIdentifier::PlainArraySmall { element_identifier, .. }
        | TypeIdentifier::PlainArrayLarge { element_identifier, .. } => {
            collect_id_hashes(element_identifier, out)
        }
        TypeIdentifier::PlainMapSmall { key_identifier, element_identifier, .. }
        | TypeIdentifier::PlainMapLarge { key_identifier, element_identifier, .. } => {
            collect_id_hashes(key_identifier, out);
            collect_id_hashes(element_identifier, out);
        }
        _ => {}
    }
}

/// Rewrite the placeholder name-based ids of `obj` to content ids via `resolver`
/// (name-hash -> content `TypeIdentifier`), and recompute every plain-collection
/// header's `equiv_kind` from its rewritten element/key. Ids absent from `resolver`
/// (an unbroken cycle) are left as name-based `MinimalTypeId` placeholders.
pub(crate) fn resolve_content_ids(
    obj: &mut CompleteTypeObject,
    resolver: &HashMap<EquivalenceHash, TypeIdentifier>,
) {
    match obj {
        CompleteTypeObject::Struct(s) => {
            if let Some(b) = &mut s.header.base_type {
                *b = rewrite_id(b, resolver);
            }
            for m in &mut s.member_seq {
                m.common.member_type_id = rewrite_id(&m.common.member_type_id, resolver);
            }
        }
        CompleteTypeObject::Union(u) => {
            u.discriminator.type_id = rewrite_id(&u.discriminator.type_id, resolver);
            for m in &mut u.member_seq {
                m.common.member_type_id = rewrite_id(&m.common.member_type_id, resolver);
            }
        }
        CompleteTypeObject::Alias(a) => {
            a.body.related_type = rewrite_id(&a.body.related_type, resolver);
        }
        CompleteTypeObject::Enum(_)
        | CompleteTypeObject::Bitmask(_)
        | CompleteTypeObject::Bitset(_) => {}
    }
}

fn rewrite_id(
    id: &TypeIdentifier,
    resolver: &HashMap<EquivalenceHash, TypeIdentifier>,
) -> TypeIdentifier {
    if let Some(h) = id.equivalence_hash() {
        return resolver.get(h).cloned().unwrap_or_else(|| id.clone());
    }
    let rebuilt = match id {
        TypeIdentifier::PlainSequenceSmall { header, bound, element_identifier } => {
            TypeIdentifier::PlainSequenceSmall {
                header: *header,
                bound: *bound,
                element_identifier: Box::new(rewrite_id(element_identifier, resolver)),
            }
        }
        TypeIdentifier::PlainSequenceLarge { header, bound, element_identifier } => {
            TypeIdentifier::PlainSequenceLarge {
                header: *header,
                bound: *bound,
                element_identifier: Box::new(rewrite_id(element_identifier, resolver)),
            }
        }
        TypeIdentifier::PlainArraySmall { header, array_bound_seq, element_identifier } => {
            TypeIdentifier::PlainArraySmall {
                header: *header,
                array_bound_seq: array_bound_seq.clone(),
                element_identifier: Box::new(rewrite_id(element_identifier, resolver)),
            }
        }
        TypeIdentifier::PlainArrayLarge { header, array_bound_seq, element_identifier } => {
            TypeIdentifier::PlainArrayLarge {
                header: *header,
                array_bound_seq: array_bound_seq.clone(),
                element_identifier: Box::new(rewrite_id(element_identifier, resolver)),
            }
        }
        TypeIdentifier::PlainMapSmall {
            header,
            bound,
            key_flags,
            key_identifier,
            element_identifier,
        } => TypeIdentifier::PlainMapSmall {
            header: *header,
            bound: *bound,
            key_flags: *key_flags,
            key_identifier: Box::new(rewrite_id(key_identifier, resolver)),
            element_identifier: Box::new(rewrite_id(element_identifier, resolver)),
        },
        TypeIdentifier::PlainMapLarge {
            header,
            bound,
            key_flags,
            key_identifier,
            element_identifier,
        } => TypeIdentifier::PlainMapLarge {
            header: *header,
            bound: *bound,
            key_flags: *key_flags,
            key_identifier: Box::new(rewrite_id(key_identifier, resolver)),
            element_identifier: Box::new(rewrite_id(element_identifier, resolver)),
        },
        _ => return id.clone(),
    };
    recompute_collection_kind(rebuilt)
}

fn enum_to_type_object(e: &XmlEnum) -> DdsResult<CompleteTypeObject> {
    let mut enum_type =
        CompleteEnumeratedType::new(TypeFlag::default(), e.name.clone(), e.bit_bound);
    let values = enum_literal_values(e)?;
    for (lit, value) in e.literals.iter().zip(values) {
        let flags = if lit.is_default {
            EnumeratedLiteralFlag::DEFAULT
        } else {
            EnumeratedLiteralFlag::default()
        };
        enum_type.add_literal(CompleteEnumeratedLiteral::new(value, flags, lit.name.clone()));
    }
    Ok(CompleteTypeObject::Enum(enum_type))
}

fn union_to_type_object(u: &XmlUnion) -> DdsResult<CompleteTypeObject> {
    let mut union_type = CompleteUnionType::new(
        TypeFlag::new(u.extensibility, false, false),
        MemberFlag::default(),
        member_type_id(&u.discriminator),
        u.name.clone(),
    );

    let mut seen_names = HashSet::new();
    let mut seen_labels = HashSet::new();
    for (index, case) in u.cases.iter().enumerate() {
        if !seen_names.insert(case.name.as_str()) {
            return Err(DdsError::Error(format!(
                "XML types: union '{}': duplicate case member name '{}'",
                u.name, case.name
            )));
        }
        let mut labels = Vec::with_capacity(case.labels.len());
        for label in &case.labels {
            let value = match label {
                XmlCaseLabel::Int(v) => *v,
                XmlCaseLabel::Name(n) => {
                    return Err(DdsError::Error(format!(
                        "XML types: union '{}': unresolved case label '{n}'",
                        u.name
                    )))
                }
            };
            if !seen_labels.insert(value) {
                return Err(DdsError::Error(format!(
                    "XML types: union '{}': duplicate case label {value}",
                    u.name
                )));
            }
            labels.push(value);
        }
        let member_id = u32::try_from(index).map_err(|_| {
            DdsError::Error(format!("XML types: union '{}': case index overflows u32", u.name))
        })?;
        let flags =
            MemberFlag::new(TryConstructKind::Discard, false, false, false, false, case.is_default);
        union_type.add_member(CompleteUnionMember::new(
            member_id,
            flags,
            member_type_id(&case.ty),
            labels,
            case.name.clone(),
        ));
    }

    Ok(CompleteTypeObject::Union(union_type))
}

fn struct_to_type_object(s: &XmlStruct) -> DdsResult<CompleteTypeObject> {
    let base_type = s.base_type.as_deref().map(name_based_type_id);
    let mut struct_type = CompleteStructType::new(
        TypeFlag::new(s.extensibility, s.nested, s.autoid_hash),
        s.name.clone(),
        base_type,
    );
    if s.nested || s.data_representation.is_some() {
        struct_type.header.detail.ann_builtin = Some(AppliedBuiltinTypeAnnotations {
            verbatim: None,
            nested: if s.nested { Some(true) } else { None },
            data_representation: s.data_representation,
        });
    }

    let mut seen_names = HashSet::new();
    let mut seen_ids = HashSet::new();
    for (index, m) in s.members.iter().enumerate() {
        if !seen_names.insert(m.name.as_str()) {
            return Err(DdsError::Error(format!(
                "XML types: struct '{}': duplicate member name '{}'",
                s.name, m.name
            )));
        }
        let member_id = resolve_member_id(m, index, s.autoid_hash)?;
        if !seen_ids.insert(member_id) {
            return Err(DdsError::Error(format!(
                "XML types: struct '{}': duplicate member id {member_id} (member '{}')",
                s.name, m.name
            )));
        }

        let mut member = CompleteStructMember::new(
            member_id,
            MemberFlag::new(
                m.try_construct,
                m.external,
                m.optional,
                // XTypes 7.2.2.4.4.4.7: key members are implicitly must-understand.
                m.must_understand || m.key,
                m.key,
                false,
            ),
            member_type_id(&m.ty),
            m.name.clone(),
        );
        if let Some(h) = &m.hashid {
            let hash_name = if h.is_empty() { m.name.clone() } else { h.clone() };
            member.detail.ann_builtin = Some(AppliedBuiltinMemberAnnotations {
                unit: None,
                min: None,
                max: None,
                hash_id: Some(hash_name),
            });
        }
        struct_type.add_member(member);
    }

    Ok(CompleteTypeObject::Struct(struct_type))
}

// SMALL/LARGE split at bound 255; unbounded (bound 0) collections use the SMALL form,
// matching the derive output (XTypes 7.3.4.5).
fn member_type_id(ty: &XmlMemberType) -> TypeIdentifier {
    match ty {
        XmlMemberType::Primitive(id) => id.clone(),
        XmlMemberType::String { bound: None } => TypeIdentifier::String8,
        XmlMemberType::String { bound: Some(b) } => match u8::try_from(*b) {
            Ok(bound) => TypeIdentifier::String8Small { bound },
            Err(_) => TypeIdentifier::String8Large { bound: *b },
        },
        XmlMemberType::WString { bound: None } => TypeIdentifier::String16,
        XmlMemberType::WString { bound: Some(b) } => match u8::try_from(*b) {
            Ok(bound) => TypeIdentifier::String16Small { bound },
            Err(_) => TypeIdentifier::String16Large { bound: *b },
        },
        XmlMemberType::Sequence { element, bound } => {
            let element_identifier = Box::new(member_type_id(element));
            match bound {
                None => TypeIdentifier::PlainSequenceSmall {
                    header: PlainCollectionHeader::default(),
                    bound: 0,
                    element_identifier,
                },
                Some(b) => match u8::try_from(*b) {
                    Ok(bound) => TypeIdentifier::PlainSequenceSmall {
                        header: PlainCollectionHeader::default(),
                        bound,
                        element_identifier,
                    },
                    Err(_) => TypeIdentifier::PlainSequenceLarge {
                        header: PlainCollectionHeader::default(),
                        bound: *b,
                        element_identifier,
                    },
                },
            }
        }
        XmlMemberType::Array { element, dims } => {
            let element_identifier = Box::new(member_type_id(element));
            let header = PlainCollectionHeader::default();
            if dims.iter().all(|d| *d <= 255) {
                TypeIdentifier::PlainArraySmall {
                    header,
                    array_bound_seq: dims.iter().map(|d| *d as u8).collect(),
                    element_identifier,
                }
            } else {
                TypeIdentifier::PlainArrayLarge {
                    header,
                    array_bound_seq: dims.clone(),
                    element_identifier,
                }
            }
        }
        XmlMemberType::Map { key, value, bound } => {
            let key_identifier = Box::new(member_type_id(key));
            let element_identifier = Box::new(member_type_id(value));
            match bound {
                Some(b) => match u8::try_from(*b) {
                    Ok(bound) => TypeIdentifier::PlainMapSmall {
                        header: PlainCollectionHeader::default(),
                        bound,
                        key_flags: CollectionElementFlag::default(),
                        key_identifier,
                        element_identifier,
                    },
                    Err(_) => TypeIdentifier::PlainMapLarge {
                        header: PlainCollectionHeader::default(),
                        bound: *b,
                        key_flags: CollectionElementFlag::default(),
                        key_identifier,
                        element_identifier,
                    },
                },
                None => TypeIdentifier::PlainMapLarge {
                    header: PlainCollectionHeader::default(),
                    bound: 0,
                    key_flags: CollectionElementFlag::default(),
                    key_identifier,
                    element_identifier,
                },
            }
        }
        XmlMemberType::NonBasic(name) => name_based_type_id(name),
    }
}

// Same algorithm as derive's compute_member_id_hash (derive/src/codegen/utils.rs).
fn member_id_hash(name: &str) -> u32 {
    let digest = md5::compute(name.as_bytes());
    u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) & 0x0FFF_FFFF
}

fn resolve_member_id(m: &XmlMember, index: usize, autoid_hash: bool) -> DdsResult<u32> {
    if let Some(id) = m.id {
        return Ok(id);
    }
    if let Some(h) = &m.hashid {
        let name = if h.is_empty() { m.name.as_str() } else { h.as_str() };
        return Ok(member_id_hash(name));
    }
    if autoid_hash {
        return Ok(member_id_hash(&m.name));
    }
    u32::try_from(index)
        .map_err(|_| DdsError::Error(format!("XML types: member index {index} overflows u32")))
}

#[cfg(test)]
mod tests {
    use super::super::parser::parse_types;
    use super::*;

    fn convert_one(xml: &str) -> CompleteTypeObject {
        let decls = parse_types(xml, false).unwrap();
        to_complete_type_object(&decls[0]).unwrap()
    }

    fn as_struct(obj: &CompleteTypeObject) -> &CompleteStructType {
        match obj {
            CompleteTypeObject::Struct(s) => s,
            other => panic!("expected struct, got {other:?}"),
        }
    }

    #[test]
    fn member_id_hash_parity_with_derive() {
        // Expected values: MD5 first 4 bytes little-endian, masked to 28 bits.
        for name in ["sensor_id", "crc", "x"] {
            let digest = md5::compute(name.as_bytes());
            let expected =
                u32::from_le_bytes([digest[0], digest[1], digest[2], digest[3]]) & 0x0FFF_FFFF;
            assert_eq!(member_id_hash(name), expected);
            assert!(member_id_hash(name) <= 0x0FFF_FFFF);
        }
    }

    #[test]
    fn id_resolution_priority() {
        let obj = convert_one(
            r#"<types><struct name="T" autoid="hash">
                 <member name="a" type="int32" id="7"/>
                 <member name="b" type="int32" hashid="crc"/>
                 <member name="c" type="int32" hashid=""/>
                 <member name="d" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        assert_eq!(s.member_seq[0].common.member_id, 7);
        assert_eq!(s.member_seq[1].common.member_id, member_id_hash("crc"));
        assert_eq!(s.member_seq[2].common.member_id, member_id_hash("c"));
        assert_eq!(s.member_seq[3].common.member_id, member_id_hash("d"));
    }

    #[test]
    fn sequential_default_ids() {
        let obj = convert_one(
            r#"<types><struct name="T">
                 <member name="a" type="int32"/>
                 <member name="b" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        assert_eq!(s.member_seq[0].common.member_id, 0);
        assert_eq!(s.member_seq[1].common.member_id, 1);
    }

    #[test]
    fn hashid_sets_ann_builtin() {
        let obj = convert_one(
            r#"<types><struct name="T">
                 <member name="a" type="int32" hashid="crc"/>
                 <member name="b" type="int32" hashid=""/>
                 <member name="c" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        let ann = s.member_seq[0].detail.ann_builtin.as_ref().unwrap();
        assert_eq!(ann.hash_id.as_deref(), Some("crc"));
        let ann = s.member_seq[1].detail.ann_builtin.as_ref().unwrap();
        assert_eq!(ann.hash_id.as_deref(), Some("b"));
        assert!(s.member_seq[2].detail.ann_builtin.is_none());
    }

    #[test]
    fn nested_sets_type_ann_builtin() {
        let obj = convert_one(
            r#"<types><struct name="T" nested="true">
                 <member name="a" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        assert_eq!(s.header.detail.ann_builtin.as_ref().unwrap().nested, Some(true));

        let obj = convert_one(
            r#"<types><struct name="T"><member name="a" type="int32"/></struct></types>"#,
        );
        assert!(as_struct(&obj).header.detail.ann_builtin.is_none());
    }

    #[test]
    fn string_and_collection_type_identifiers() {
        let obj = convert_one(
            r#"<types><struct name="T">
                 <member name="a" type="string"/>
                 <member name="b" type="string" stringMaxLength="255"/>
                 <member name="c" type="string" stringMaxLength="256"/>
                 <member name="d" type="wstring" stringMaxLength="16"/>
                 <member name="e" type="int32" sequenceMaxLength="-1"/>
                 <member name="f" type="int32" sequenceMaxLength="255"/>
                 <member name="g" type="int32" sequenceMaxLength="256"/>
                 <member name="h" type="float64" arrayDimensions="2,3"/>
                 <member name="i" type="string" sequenceMaxLength="-1"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        let id = |i: usize| &s.member_seq[i].common.member_type_id;
        assert_eq!(id(0), &TypeIdentifier::String8);
        assert_eq!(id(1), &TypeIdentifier::String8Small { bound: 255 });
        assert_eq!(id(2), &TypeIdentifier::String8Large { bound: 256 });
        assert_eq!(id(3), &TypeIdentifier::String16Small { bound: 16 });
        assert_eq!(
            id(4),
            &TypeIdentifier::PlainSequenceSmall {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(TypeIdentifier::Int32),
            }
        );
        assert_eq!(
            id(5),
            &TypeIdentifier::PlainSequenceSmall {
                header: PlainCollectionHeader::default(),
                bound: 255,
                element_identifier: Box::new(TypeIdentifier::Int32),
            }
        );
        assert_eq!(
            id(6),
            &TypeIdentifier::PlainSequenceLarge {
                header: PlainCollectionHeader::default(),
                bound: 256,
                element_identifier: Box::new(TypeIdentifier::Int32),
            }
        );
        assert_eq!(
            id(7),
            &TypeIdentifier::PlainArraySmall {
                header: PlainCollectionHeader::default(),
                array_bound_seq: vec![2, 3],
                element_identifier: Box::new(TypeIdentifier::Float64),
            }
        );
        assert_eq!(
            id(8),
            &TypeIdentifier::PlainSequenceSmall {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(TypeIdentifier::String8),
            }
        );
    }

    #[test]
    fn enum_values_and_flags() {
        let obj = convert_one(
            r#"<types><enum name="Color" bit_bound="16">
                 <enumerator name="RED"/>
                 <enumerator name="GREEN" value="5"/>
                 <enumerator name="BLUE" default_literal="true"/>
               </enum></types>"#,
        );
        let CompleteTypeObject::Enum(e) = &obj else { panic!("expected enum") };
        assert_eq!(e.header.common.bit_bound, 16);
        assert_eq!(e.header.detail.type_name, "Color");
        let values: Vec<i32> = e.literal_seq.iter().map(|l| l.common.value).collect();
        assert_eq!(values, [0, 5, 6]);
        assert!(!e.literal_seq[1].common.flags.is_default());
        assert!(e.literal_seq[2].common.flags.is_default());
    }

    #[test]
    fn enum_duplicates_rejected() {
        let dup_name = parse_types(
            r#"<types><enum name="E">
                 <enumerator name="A"/><enumerator name="A" value="1"/>
               </enum></types>"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            to_complete_type_object(&dup_name[0]),
            Err(DdsError::Error(msg)) if msg.contains("duplicate enumerator 'A'")
        ));

        let dup_value = parse_types(
            r#"<types><enum name="E">
                 <enumerator name="A" value="3"/><enumerator name="B" value="3"/>
               </enum></types>"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            to_complete_type_object(&dup_value[0]),
            Err(DdsError::Error(msg)) if msg.contains("duplicate value 3")
        ));
    }

    #[test]
    fn nonbasic_uses_name_based_id() {
        let obj = convert_one(
            r#"<types><struct name="T">
                 <member name="p" type="nonBasic" nonBasicTypeName="Point"/>
                 <member name="ps" type="nonBasic" nonBasicTypeName="Point" sequenceMaxLength="-1"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        assert_eq!(s.member_seq[0].common.member_type_id, name_based_type_id("Point"));
        assert_eq!(
            s.member_seq[1].common.member_type_id,
            TypeIdentifier::PlainSequenceSmall {
                header: PlainCollectionHeader::default(),
                bound: 0,
                element_identifier: Box::new(name_based_type_id("Point")),
            }
        );
        assert!(matches!(name_based_type_id("Point"), TypeIdentifier::MinimalTypeId(_)));
    }

    #[test]
    fn map_type_identifiers() {
        let obj = convert_one(
            r#"<types><struct name="T">
                 <member name="a" type="int32" key_type="string" mapMaxLength="16"/>
                 <member name="b" type="string" key_type="uint64" mapMaxLength="300"/>
                 <member name="c" type="nonBasic" nonBasicTypeName="P" key_type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        let id = |i: usize| &s.member_seq[i].common.member_type_id;
        assert_eq!(
            id(0),
            &TypeIdentifier::PlainMapSmall {
                header: PlainCollectionHeader::default(),
                bound: 16,
                key_flags: CollectionElementFlag::default(),
                key_identifier: Box::new(TypeIdentifier::String8),
                element_identifier: Box::new(TypeIdentifier::Int32),
            }
        );
        assert_eq!(
            id(1),
            &TypeIdentifier::PlainMapLarge {
                header: PlainCollectionHeader::default(),
                bound: 300,
                key_flags: CollectionElementFlag::default(),
                key_identifier: Box::new(TypeIdentifier::Uint64),
                element_identifier: Box::new(TypeIdentifier::String8),
            }
        );
        assert_eq!(
            id(2),
            &TypeIdentifier::PlainMapLarge {
                header: PlainCollectionHeader::default(),
                bound: 0,
                key_flags: CollectionElementFlag::default(),
                key_identifier: Box::new(TypeIdentifier::Int32),
                element_identifier: Box::new(name_based_type_id("P")),
            }
        );
    }

    #[test]
    fn array_of_sequences_type_identifier() {
        let obj = convert_one(
            r#"<types><struct name="T">
                 <member name="a" type="int32" sequenceMaxLength="8" arrayDimensions="2"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        assert_eq!(
            &s.member_seq[0].common.member_type_id,
            &TypeIdentifier::PlainArraySmall {
                header: PlainCollectionHeader::default(),
                array_bound_seq: vec![2],
                element_identifier: Box::new(TypeIdentifier::PlainSequenceSmall {
                    header: PlainCollectionHeader::default(),
                    bound: 8,
                    element_identifier: Box::new(TypeIdentifier::Int32),
                }),
            }
        );
    }

    fn as_union(obj: &CompleteTypeObject) -> &crate::xtypes::CompleteUnionType {
        match obj {
            CompleteTypeObject::Union(u) => u,
            other => panic!("expected union, got {other:?}"),
        }
    }

    #[test]
    fn union_members_and_labels() {
        let obj = convert_one(
            r#"<types><union name="U" extensibility="final">
                 <discriminator type="int16"/>
                 <case><caseDiscriminator value="0"/><caseDiscriminator value="2"/><member name="a" type="uint32"/></case>
                 <case><caseDiscriminator value="default"/><member name="b" type="float64"/></case>
               </union></types>"#,
        );
        let u = as_union(&obj);
        assert_eq!(u.discriminator.type_id, TypeIdentifier::Int16);
        assert_eq!(u.header.type_name, "U");
        assert_eq!(u.member_seq.len(), 2);
        assert_eq!(u.member_seq[0].common.member_id, 0);
        assert_eq!(u.member_seq[0].common.label_seq, vec![0, 2]);
        assert_eq!(u.member_seq[0].common.member_type_id, TypeIdentifier::Uint32);
        assert!(!u.member_seq[0].common.member_flags.is_default());
        assert_eq!(u.member_seq[1].common.member_id, 1);
        assert!(u.member_seq[1].common.label_seq.is_empty());
        assert!(u.member_seq[1].common.member_flags.is_default());
    }

    #[test]
    fn union_enum_discriminator_uses_name_hash() {
        let obj = convert_one(
            r#"<types><union name="U">
                 <discriminator type="nonBasic" nonBasicTypeName="Mode"/>
                 <case><caseDiscriminator value="3"/><member name="m" type="int32"/></case>
               </union></types>"#,
        );
        let u = as_union(&obj);
        assert_eq!(u.discriminator.type_id, name_based_type_id("Mode"));
        assert_eq!(u.member_seq[0].common.label_seq, vec![3]);
    }

    #[test]
    fn union_duplicate_label_rejected() {
        let decls = parse_types(
            r#"<types><union name="U"><discriminator type="int32"/>
                 <case><caseDiscriminator value="1"/><member name="a" type="int32"/></case>
                 <case><caseDiscriminator value="1"/><member name="b" type="int32"/></case>
               </union></types>"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            to_complete_type_object(&decls[0]),
            Err(DdsError::Error(msg)) if msg.contains("duplicate case label 1")
        ));
    }

    #[test]
    fn bitmask_flags_and_positions() {
        let obj = convert_one(
            r#"<types><bitmask name="Flags" bit_bound="16">
                 <bit_value name="A"/>
                 <bit_value name="B" position="4"/>
               </bitmask></types>"#,
        );
        let CompleteTypeObject::Bitmask(b) = &obj else { panic!("expected bitmask") };
        assert_eq!(b.header.common.bit_bound, 16);
        assert_eq!(b.header.detail.type_name, "Flags");
        assert_eq!(b.flag_seq[0].common.position, 0);
        assert_eq!(b.flag_seq[0].detail.name, "A");
        assert_eq!(b.flag_seq[1].common.position, 4);
    }

    #[test]
    fn bitset_padding_advances_position() {
        let obj = convert_one(
            r#"<types><bitset name="Hdr">
                 <bitfield name="version" bit_bound="4" type="uint8"/>
                 <bitfield bit_bound="4"/>
                 <bitfield name="flags" bit_bound="8" type="uint16"/>
               </bitset></types>"#,
        );
        let CompleteTypeObject::Bitset(b) = &obj else { panic!("expected bitset") };
        assert_eq!(b.field_seq.len(), 2);
        assert_eq!(b.field_seq[0].common.position, 0);
        assert_eq!(b.field_seq[0].common.bitcount, 4);
        assert_eq!(b.field_seq[0].common.holder_type, TypeIdentifier::Uint8);
        assert_eq!(b.field_seq[1].common.position, 8);
        assert_eq!(b.field_seq[1].detail.name, "flags");
        assert_eq!(b.field_seq[1].common.holder_type, TypeIdentifier::Uint16);
    }

    #[test]
    fn bitmask_position_past_bound_rejected() {
        let decls = parse_types(
            r#"<types><bitmask name="B" bit_bound="4"><bit_value name="A" position="5"/></bitmask></types>"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            to_complete_type_object(&decls[0]),
            Err(DdsError::Error(msg)) if msg.contains("position 5 exceeds bit_bound 4")
        ));
    }

    #[test]
    fn bitset_over_64_bits_rejected() {
        let decls = parse_types(
            r#"<types><bitset name="S">
                 <bitfield name="a" bit_bound="40" type="uint64"/>
                 <bitfield name="b" bit_bound="40" type="uint64"/>
               </bitset></types>"#,
            false,
        )
        .unwrap();
        assert!(matches!(
            to_complete_type_object(&decls[0]),
            Err(DdsError::Error(msg)) if msg.contains("total bit count exceeds 64")
        ));
    }

    #[test]
    fn data_representation_sets_type_ann_builtin() {
        let obj = convert_one(
            r#"<types><struct name="T" data_representation="xcdr2">
                 <member name="a" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        let ann = s.header.detail.ann_builtin.as_ref().unwrap();
        assert_eq!(ann.data_representation, Some(0b100));
        assert_eq!(ann.nested, None);

        let obj = convert_one(
            r#"<types><struct name="T" nested="true" data_representation="xcdr1|xcdr2">
                 <member name="a" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        let ann = s.header.detail.ann_builtin.as_ref().unwrap();
        assert_eq!(ann.data_representation, Some(0b101));
        assert_eq!(ann.nested, Some(true));
    }

    #[test]
    fn duplicate_members_rejected() {
        let decls = parse_types(
            r#"<types><struct name="T">
                 <member name="a" type="int32"/>
                 <member name="a" type="int32"/>
               </struct></types>"#,
            false,
        )
        .unwrap();
        let msg = match to_complete_type_object(&decls[0]) {
            Err(DdsError::Error(msg)) => msg,
            other => panic!("expected error, got {other:?}"),
        };
        assert!(msg.contains("duplicate member name 'a'"));

        let decls = parse_types(
            r#"<types><struct name="T">
                 <member name="a" type="int32" id="3"/>
                 <member name="b" type="int32" id="3"/>
               </struct></types>"#,
            false,
        )
        .unwrap();
        let msg = match to_complete_type_object(&decls[0]) {
            Err(DdsError::Error(msg)) => msg,
            other => panic!("expected error, got {other:?}"),
        };
        assert!(msg.contains("duplicate member id 3"));
    }

    #[test]
    fn base_type_sets_name_hash() {
        let obj = convert_one(
            r#"<types><struct name="T" baseType="P">
                 <member name="a" type="int32"/>
               </struct></types>"#,
        );
        let s = as_struct(&obj);
        assert_eq!(s.header.base_type, Some(name_based_type_id("P")));
    }
}
