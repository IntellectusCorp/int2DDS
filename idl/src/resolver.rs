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

/// Compute member_id from a name using MD5 first 4 bytes (little-endian) masked to 28 bits.
/// Must match `int2dds_derive::codegen::utils::compute_member_id_hash` bit-for-bit so that
/// derive-macro output and IDL-generator output produce identical wire IDs.
pub fn compute_member_id_hash(name: &str) -> u32 {
    let digest = md5::compute(name.as_bytes());
    let bytes: [u8; 4] = [digest[0], digest[1], digest[2], digest[3]];
    u32::from_le_bytes(bytes) & 0x0FFF_FFFF
}

/// First positional argument, or the `value` named argument.
/// Covers both `@ann(x)` and the OMG standard named form `@ann(value=x)`.
fn annotation_value(ann: &Annotation) -> Option<&ConstExpr> {
    ann.params.iter().find_map(|p| match p {
        AnnotationParam::Positional(e) => Some(e),
        AnnotationParam::Named(k, e) => (k == "value").then_some(e),
    })
}

/// Lower a parsed constant expression to a resolved constant value.
fn const_value(expr: &ConstExpr) -> ConstValue {
    match expr {
        ConstExpr::Int(v) => ConstValue::Int(*v),
        ConstExpr::Float(v) => ConstValue::Float(*v),
        ConstExpr::String(v) => ConstValue::Str(v.clone()),
        ConstExpr::Bool(v) => ConstValue::Bool(*v),
        ConstExpr::Ident(v) => ConstValue::Ident(v.clone()),
    }
}

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

/// Resolve the full translation unit `all` (root file plus everything it
/// `#include`s) but emit only the types and constants declared directly in
/// `root`. Included definitions still populate the symbol table so cross-file
/// references resolve; they are dropped from the returned model so each file's
/// output stays self-contained and free of duplicate/colliding definitions.
pub fn resolve_scoped(root: &[Definition], all: Vec<Definition>) -> Result<IdlModel, ResolveError> {
    let keep = declared_qualified_names(root)?;
    let mut model = resolve(all)?;
    model.retain_qualified(&keep);
    Ok(model)
}

/// Qualified names of every emittable definition declared in `defs`.
fn declared_qualified_names(defs: &[Definition]) -> Result<HashSet<String>, ResolveError> {
    let mut r = Resolver::new();
    r.collect_definitions(defs, "")?;
    let mut keep = HashSet::new();
    keep.extend(r.struct_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.enum_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.bitmask_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.bitset_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.union_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.interface_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.exception_defs.iter().map(|(q, _)| q.clone()));
    keep.extend(r.const_defs.iter().map(|(q, _)| q.clone()));
    Ok(keep)
}

