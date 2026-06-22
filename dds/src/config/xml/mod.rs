//! XML type representation (XTypes 7.3.2) loading.
//!
//! Loads types defined in XML at runtime and exposes them as
//! [`DynamicTypeSupport`] for use with the dynamic pub-sub API.

mod ast;
mod convert;
mod parser;

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::xtypes::{CompleteTypeObject, DynamicTypeSupport, TypeObject, TypeRegistry};

use ast::{XmlCaseLabel, XmlMemberType, XmlStruct, XmlTypeDecl};

pub struct XmlTypeRegistry {
    registry: TypeRegistry,
    names: Vec<String>,
    typedefs: HashMap<String, XmlMemberType>,
}

impl XmlTypeRegistry {
    pub fn new() -> Self {
        Self { registry: TypeRegistry::new(), names: Vec::new(), typedefs: HashMap::new() }
    }

    pub fn from_file(path: impl AsRef<Path>) -> DdsResult<Self> {
        let mut registry = Self::new();
        registry.load_file(path)?;
        Ok(registry)
    }

    pub fn load_file(&mut self, path: impl AsRef<Path>) -> DdsResult<()> {
        self.load_file_with(path.as_ref(), false, &mut HashSet::new())
    }

    /// Like [`Self::load_file`] but skips unknown vendor attributes/elements with a warning.
    pub fn load_file_lenient(&mut self, path: impl AsRef<Path>) -> DdsResult<()> {
        self.load_file_with(path.as_ref(), true, &mut HashSet::new())
    }

    // Loads files it <include>s first; the canonicalized-path set breaks cycles and dedups.
    fn load_file_with(
        &mut self,
        path: &Path,
        lenient: bool,
        visited: &mut HashSet<PathBuf>,
    ) -> DdsResult<()> {
        let canonical = fs::canonicalize(path).map_err(|e| {
            DdsError::Error(format!("Failed to resolve XML types file {}: {e:?}", path.display()))
        })?;
        if !visited.insert(canonical.clone()) {
            return Ok(());
        }
        let xml = read_file(&canonical)?;
        for file in parser::parse_includes(&xml, lenient)? {
            let included = match canonical.parent() {
                Some(dir) => dir.join(&file),
                None => PathBuf::from(&file),
            };
            self.load_file_with(&included, lenient, visited)?;
        }
        self.load(&xml, lenient)
    }

    pub fn load_str(&mut self, xml: &str) -> DdsResult<()> {
        self.load(xml, false)
    }

    /// Like [`Self::load_str`] but skips unknown vendor attributes/elements with a warning.
    pub fn load_str_lenient(&mut self, xml: &str) -> DdsResult<()> {
        self.load(xml, true)
    }

