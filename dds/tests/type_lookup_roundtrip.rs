//! Wire-level contract for the TypeLookup service: a replier serves the
//! complete closure of a nested type via `getTypes`, the bytes survive a
//! `TypeLookup_Reply` round-trip, and the requester's registry ends up able to
//! resolve the nested type.
//!
//! The live two-participant discovery path is covered separately; this test
//! pins the data flow that `type_lookup_logic` depends on.

use int2dds::rtps::common::{guid::Guid, sequence::SequenceNumber};
use int2dds::xtypes::{
    CompleteStructMember, CompleteStructType, CompleteTypeObject, DynamicType, EquivalenceHash,
    ExtensibilityKind, GetTypesOut, MemberFlag, ReplyHeader, SampleIdentity, TypeFlag,
    TypeIdentifier, TypeLookupReply, TypeLookupReturn, TypeObject, TypeRegistry,
};

fn nested_pair() -> (CompleteTypeObject, EquivalenceHash, CompleteTypeObject, EquivalenceHash) {
    let inner = CompleteTypeObject::Struct(CompleteStructType::new(
        TypeFlag::new(ExtensibilityKind::Final, false, false),
        "Inner".to_string(),
        None,
    ));
    let inner_hash = EquivalenceHash::compute(&inner.serialize());

    let mut outer_struct = CompleteStructType::new(
        TypeFlag::new(ExtensibilityKind::Final, false, false),
        "Outer".to_string(),
        None,
    );
    outer_struct.add_member(CompleteStructMember::new(
        0,
        MemberFlag::default(),
        TypeIdentifier::Int32,
        "id".to_string(),
    ));
    outer_struct.add_member(CompleteStructMember::new(
        1,
        MemberFlag::default(),
        TypeIdentifier::CompleteTypeId(inner_hash),
        "child".to_string(),
    ));
    let outer = CompleteTypeObject::Struct(outer_struct);
    let outer_hash = EquivalenceHash::compute(&outer.serialize());
    (inner, inner_hash, outer, outer_hash)
}

#[test]
fn get_types_closure_reply_resolves_nested_on_requester() {
    let (inner, inner_hash, outer, outer_hash) = nested_pair();

    // Replier registry holds the full local type closure.
    let mut replier = TypeRegistry::new();
    replier.register_type_object(TypeObject::Complete(inner));
    replier.register_type_object(TypeObject::Complete(outer.clone()));

    // Replier answers getTypes(outer) with the transitive closure.
    let types = replier
        .complete_closure(&outer_hash)
        .into_iter()
        .map(|(h, obj)| (TypeIdentifier::CompleteTypeId(h), TypeObject::Complete(obj)))
        .collect::<Vec<_>>();
    assert_eq!(types.len(), 2, "closure must carry both outer and inner");

    let reply = TypeLookupReply {
        header: ReplyHeader {
            related_request_id: SampleIdentity::new(
                Guid::new(
                    [7u8; 12],
                    int2dds::rtps::common::entity_id::EntityId::TYPE_LOOKUP_REQUEST_WRITER,
                ),
                SequenceNumber::new(0, 1),
            ),
            remote_exception_code: 0,
        },
        data: TypeLookupReturn::GetTypes(GetTypesOut { types }),
    };

    // Wire round-trip (what the requester actually receives).
    let wire = reply.serialize();
    let received = TypeLookupReply::deserialize(&wire).expect("reply must deserialize");

    // Requester registers everything the reply carried.
    let mut requester = TypeRegistry::new();
    let TypeLookupReturn::GetTypes(out) = received.data else {
        panic!("expected getTypes return");
    };
    for (_, type_object) in out.types {
        requester.register_type_object(type_object);
    }

    // The requester can now resolve the nested type with no missing deps.
    assert!(requester.contains(&outer_hash));
    assert!(requester.contains(&inner_hash));
    assert!(requester.missing_dependencies(&outer_hash).is_empty());

    // And building a DynamicType against that registry succeeds (nested child
    // resolves through the registry rather than staying an unresolved ref).
    let dynamic = DynamicType::from_type_object_with_registry(
        std::sync::Arc::new(outer),
        TypeIdentifier::CompleteTypeId(outer_hash),
        &requester,
    );
    assert!(dynamic.is_ok(), "nested DynamicType must build from fetched closure");
}
