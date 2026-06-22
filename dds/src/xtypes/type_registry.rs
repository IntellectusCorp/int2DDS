use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use super::type_object::{
    CompleteTypeObject, EquivalenceHash, MinimalTypeObject, TypeIdentifier, TypeObject,
};

pub struct TypeRegistry {
    complete_objects: HashMap<EquivalenceHash, CompleteTypeObject>,
    minimal_objects: HashMap<EquivalenceHash, MinimalTypeObject>,
    type_names: HashMap<String, EquivalenceHash>,
    dependencies: HashMap<EquivalenceHash, Vec<EquivalenceHash>>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            complete_objects: HashMap::new(),
            minimal_objects: HashMap::new(),
            type_names: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    pub fn register_complete(
        &mut self,
        hash: EquivalenceHash,
        type_name: String,
        obj: CompleteTypeObject,
    ) {
        self.type_names.insert(type_name, hash);
        self.complete_objects.insert(hash, obj);
    }

    pub fn register_minimal(&mut self, hash: EquivalenceHash, obj: MinimalTypeObject) {
        self.minimal_objects.insert(hash, obj);
    }

    pub fn register_type_object(&mut self, type_obj: TypeObject) {
        let hash = type_obj.compute_hash();
        self.register_type_object_keyed(hash, type_obj);
    }

    pub fn register_type_object_with_id(&mut self, id: &TypeIdentifier, type_obj: TypeObject) {
        let hash = id.equivalence_hash().copied().unwrap_or_else(|| type_obj.compute_hash());
        self.register_type_object_keyed(hash, type_obj);
    }

    fn register_type_object_keyed(&mut self, hash: EquivalenceHash, type_obj: TypeObject) {
        match type_obj {
            TypeObject::Complete(c) => {
                let deps = referenced_hashes(&c);
                if !deps.is_empty() {
                    self.dependencies.insert(hash, deps);
                }
                let name = extract_complete_type_name(&c);
                self.register_complete(hash, name, c);
            }
            TypeObject::Minimal(m) => {
                self.register_minimal(hash, m);
            }
        }
    }

    pub fn register_dependencies(&mut self, hash: EquivalenceHash, deps: Vec<EquivalenceHash>) {
        self.dependencies.insert(hash, deps);
    }

    pub fn lookup_complete(&self, hash: &EquivalenceHash) -> Option<&CompleteTypeObject> {
        self.complete_objects.get(hash)
    }

    pub fn lookup_minimal(&self, hash: &EquivalenceHash) -> Option<&MinimalTypeObject> {
        self.minimal_objects.get(hash)
    }

    pub fn lookup_by_name(&self, name: &str) -> Option<&EquivalenceHash> {
        self.type_names.get(name)
    }

    pub fn get_dependencies(&self, hash: &EquivalenceHash) -> Option<&Vec<EquivalenceHash>> {
        self.dependencies.get(hash)
    }

    pub fn resolve_type(&self, type_id: &TypeIdentifier) -> Option<&CompleteTypeObject> {
        match type_id {
            TypeIdentifier::CompleteTypeId(hash) | TypeIdentifier::MinimalTypeId(hash) => {
                self.complete_objects.get(hash)
            }
            _ => None,
        }
    }

    pub fn contains(&self, hash: &EquivalenceHash) -> bool {
        self.complete_objects.contains_key(hash) || self.minimal_objects.contains_key(hash)
    }

    /// Directly-referenced type hashes of `obj` absent from this registry — i.e.
    /// whether an inline TypeObject still needs a TypeLookup fetch for its members.
    pub fn missing_direct_dependencies(&self, obj: &CompleteTypeObject) -> Vec<EquivalenceHash> {
        referenced_hashes(obj).into_iter().filter(|h| !self.contains(h)).collect()
    }

    pub fn missing_dependencies(&self, hash: &EquivalenceHash) -> Vec<EquivalenceHash> {
        match self.dependencies.get(hash) {
            Some(deps) => deps.iter().filter(|d| !self.contains(d)).cloned().collect(),
            None => Vec::new(),
        }
    }

    pub fn transitive_dependency_hashes(&self, roots: &[EquivalenceHash]) -> Vec<EquivalenceHash> {
        let mut out = Vec::new();
        let mut seen: std::collections::HashSet<EquivalenceHash> = roots.iter().copied().collect();
        let mut stack: Vec<EquivalenceHash> = roots.iter().rev().copied().collect();
        while let Some(h) = stack.pop() {
            if let Some(deps) = self.dependencies.get(&h) {
                for &dep in deps {
                    if seen.insert(dep) {
                        out.push(dep);
                        stack.push(dep);
                    }
                }
            }
        }
        out
    }

