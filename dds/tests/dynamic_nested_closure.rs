//! The derive macro must emit the transitive closure of a type's nested
//! `TypeObject`s, keyed by the name-based `MinimalTypeId` under which members
//! reference them (the same scheme as `type_to_identifier` and the FFI
//! `add_named_type_field`). Once that closure is registered, a `DynamicType`
//! built from only the top-level object must resolve every nested member to a
//! `TypeRef` rather than leaving it an unresolved `ExternalType`.
//!
//! This is the local (no-wire) half of nested dynamic support; the live
//! two-participant TypeLookup variant is covered separately.

use std::sync::Arc;

use int2dds::topic::type_support::DdsType;
use int2dds::xtypes::{
    CompleteTypeObject, DynamicType, DynamicTypeKind, HasTypeObject, TypeObject, TypeRegistry,
};

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct Leaf {
    a: i32,
    b: i64,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds")]
#[repr(i32)]
enum Color {
    Red = 0,
    Green = 1,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct Branch {
    tag: i32,
    leaf: Leaf,
    color: Color,
}

#[derive(DdsType)]
#[dds_type(crate_path = "int2dds", extensibility = "Appendable")]
struct Tree {
    id: i32,
    direct: Branch,
}

/// Build the closure exactly the way a writer/reader populates the registry:
/// the top type under its own identifier, then nested members under their
/// name-based ids.
fn closure_of<T: HasTypeObject>() -> Vec<(int2dds::xtypes::TypeIdentifier, TypeObject)> {
    let mut out = vec![(T::type_identifier(), TypeObject::Complete(T::complete_type_object()))];
    T::collect_nested_type_objects(&mut out);
    out
}

fn registry_with<T: HasTypeObject>() -> TypeRegistry {
    let mut registry = TypeRegistry::new();
    for (id, obj) in closure_of::<T>() {
        registry.register_type_object_with_id(&id, obj);
    }
    registry
}

fn member<'a>(dt: &'a DynamicType, name: &str) -> &'a DynamicTypeKind {
    let desc = dt.as_struct().expect("struct type");
    &desc.members().iter().find(|m| &*m.name == name).expect("member present").member_type
}

#[test]
fn closure_includes_transitive_nested_objects() {
    let closure = closure_of::<Tree>();
    // Tree (under content id + its own name id), Branch, Leaf.
    let names: Vec<String> = closure
        .iter()
        .map(|(_, o)| match o {
            TypeObject::Complete(CompleteTypeObject::Struct(s)) => {
                s.header.detail.type_name.clone()
            }
            _ => String::new(),
        })
        .collect();
    assert!(names.iter().any(|n| n == "Branch"), "closure must carry Branch: {:?}", names);
    assert!(names.iter().any(|n| n == "Leaf"), "closure must carry Leaf: {:?}", names);
}

#[test]
fn nested_members_resolve_to_typeref() {
    let registry = registry_with::<Tree>();
    let dt = DynamicType::from_type_object_with_registry(
        Arc::new(Tree::complete_type_object()),
        Tree::type_identifier(),
        &registry,
    )
    .expect("build Tree dynamic type");

    // Direct nested struct member resolves, not left external.
    match member(&dt, "direct") {
        DynamicTypeKind::TypeRef(branch) => {
            // The grandchild struct (Leaf inside Branch) resolves too.
            assert!(
                matches!(member(branch, "leaf"), DynamicTypeKind::TypeRef(_)),
                "Branch.leaf must resolve to TypeRef, got {:?}",
                member(branch, "leaf")
            );
            // And a nested enum member resolves as well.
            assert!(
                matches!(member(branch, "color"), DynamicTypeKind::TypeRef(_)),
                "Branch.color must resolve to TypeRef, got {:?}",
                member(branch, "color")
            );
        }
        other => panic!("Tree.direct must resolve to TypeRef, got {:?}", other),
    }
}

#[test]
fn unregistered_nested_stays_external() {
    // With an empty registry the same build leaves nested members external,
    // proving the closure registration is what makes resolution succeed.
    let empty = TypeRegistry::new();
    let dt = DynamicType::from_type_object_with_registry(
        Arc::new(Tree::complete_type_object()),
        Tree::type_identifier(),
        &empty,
    )
    .expect("build Tree dynamic type");
    assert!(
        matches!(member(&dt, "direct"), DynamicTypeKind::ExternalType { .. }),
        "without registration the nested member must stay ExternalType"
    );
}