    fn load(&mut self, xml: &str, lenient: bool) -> DdsResult<()> {
        let mut decls = parser::parse_types(xml, lenient)?;

        let mut known: HashSet<String> = self.names.iter().cloned().collect();
        known.extend(self.typedefs.keys().cloned());
        for decl in &decls {
            known.insert(decl.name().to_string());
        }

        // Resolve nonBasic refs to fully-qualified names (IDL scoping) before conversion —
        // the resolved name drives the name-based hash.
        for decl in &mut decls {
            let owner = decl.name().to_string();
            for (label, ty) in decl.member_types_mut() {
                ty.visit_nonbasic_mut(&mut |name| match resolve_name(name, &owner, &known) {
                    Some(full) => {
                        *name = full;
                        Ok(())
                    }
                    None => Err(DdsError::Error(format!(
                        "XML types: '{owner}': member '{label}' references unknown type '{name}'"
                    ))),
                })?;
            }
        }

        // Resolve struct base types with IDL scoping, validate existence, reject cycles.
        for decl in &mut decls {
            let XmlTypeDecl::Struct(s) = decl else { continue };
            let Some(base) = s.base_type.clone() else { continue };
            match resolve_name(&base, &s.name, &known) {
                Some(full) => s.base_type = Some(full),
                None => {
                    return Err(DdsError::Error(format!(
                        "XML types: struct '{}': unknown base type '{base}'",
                        s.name
                    )))
                }
            }
        }
        let bases: HashMap<&str, &str> = decls
            .iter()
            .filter_map(|d| match d {
                XmlTypeDecl::Struct(s) => s.base_type.as_deref().map(|b| (s.name.as_str(), b)),
                _ => None,
            })
            .collect();
        for start in bases.keys() {
            let mut seen = HashSet::new();
            let mut cur = *start;
            while let Some(&base) = bases.get(cur) {
                if !seen.insert(cur) {
                    return Err(DdsError::Error(format!(
                        "XML types: inheritance cycle involving '{cur}'"
                    )));
                }
                cur = base;
            }
        }

        for decl in &decls {
            let name = decl.name();
            if matches!(decl, XmlTypeDecl::Typedef(_)) {
                if self.registry.lookup_by_name(name).is_some()
                    || decls
                        .iter()
                        .any(|d| !matches!(d, XmlTypeDecl::Typedef(_)) && d.name() == name)
                {
                    return Err(DdsError::Error(format!(
                        "XML types: '{name}' declared as both typedef and type"
                    )));
                }
            } else if self.typedefs.contains_key(name) {
                return Err(DdsError::Error(format!(
                    "XML types: '{name}' declared as both typedef and type"
                )));
            }
        }

        let mut new_typedefs: HashMap<String, XmlMemberType> = HashMap::new();
        for decl in &decls {
            let XmlTypeDecl::Typedef(t) = decl else { continue };
            if let Some(prev) = new_typedefs.get(&t.name) {
                if *prev != t.ty {
                    return Err(DdsError::Error(format!(
                        "XML types: typedef '{}' redefined with different content",
                        t.name
                    )));
                }
                continue;
            }
            new_typedefs.insert(t.name.clone(), t.ty.clone());
        }
        let mut all_typedefs = self.typedefs.clone();
        all_typedefs.extend(new_typedefs.iter().map(|(k, v)| (k.clone(), v.clone())));
        for (name, ty) in &new_typedefs {
            let mut stack = vec![name.clone()];
            let flattened = ty.flatten_typedefs(&all_typedefs, &mut stack)?;
            if let Some(prev) = self.typedefs.get(name) {
                if *prev != flattened {
                    return Err(DdsError::Error(format!(
                        "XML types: typedef '{name}' redefined with different content"
                    )));
                }
                continue;
            }
            self.typedefs.insert(name.clone(), flattened);
        }

        // Flatten typedefs in members — derive has none, so this keeps byte-parity.
        for decl in &mut decls {
            match decl {
                XmlTypeDecl::Struct(s) => {
                    for m in &mut s.members {
                        m.ty = m.ty.flatten_typedefs(&self.typedefs, &mut Vec::new())?;
                    }
                }
                XmlTypeDecl::Union(u) => {
                    for c in &mut u.cases {
                        c.ty = c.ty.flatten_typedefs(&self.typedefs, &mut Vec::new())?;
                    }
                }
                _ => {}
            }
        }

        // Resolve union case labels written as enum literal names into their values.
        let mut enum_values: HashMap<String, HashMap<String, i32>> = HashMap::new();
        for decl in &decls {
            if let XmlTypeDecl::Enum(e) = decl {
                let vals = convert::enum_literal_values(e)?;
                let table = e.literals.iter().zip(vals).map(|(l, v)| (l.name.clone(), v)).collect();
                enum_values.insert(e.name.clone(), table);
            }
        }
        for decl in &mut decls {
            let XmlTypeDecl::Union(u) = decl else { continue };
            let disc_enum = match &u.discriminator {
                XmlMemberType::NonBasic(n) => Some(n.clone()),
                _ => None,
            };
            let uname = u.name.clone();
            for case in &mut u.cases {
                for label in &mut case.labels {
                    let XmlCaseLabel::Name(n) = label else { continue };
                    let enum_name = disc_enum.as_ref().ok_or_else(|| {
                        DdsError::Error(format!(
                            "XML types: union '{uname}': named case label '{n}' requires an enum discriminator"
                        ))
                    })?;
                    let value = enum_values
                        .get(enum_name)
                        .and_then(|t| t.get(n).copied())
                        .or_else(|| enum_literal_from_registry(&self.registry, enum_name, n))
                        .ok_or_else(|| {
                            DdsError::Error(format!(
                                "XML types: union '{uname}': case label '{n}' not found in enum '{enum_name}'"
                            ))
                        })?;
                    *label = XmlCaseLabel::Int(value);
                }
            }
        }

        let structs: HashMap<&str, &XmlStruct> = decls
            .iter()
            .filter_map(|d| match d {
                XmlTypeDecl::Struct(s) => Some((s.name.as_str(), s)),
                _ => None,
            })
            .collect();
        let mut done = HashSet::new();
        for name in structs.keys() {
            check_struct_cycles(name, &structs, &mut Vec::new(), &mut done)?;
        }

        for decl in &decls {
            if matches!(decl, XmlTypeDecl::Typedef(_)) {
                continue;
            }
            let name = decl.name().to_string();
            let complete = convert::to_complete_type_object(decl)?;
            if let Some(existing) = self.get_type_object(&name) {
                if existing.serialize() == complete.serialize() {
                    continue;
                }
                return Err(DdsError::Error(format!(
                    "XML types: type '{name}' redefined with different content"
                )));
            }
            // Keyed by name hash, matching derive's nested member reference ids.
            self.registry.register_type_object_with_id(
                &convert::name_based_type_id(&name),
                TypeObject::Complete(complete),
            );
            self.names.push(name);
        }
        Ok(())
    }

