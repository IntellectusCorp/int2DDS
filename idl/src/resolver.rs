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
    struct_defs: Vec<(String, StructDef)>,  // (qualified_name, def)
    enum_defs: Vec<(String, EnumDef)>,
    known_types: HashSet<String>,
}

impl Resolver {
    fn new() -> Self {
        Self {
            typedefs: HashMap::new(),
            struct_defs: Vec::new(),
            enum_defs: Vec::new(),
            known_types: HashSet::new(),
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
                    let new_scope = if scope.is_empty() {
                        m.name.clone()
                    } else {
                        format!("{}::{}", scope, m.name)
                    };
                    self.collect_definitions(&m.definitions, &new_scope)?;
                }
                Definition::Struct(s) => {
                    let qname = if scope.is_empty() {
                        s.name.clone()
                    } else {
                        format!("{}::{}", scope, s.name)
                    };
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(s.name.clone()); // short name too
                    self.struct_defs.push((qname, s.clone()));
                }
                Definition::Enum(e) => {
                    let qname = if scope.is_empty() {
                        e.name.clone()
                    } else {
                        format!("{}::{}", scope, e.name)
                    };
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(e.name.clone());
                    self.enum_defs.push((qname, e.clone()));
                }
                Definition::Typedef(td) => {
                    let qname = if scope.is_empty() {
                        td.name.clone()
                    } else {
                        format!("{}::{}", scope, td.name)
                    };
                    self.typedefs.insert(qname, td.type_spec.clone());
                    self.typedefs.insert(td.name.clone(), td.type_spec.clone());
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

        let mut structs = Vec::new();
        for (qname, sdef) in &self.struct_defs {
            structs.push(self.resolve_struct(qname, sdef)?);
        }

        // Topological sort structs by dependencies
        structs = self.topo_sort_structs(structs);

        Ok(IdlModel { structs, enums })
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
            variants.push(ResolvedEnumVariant {
                name: v.name.clone(),
                value,
            });
        }

        Ok(ResolvedEnum {
            name: edef.name.clone(),
            qualified_name: qname.to_string(),
            variants,
        })
    }

    fn resolve_struct(
        &self,
        qname: &str,
        sdef: &StructDef,
    ) -> Result<ResolvedStruct, ResolveError> {
        let extensibility = self.extract_extensibility(&sdef.annotations)?;

        let mut members = Vec::new();
        for m in &sdef.members {
            members.push(self.resolve_member(m)?);
        }

        Ok(ResolvedStruct {
            name: sdef.name.clone(),
            qualified_name: qname.to_string(),
            extensibility,
            members,
        })
    }

    fn resolve_member(&self, m: &StructMember) -> Result<ResolvedMember, ResolveError> {
        let resolved_type = self.resolve_type_spec(&m.type_spec)?;
        let is_key = m.annotations.iter().any(|a| a.name == "key");
        let is_optional = m.annotations.iter().any(|a| a.name == "optional");
        let must_understand = m.annotations.iter().any(|a| a.name == "must_understand");

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

        Ok(ResolvedMember {
            name: m.name.clone(),
            resolved_type,
            is_key,
            member_id,
            is_optional,
            must_understand,
        })
    }

    fn resolve_type_spec(&self, ts: &TypeSpec) -> Result<ResolvedType, ResolveError> {
        match ts {
            TypeSpec::Boolean => Ok(ResolvedType::Bool),
            TypeSpec::Octet => Ok(ResolvedType::U8),
            TypeSpec::Char => Ok(ResolvedType::Char),
            TypeSpec::Int16 => Ok(ResolvedType::I16),
            TypeSpec::Uint16 => Ok(ResolvedType::U16),
            TypeSpec::Int32 => Ok(ResolvedType::I32),
            TypeSpec::Uint32 => Ok(ResolvedType::U32),
            TypeSpec::Int64 => Ok(ResolvedType::I64),
            TypeSpec::Uint64 => Ok(ResolvedType::U64),
            TypeSpec::Float32 => Ok(ResolvedType::F32),
            TypeSpec::Float64 => Ok(ResolvedType::F64),
            TypeSpec::String(bound) => Ok(ResolvedType::String { bound: *bound }),
            TypeSpec::Sequence(elem, bound) => Ok(ResolvedType::Sequence {
                element: Box::new(self.resolve_type_spec(elem)?),
                bound: *bound,
            }),
            TypeSpec::Array(elem, size) => Ok(ResolvedType::Array {
                element: Box::new(self.resolve_type_spec(elem)?),
                size: *size,
            }),
            TypeSpec::Named(name) => {
                // Check typedef first
                if let Some(target) = self.typedefs.get(name) {
                    return self.resolve_type_spec(target);
                }
                // Check known struct/enum
                if self.known_types.contains(name) {
                    // Determine if it's an enum or struct
                    if self.enum_defs.iter().any(|(q, _)| q == name || q.ends_with(&format!("::{}", name))) {
                        Ok(ResolvedType::Enum(name.clone()))
                    } else {
                        Ok(ResolvedType::Struct(name.clone()))
                    }
                } else {
                    Err(ResolveError {
                        message: format!("unresolved type: '{}'", name),
                    })
                }
            }
        }
    }

    fn extract_extensibility(
        &self,
        annotations: &[Annotation],
    ) -> Result<ExtensibilityKind, ResolveError> {
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
                        "FINAL" => Ok(ExtensibilityKind::Final),
                        "APPENDABLE" => Ok(ExtensibilityKind::Appendable),
                        "MUTABLE" => Ok(ExtensibilityKind::Mutable),
                        _ => Err(ResolveError {
                            message: format!("unknown extensibility: '{}'", value),
                        }),
                    };
                }
            }
            // Shorthand: @final, @appendable, @mutable
            match ann.name.to_lowercase().as_str() {
                "final" => return Ok(ExtensibilityKind::Final),
                "appendable" => return Ok(ExtensibilityKind::Appendable),
                "mutable" => return Ok(ExtensibilityKind::Mutable),
                _ => {}
            }
        }
        Ok(ExtensibilityKind::default())
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
        assert_eq!(s.extensibility, ExtensibilityKind::Appendable);
        assert_eq!(s.members.len(), 2);
        assert!(matches!(s.members[0].resolved_type, ResolvedType::U32));
        assert!(matches!(
            s.members[1].resolved_type,
            ResolvedType::String { bound: None }
        ));
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
}
