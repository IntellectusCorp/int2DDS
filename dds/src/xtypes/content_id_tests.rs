// Derive-generated TypeObjects reference nested composites by their content-based
// hash ids, and the derive-side minimal object matches the registry-derived one.

use crate::dcps::topic::type_support::DdsType;
use crate::xtypes::{
    build_minimal_closure, CompleteStructMember, CompleteTypeObject, EquivalenceKind,
    HasTypeObject, MinimalStructMember, MinimalTypeObject, PlainCollectionHeader, TypeIdentifier,
    TypeObject,
};

#[derive(DdsType)]
#[dds_type(crate_path = "crate", extensibility = "Appendable")]
struct Inner {
    a: i32,
    b: String,
}

#[derive(DdsType)]
#[dds_type(crate_path = "crate", extensibility = "Appendable")]
struct Outer {
    inner: Inner,
    many: Vec<Inner>,
    scalars: Vec<i32>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "crate", extensibility = "Appendable")]
struct NestedColl {
    grid: Vec<Vec<Inner>>,
}

#[derive(DdsType)]
#[dds_type(crate_path = "crate", extensibility = "Appendable")]
struct Mapped {
    counts: std::collections::HashMap<String, i32>,
    entries: std::collections::HashMap<i32, Inner>,
    #[dds(bound = 4)]
    small: std::collections::BTreeMap<String, f64>,
}

#[test]
fn map_members_carry_plain_map_ids_and_nested_closure() {
    let complete = Mapped::complete_type_object();
    let members = complete_members(&complete);
    match &members[0].common.member_type_id {
        TypeIdentifier::PlainMapSmall {
            header, bound, key_identifier, element_identifier, ..
        } => {
            assert_eq!(header.equiv_kind, EquivalenceKind::Both);
            assert_eq!(*bound, 0);
            assert_eq!(**key_identifier, TypeIdentifier::String8);
            assert_eq!(**element_identifier, TypeIdentifier::Int32);
        }
        other => panic!("counts must be PlainMapSmall, got {:?}", other),
    }
    match &members[1].common.member_type_id {
        TypeIdentifier::PlainMapSmall { header, key_identifier, element_identifier, .. } => {
            assert_eq!(header.equiv_kind, EquivalenceKind::Complete);
            assert_eq!(**key_identifier, TypeIdentifier::Int32);
            assert_eq!(**element_identifier, Inner::type_identifier());
        }
        other => panic!("entries must be PlainMapSmall, got {:?}", other),
    }
    match &members[2].common.member_type_id {
        TypeIdentifier::PlainMapSmall { bound, .. } => assert_eq!(*bound, 4),
        other => panic!("small must keep its bound, got {:?}", other),
    }

    let minimal = Mapped::minimal_type_object();
    match &minimal_members(&minimal)[1].common.member_type_id {
        TypeIdentifier::PlainMapSmall { header, element_identifier, .. } => {
            assert_eq!(header.equiv_kind, EquivalenceKind::Minimal);
            assert_eq!(**element_identifier, Inner::minimal_type_identifier());
        }
        other => panic!("entries (minimal) must be PlainMapSmall, got {:?}", other),
    }

    let mut closure: Vec<(TypeIdentifier, TypeObject)> = Vec::new();
    Mapped::collect_nested_type_objects(&mut closure);
    assert!(
        closure.iter().any(|(id, _)| *id == Inner::type_identifier()),
        "map value type must be part of the nested closure"
    );
}

fn complete_members(o: &CompleteTypeObject) -> &Vec<CompleteStructMember> {
    match o {
        CompleteTypeObject::Struct(s) => &s.member_seq,
        _ => panic!("expected struct"),
    }
}

fn minimal_members(o: &MinimalTypeObject) -> &Vec<MinimalStructMember> {
    match o {
        MinimalTypeObject::Struct(s) => &s.member_seq,
        _ => panic!("expected struct"),
    }
}

#[test]
fn complete_member_id_is_child_complete_content_id() {
    let outer = Outer::complete_type_object();
    let members = complete_members(&outer);
    let inner_member = &members[0];
    assert_eq!(inner_member.common.member_type_id, Inner::type_identifier());
    assert!(matches!(Inner::type_identifier(), TypeIdentifier::CompleteTypeId(_)));
}

#[test]
fn minimal_member_id_is_child_minimal_content_id() {
    let outer = Outer::minimal_type_object();
    let members = minimal_members(&outer);
    let inner_member = &members[0];
    assert_eq!(inner_member.common.member_type_id, Inner::minimal_type_identifier());
    assert!(matches!(Inner::minimal_type_identifier(), TypeIdentifier::MinimalTypeId(_)));
}

#[test]
fn composite_sequence_carries_per_ek_element_and_kind() {
    // Complete: Vec<Inner> element == Inner complete id, header equiv_kind == Complete.
    let complete = Outer::complete_type_object();
    let many_c = &complete_members(&complete)[1].common.member_type_id;
    match many_c {
        TypeIdentifier::PlainSequenceSmall { header, element_identifier, .. } => {
            assert_eq!(**element_identifier, Inner::type_identifier());
            assert_eq!(header.equiv_kind, EquivalenceKind::Complete);
        }
        other => panic!("many must be PlainSequenceSmall, got {:?}", other),
    }

    // Minimal: Vec<Inner> element == Inner minimal id, header equiv_kind == Minimal.
    let minimal = Outer::minimal_type_object();
    let many_m = &minimal_members(&minimal)[1].common.member_type_id;
    match many_m {
        TypeIdentifier::PlainSequenceSmall { header, element_identifier, .. } => {
            assert_eq!(**element_identifier, Inner::minimal_type_identifier());
            assert_eq!(header.equiv_kind, EquivalenceKind::Minimal);
        }
        other => panic!("many must be PlainSequenceSmall, got {:?}", other),
    }
}