    pub fn get_type_object(&self, name: &str) -> Option<&CompleteTypeObject> {
        let hash = self.registry.lookup_by_name(name)?;
        self.registry.lookup_complete(hash)
    }

    pub fn get(&self, name: &str) -> DdsResult<DynamicTypeSupport> {
        let complete = self
            .get_type_object(name)
            .ok_or_else(|| DdsError::Error(format!("XML types: type '{name}' not found")))?;
        DynamicTypeSupport::from_type_object_with_registry(
            TypeObject::Complete(complete.clone()),
            &self.registry,
        )
    }

    pub fn type_names(&self) -> &[String] {
        &self.names
    }
}

impl Default for XmlTypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn read_file(path: impl AsRef<Path>) -> DdsResult<String> {
    fs::read_to_string(path.as_ref())
        .map_err(|e| DdsError::Error(format!("Failed to read XML types file: {e:?}")))
}

fn enum_literal_from_registry(registry: &TypeRegistry, enum_name: &str, lit: &str) -> Option<i32> {
    let hash = registry.lookup_by_name(enum_name)?;
    match registry.lookup_complete(hash)? {
        CompleteTypeObject::Enum(e) => {
            e.literal_seq.iter().find(|l| l.detail.name == lit).map(|l| l.common.value)
        }
        _ => None,
    }
}

fn resolve_name(target: &str, owner: &str, known: &HashSet<String>) -> Option<String> {
    let mut scope = owner.rsplit_once("::").map_or("", |(s, _)| s);
    loop {
        let candidate =
            if scope.is_empty() { target.to_string() } else { format!("{scope}::{target}") };
        if known.contains(&candidate) {
            return Some(candidate);
        }
        if scope.is_empty() {
            return None;
        }
        scope = scope.rsplit_once("::").map_or("", |(s, _)| s);
    }
}

// Cycles must be broken by an external member (boxed indirection), matching derive.
fn check_struct_cycles<'a>(
    name: &'a str,
    structs: &HashMap<&'a str, &'a XmlStruct>,
    stack: &mut Vec<&'a str>,
    done: &mut HashSet<&'a str>,
) -> DdsResult<()> {
    if done.contains(name) {
        return Ok(());
    }
    let Some(s) = structs.get(name) else { return Ok(()) };
    if stack.contains(&name) {
        return Err(DdsError::Error(format!(
            "XML types: circular reference involving '{name}' requires an 'external' member"
        )));
    }
    stack.push(name);
    for m in &s.members {
        if m.external {
            continue;
        }
        let mut refs = Vec::new();
        m.ty.referenced_names(&mut refs);
        for target in refs {
            check_struct_cycles(target, structs, stack, done)?;
        }
    }
    stack.pop();
    done.insert(name);
    Ok(())
}