struct Resolver {
    typedefs: HashMap<String, TypeSpec>,
    struct_defs: Vec<(String, StructDef)>, // (qualified_name, def)
    enum_defs: Vec<(String, EnumDef)>,
    bitmask_defs: Vec<(String, BitmaskDef)>,
    bitset_defs: Vec<(String, BitsetDef)>,
    union_defs: Vec<(String, UnionDef)>,
    interface_defs: Vec<(String, InterfaceDef)>,
    exception_defs: Vec<(String, ExceptionDef)>,
    const_defs: Vec<(String, ConstDef)>,
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
            interface_defs: Vec::new(),
            exception_defs: Vec::new(),
            const_defs: Vec::new(),
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
                Definition::Interface(iface) => {
                    let qname = Self::qualified_name(scope, &iface.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(iface.name.clone());
                    self.interface_defs.push((qname, iface.clone()));
                }
                Definition::Exception(exc) => {
                    let qname = Self::qualified_name(scope, &exc.name);
                    self.known_types.insert(qname.clone());
                    self.known_types.insert(exc.name.clone());
                    self.exception_defs.push((qname, exc.clone()));
                }
                Definition::Const(c) => {
                    let qname = Self::qualified_name(scope, &c.name);
                    self.const_defs.push((qname, c.clone()));
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

        let mut interfaces = Vec::new();
        for (qname, idef) in &self.interface_defs {
            interfaces.push(self.resolve_interface(qname, idef)?);
        }

        let mut exceptions = Vec::new();
        for (qname, edef) in &self.exception_defs {
            exceptions.push(self.resolve_exception(qname, edef)?);
        }

        let mut constants = Vec::new();
        for (qname, cdef) in &self.const_defs {
            constants.push(ResolvedConst {
                name: cdef.name.clone(),
                qualified_name: qname.to_string(),
                resolved_type: self.resolve_type_spec(&cdef.type_spec)?,
                value: const_value(&cdef.value),
            });
        }

        Ok(IdlModel {
            structs,
            enums,
            bitmasks,
            bitsets,
            unions,
            interfaces,
            exceptions,
            constants,
        })
    }

    fn resolve_enum(&self, qname: &str, edef: &EnumDef) -> Result<ResolvedEnum, ResolveError> {
        let mut variants = Vec::new();
        let mut next_value: i32 = 0;

        for v in &edef.variants {
            // Support both `= N` inline syntax and `@value(N)` annotation
            let explicit = v.value.or_else(|| self.extract_value(&v.annotations));
            let value = if let Some(explicit) = explicit {
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

    fn resolve_bitmask(
        &self,
        qname: &str,
        bdef: &BitmaskDef,
    ) -> Result<ResolvedBitmask, ResolveError> {
        // Extract @bit_bound from annotations (default 32)
        let bit_bound = self.extract_bit_bound(&bdef.annotations)?.unwrap_or(32);

        let mut flags = Vec::new();
        let mut next_position: u32 = 0;

        for flag in &bdef.flags {
            let position = self.extract_position(&flag.annotations)?.unwrap_or(next_position);
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

    fn resolve_bitset(
        &self,
        qname: &str,
        bdef: &BitsetDef,
    ) -> Result<ResolvedBitset, ResolveError> {
        let mut fields = Vec::new();
        let mut total_bits: u32 = 0;

        for f in &bdef.fields {
            total_bits += f.bit_width;
            fields.push(ResolvedBitsetField { name: f.name.clone(), bit_width: f.bit_width });
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
                member: ResolvedUnionCaseMember { name: c.member.name.clone(), resolved_type },
            });
        }

        let default_case = if let Some(dc) = &udef.default_case {
            let resolved_type = self.resolve_type_spec(&dc.type_spec)?;
            Some(ResolvedUnionCaseMember { name: dc.name.clone(), resolved_type })
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
            _ => Err(ResolveError { message: "unsupported union case label type".to_string() }),
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
                return Err(ResolveError { message: format!("unresolved base type: '{}'", base) });
            }
        }

        let mut members = Vec::new();
        for m in &sdef.members {
            members.push(self.resolve_member(m, autoid)?);
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

    fn resolve_member(
        &self,
        m: &StructMember,
        autoid: Option<AutoIdKind>,
    ) -> Result<ResolvedMember, ResolveError> {
        let resolved_type = self.resolve_type_spec(&m.type_spec)?;
        let is_key = m.annotations.iter().any(|a| a.name == "key");
        let is_optional = m.annotations.iter().any(|a| a.name == "optional");
        let must_understand = m.annotations.iter().any(|a| a.name == "must_understand");
        let is_external = m.annotations.iter().any(|a| a.name == "external");

        let explicit_id = m.annotations.iter().find_map(|a| {
            if a.name == "id" {
                match annotation_value(a) {
                    Some(ConstExpr::Int(v)) => Some(*v as u32),
                    _ => None,
                }
            } else {
                None
            }
        });

        let default_value = self.extract_default(&m.annotations);
        let hashid = self.extract_hashid(&m.annotations);

        let member_id = if let Some(id) = explicit_id {
            Some(id)
        } else if let Some(hash_custom) = hashid.as_ref() {
            let hash_input = hash_custom.as_deref().unwrap_or(m.name.as_str());
            Some(compute_member_id_hash(hash_input))
        } else if matches!(autoid, Some(AutoIdKind::Hash)) {
            Some(compute_member_id_hash(&m.name))
        } else {
            None
        };

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
                    if self
                        .enum_defs
                        .iter()
                        .any(|(q, _)| q == name || q.ends_with(&format!("::{}", name)))
                    {
                        Ok(ResolvedType::Enum(name.clone()))
                    } else if self
                        .bitmask_defs
                        .iter()
                        .any(|(q, _)| q == name || q.ends_with(&format!("::{}", name)))
                    {
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
    ) -> Result<ExtensibilityKind, ResolveError> {
        for ann in annotations {
            if ann.name == "extensibility" {
                if let Some(param) = annotation_value(ann) {
                    let value = match param {
                        ConstExpr::Ident(s) => s.as_str(),
                        ConstExpr::String(s) => s.as_str(),
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

    fn extract_autoid(
        &self,
        annotations: &[Annotation],
    ) -> Result<Option<AutoIdKind>, ResolveError> {
        for ann in annotations {
            if ann.name == "autoid" {
                if let Some(param) = annotation_value(ann) {
                    let value = match param {
                        ConstExpr::Ident(s) => s.as_str(),
                        ConstExpr::String(s) => s.as_str(),
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
                if let Some(ConstExpr::Int(v)) = annotation_value(ann) {
                    return Ok(Some(*v as u32));
                }
            }
        }
        Ok(None)
    }

    fn extract_value(&self, annotations: &[Annotation]) -> Option<i64> {
        for ann in annotations {
            if ann.name == "value" {
                if let Some(ConstExpr::Int(v)) = annotation_value(ann) {
                    return Some(*v);
                }
            }
        }
        None
    }

    fn extract_position(&self, annotations: &[Annotation]) -> Result<Option<u32>, ResolveError> {
        for ann in annotations {
            if ann.name == "position" {
                if let Some(ConstExpr::Int(v)) = annotation_value(ann) {
                    return Ok(Some(*v as u32));
                }
            }
        }
        Ok(None)
    }

    fn extract_default(&self, annotations: &[Annotation]) -> Option<ConstValue> {
        for ann in annotations {
            if ann.name == "default" {
                return Some(const_value(annotation_value(ann)?));
            }
        }
        None
    }

    fn extract_hashid(&self, annotations: &[Annotation]) -> Option<Option<String>> {
        for ann in annotations {
            if ann.name == "hashid" {
                return if let Some(ConstExpr::String(name)) = annotation_value(ann) {
                    Some(Some(name.clone()))
                } else {
                    Some(None) // bare @hashid -> hash field name
                };
            }
        }
        None
    }

    fn resolve_interface(
        &self,
        qname: &str,
        idef: &InterfaceDef,
    ) -> Result<ResolvedInterface, ResolveError> {
        let mut operations = Vec::new();
        for op in &idef.operations {
            let return_type = match &op.return_type {
                Some(ts) => Some(self.resolve_type_spec(ts)?),
                None => None,
            };
            let mut params = Vec::new();
            for p in &op.params {
                params.push(ResolvedParam {
                    name: p.name.clone(),
                    resolved_type: self.resolve_type_spec(&p.type_spec)?,
                    direction: match p.direction {
                        ParamDirection::In => ResolvedParamDirection::In,
                        ParamDirection::Out => ResolvedParamDirection::Out,
                        ParamDirection::Inout => ResolvedParamDirection::Inout,
                    },
                });
            }
            operations.push(ResolvedOperation {
                name: op.name.clone(),
                return_type,
                params,
                raises: op.raises.clone(),
            });
        }

        let mut attributes = Vec::new();
        for attr in &idef.attributes {
            attributes.push(ResolvedAttribute {
                name: attr.name.clone(),
                resolved_type: self.resolve_type_spec(&attr.type_spec)?,
                readonly: attr.readonly,
                raises: attr.raises.clone(),
            });
        }

        Ok(ResolvedInterface {
            name: idef.name.clone(),
            qualified_name: qname.to_string(),
            base_interfaces: idef.base_interfaces.clone(),
            operations,
            attributes,
        })
    }

    fn resolve_exception(
        &self,
        qname: &str,
        edef: &ExceptionDef,
    ) -> Result<ResolvedException, ResolveError> {
        let mut members = Vec::new();
        for m in &edef.members {
            members.push(self.resolve_member(m, None)?);
        }
        Ok(ResolvedException {
            name: edef.name.clone(),
            qualified_name: qname.to_string(),
            members,
        })
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
                Self::collect_struct_deps(&m.resolved_type, &name_to_idx, i, &mut deps);
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
                Self::collect_struct_deps(element, name_to_idx, from, deps);
            }
            ResolvedType::Map { key, value, .. } => {
                Self::collect_struct_deps(key, name_to_idx, from, deps);
                Self::collect_struct_deps(value, name_to_idx, from, deps);
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
    fn test_resolve_enum_value_annotation() {
        let defs = parse_idl(
            r#"
            enum Priority {
                LOW,
                @value(5) MEDIUM,
                @value(10) HIGH,
                @value(100) CRITICAL
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.enums.len(), 1);
        let e = &model.enums[0];
        assert_eq!(e.variants[0].value, 0); // LOW (auto)
        assert_eq!(e.variants[1].value, 5); // @value(5)
        assert_eq!(e.variants[2].value, 10); // @value(10)
        assert_eq!(e.variants[3].value, 100); // @value(100)
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
    fn test_resolve_default_named_param() {
        // rosidl emits the OMG named form `@default(value=X)`.
        let defs = parse_idl(
            r#"
            struct Data {
                @default(value=42) long count;
            };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert!(matches!(model.structs[0].members[0].default_value, Some(ConstValue::Int(42))));
    }

    #[test]
    fn test_resolve_constants() {
        let defs = parse_idl(
            r#"
            module pkg { module msg {
                module T_Constants {
                    const int8 STATUS_NO_FIX = -1;
                    const uint8 STATUS_FIX = 0;
                    const string MODE = "auto";
                };
            }; };
            "#,
        )
        .unwrap();
        let model = resolve(defs).unwrap();
        assert_eq!(model.constants.len(), 3);
        let no_fix = model.constants.iter().find(|c| c.name == "STATUS_NO_FIX").unwrap();
        assert_eq!(no_fix.qualified_name, "pkg::msg::T_Constants::STATUS_NO_FIX");
        assert!(matches!(no_fix.resolved_type, ResolvedType::I8));
        assert!(matches!(no_fix.value, ConstValue::Int(-1)));
        let mode = model.constants.iter().find(|c| c.name == "MODE").unwrap();
        assert!(matches!(&mode.value, ConstValue::Str(s) if s == "auto"));
    }

    #[test]
    fn test_resolve_scoped_emits_only_root() {
        // Simulates `#include`: `included` is the dependency's definitions,
        // `root` references it. Only the root's own types/constants are emitted.
        let included = parse_idl(
            "module dep { module msg {
                struct Header { uint32 stamp; };
                const long DEP_OK = 1;
            }; };",
        )
        .unwrap();
        let root = parse_idl(
            "module app { module msg {
                struct Msg { dep::msg::Header header; long x; };
                const long APP_OK = 2;
            }; };",
        )
        .unwrap();

        let mut all = included.clone();
        all.extend(root.clone());

        let model = resolve_scoped(&root, all).unwrap();

        // Root type emitted; included Header dropped, but still resolved as a member.
        assert_eq!(model.structs.len(), 1);
        let msg = &model.structs[0];
        assert_eq!(msg.qualified_name, "app::msg::Msg");
        assert!(matches!(
            &msg.members[0].resolved_type,
            ResolvedType::Struct(n) if n.ends_with("Header")
        ));

        // Only the root constant survives.
        assert_eq!(model.constants.len(), 1);
        assert_eq!(model.constants[0].qualified_name, "app::msg::APP_OK");
    }

    #[test]
    fn test_resolve_scoped_avoids_leaf_collision() {
        // Two packages declare a same-leaf `Status`; root declares only one.
        let included =
            parse_idl("module dep { module msg { struct Status { long a; }; }; };").unwrap();
        let root = parse_idl("module app { module msg { struct Status { long b; }; }; };").unwrap();
        let mut all = included.clone();
        all.extend(root.clone());

        let model = resolve_scoped(&root, all).unwrap();
        assert_eq!(model.structs.len(), 1);
        assert_eq!(model.structs[0].qualified_name, "app::msg::Status");
    }

    #[test]
    fn test_resolve_scoped_no_includes_is_identity() {
        // With root == all (no includes) nothing is filtered out.
        let defs = parse_idl("struct A { long x; }; struct B { long y; };").unwrap();
        let model = resolve_scoped(&defs, defs.clone()).unwrap();
        assert_eq!(model.structs.len(), 2);
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
        assert!(
            matches!(&s.members[0].resolved_type, ResolvedType::Map { key, value, bound: None }
            if matches!(key.as_ref(), ResolvedType::I32) && matches!(value.as_ref(), ResolvedType::String { bound: None }))
        );
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
