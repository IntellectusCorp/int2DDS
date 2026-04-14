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
        match type_obj {
            TypeObject::Complete(c) => {
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

    pub fn missing_dependencies(&self, hash: &EquivalenceHash) -> Vec<EquivalenceHash> {
        match self.dependencies.get(hash) {
            Some(deps) => deps.iter().filter(|d| !self.contains(d)).cloned().collect(),
            None => Vec::new(),
        }
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
}