#[test]
fn primitive_sequence_stays_fully_descriptive() {
    let complete = Outer::complete_type_object();
    let scalars = &complete_members(&complete)[2].common.member_type_id;
    match scalars {
        TypeIdentifier::PlainSequenceSmall { header, element_identifier, .. } => {
            assert_eq!(**element_identifier, TypeIdentifier::Int32);
            assert_eq!(
                *header,
                PlainCollectionHeader { equiv_kind: EquivalenceKind::Both, ..Default::default() }
            );
            assert!(element_identifier.equivalence_hash().is_none());
        }
        other => panic!("scalars must be PlainSequenceSmall, got {:?}", other),
    }
}

#[test]
fn derive_minimal_matches_registry_derived_minimal() {
    // Collect the complete closure exactly as the writer path does.
    let mut closure: Vec<(TypeIdentifier, TypeObject)> = Vec::new();
    Outer::collect_nested_type_objects(&mut closure);

    let completes: Vec<(TypeIdentifier, CompleteTypeObject)> = closure
        .into_iter()
        .filter_map(|(id, obj)| match obj {
            TypeObject::Complete(c) => Some((id, c)),
            TypeObject::Minimal(_) => None,
        })
        .collect();

    let derived = build_minimal_closure(&completes);

    let outer_complete_hash = *Outer::type_identifier().equivalence_hash().unwrap();
    let registry_min_hash = derived
        .iter()
        .find(|(complete_key, _, _)| *complete_key == outer_complete_hash)
        .map(|(_, min_hash, _)| *min_hash)
        .expect("registry must derive a minimal for Outer");

    let derive_min_hash = *Outer::minimal_type_identifier().equivalence_hash().unwrap();
    assert_eq!(
        derive_min_hash, registry_min_hash,
        "derive-side minimal hash must equal registry-derived minimal hash"
    );
}

#[test]
fn nested_composite_collection_minimal_matches_registry() {
    // Vec<Vec<Inner>>: the registry-derived minimal must recompute BOTH the outer and
    // inner sequence headers to EK_MINIMAL (transitively), byte-matching derive.
    let mut closure: Vec<(TypeIdentifier, TypeObject)> = Vec::new();
    NestedColl::collect_nested_type_objects(&mut closure);
    let completes: Vec<(TypeIdentifier, CompleteTypeObject)> = closure
        .into_iter()
        .filter_map(|(id, obj)| match obj {
            TypeObject::Complete(c) => Some((id, c)),
            TypeObject::Minimal(_) => None,
        })
        .collect();

    let derived = build_minimal_closure(&completes);

    let outer_hash = *NestedColl::type_identifier().equivalence_hash().unwrap();
    let (_, registry_min_hash, registry_min) = derived
        .iter()
        .find(|(k, _, _)| *k == outer_hash)
        .expect("registry must derive a minimal for NestedColl");

    // The recomputed headers at both nesting levels must be EK_MINIMAL.
    match registry_min {
        MinimalTypeObject::Struct(s) => match &s.member_seq[0].common.member_type_id {
            TypeIdentifier::PlainSequenceSmall { header, element_identifier, .. } => {
                assert_eq!(header.equiv_kind, EquivalenceKind::Minimal, "outer header");
                match &**element_identifier {
                    TypeIdentifier::PlainSequenceSmall { header, element_identifier, .. } => {
                        assert_eq!(header.equiv_kind, EquivalenceKind::Minimal, "inner header");
                        assert_eq!(**element_identifier, Inner::minimal_type_identifier());
                    }
                    other => panic!("inner must be a sequence, got {:?}", other),
                }
            }
            other => panic!("grid must be a sequence, got {:?}", other),
        },
        _ => panic!("expected struct"),
    }

    let derive_min_hash = *NestedColl::minimal_type_identifier().equivalence_hash().unwrap();
    assert_eq!(
        derive_min_hash, *registry_min_hash,
        "derive-side minimal hash must equal registry-derived minimal hash for Vec<Vec<Inner>>"
    );
}

#[derive(DdsType)]
#[dds_type(crate_path = "crate", type_name = "sensors::Thing")]
struct ScopedThing {
    value: i32,
}

#[derive(DdsType)]
#[dds_type(crate_path = "crate")]
struct HolderOfScoped {
    thing: ScopedThing,
}

#[test]
fn nested_member_ref_matches_registered_id() {
    // A type_name override must NOT desync the name-based nested id scheme: a referencing
    // struct's member id (derived from the Rust type ident) must equal the id under which
    // the referenced type registers itself in the nested set.
    let mut scoped_set = Vec::new();
    ScopedThing::collect_nested_type_objects(&mut scoped_set);
    let scoped_reg_id = scoped_set[0].0.clone();

    let member_id = match HolderOfScoped::complete_type_object() {
        CompleteTypeObject::Struct(s) => s.member_seq[0].common.member_type_id.clone(),
        other => panic!("expected struct, got {other:?}"),
    };
    assert_eq!(
        member_id, scoped_reg_id,
        "nested member ref must resolve to the referenced type's registered nested id"
    );
}
