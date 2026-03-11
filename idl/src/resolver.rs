/// Type resolution pass: lowers AST to internal representation (IR).
///
/// - Resolves typedefs by substituting the underlying type
/// - Validates all named type references exist
/// - Auto-assigns enum discriminant values
/// - Extracts annotations into structured fields (is_key, member_id, etc.)
/// - Topologically sorts structs so nested types come first
use std::collections::{HashMap, HashSet};

use crate::parser::ast::*;
use crate::types::*;

#[derive(Debug)]
pub struct ResolveError {
    pub message: String,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub fn resolve(definitions: Vec<Definition>) -> Result<IdlModel, ResolveError> {
    let mut resolver = Resolver::new();
    resolver.collect_definitions(&definitions, "")?;
    resolver.resolve_all()
}

struct Resolver {
    typedefs: HashMap<String, TypeSpec>,
    struct_defs: Vec<(String, StructDef)>, // (qualified_name, def)
    enum_defs: Vec<(String, EnumDef)>,
    bitmask_defs: Vec<(String, BitmaskDef)>,
    bitset_defs: Vec<(String, BitsetDef)>,
    union_defs: Vec<(String, UnionDef)>,
    known_types: HashSet<String>,
}

impl Resolver {
    fn new() -> Self {
        Self {
            typedefs: HashMap::new(),
            struct_defs: Vec::new(),
            enum_defs: Vec::new(),
            bitmask_defs: Vec::new(),
            bitset_defs: Vec::new(),
            union_defs: Vec::new(),
            known_types: HashSet::new(),
        }
    }

    fn qualified_name(scope: &str, name: &str) -> String {
        if scope.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", scope, name)
        }
    }

