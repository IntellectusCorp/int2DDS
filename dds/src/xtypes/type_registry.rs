use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use log::warn;

use super::type_object::*;
use super::type_object_xcdr::spec_hash;

pub struct TypeRegistry {
    complete_objects: HashMap<EquivalenceHash, CompleteTypeObject>,
    minimal_objects: HashMap<EquivalenceHash, MinimalTypeObject>,
    type_names: HashMap<String, EquivalenceHash>,
    dependencies: HashMap<EquivalenceHash, Vec<EquivalenceHash>>,
    complete_to_minimal: HashMap<EquivalenceHash, EquivalenceHash>,
    minimal_to_complete: HashMap<EquivalenceHash, EquivalenceHash>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            complete_objects: HashMap::new(),
            minimal_objects: HashMap::new(),
            type_names: HashMap::new(),
            dependencies: HashMap::new(),
            complete_to_minimal: HashMap::new(),
            minimal_to_complete: HashMap::new(),
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
        self.try_derive_minimal(&hash);
    }

    pub fn register_type_object_with_id(&mut self, id: &TypeIdentifier, type_obj: TypeObject) {
        let hash = id.equivalence_hash().copied().unwrap_or_else(|| type_obj.compute_hash());
        self.register_type_object_keyed(hash, type_obj);
        self.try_derive_minimal(&hash);
    }

    /// Register a full type closure (root + nested), then derive and register the
    /// minimal equivalent of every complete entry, recording both directions of the
    /// complete<->minimal hash mapping. The single entry point local registration
    /// uses so member ids are rewritten across the whole closure at once.
    pub fn register_closure(&mut self, closure: &[(TypeIdentifier, TypeObject)]) {
        let mut completes: Vec<(TypeIdentifier, CompleteTypeObject)> = Vec::new();
        for (id, obj) in closure {
            let hash = id.equivalence_hash().copied().unwrap_or_else(|| obj.compute_hash());
            match obj {
                TypeObject::Complete(c) => {
                    self.register_type_object_keyed(hash, TypeObject::Complete(c.clone()));
                    completes.push((id.clone(), c.clone()));
                }
                TypeObject::Minimal(m) => self.register_minimal(hash, m.clone()),
            }
        }
        for (complete_key, min_hash, min_obj) in build_minimal_closure(&completes) {
            self.register_minimal(min_hash, min_obj);
            self.complete_to_minimal.insert(complete_key, min_hash);
            self.minimal_to_complete.insert(min_hash, complete_key);
        }
    }

    /// Best-effort minimal derivation for an already-registered complete object.
    /// Succeeds (and records the mapping) only when every direct dependency already
    /// has a known minimal hash; otherwise leaves the object complete-only. Cheap to
    /// re-attempt once a missing dependency's minimal becomes available.
    pub fn try_derive_minimal(&mut self, hash: &EquivalenceHash) -> bool {
        if self.complete_to_minimal.contains_key(hash) {
            return true;
        }
        let c = match self.complete_objects.get(hash) {
            Some(c) => c.clone(),
            None => return false,
        };
        let mut map = HashMap::new();
        for dep in referenced_hashes(&c) {
            if dep == *hash {
                continue;
            }
            match self.complete_to_minimal.get(&dep) {
                Some(min) => {
                    map.insert(dep, *min);
                }
                None => return false,
            }
        }
        let min = derive_minimal_object(&c, &map);
        let min_hash = spec_hash(&TypeObject::Minimal(min.clone()));
        self.register_minimal(min_hash, min);
        self.complete_to_minimal.insert(*hash, min_hash);
        self.minimal_to_complete.insert(min_hash, *hash);
        true
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

    /// Record a peer-supplied COMPLETE<->MINIMAL hash correspondence (from a
    /// `getTypes` reply's `complete_to_minimal`). Never overrides a locally derived
    /// mapping, so our own (authoritative) derivation wins on conflict.
    pub fn note_complete_to_minimal(
        &mut self,
        complete: EquivalenceHash,
        minimal: EquivalenceHash,
    ) {
        self.complete_to_minimal.entry(complete).or_insert(minimal);
        self.minimal_to_complete.entry(minimal).or_insert(complete);
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

    /// Resolve to the object of the requested equivalence kind when available,
    /// falling back to the complementary kind (translated via the complete<->minimal
    /// mapping) otherwise.
    pub fn resolve_type_object(&self, type_id: &TypeIdentifier) -> Option<TypeObject> {
        match type_id {
            TypeIdentifier::MinimalTypeId(hash) => {
                if let Some(m) = self.minimal_objects.get(hash) {
                    return Some(TypeObject::Minimal(m.clone()));
                }
                if let Some(c) = self.complete_objects.get(hash) {
                    return Some(TypeObject::Complete(c.clone()));
                }
                self.minimal_to_complete
                    .get(hash)
                    .and_then(|ck| self.complete_objects.get(ck))
                    .map(|c| TypeObject::Complete(c.clone()))
            }
            TypeIdentifier::CompleteTypeId(hash) => {
                if let Some(c) = self.complete_objects.get(hash) {
                    return Some(TypeObject::Complete(c.clone()));
                }
                if let Some(m) = self.minimal_objects.get(hash) {
                    return Some(TypeObject::Minimal(m.clone()));
                }
                self.complete_to_minimal
                    .get(hash)
                    .and_then(|mk| self.minimal_objects.get(mk))
                    .map(|m| TypeObject::Minimal(m.clone()))
            }
            _ => None,
        }
    }

    /// The minimal hash derived for a complete hash, if the mapping exists.
    pub fn minimal_hash_of(&self, complete: &EquivalenceHash) -> Option<EquivalenceHash> {
        self.complete_to_minimal.get(complete).copied()
    }

    /// The complete hash a minimal hash was derived from, if the mapping exists.
    pub fn complete_hash_of(&self, minimal: &EquivalenceHash) -> Option<EquivalenceHash> {
        self.minimal_to_complete.get(minimal).copied()
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
    /// paired with its content-based `CompleteTypeId` — the same nested closure a
    /// derive `DdsType` advertises.
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
                out.push((TypeIdentifier::CompleteTypeId(h), TypeObject::Complete(obj.clone())));
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

/// Derive the minimal equivalent of each complete object in a closure, rewriting
/// hash member ids that reference other closure entries to their `MinimalTypeId`.
pub fn build_minimal_closure(
    closure: &[(TypeIdentifier, CompleteTypeObject)],
) -> Vec<(EquivalenceHash, EquivalenceHash, MinimalTypeObject)> {
    let n = closure.len();
    let keys: Vec<EquivalenceHash> = closure
        .iter()
        .map(|(id, c)| {
            id.equivalence_hash()
                .copied()
                .unwrap_or_else(|| spec_hash(&TypeObject::Complete(c.clone())))
        })
        .collect();
    let key_index: HashMap<EquivalenceHash, usize> =
        keys.iter().enumerate().map(|(i, h)| (*h, i)).collect();
    let deps: Vec<Vec<usize>> = closure
        .iter()
        .enumerate()
        .map(|(i, (_, c))| {
            let mut d = Vec::new();
            for h in referenced_hashes(c) {
                if let Some(&j) = key_index.get(&h) {
                    if j != i && !d.contains(&j) {
                        d.push(j);
                    }
                }
            }
            d
        })
        .collect();

    let mut resolved: HashMap<EquivalenceHash, EquivalenceHash> = HashMap::new();
    let mut done = vec![false; n];
    let mut out: Vec<(EquivalenceHash, EquivalenceHash, MinimalTypeObject)> = Vec::new();

    loop {
        let mut progressed = false;
        for i in 0..n {
            if done[i] || !deps[i].iter().all(|&j| done[j]) {
                continue;
            }
            let min = derive_minimal_object(&closure[i].1, &resolved);
            let min_hash = spec_hash(&TypeObject::Minimal(min.clone()));
            resolved.insert(keys[i], min_hash);
            done[i] = true;
            progressed = true;
            out.push((keys[i], min_hash, min));
        }
        if !progressed {
            break;
        }
    }

    if done.iter().any(|d| !d) {
        warn!(
            "build_minimal_closure: cyclic type dependency detected; leaving cyclic \
             member ids unrewritten (mutually-recursive types are unsupported)"
        );
        let frozen = resolved.clone();
        for i in 0..n {
            if done[i] {
                continue;
            }
            let min = derive_minimal_object(&closure[i].1, &frozen);
            let min_hash = spec_hash(&TypeObject::Minimal(min.clone()));
            resolved.insert(keys[i], min_hash);
            done[i] = true;
            out.push((keys[i], min_hash, min));
        }
    }

    out
}

/// Build the minimal `TypeObject` for a complete one, rewriting member/related ids
/// via `map` (complete closure key -> minimal hash) and hashing member names.
fn derive_minimal_object(
    c: &CompleteTypeObject,
    map: &HashMap<EquivalenceHash, EquivalenceHash>,
) -> MinimalTypeObject {
    match c {
        CompleteTypeObject::Struct(s) => MinimalTypeObject::Struct(MinimalStructType {
            struct_flags: s.struct_flags,
            header: MinimalStructHeader {
                base_type: s.header.base_type.as_ref().map(|b| rewrite_id(b, map)),
            },
            member_seq: s
                .member_seq
                .iter()
                .map(|m| MinimalStructMember {
                    common: CommonStructMember {
                        member_id: m.common.member_id,
                        member_flags: m.common.member_flags,
                        member_type_id: rewrite_id(&m.common.member_type_id, map),
                    },
                    name_hash: compute_name_hash(&m.detail.name),
                })
                .collect(),
        }),
        CompleteTypeObject::Union(u) => MinimalTypeObject::Union(MinimalUnionType {
            union_flags: u.union_flags,
            discriminator: CommonDiscriminatorMember {
                member_flags: u.discriminator.member_flags,
                type_id: rewrite_id(&u.discriminator.type_id, map),
            },
            member_seq: u
                .member_seq
                .iter()
                .map(|m| MinimalUnionMember {
                    common: CommonUnionMember {
                        member_id: m.common.member_id,
                        member_flags: m.common.member_flags,
                        member_type_id: rewrite_id(&m.common.member_type_id, map),
                        label_seq: m.common.label_seq.clone(),
                    },
                    name_hash: compute_name_hash(&m.detail.name),
                })
                .collect(),
        }),
        CompleteTypeObject::Enum(e) => MinimalTypeObject::Enum(MinimalEnumeratedType {
            enum_flags: e.enum_flags,
            header: MinimalEnumeratedHeader {
                common: CommonEnumeratedHeader { bit_bound: e.header.common.bit_bound },
            },
            literal_seq: e
                .literal_seq
                .iter()
                .map(|l| MinimalEnumeratedLiteral {
                    common: CommonEnumeratedLiteral {
                        value: l.common.value,
                        flags: l.common.flags,
                    },
                    name_hash: compute_name_hash(&l.detail.name),
                })
                .collect(),
        }),
        CompleteTypeObject::Alias(a) => MinimalTypeObject::Alias(MinimalAliasType {
            alias_flags: a.alias_flags,
            body: CommonAliasBody {
                related_flags: a.body.related_flags,
                related_type: rewrite_id(&a.body.related_type, map),
            },
        }),
        CompleteTypeObject::Bitmask(b) => MinimalTypeObject::Bitmask(MinimalBitmaskType {
            bitmask_flags: b.bitmask_flags,
            header: CommonEnumeratedHeader { bit_bound: b.header.common.bit_bound },
            flag_seq: b
                .flag_seq
                .iter()
                .map(|f| MinimalBitflag {
                    common: CommonBitflag { position: f.common.position, flags: f.common.flags },
                    name_hash: compute_name_hash(&f.detail.name),
                })
                .collect(),
        }),
        CompleteTypeObject::Bitset(b) => MinimalTypeObject::Bitset(MinimalBitsetType {
            bitset_flags: b.bitset_flags,
            field_seq: b
                .field_seq
                .iter()
                .map(|f| MinimalBitfield {
                    common: f.common.clone(),
                    name_hash: compute_name_hash(&f.detail.name),
                })
                .collect(),
        }),
    }
}

/// Rewrite a hash id whose hash is a closure key to the corresponding `MinimalTypeId`.
/// Recurses into plain collection element/key ids and, when such an element/key was a
/// rewritten hash id, sets the collection header's `equiv_kind` to Minimal. Non-hash
/// ids and hash ids outside the closure pass through unchanged.
fn rewrite_id(
    id: &TypeIdentifier,
    map: &HashMap<EquivalenceHash, EquivalenceHash>,
) -> TypeIdentifier {
    if let Some(h) = id.equivalence_hash() {
        return match map.get(h) {
            Some(min) => TypeIdentifier::MinimalTypeId(*min),
            None => id.clone(),
        };
    }
    // Rewrite inner element/key ids, then recompute the header's equiv_kind from the
    // rewritten element/key so nested collections (e.g. Vec<Vec<Inner>>) stay correct.
    let rebuilt = match id {
        TypeIdentifier::PlainSequenceSmall { header, bound, element_identifier } => {
            TypeIdentifier::PlainSequenceSmall {
                header: *header,
                bound: *bound,
                element_identifier: Box::new(rewrite_id(element_identifier, map)),
            }
        }
        TypeIdentifier::PlainSequenceLarge { header, bound, element_identifier } => {
            TypeIdentifier::PlainSequenceLarge {
                header: *header,
                bound: *bound,
                element_identifier: Box::new(rewrite_id(element_identifier, map)),
            }
        }
        TypeIdentifier::PlainArraySmall { header, array_bound_seq, element_identifier } => {
            TypeIdentifier::PlainArraySmall {
                header: *header,
                array_bound_seq: array_bound_seq.clone(),
                element_identifier: Box::new(rewrite_id(element_identifier, map)),
            }
        }
        TypeIdentifier::PlainArrayLarge { header, array_bound_seq, element_identifier } => {
            TypeIdentifier::PlainArrayLarge {
                header: *header,
                array_bound_seq: array_bound_seq.clone(),
                element_identifier: Box::new(rewrite_id(element_identifier, map)),
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
            key_identifier: Box::new(rewrite_id(key_identifier, map)),
            element_identifier: Box::new(rewrite_id(element_identifier, map)),
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
            key_identifier: Box::new(rewrite_id(key_identifier, map)),
            element_identifier: Box::new(rewrite_id(element_identifier, map)),
        },
        _ => return id.clone(),
    };
    recompute_collection_kind(rebuilt)
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
        TypeIdentifier::PlainMapSmall { key_identifier, element_identifier, .. }
        | TypeIdentifier::PlainMapLarge { key_identifier, element_identifier, .. } => {
            collect_from_identifier(key_identifier, out);
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

impl super::type_compatibility::TypeResolver for TypeRegistry {
    fn resolve(&self, id: &TypeIdentifier) -> Option<TypeObject> {
        self.resolve_type_object(id)
    }

    fn resolve_complete(&self, id: &TypeIdentifier) -> Option<TypeObject> {
        match id {
            TypeIdentifier::CompleteTypeId(hash) => {
                self.complete_objects.get(hash).map(|c| TypeObject::Complete(c.clone()))
            }
            TypeIdentifier::MinimalTypeId(hash) => self
                .complete_objects
                .get(hash)
                .or_else(|| {
                    self.minimal_to_complete.get(hash).and_then(|ck| self.complete_objects.get(ck))
                })
                .map(|c| TypeObject::Complete(c.clone())),
            _ => None,
        }
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
    use crate::xtypes::serialize_type_object;

    fn final_struct(name: &str) -> CompleteStructType {
        CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Final, false, false),
            name.into(),
            None,
        )
    }

    fn struct_with_member(name: &str, member: TypeIdentifier) -> CompleteTypeObject {
        let mut s = final_struct(name);
        s.add_member(CompleteStructMember::new(0, MemberFlag::default(), member, "child".into()));
        CompleteTypeObject::Struct(s)
    }

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
        let hash = TypeObject::Complete(complete.clone()).compute_hash();

        registry.register_complete(hash, "TestStruct".to_string(), complete.clone());

        assert!(registry.lookup_complete(&hash).is_some());
        assert_eq!(registry.lookup_by_name("TestStruct"), Some(&hash));
        assert!(registry.lookup_by_name("NonExistent").is_none());
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
        let inner_hash = TypeObject::Complete(inner.clone()).compute_hash();

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
        let outer_hash = TypeObject::Complete(outer.clone()).compute_hash();
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
    fn dependency_closure_pairs_nested_with_complete_id() {
        let mut registry = TypeRegistry::new();
        let (inner, inner_hash, outer, _outer_hash) = nested_pair();
        registry.register_type_object(TypeObject::Complete(inner));

        let closure = registry.dependency_closure_of(&outer);
        assert_eq!(closure.len(), 1);
        assert_eq!(closure[0].0, TypeIdentifier::CompleteTypeId(inner_hash));
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

    #[test]
    fn build_minimal_closure_rewrites_nested_hash_member() {
        let inner_key = EquivalenceHash::compute(b"inner");
        let outer_key = EquivalenceHash::compute(b"outer");
        let inner = CompleteTypeObject::Struct(final_struct("Inner"));
        let outer = struct_with_member("Outer", TypeIdentifier::CompleteTypeId(inner_key));

        let closure = vec![
            (TypeIdentifier::CompleteTypeId(outer_key), outer),
            (TypeIdentifier::CompleteTypeId(inner_key), inner),
        ];
        let derived = build_minimal_closure(&closure);
        assert_eq!(derived.len(), 2);

        let inner_min = derived.iter().find(|(k, ..)| *k == inner_key).unwrap().1;
        let outer_entry = derived.iter().find(|(k, ..)| *k == outer_key).unwrap();
        match &outer_entry.2 {
            MinimalTypeObject::Struct(s) => assert_eq!(
                s.member_seq[0].common.member_type_id,
                TypeIdentifier::MinimalTypeId(inner_min)
            ),
            _ => panic!("expected struct"),
        }

        // Deriving twice yields byte-identical minimal objects and stable hashes.
        let again = build_minimal_closure(&closure);
        for (k, h, obj) in &derived {
            let other = again.iter().find(|(k2, ..)| k2 == k).unwrap();
            assert_eq!(*h, other.1, "minimal hash stable");
            assert_eq!(
                serialize_type_object(&TypeObject::Minimal(obj.clone())),
                serialize_type_object(&TypeObject::Minimal(other.2.clone()))
            );
        }
    }

    #[test]
    fn build_minimal_closure_rewrites_collection_element() {
        let inner_key = EquivalenceHash::compute(b"inner_c");
        let outer_key = EquivalenceHash::compute(b"outer_c");
        let inner = CompleteTypeObject::Struct(final_struct("InnerC"));
        let seq = TypeIdentifier::PlainSequenceLarge {
            header: PlainCollectionHeader {
                equiv_kind: EquivalenceKind::Both,
                element_flags: CollectionElementFlag(0),
            },
            bound: 0,
            element_identifier: Box::new(TypeIdentifier::CompleteTypeId(inner_key)),
        };
        let outer = struct_with_member("OuterC", seq);

        let closure = vec![
            (TypeIdentifier::CompleteTypeId(outer_key), outer),
            (TypeIdentifier::CompleteTypeId(inner_key), inner),
        ];
        let derived = build_minimal_closure(&closure);
        let inner_min = derived.iter().find(|(k, ..)| *k == inner_key).unwrap().1;
        let outer_entry = derived.iter().find(|(k, ..)| *k == outer_key).unwrap();
        match &outer_entry.2 {
            MinimalTypeObject::Struct(s) => match &s.member_seq[0].common.member_type_id {
                TypeIdentifier::PlainSequenceLarge { header, element_identifier, .. } => {
                    assert_eq!(header.equiv_kind, EquivalenceKind::Minimal);
                    assert_eq!(**element_identifier, TypeIdentifier::MinimalTypeId(inner_min));
                }
                other => panic!("expected sequence, got {:?}", other),
            },
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn referenced_hashes_collects_map_key_and_value() {
        let key_hash = EquivalenceHash::compute(b"map_key");
        let val_hash = EquivalenceHash::compute(b"map_val");
        let map = TypeIdentifier::PlainMapSmall {
            header: PlainCollectionHeader {
                equiv_kind: EquivalenceKind::Both,
                element_flags: CollectionElementFlag(0),
            },
            bound: 8,
            key_flags: CollectionElementFlag(0),
            key_identifier: Box::new(TypeIdentifier::CompleteTypeId(key_hash)),
            element_identifier: Box::new(TypeIdentifier::CompleteTypeId(val_hash)),
        };
        let outer = struct_with_member("OuterMap", map);
        let deps = referenced_hashes(&outer);
        assert!(deps.contains(&key_hash), "map key type must be a dependency");
        assert!(deps.contains(&val_hash), "map value type must be a dependency");
    }

    #[test]
    fn register_closure_exposes_both_equivalence_kinds() {
        let inner_key = EquivalenceHash::compute(b"reg_inner");
        let outer_key = EquivalenceHash::compute(b"reg_outer");
        let inner = CompleteTypeObject::Struct(final_struct("RInner"));
        let outer = struct_with_member("ROuter", TypeIdentifier::CompleteTypeId(inner_key));

        let mut registry = TypeRegistry::new();
        registry.register_closure(&[
            (TypeIdentifier::CompleteTypeId(outer_key), TypeObject::Complete(outer.clone())),
            (TypeIdentifier::CompleteTypeId(inner_key), TypeObject::Complete(inner.clone())),
        ]);

        let derived = build_minimal_closure(&[
            (TypeIdentifier::CompleteTypeId(outer_key), outer),
            (TypeIdentifier::CompleteTypeId(inner_key), inner),
        ]);
        let outer_min = derived.iter().find(|(k, ..)| *k == outer_key).unwrap().1;

        assert_eq!(registry.complete_to_minimal.get(&outer_key), Some(&outer_min));
        assert_eq!(registry.minimal_to_complete.get(&outer_min), Some(&outer_key));

        assert!(matches!(
            registry.resolve_type_object(&TypeIdentifier::CompleteTypeId(outer_key)),
            Some(TypeObject::Complete(_))
        ));
        assert!(matches!(
            registry.resolve_type_object(&TypeIdentifier::MinimalTypeId(outer_min)),
            Some(TypeObject::Minimal(_))
        ));
        // A complete-kind request for a hash we only hold as minimal returns the minimal.
        assert!(matches!(
            registry.resolve_type_object(&TypeIdentifier::CompleteTypeId(outer_min)),
            Some(TypeObject::Minimal(_))
        ));
    }

    #[test]
    fn try_derive_minimal_after_dependency_arrives() {
        let inner_key = EquivalenceHash::compute(b"late_inner");
        let outer_key = EquivalenceHash::compute(b"late_outer");
        let inner = CompleteTypeObject::Struct(final_struct("LInner"));
        let outer = struct_with_member("LOuter", TypeIdentifier::CompleteTypeId(inner_key));

        let mut registry = TypeRegistry::new();
        // Outer arrives while its dependency is missing -> complete-only.
        registry.register_type_object_with_id(
            &TypeIdentifier::CompleteTypeId(outer_key),
            TypeObject::Complete(outer),
        );
        assert!(!registry.complete_to_minimal.contains_key(&outer_key));

        // Inner arrives (no deps) -> auto-derives its minimal.
        registry.register_type_object_with_id(
            &TypeIdentifier::CompleteTypeId(inner_key),
            TypeObject::Complete(inner),
        );
        assert!(registry.complete_to_minimal.contains_key(&inner_key));

        // Re-attempt now succeeds.
        assert!(registry.try_derive_minimal(&outer_key));
        assert!(registry.complete_to_minimal.contains_key(&outer_key));
    }

    #[test]
    fn build_minimal_closure_handles_cycle_without_hang() {
        let a_key = EquivalenceHash::compute(b"cycle_a");
        let b_key = EquivalenceHash::compute(b"cycle_b");
        let a = struct_with_member("CycA", TypeIdentifier::CompleteTypeId(b_key));
        let b = struct_with_member("CycB", TypeIdentifier::CompleteTypeId(a_key));

        let derived = build_minimal_closure(&[
            (TypeIdentifier::CompleteTypeId(a_key), a),
            (TypeIdentifier::CompleteTypeId(b_key), b),
        ]);
        assert_eq!(derived.len(), 2);
        // Cyclic member ids are left unrewritten (still CompleteTypeId).
        for (key, _, obj) in &derived {
            match obj {
                MinimalTypeObject::Struct(s) => {
                    let expected = if *key == a_key { b_key } else { a_key };
                    assert_eq!(
                        s.member_seq[0].common.member_type_id,
                        TypeIdentifier::CompleteTypeId(expected)
                    );
                }
                _ => panic!("expected struct"),
            }
        }
    }
}