    /// Every complete type transitively referenced by `root` (excluding `root`),
    /// paired with the name-based `MinimalTypeId` used to reference it — the same
    /// nested closure a derive `DdsType` advertises.
    pub fn dependency_closure_of(
        &self,
        root: &CompleteTypeObject,
    ) -> Vec<(TypeIdentifier, TypeObject)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut stack = referenced_hashes(root);
        while let Some(h) = stack.pop() {
            if !seen.insert(h) {
                continue;
            }
            if let Some(obj) = self.complete_objects.get(&h) {
                stack.extend(referenced_hashes(obj));
                out.push((TypeIdentifier::MinimalTypeId(h), TypeObject::Complete(obj.clone())));
            }
        }
        out
    }

    pub fn complete_closure(
        &self,
        hash: &EquivalenceHash,
    ) -> Vec<(EquivalenceHash, CompleteTypeObject)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut stack = vec![*hash];
        while let Some(h) = stack.pop() {
            if !seen.insert(h) {
                continue;
            }
            if let Some(obj) = self.complete_objects.get(&h) {
                out.push((h, obj.clone()));
                if let Some(deps) = self.dependencies.get(&h) {
                    stack.extend(deps.iter().copied());
                }
            }
        }
        out
    }
}

pub(crate) fn referenced_hashes(obj: &CompleteTypeObject) -> Vec<EquivalenceHash> {
    let mut hashes = Vec::new();
    let mut push = |id: &TypeIdentifier| collect_from_identifier(id, &mut hashes);
    match obj {
        CompleteTypeObject::Struct(s) => {
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
        CompleteTypeObject::Alias(a) => {
            push(&a.body.related_type);
        }
        CompleteTypeObject::Enum(_)
        | CompleteTypeObject::Bitmask(_)
        | CompleteTypeObject::Bitset(_) => {}
    }
    hashes
}

fn collect_from_identifier(id: &TypeIdentifier, out: &mut Vec<EquivalenceHash>) {
    if let Some(hash) = id.equivalence_hash() {
        out.push(*hash);
        return;
    }
    match id {
        TypeIdentifier::PlainSequenceSmall { element_identifier, .. }
        | TypeIdentifier::PlainSequenceLarge { element_identifier, .. }
        | TypeIdentifier::PlainArraySmall { element_identifier, .. }
        | TypeIdentifier::PlainArrayLarge { element_identifier, .. } => {
            collect_from_identifier(element_identifier, out);
        }
        _ => {}
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub type SharedTypeRegistry = Arc<RwLock<TypeRegistry>>;

pub fn new_shared_registry() -> SharedTypeRegistry {
    Arc::new(RwLock::new(TypeRegistry::new()))
}

fn extract_complete_type_name(obj: &CompleteTypeObject) -> String {
    match obj {
        CompleteTypeObject::Struct(s) => s.header.detail.type_name.clone(),
        CompleteTypeObject::Enum(e) => e.header.detail.type_name.clone(),
        CompleteTypeObject::Union(u) => u.header.type_name.clone(),
        CompleteTypeObject::Alias(a) => a.header.type_name.clone(),
        CompleteTypeObject::Bitmask(b) => b.header.detail.type_name.clone(),
        CompleteTypeObject::Bitset(b) => b.header.type_name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xtypes::type_object::*;

    #[test]
    fn test_register_and_lookup() {
        let mut registry = TypeRegistry::new();

        let mut struct_type = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Final, false, false),
            "TestStruct".to_string(),
            None,
        );
        struct_type.add_member(CompleteStructMember::new(
            0,
            MemberFlag::default(),
            TypeIdentifier::Int32,
            "value".to_string(),
        ));

        let complete = CompleteTypeObject::Struct(struct_type);
        let hash = EquivalenceHash::compute(&complete.serialize());

        registry.register_complete(hash, "TestStruct".to_string(), complete.clone());

        assert!(registry.lookup_complete(&hash).is_some());
        assert_eq!(registry.lookup_by_name("TestStruct"), Some(&hash));
        assert!(registry.lookup_by_name("NonExistent").is_none());
    }

    #[test]
    fn test_resolve_type() {
        let mut registry = TypeRegistry::new();

        let struct_type = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Final, false, false),
            "MyType".to_string(),
            None,
        );
        let complete = CompleteTypeObject::Struct(struct_type);
        let hash = EquivalenceHash::compute(&complete.serialize());

        registry.register_complete(hash, "MyType".to_string(), complete);

        let type_id = TypeIdentifier::CompleteTypeId(hash);
        assert!(registry.resolve_type(&type_id).is_some());

        let minimal_id = TypeIdentifier::MinimalTypeId(hash);
        assert!(registry.resolve_type(&minimal_id).is_some());

        assert!(registry.resolve_type(&TypeIdentifier::Int32).is_none());
    }

    #[test]
    fn test_missing_dependencies() {
        let mut registry = TypeRegistry::new();

        let hash_a = EquivalenceHash::compute(b"type_a");
        let hash_b = EquivalenceHash::compute(b"type_b");
        let hash_c = EquivalenceHash::compute(b"type_c");

        let struct_type = CompleteStructType::new(TypeFlag::default(), "TypeA".to_string(), None);
        registry.register_complete(
            hash_a,
            "TypeA".to_string(),
            CompleteTypeObject::Struct(struct_type),
        );

        registry.register_dependencies(hash_a, vec![hash_b, hash_c]);

        let missing = registry.missing_dependencies(&hash_a);
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&hash_b));
        assert!(missing.contains(&hash_c));
    }

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
            TypeIdentifier::CompleteTypeId(inner_hash),
            "child".to_string(),
        ));
        let outer = CompleteTypeObject::Struct(outer_struct);
        let outer_hash = EquivalenceHash::compute(&outer.serialize());
        (inner, inner_hash, outer, outer_hash)
    }

    #[test]
    fn register_records_dependencies_and_full_closure() {
        let mut registry = TypeRegistry::new();
        let (inner, inner_hash, outer, outer_hash) = nested_pair();

        registry.register_type_object(TypeObject::Complete(inner));
        registry.register_type_object(TypeObject::Complete(outer));

        // register_type_object walked the outer member and recorded inner as a dep.
        assert_eq!(registry.get_dependencies(&outer_hash), Some(&vec![inner_hash]));
        assert!(registry.missing_dependencies(&outer_hash).is_empty());

        let closure = registry.complete_closure(&outer_hash);
        assert_eq!(closure.len(), 2);
        assert!(closure.iter().any(|(h, _)| *h == outer_hash));
        assert!(closure.iter().any(|(h, _)| *h == inner_hash));
    }

    #[test]
    fn transitive_deps_exclude_roots_and_list_children() {
        let mut registry = TypeRegistry::new();
        let (inner, inner_hash, outer, outer_hash) = nested_pair();
        registry.register_type_object(TypeObject::Complete(inner));
        registry.register_type_object(TypeObject::Complete(outer));

        let deps = registry.transitive_dependency_hashes(&[outer_hash]);
        assert_eq!(deps, vec![inner_hash], "root excluded, transitive child listed");

        // A leaf with no dependencies yields nothing.
        assert!(registry.transitive_dependency_hashes(&[inner_hash]).is_empty());
    }

    #[test]
    fn dependency_closure_pairs_nested_with_minimal_id() {
        let mut registry = TypeRegistry::new();
        let (inner, inner_hash, outer, _outer_hash) = nested_pair();
        registry.register_type_object(TypeObject::Complete(inner));

        let closure = registry.dependency_closure_of(&outer);
        assert_eq!(closure.len(), 1);
        assert_eq!(closure[0].0, TypeIdentifier::MinimalTypeId(inner_hash));
    }

    #[test]
    fn missing_direct_dependencies_tracks_registry_contents() {
        let (inner, inner_hash, outer, _outer_hash) = nested_pair();

        let empty = TypeRegistry::new();
        assert_eq!(empty.missing_direct_dependencies(&outer), vec![inner_hash]);

        let mut registry = TypeRegistry::new();
        registry.register_type_object(TypeObject::Complete(inner));
        assert!(registry.missing_direct_dependencies(&outer).is_empty());
    }

    #[test]
    fn closure_and_missing_when_inner_absent() {
        let mut registry = TypeRegistry::new();
        let (_inner, inner_hash, outer, outer_hash) = nested_pair();

        registry.register_type_object(TypeObject::Complete(outer));

        assert_eq!(registry.missing_dependencies(&outer_hash), vec![inner_hash]);
        // The closure only yields what is resolvable locally.
        let closure = registry.complete_closure(&outer_hash);
        assert_eq!(closure.len(), 1);
        assert_eq!(closure[0].0, outer_hash);
    }
}