    /// First pass: collect all type names and definitions.
    fn collect_definitions(
        &mut self,
        defs: &[Definition],
        scope: &str,
    ) -> Result<(), ResolveError> {
        for def in defs {
            match def {
                Definition::Module(m) => {
                    let new_scope = Self::qualified_name(scope, &m.name);
                    self.collect_definitions(&m.definitions, &new_scope)?;
                }
                Definition::Struct(s) => {
                    let qname = Self::qualified_name(scope, &s.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(s.name.clone());
                    self.struct_defs.push((qname, s.clone()));
                }
                Definition::Enum(e) => {
                    let qname = Self::qualified_name(scope, &e.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(e.name.clone());
                    self.enum_defs.push((qname, e.clone()));
                }
                Definition::Typedef(td) => {
                    let qname = Self::qualified_name(scope, &td.name);
                    self.typedefs.insert(qname, td.type_spec.clone());
                    self.typedefs.insert(td.name.clone(), td.type_spec.clone());
                }
                Definition::Bitmask(b) => {
                    let qname = Self::qualified_name(scope, &b.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(b.name.clone());
                    self.bitmask_defs.push((qname, b.clone()));
                }
                Definition::Bitset(b) => {
                    let qname = Self::qualified_name(scope, &b.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(b.name.clone());
                    self.bitset_defs.push((qname, b.clone()));
                }
                Definition::Union(u) => {
                    let qname = Self::qualified_name(scope, &u.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(u.name.clone());
                    self.union_defs.push((qname, u.clone()));
                }
            }
        }
        Ok(())
    }

    /// Second pass: resolve all types.
    fn resolve_all(&self) -> Result<IdlModel, ResolveError> {
        let mut enums = Vec::new();
        for (qname, edef) in &self.enum_defs {
            enums.push(self.resolve_enum(qname, edef)?);
        }

        let mut bitmasks = Vec::new();
        for (qname, bdef) in &self.bitmask_defs {
            bitmasks.push(self.resolve_bitmask(qname, bdef)?);
        }

        let mut bitsets = Vec::new();
        for (qname, bdef) in &self.bitset_defs {
            bitsets.push(self.resolve_bitset(qname, bdef)?);
        }

        let mut unions = Vec::new();
        for (qname, udef) in &self.union_defs {
            unions.push(self.resolve_union(qname, udef)?);
        }

        let mut structs = Vec::new();
        for (qname, sdef) in &self.struct_defs {
            structs.push(self.resolve_struct(qname, sdef)?);
        }

        // Topological sort structs by dependencies
        structs = self.topo_sort_structs(structs);

        Ok(IdlModel { structs, enums, bitmasks, bitsets, unions })
    }

    fn resolve_enum(&self, qname: &str, edef: &EnumDef) -> Result<ResolvedEnum, ResolveError> {
        let mut variants = Vec::new();
        let mut next_value: i32 = 0;

        for v in &edef.variants {
            let value = if let Some(explicit) = v.value {
                let val = explicit as i32;
                next_value = val + 1;
                val
            } else {
                let val = next_value;
                next_value += 1;
                val
            };
            variants.push(ResolvedEnumVariant { name: v.name.clone(), value });
        }

        Ok(ResolvedEnum { name: edef.name.clone(), qualified_name: qname.to_string(), variants })
    }

    fn resolve_bitmask(&self, qname: &str, bdef: &BitmaskDef) -> Result<ResolvedBitmask, ResolveError> {
        // Extract @bit_bound from annotations (default 32)
        let bit_bound = self.extract_bit_bound(&bdef.annotations)?.unwrap_or(32);

        let mut flags = Vec::new();
        let mut next_position: u32 = 0;

        for flag in &bdef.flags {
            let position = self.extract_position(&flag.annotations)?.unwrap_or_else(|| {
                let pos = next_position;
                pos
            });
            if position >= bit_bound {
                return Err(ResolveError {
                    message: format!(
                        "bitmask '{}': flag '{}' position {} exceeds bit_bound {}",
                        bdef.name, flag.name, position, bit_bound
                    ),
                });
            }
            next_position = position + 1;
            flags.push(ResolvedBitmaskFlag { name: flag.name.clone(), position });
        }

        Ok(ResolvedBitmask {
            name: bdef.name.clone(),
            qualified_name: qname.to_string(),
            bit_bound,
            flags,
        })
    }

    fn resolve_bitset(&self, qname: &str, bdef: &BitsetDef) -> Result<ResolvedBitset, ResolveError> {
        let mut fields = Vec::new();
        let mut total_bits: u32 = 0;

        for f in &bdef.fields {
            total_bits += f.bit_width;
            fields.push(ResolvedBitsetField {
                name: f.name.clone(),
                bit_width: f.bit_width,
            });
        }

        if total_bits > 64 {
            return Err(ResolveError {
                message: format!(
                    "bitset '{}': total bits {} exceeds maximum 64",
                    bdef.name, total_bits
                ),
            });
        }

        Ok(ResolvedBitset {
            name: bdef.name.clone(),
            qualified_name: qname.to_string(),
            fields,
            total_bits,
        })
    }

    fn resolve_union(&self, qname: &str, udef: &UnionDef) -> Result<ResolvedUnion, ResolveError> {
        let discriminant_type = self.resolve_type_spec(&udef.discriminant_type)?;
        let extensibility = self.extract_extensibility(&udef.annotations)?;

        let mut cases = Vec::new();
        for c in &udef.cases {
            let mut labels = Vec::new();
            for label in &c.labels {
                labels.push(self.resolve_union_label(label)?);
            }
            let resolved_type = self.resolve_type_spec(&c.member.type_spec)?;
            cases.push(ResolvedUnionCase {
                labels,
                member: ResolvedUnionCaseMember {
                    name: c.member.name.clone(),
                    resolved_type,
                },
            });
        }

        let default_case = if let Some(dc) = &udef.default_case {
            let resolved_type = self.resolve_type_spec(&dc.type_spec)?;
            Some(ResolvedUnionCaseMember {
                name: dc.name.clone(),
                resolved_type,
            })
        } else {
            None
        };

        Ok(ResolvedUnion {
            name: udef.name.clone(),
            qualified_name: qname.to_string(),
            discriminant_type,
            cases,
            default_case,
            extensibility,
        })
    }

    fn resolve_union_label(&self, expr: &ConstExpr) -> Result<ResolvedUnionLabel, ResolveError> {
        match expr {
            ConstExpr::Int(v) => Ok(ResolvedUnionLabel::Int(*v)),
            ConstExpr::Bool(v) => Ok(ResolvedUnionLabel::Bool(*v)),
            ConstExpr::Ident(name) => Ok(ResolvedUnionLabel::Ident(name.clone())),
            _ => Err(ResolveError {
                message: "unsupported union case label type".to_string(),
            }),
        }
    }

    fn resolve_struct(
        &self,
        qname: &str,
        sdef: &StructDef,
    ) -> Result<ResolvedStruct, ResolveError> {
        let extensibility = self.extract_extensibility(&sdef.annotations)?;
        let autoid = self.extract_autoid(&sdef.annotations)?;

        // Validate base_type exists if specified
        if let Some(base) = &sdef.base_type {
            if !self.known_types.contains(base) {
                return Err(ResolveError {
                    message: format!("unresolved base type: '{}'", base),
                });
            }
        }

        let mut members = Vec::new();
        for m in &sdef.members {
            members.push(self.resolve_member(m)?);
        }

        Ok(ResolvedStruct {
            name: sdef.name.clone(),
            qualified_name: qname.to_string(),
            extensibility,
            autoid,
            base_type: sdef.base_type.clone(),
            members,
        })
    }

    fn resolve_member(&self, m: &StructMember) -> Result<ResolvedMember, ResolveError> {
        let resolved_type = self.resolve_type_spec(&m.type_spec)?;
        let is_key = m.annotations.iter().any(|a| a.name == "key");
        let is_optional = m.annotations.iter().any(|a| a.name == "optional");
        let must_understand = m.annotations.iter().any(|a| a.name == "must_understand");
        let is_external = m.annotations.iter().any(|a| a.name == "external");

        let member_id = m.annotations.iter().find_map(|a| {
            if a.name == "id" {
                a.params.first().and_then(|p| match p {
                    AnnotationParam::Positional(ConstExpr::Int(v)) => Some(*v as u32),
                    _ => None,
                })
            } else {
                None
            }
        });

        let default_value = self.extract_default(&m.annotations);
        let hashid = self.extract_hashid(&m.annotations);

        Ok(ResolvedMember {
            name: m.name.clone(),
            resolved_type,
            is_key,
            member_id,
            is_optional,
            must_understand,
            is_external,
            default_value,
            hashid,
        })
    }

    fn resolve_type_spec(&self, ts: &TypeSpec) -> Result<ResolvedType, ResolveError> {
        match ts {
            TypeSpec::Boolean => Ok(ResolvedType::Bool),
            TypeSpec::Octet => Ok(ResolvedType::U8),
            TypeSpec::Char => Ok(ResolvedType::Char),
            TypeSpec::WChar => Ok(ResolvedType::WChar),
            TypeSpec::Int16 => Ok(ResolvedType::I16),
            TypeSpec::Uint16 => Ok(ResolvedType::U16),
            TypeSpec::Int32 => Ok(ResolvedType::I32),
            TypeSpec::Uint32 => Ok(ResolvedType::U32),
            TypeSpec::Int64 => Ok(ResolvedType::I64),
            TypeSpec::Uint64 => Ok(ResolvedType::U64),
            TypeSpec::Float32 => Ok(ResolvedType::F32),
            TypeSpec::Float64 => Ok(ResolvedType::F64),
            TypeSpec::String(bound) => Ok(ResolvedType::String { bound: *bound }),
            TypeSpec::WString(bound) => Ok(ResolvedType::WString { bound: *bound }),
            TypeSpec::Sequence(elem, bound) => Ok(ResolvedType::Sequence {
                element: Box::new(self.resolve_type_spec(elem)?),
                bound: *bound,
            }),
            TypeSpec::Array(elem, size) => Ok(ResolvedType::Array {
                element: Box::new(self.resolve_type_spec(elem)?),
                size: *size,
            }),
            TypeSpec::Map(key, value, bound) => Ok(ResolvedType::Map {
                key: Box::new(self.resolve_type_spec(key)?),
                value: Box::new(self.resolve_type_spec(value)?),
                bound: *bound,
            }),
            TypeSpec::Named(name) => {
                // OMG IDL 4.2 integer type aliases
                match name.as_str() {
                    "int8" => return Ok(ResolvedType::I8),
                    "uint8" => return Ok(ResolvedType::U8),
                    "int16" => return Ok(ResolvedType::I16),
                    "uint16" => return Ok(ResolvedType::U16),
                    "int32" => return Ok(ResolvedType::I32),
                    "uint32" => return Ok(ResolvedType::U32),
                    "int64" => return Ok(ResolvedType::I64),
                    "uint64" => return Ok(ResolvedType::U64),
                    _ => {}
                }
                // Check typedef first
                if let Some(target) = self.typedefs.get(name) {
                    return self.resolve_type_spec(target);
                }
                // Check known struct/enum/bitmask
                if self.known_types.contains(name) {
                    if self.enum_defs.iter().any(|(q, _)| q == name || q.ends_with(&format!("::{}", name))) {
                        Ok(ResolvedType::Enum(name.clone()))
                    } else if self.bitmask_defs.iter().any(|(q, _)| q == name || q.ends_with(&format!("::{}", name))) {
                        Ok(ResolvedType::Bitmask(name.clone()))
                    } else {
                        Ok(ResolvedType::Struct(name.clone()))
                    }
                } else {
                    Err(ResolveError { message: format!("unresolved type: '{}'", name) })
                }
            }
        }
    }

    fn extract_extensibility(
        &self,
        annotations: &[Annotation],
    ) -> Result<Option<ExtensibilityKind>, ResolveError> {
        for ann in annotations {
            if ann.name == "extensibility" {
                if let Some(param) = ann.params.first() {
                    let value = match param {
                        AnnotationParam::Positional(ConstExpr::Ident(s)) => s.as_str(),
                        AnnotationParam::Positional(ConstExpr::String(s)) => s.as_str(),
                        _ => {
                            return Err(ResolveError {
                                message: "invalid @extensibility parameter".to_string(),
                            })
                        }
                    };
                    return match value.to_uppercase().as_str() {
                        "FINAL" => Ok(Some(ExtensibilityKind::Final)),
                        "APPENDABLE" => Ok(Some(ExtensibilityKind::Appendable)),
                        "MUTABLE" => Ok(Some(ExtensibilityKind::Mutable)),
                        _ => Err(ResolveError {
                            message: format!("unknown extensibility: '{}'", value),
                        }),
                    };
                }
            }
            // Shorthand: @final, @appendable, @mutable
            match ann.name.to_lowercase().as_str() {
                "final" => return Ok(Some(ExtensibilityKind::Final)),
                "appendable" => return Ok(Some(ExtensibilityKind::Appendable)),
                "mutable" => return Ok(Some(ExtensibilityKind::Mutable)),
                _ => {}
            }
        }
        Ok(None)
    }

    fn extract_autoid(&self, annotations: &[Annotation]) -> Result<Option<AutoIdKind>, ResolveError> {
        for ann in annotations {
            if ann.name == "autoid" {
                if let Some(param) = ann.params.first() {
                    let value = match param {
                        AnnotationParam::Positional(ConstExpr::Ident(s)) => s.as_str(),
                        AnnotationParam::Positional(ConstExpr::String(s)) => s.as_str(),
                        _ => {
                            return Err(ResolveError {
                                message: "invalid @autoid parameter".to_string(),
                            })
                        }
                    };
                    return match value.to_uppercase().as_str() {
                        "HASH" => Ok(Some(AutoIdKind::Hash)),
                        "SEQUENTIAL" => Ok(Some(AutoIdKind::Sequential)),
                        _ => Err(ResolveError {
                            message: format!("unknown autoid kind: '{}'", value),
                        }),
                    };
                }
                // bare @autoid defaults to Hash
                return Ok(Some(AutoIdKind::Hash));
            }
        }
        Ok(None)
    }

    fn extract_bit_bound(&self, annotations: &[Annotation]) -> Result<Option<u32>, ResolveError> {
        for ann in annotations {
            if ann.name == "bit_bound" {
                if let Some(AnnotationParam::Positional(ConstExpr::Int(v))) = ann.params.first() {
                    return Ok(Some(*v as u32));
                }
            }
        }
        Ok(None)
    }

    fn extract_position(&self, annotations: &[Annotation]) -> Result<Option<u32>, ResolveError> {
        for ann in annotations {
            if ann.name == "position" {
                if let Some(AnnotationParam::Positional(ConstExpr::Int(v))) = ann.params.first() {
                    return Ok(Some(*v as u32));
                }
            }
        }
        Ok(None)
    }

    fn extract_default(&self, annotations: &[Annotation]) -> Option<ConstValue> {
        for ann in annotations {
            if ann.name == "default" {
                if let Some(param) = ann.params.first() {
                    return match param {
                        AnnotationParam::Positional(ConstExpr::Int(v)) => Some(ConstValue::Int(*v)),
                        AnnotationParam::Positional(ConstExpr::Float(v)) => Some(ConstValue::Float(*v)),
                        AnnotationParam::Positional(ConstExpr::String(v)) => Some(ConstValue::Str(v.clone())),
                        AnnotationParam::Positional(ConstExpr::Bool(v)) => Some(ConstValue::Bool(*v)),
                        AnnotationParam::Positional(ConstExpr::Ident(v)) => Some(ConstValue::Ident(v.clone())),
                        _ => None,
                    };
                }
            }
        }
        None
    }

    fn extract_hashid(&self, annotations: &[Annotation]) -> Option<Option<String>> {
        for ann in annotations {
            if ann.name == "hashid" {
                return if let Some(AnnotationParam::Positional(ConstExpr::String(name))) = ann.params.first() {
                    Some(Some(name.clone()))
                } else {
                    Some(None) // bare @hashid -> hash field name
                };
            }
        }
        None
    }

    /// Topological sort: structs that depend on other structs come after their dependencies.
    fn topo_sort_structs(&self, structs: Vec<ResolvedStruct>) -> Vec<ResolvedStruct> {
        let name_to_idx: HashMap<&str, usize> = structs
            .iter()
            .enumerate()
            .flat_map(|(i, s)| {
                let mut entries = vec![(s.name.as_str(), i)];
                entries.push((s.qualified_name.as_str(), i));
                entries
            })
            .collect();

        let n = structs.len();
        let mut deps: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (i, s) in structs.iter().enumerate() {
            for m in &s.members {
                self.collect_struct_deps(&m.resolved_type, &name_to_idx, i, &mut deps);
            }
            // base_type dependency
            if let Some(base) = &s.base_type {
                if let Some(&dep_idx) = name_to_idx.get(base.as_str()) {
                    if dep_idx != i {
                        deps[i].push(dep_idx);
                    }
                }
            }
        }

        // Kahn's algorithm
        let mut in_degree = vec![0usize; n];
        for d in &deps {
            for &dep in d {
                in_degree[dep] += 1;
            }
        }

        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::new();

        while let Some(node) = queue.pop() {
            order.push(node);
            for &dep in &deps[node] {
                in_degree[dep] -= 1;
                if in_degree[dep] == 0 {
                    queue.push(dep);
                }
            }
        }

        // If cycle detected, just use original order
        if order.len() != n {
            return structs;
        }

        order.reverse();
        order.into_iter().map(|i| structs[i].clone()).collect()
    }

    fn collect_struct_deps(
        &self,
        ty: &ResolvedType,
        name_to_idx: &HashMap<&str, usize>,
        from: usize,
        deps: &mut [Vec<usize>],
    ) {
        match ty {
            ResolvedType::Struct(name) => {
                if let Some(&dep_idx) = name_to_idx.get(name.as_str()) {
                    if dep_idx != from {
                        deps[from].push(dep_idx);
                    }
                }
            }
            ResolvedType::Sequence { element, .. } | ResolvedType::Array { element, .. } => {
                self.collect_struct_deps(element, name_to_idx, from, deps);
            }
            ResolvedType::Map { key, value, .. } => {
                self.collect_struct_deps(key, name_to_idx, from, deps);
                self.collect_struct_deps(value, name_to_idx, from, deps);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_idl;

    #[test]
    fn test_resolve_hello_world() {
        let defs = parse_idl(
            r#"
            @extensibility(APPENDABLE)
            struct HelloWorld {
                unsigned long index;
                string message;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.structs.len(), 1);
        let s = &model.structs[0];
        assert_eq!(s.name, "HelloWorld");
        assert_eq!(s.extensibility, Some(ExtensibilityKind::Appendable));
        assert_eq!(s.members.len(), 2);
        assert!(matches!(s.members[0].resolved_type, ResolvedType::U32));
        assert!(matches!(s.members[1].resolved_type, ResolvedType::String { bound: None }));
    }

    #[test]
    fn test_resolve_enum_values() {
        let defs = parse_idl("enum Color { RED, GREEN = 5, BLUE };").unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.enums.len(), 1);
        let e = &model.enums[0];
        assert_eq!(e.variants[0].value, 0);
        assert_eq!(e.variants[1].value, 5);
        assert_eq!(e.variants[2].value, 6);
    }

    #[test]
    fn test_resolve_typedef() {
        let defs = parse_idl(
            r#"
            typedef sequence<long> IntList;
            struct Data {
                IntList values;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let s = &model.structs[0];
        assert!(matches!(
            &s.members[0].resolved_type,
            ResolvedType::Sequence { element, bound: None }
                if matches!(element.as_ref(), ResolvedType::I32)
        ));
    }

    #[test]
    fn test_resolve_key_and_id() {
        let defs = parse_idl(
            r#"
            @extensibility(MUTABLE)
            struct Keyed {
                @key @id(0) long id;
                @id(1) string name;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let s = &model.structs[0];
        assert!(s.members[0].is_key);
        assert_eq!(s.members[0].member_id, Some(0));
        assert!(!s.members[1].is_key);
        assert_eq!(s.members[1].member_id, Some(1));
    }

    #[test]
    fn test_resolve_wstring() {
        let defs = parse_idl(
            r#"
            struct WideData {
                wchar wc;
                wstring ws;
                wstring<128> bounded_ws;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let s = &model.structs[0];
        assert!(matches!(s.members[0].resolved_type, ResolvedType::WChar));
        assert!(matches!(s.members[1].resolved_type, ResolvedType::WString { bound: None }));
        assert!(matches!(s.members[2].resolved_type, ResolvedType::WString { bound: Some(128) }));
    }

    #[test]
    fn test_resolve_external() {
        let defs = parse_idl(
            r#"
            struct Data {
                @external long large_data;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert!(model.structs[0].members[0].is_external);
    }

    #[test]
    fn test_resolve_default() {
        let defs = parse_idl(
            r#"
            struct Data {
                @default(42) long count;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert!(matches!(model.structs[0].members[0].default_value, Some(ConstValue::Int(42))));
    }

    #[test]
    fn test_resolve_hashid() {
        let defs = parse_idl(
            r#"
            struct Data {
                @hashid long field_a;
                @hashid("custom") long field_b;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.structs[0].members[0].hashid, Some(None));
        assert_eq!(model.structs[0].members[1].hashid, Some(Some("custom".to_string())));
    }

    #[test]
    fn test_resolve_autoid() {
        let defs = parse_idl(
            r#"
            @autoid(HASH)
            struct Data {
                long x;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.structs[0].autoid, Some(AutoIdKind::Hash));
    }

    #[test]
    fn test_resolve_inheritance() {
        let defs = parse_idl(
            r#"
            struct Base {
                long x;
            };
            struct Derived : Base {
                long y;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let derived = model.structs.iter().find(|s| s.name == "Derived").unwrap();
        assert_eq!(derived.base_type, Some("Base".to_string()));
    }

    #[test]
    fn test_resolve_map() {
        let defs = parse_idl(
            r#"
            struct MapData {
                map<long, string> lookup;
                map<string, double, 100> bounded_map;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        let s = &model.structs[0];
        assert!(matches!(&s.members[0].resolved_type, ResolvedType::Map { key, value, bound: None }
            if matches!(key.as_ref(), ResolvedType::I32) && matches!(value.as_ref(), ResolvedType::String { bound: None })));
        assert!(matches!(&s.members[1].resolved_type, ResolvedType::Map { bound: Some(100), .. }));
    }

    #[test]
    fn test_resolve_bitmask() {
        let defs = parse_idl(
            r#"
            @bit_bound(8)
            bitmask MyFlags {
                FLAG_A,
                @position(3) FLAG_B,
                FLAG_C
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.bitmasks.len(), 1);
        let b = &model.bitmasks[0];
        assert_eq!(b.bit_bound, 8);
        assert_eq!(b.flags[0].position, 0);
        assert_eq!(b.flags[1].position, 3);
        assert_eq!(b.flags[2].position, 4);
    }

    #[test]
    fn test_resolve_bitset() {
        let defs = parse_idl(
            r#"
            bitset MyBitset {
                bitfield<3> field_a;
                bitfield<5> field_b;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.bitsets.len(), 1);
        let b = &model.bitsets[0];
        assert_eq!(b.total_bits, 8);
        assert_eq!(b.fields[0].bit_width, 3);
        assert_eq!(b.fields[1].bit_width, 5);
    }

    #[test]
    fn test_resolve_union() {
        let defs = parse_idl(
            r#"
            union MyUnion switch(long) {
                case 0: long int_val;
                case 1:
                case 2: string str_val;
                default: octet default_val;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.unions.len(), 1);
        let u = &model.unions[0];
        assert!(matches!(u.discriminant_type, ResolvedType::I32));
        assert_eq!(u.cases.len(), 2);
        assert_eq!(u.cases[0].labels.len(), 1);
        assert_eq!(u.cases[1].labels.len(), 2);
        assert!(u.default_case.is_some());
    }
}
