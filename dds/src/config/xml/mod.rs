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

use log::warn;

use crate::dcps::core::error::{DdsError, DdsResult};
use crate::xtypes::{
    CompleteTypeObject, DynamicTypeSupport, EquivalenceHash, TypeIdentifier, TypeObject,
    TypeRegistry,
};

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

        let mut batch: Vec<(String, CompleteTypeObject)> = Vec::new();
        for decl in &decls {
            if matches!(decl, XmlTypeDecl::Typedef(_)) {
                continue;
            }
            batch.push((decl.name().to_string(), convert::to_complete_type_object(decl)?));
        }
        self.register_batch(batch)
    }

    // Topologically orders `batch` by intra-batch name references and rewrites each
    // type's placeholder ids to content ids (dependencies first, so their content
    // hashes are already known) before registering it.
    fn register_batch(&mut self, batch: Vec<(String, CompleteTypeObject)>) -> DdsResult<()> {
        let n = batch.len();
        let names: Vec<String> = batch.iter().map(|(nm, _)| nm.clone()).collect();
        let objs: Vec<CompleteTypeObject> = batch.into_iter().map(|(_, c)| c).collect();

        // Seed the name-hash -> content-id resolver with already-registered types.
        let mut resolver: HashMap<EquivalenceHash, TypeIdentifier> = HashMap::new();
        for name in &self.names {
            if let Some(hash) = self.registry.lookup_by_name(name) {
                resolver.insert(
                    EquivalenceHash::compute(name.as_bytes()),
                    TypeIdentifier::CompleteTypeId(*hash),
                );
            }
        }

        let index: HashMap<EquivalenceHash, usize> = names
            .iter()
            .enumerate()
            .map(|(i, nm)| (EquivalenceHash::compute(nm.as_bytes()), i))
            .collect();
        let deps: Vec<Vec<usize>> = objs
            .iter()
            .map(|c| {
                let mut d = Vec::new();
                for h in convert::referenced_name_hashes(c) {
                    if let Some(&j) = index.get(&h) {
                        if !d.contains(&j) {
                            d.push(j);
                        }
                    }
                }
                d
            })
            .collect();

        let mut done = vec![false; n];
        let mut remaining = n;
        while remaining > 0 {
            let mut progressed = false;
            for i in 0..n {
                if done[i] || !deps[i].iter().all(|&j| j == i || done[j]) {
                    continue;
                }
                self.register_rewritten(&names[i], objs[i].clone(), &mut resolver)?;
                done[i] = true;
                remaining -= 1;
                progressed = true;
            }
            if !progressed {
                warn!(
                    "XML types: reference cycle detected; leaving cyclic member ids name-based \
                     (mutually-recursive types are unsupported)"
                );
                for i in 0..n {
                    if !done[i] {
                        self.register_rewritten(&names[i], objs[i].clone(), &mut resolver)?;
                    }
                }
                break;
            }
        }
        Ok(())
    }

    // Rewrites one converted type's placeholder ids to content ids, records its own
    // content id in `resolver`, and registers it (deriving its minimal equivalent).
    fn register_rewritten(
        &mut self,
        name: &str,
        mut obj: CompleteTypeObject,
        resolver: &mut HashMap<EquivalenceHash, TypeIdentifier>,
    ) -> DdsResult<()> {
        convert::resolve_content_ids(&mut obj, resolver);
        let content_hash = TypeObject::Complete(obj.clone()).compute_hash();
        resolver.insert(
            EquivalenceHash::compute(name.as_bytes()),
            TypeIdentifier::CompleteTypeId(content_hash),
        );
        if let Some(existing) = self.registry.lookup_by_name(name).copied() {
            if existing == content_hash {
                return Ok(());
            }
            return Err(DdsError::Error(format!(
                "XML types: type '{name}' redefined with different content"
            )));
        }
        self.registry.register_type_object_with_id(
            &TypeIdentifier::CompleteTypeId(content_hash),
            TypeObject::Complete(obj),
        );
        self.names.push(name.to_string());
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

#[cfg(test)]
mod tests {
    // Bitmask bit names (Read/Write/Admin) must byte-match the XML <bit_value> labels,
    // so they intentionally keep their non-UPPER_CASE spelling in the derived constants.
    #![allow(non_upper_case_globals)]

    use super::*;
    use crate::dcps::topic::type_support::DdsType;
    use crate::serialize::WString;
    use crate::xtypes::{
        CompleteStructMember, CompleteStructType, ExtensibilityKind, HasTypeObject, MemberFlag,
        TryConstructKind, TypeFlag,
    };

    fn load(xml: &str) -> XmlTypeRegistry {
        let mut registry = XmlTypeRegistry::new();
        registry.load_str(xml).unwrap();
        registry
    }

    fn temp_xml_dir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("int2dds_xml_inc_{}_{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    const UNION_XML: &str = r#"<types>
      <struct name="Point" nested="true">
        <member name="x" type="int32"/>
        <member name="y" type="int32"/>
      </struct>
      <union name="Shape">
        <discriminator type="int32"/>
        <case><caseDiscriminator value="0"/><member name="Circle" type="float64"/></case>
        <case><caseDiscriminator value="1"/><member name="Rect" type="nonBasic" nonBasicTypeName="Point"/></case>
        <case><caseDiscriminator value="2"/><member name="Count" type="uint32"/></case>
      </union>
    </types>"#;

    const NESTED_XML: &str = r#"<types>
      <enum name="Color">
        <enumerator name="Red"/>
        <enumerator name="Green" value="5"/>
        <enumerator name="Blue" default_literal="true"/>
      </enum>
      <struct name="Point" nested="true">
        <member name="x" type="int32"/>
        <member name="y" type="int32"/>
      </struct>
      <struct name="Holder">
        <member name="color" type="nonBasic" nonBasicTypeName="Color"/>
        <member name="origin" type="nonBasic" nonBasicTypeName="Point"/>
        <member name="path" type="nonBasic" nonBasicTypeName="Point" sequenceMaxLength="-1"/>
      </struct>
    </types>"#;

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct AllPrimitives {
        f_bool: bool,
        f_byte: u8,
        f_char: char,
        f_i8: i8,
        f_i16: i16,
        f_i32: i32,
        f_i64: i64,
        f_u16: u16,
        f_u32: u32,
        f_u64: u64,
        f_f32: f32,
        f_f64: f64,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", extensibility = "Mutable", autoid = "Hash")]
    struct AnnotatedType {
        #[dds(id = 10, key)]
        sensor_id: u32,
        #[dds(hashid = "crc")]
        checksum: u32,
        #[dds(must_understand)]
        flags: u16,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", type_name = "sensors::Thing")]
    struct ScopedThing {
        value: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", nested)]
    struct NestedAnnotated {
        x: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", data_representation(XCDR2))]
    struct Xcdr2Only {
        x: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct StringsAndCollections {
        name: String,
        note: WString,
        values: Vec<i32>,
        tags: Vec<String>,
        samples: [f64; 4],
        raw: [u8; 3],
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    enum Color {
        Red,
        Green = 5,
        #[dds(default_literal)]
        Blue,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", nested)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct Holder {
        color: Color,
        origin: Point,
        path: Vec<Point>,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    #[repr(i32)]
    enum Shape {
        Circle(f64),
        Rect(Point),
        Count(u32),
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", bitmask, bit_bound = 8)]
    enum Permissions {
        #[dds(position = 0)]
        Read,
        #[dds(position = 1)]
        Write,
        #[dds(position = 5)]
        Admin,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate", bitset)]
    struct PackedHeader {
        #[dds(bitfield = 4)]
        version: u8,
        #[dds(bitfield = 12)]
        length: u16,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct BaseMsg {
        id: i32,
        label: u32,
    }

    #[derive(DdsType)]
    #[dds_type(crate_path = "crate")]
    struct DerivedMsg {
        #[dds(parent)]
        base: BaseMsg,
        value: f64,
    }

    #[test]
    fn byte_identical_primitives_vs_derive() {
        let registry = load(
            r#"<types><struct name="AllPrimitives">
             <member name="f_bool" type="boolean"/>
             <member name="f_byte" type="byte"/>
             <member name="f_char" type="char8"/>
             <member name="f_i8" type="int8"/>
             <member name="f_i16" type="int16"/>
             <member name="f_i32" type="int32"/>
             <member name="f_i64" type="int64"/>
             <member name="f_u16" type="uint16"/>
             <member name="f_u32" type="uint32"/>
             <member name="f_u64" type="uint64"/>
             <member name="f_f32" type="float32"/>
             <member name="f_f64" type="float64"/>
           </struct></types>"#,
        );
        let from_xml = registry.get_type_object("AllPrimitives").unwrap();
        let from_derive = AllPrimitives::complete_type_object();
        assert_eq!(from_xml.serialize(), from_derive.serialize());
    }

    #[test]
    fn byte_identical_annotations_vs_derive() {
        let registry = load(
            r#"<types><struct name="AnnotatedType" extensibility="mutable" autoid="hash">
             <member name="sensor_id" type="uint32" id="10" key="true"/>
             <member name="checksum" type="uint32" hashid="crc"/>
             <member name="flags" type="uint16" mustUnderstand="true"/>
           </struct></types>"#,
        );
        let from_xml = registry.get_type_object("AnnotatedType").unwrap();
        let from_derive = AnnotatedType::complete_type_object();
        assert_eq!(from_xml.serialize(), from_derive.serialize());
    }

    #[test]
    fn byte_identical_type_name_override_vs_xml() {
        // The TypeObject's QualifiedTypeName must reflect the type_name override
        // (the ROS2 mangling path relies on this), byte-identical to the XML module path.
        let registry = load(
            r#"<types><module name="sensors">
             <struct name="Thing"><member name="value" type="int32"/></struct>
           </module></types>"#,
        );
        let from_xml = registry.get_type_object("sensors::Thing").unwrap();
        let from_derive = ScopedThing::complete_type_object();
        assert_eq!(from_xml.serialize(), from_derive.serialize());
    }

    #[test]
    fn byte_identical_nested_vs_derive() {
        let registry = load(
            r#"<types><struct name="NestedAnnotated" nested="true">
             <member name="x" type="int32"/>
           </struct></types>"#,
        );
        let from_xml = registry.get_type_object("NestedAnnotated").unwrap();
        let from_derive = NestedAnnotated::complete_type_object();
        assert_eq!(from_xml.serialize(), from_derive.serialize());
    }

    #[test]
    fn byte_identical_strings_collections_vs_derive() {
        let registry = load(
            r#"<types><struct name="StringsAndCollections">
             <member name="name" type="string"/>
             <member name="note" type="wstring"/>
             <member name="values" type="int32" sequenceMaxLength="-1"/>
             <member name="tags" type="string" sequenceMaxLength="-1"/>
             <member name="samples" type="float64" arrayDimensions="4"/>
             <member name="raw" type="byte" arrayDimensions="3"/>
           </struct></types>"#,
        );
        let from_xml = registry.get_type_object("StringsAndCollections").unwrap();
        let from_derive = StringsAndCollections::complete_type_object();
        assert_eq!(from_xml.serialize(), from_derive.serialize());
    }

    #[test]
    fn byte_identical_inheritance_vs_derive() {
        // Derive embeds the parent as a named member alongside base_type; the XML mirrors that.
        let registry = load(
            r#"<types>
             <struct name="BaseMsg">
               <member name="id" type="int32"/>
               <member name="label" type="uint32"/>
             </struct>
             <struct name="DerivedMsg" baseType="BaseMsg">
               <member name="base" type="nonBasic" nonBasicTypeName="BaseMsg"/>
               <member name="value" type="float64"/>
             </struct>
           </types>"#,
        );
        assert_eq!(
            registry.get_type_object("DerivedMsg").unwrap().serialize(),
            DerivedMsg::complete_type_object().serialize()
        );
    }

    #[test]
    fn standard_inheritance_flattens_parent_members() {
        // baseType alone = standard XTypes inheritance: parent members flattened ahead of child's.
        let registry = load(
            r#"<types>
             <struct name="Animal"><member name="legs" type="int32"/></struct>
             <struct name="Mammal" baseType="Animal"><member name="fur" type="boolean"/></struct>
             <struct name="Dog" baseType="Mammal"><member name="name" type="string"/></struct>
           </types>"#,
        );
        let support = registry.get("Dog").unwrap();
        let data = support.create_data();
        let names: Vec<String> =
            data.dynamic_type().members().unwrap().iter().map(|m| m.name.to_string()).collect();
        assert_eq!(names, ["legs", "fur", "name"]);
    }

    #[test]
    fn unknown_base_type_rejected() {
        let mut registry = XmlTypeRegistry::new();
        let err = registry
            .load_str(r#"<types><struct name="T" baseType="Ghost"><member name="x" type="int32"/></struct></types>"#)
            .unwrap_err();
        assert!(format!("{err:?}").contains("unknown base type 'Ghost'"));
    }

    #[test]
    fn inheritance_cycle_rejected() {
        let mut registry = XmlTypeRegistry::new();
        let err = registry
            .load_str(
                r#"<types>
                 <struct name="A" baseType="B"><member name="a" type="int32"/></struct>
                 <struct name="B" baseType="A"><member name="b" type="int32"/></struct>
               </types>"#,
            )
            .unwrap_err();
        assert!(format!("{err:?}").contains("inheritance cycle"));
    }

    #[test]
    fn byte_identical_bitmask_vs_derive() {
        let registry = load(
            r#"<types><bitmask name="Permissions" bit_bound="8">
             <bit_value name="Read"/>
             <bit_value name="Write"/>
             <bit_value name="Admin" position="5"/>
           </bitmask></types>"#,
        );
        assert_eq!(
            registry.get_type_object("Permissions").unwrap().serialize(),
            Permissions::complete_type_object().serialize()
        );
    }

    #[test]
    fn byte_identical_bitset_vs_derive() {
        let registry = load(
            r#"<types><bitset name="PackedHeader">
             <bitfield name="version" bit_bound="4" type="byte"/>
             <bitfield name="length" bit_bound="12" type="uint16"/>
           </bitset></types>"#,
        );
        assert_eq!(
            registry.get_type_object("PackedHeader").unwrap().serialize(),
            PackedHeader::complete_type_object().serialize()
        );
    }

    #[test]
    fn byte_identical_union_vs_derive() {
        let registry = load(UNION_XML);
        assert_eq!(
            registry.get_type_object("Shape").unwrap().serialize(),
            Shape::complete_type_object().serialize()
        );
    }

    #[test]
    fn union_enum_discriminator_resolves_labels() {
        // Enum-typed discriminator with case labels written as enum literal names.
        let registry = load(
            r#"<types>
             <enum name="Mode">
               <enumerator name="IDLE"/>
               <enumerator name="ACTIVE" value="5"/>
             </enum>
             <union name="Command">
               <discriminator type="nonBasic" nonBasicTypeName="Mode"/>
               <case><caseDiscriminator value="IDLE"/><member name="sleep_ms" type="uint32"/></case>
               <case><caseDiscriminator value="ACTIVE"/><member name="speed" type="float64"/></case>
             </union>
           </types>"#,
        );
        let obj = registry.get_type_object("Command").unwrap();
        let CompleteTypeObject::Union(u) = obj else { panic!("expected union") };
        assert_eq!(u.member_seq[0].common.label_seq, vec![0]);
        assert_eq!(u.member_seq[1].common.label_seq, vec![5]);
        registry.get("Command").unwrap();
    }

    #[test]
    fn byte_identical_enum_and_nested_vs_derive() {
        let registry = load(NESTED_XML);
        assert_eq!(
            registry.get_type_object("Color").unwrap().serialize(),
            Color::complete_type_object().serialize()
        );
        assert_eq!(
            registry.get_type_object("Holder").unwrap().serialize(),
            Holder::complete_type_object().serialize()
        );
    }

    #[test]
    fn byte_identical_idl_alias_names_vs_derive() {
        let registry = load(
            r#"<types><struct name="AllPrimitives">
             <member name="f_bool" type="boolean"/>
             <member name="f_byte" type="octet"/>
             <member name="f_char" type="char"/>
             <member name="f_i8" type="int8"/>
             <member name="f_i16" type="short"/>
             <member name="f_i32" type="long"/>
             <member name="f_i64" type="longLong"/>
             <member name="f_u16" type="unsignedShort"/>
             <member name="f_u32" type="unsignedLong"/>
             <member name="f_u64" type="unsignedLongLong"/>
             <member name="f_f32" type="float"/>
             <member name="f_f64" type="double"/>
           </struct></types>"#,
        );
        assert_eq!(
            registry.get_type_object("AllPrimitives").unwrap().serialize(),
            AllPrimitives::complete_type_object().serialize()
        );
    }

    #[test]
    fn byte_identical_data_representation_vs_derive() {
        let registry = load(
            r#"<types><struct name="Xcdr2Only" data_representation="xcdr2">
             <member name="x" type="int32"/>
           </struct></types>"#,
        );
        assert_eq!(
            registry.get_type_object("Xcdr2Only").unwrap().serialize(),
            Xcdr2Only::complete_type_object().serialize()
        );
    }

    #[test]
    fn typedef_flattens_to_derive_equivalent() {
        let registry = load(
            r#"<types>
             <typedef name="Ids" type="int32" sequenceMaxLength="-1"/>
             <typedef name="Tags" type="string" sequenceMaxLength="-1"/>
             <typedef name="Samples" type="float64" arrayDimensions="4"/>
             <typedef name="Raw" type="byte" arrayDimensions="3"/>
             <struct name="StringsAndCollections">
               <member name="name" type="string"/>
               <member name="note" type="wstring"/>
               <member name="values" type="nonBasic" nonBasicTypeName="Ids"/>
               <member name="tags" type="nonBasic" nonBasicTypeName="Tags"/>
               <member name="samples" type="nonBasic" nonBasicTypeName="Samples"/>
               <member name="raw" type="nonBasic" nonBasicTypeName="Raw"/>
             </struct>
           </types>"#,
        );
        assert_eq!(
            registry.get_type_object("StringsAndCollections").unwrap().serialize(),
            StringsAndCollections::complete_type_object().serialize()
        );
        assert!(registry.get_type_object("Ids").is_none());
        assert_eq!(registry.type_names(), ["StringsAndCollections"]);
    }

    #[test]
    fn typedef_chain_through_struct_alias_matches_derive() {
        let registry = load(
            r#"<types>
             <enum name="Color">
               <enumerator name="Red"/>
               <enumerator name="Green" value="5"/>
               <enumerator name="Blue" default_literal="true"/>
             </enum>
             <struct name="Point" nested="true">
               <member name="x" type="int32"/>
               <member name="y" type="int32"/>
             </struct>
             <typedef name="Position" type="nonBasic" nonBasicTypeName="Point"/>
             <typedef name="Path" type="nonBasic" nonBasicTypeName="Position" sequenceMaxLength="-1"/>
             <struct name="Holder">
               <member name="color" type="nonBasic" nonBasicTypeName="Color"/>
               <member name="origin" type="nonBasic" nonBasicTypeName="Position"/>
               <member name="path" type="nonBasic" nonBasicTypeName="Path"/>
             </struct>
           </types>"#,
        );
        assert_eq!(
            registry.get_type_object("Holder").unwrap().serialize(),
            Holder::complete_type_object().serialize()
        );
        registry.get("Holder").unwrap();
    }

    #[test]
    fn relative_module_references_resolve() {
        let registry = load(
            r#"<types>
             <enum name="Mode"><enumerator name="IDLE"/></enum>
             <module name="geo">
               <struct name="Point" nested="true"><member name="x" type="int32"/></struct>
               <module name="inner">
                 <struct name="Holder">
                   <member name="p" type="nonBasic" nonBasicTypeName="Point"/>
                   <member name="m" type="nonBasic" nonBasicTypeName="Mode"/>
                 </struct>
               </module>
             </module>
           </types>"#,
        );

        let content_id = |name: &str| {
            TypeIdentifier::CompleteTypeId(
                TypeObject::Complete(registry.get_type_object(name).unwrap().clone())
                    .compute_hash(),
            )
        };
        let holder = registry.get_type_object("geo::inner::Holder").unwrap();
        let CompleteTypeObject::Struct(s) = holder else { panic!("expected struct") };
        assert_eq!(s.member_seq[0].common.member_type_id, content_id("geo::Point"));
        assert_eq!(s.member_seq[1].common.member_type_id, content_id("Mode"));
        registry.get("geo::inner::Holder").unwrap();
    }

    #[test]
    fn cycles_require_external() {
        let cyclic = |external: &str| {
            format!(
                r#"<types>
                 <struct name="A" nested="true">
                   <member name="b" type="nonBasic" nonBasicTypeName="B"/>
                 </struct>
                 <struct name="B" nested="true">
                   <member name="a" type="nonBasic" nonBasicTypeName="A"{external}/>
                 </struct>
               </types>"#
            )
        };
        let mut registry = XmlTypeRegistry::new();
        let err = registry.load_str(&cyclic("")).unwrap_err();
        assert!(format!("{err:?}").contains("circular reference"));

        let mut registry = XmlTypeRegistry::new();
        registry.load_str(&cyclic(r#" external="true""#)).unwrap();

        let mut registry = XmlTypeRegistry::new();
        let err = registry
            .load_str(
                r#"<types>
                 <typedef name="X" type="nonBasic" nonBasicTypeName="Y"/>
                 <typedef name="Y" type="nonBasic" nonBasicTypeName="X"/>
               </types>"#,
            )
            .unwrap_err();
        assert!(format!("{err:?}").contains("circular typedef"));
    }

    #[test]
    fn lenient_load_skips_vendor_extras() {
        let xml = r#"<dds>
         <profiles><participant profile_name="x"/></profiles>
         <types>
           <const name="MAX" type="uint32" value="3"/>
           <struct name="T" useVector="true">
             <member name="x" type="int32" transferMode="p"/>
           </struct>
         </types>
       </dds>"#;
        let mut strict = XmlTypeRegistry::new();
        assert!(strict.load_str(xml).is_err());

        let mut registry = XmlTypeRegistry::new();
        registry.load_str_lenient(xml).unwrap();
        registry.get("T").unwrap();
    }

    #[test]
    fn unknown_reference_rejected_at_load() {
        let mut registry = XmlTypeRegistry::new();
        let err = registry
            .load_str(
                r#"<types><struct name="T">
                 <member name="p" type="nonBasic" nonBasicTypeName="Missing"/>
               </struct></types>"#,
            )
            .unwrap_err();
        assert!(format!("{err:?}").contains("references unknown type 'Missing'"));
    }

    #[test]
    fn primitives_without_rust_counterpart() {
        let registry = load(
            r#"<types><struct name="Extra">
             <member name="a" type="uint8"/>
             <member name="b" type="char16" optional="true"/>
             <member name="c" type="float128"/>
           </struct></types>"#,
        );
        let mut expected = CompleteStructType::new(
            TypeFlag::new(ExtensibilityKind::Appendable, false, false),
            "Extra".to_string(),
            None,
        );
        let flags = MemberFlag::new(TryConstructKind::Discard, false, false, false, false, false);
        let optional = MemberFlag::new(TryConstructKind::Discard, false, true, false, false, false);
        expected.add_member(CompleteStructMember::new(
            0,
            flags,
            TypeIdentifier::Uint8,
            "a".to_string(),
        ));
        expected.add_member(CompleteStructMember::new(
            1,
            optional,
            TypeIdentifier::Char16,
            "b".to_string(),
        ));
        expected.add_member(CompleteStructMember::new(
            2,
            flags,
            TypeIdentifier::Float128,
            "c".to_string(),
        ));
        let expected = CompleteTypeObject::Struct(expected);
        assert_eq!(registry.get_type_object("Extra").unwrap(), &expected);
    }

    #[test]
    fn module_qualified_lookup() {
        let registry = load(
            r#"<types><module name="sensors">
             <struct name="T"><member name="x" type="int32"/></struct>
           </module></types>"#,
        );
        assert!(registry.get_type_object("sensors::T").is_some());
        assert!(registry.get_type_object("T").is_none());
        assert_eq!(registry.type_names(), ["sensors::T"]);
    }

    #[test]
    fn redefinition_policy() {
        let xml = r#"<types><struct name="T"><member name="x" type="int32"/></struct></types>"#;
        let mut registry = load(xml);
        registry.load_str(xml).unwrap();
        assert_eq!(registry.type_names().len(), 1);

        let conflicting =
            r#"<types><struct name="T"><member name="x" type="int64"/></struct></types>"#;
        let err = registry.load_str(conflicting).unwrap_err();
        assert!(format!("{err:?}").contains("redefined with different content"));
    }

    #[test]
    fn get_unknown_type_fails() {
        let registry = XmlTypeRegistry::new();
        assert!(registry.get("Nope").is_err());
    }

    #[test]
    fn const_bounds_match_literal_bounds() {
        let with_const = load(
            r#"<types>
             <const name="MAX_ITEMS" type="int32" value="16"/>
             <const name="ROWS" type="uint32" value="3"/>
             <struct name="T">
               <member name="names" type="string" sequenceMaxLength="MAX_ITEMS"/>
               <member name="grid" type="float64" arrayDimensions="ROWS, 2"/>
             </struct>
           </types>"#,
        );
        let with_literal = load(
            r#"<types><struct name="T">
             <member name="names" type="string" sequenceMaxLength="16"/>
             <member name="grid" type="float64" arrayDimensions="3, 2"/>
           </struct></types>"#,
        );
        assert_eq!(
            with_const.get_type_object("T").unwrap().serialize(),
            with_literal.get_type_object("T").unwrap().serialize(),
        );
    }

    #[test]
    fn forward_dcl_does_not_block_definition() {
        let registry = load(
            r#"<types>
             <forward_dcl name="Point" kind="struct"/>
             <struct name="Line">
               <member name="start" type="nonBasic" nonBasicTypeName="Point"/>
             </struct>
             <struct name="Point">
               <member name="x" type="int32"/>
               <member name="y" type="int32"/>
             </struct>
           </types>"#,
        );
        assert!(registry.get("Line").is_ok());
        assert!(registry.get_type_object("Point").is_some());
    }

    #[test]
    fn include_resolves_relative_to_including_file() {
        let dir = temp_xml_dir("compose");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(
            dir.join("sub").join("base.xml"),
            r#"<types><struct name="Point">
             <member name="x" type="int32"/><member name="y" type="int32"/>
           </struct></types>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("main.xml"),
            r#"<dds>
             <include file="sub/base.xml"/>
             <types><struct name="Line">
               <member name="start" type="nonBasic" nonBasicTypeName="Point"/>
             </struct></types>
           </dds>"#,
        )
        .unwrap();
        let registry = XmlTypeRegistry::from_file(dir.join("main.xml")).unwrap();
        assert!(registry.get_type_object("Point").is_some());
        assert!(registry.get("Line").is_ok());
    }

    #[test]
    fn include_cycle_is_safe() {
        let dir = temp_xml_dir("cycle");
        std::fs::write(
            dir.join("a.xml"),
            r#"<dds><include file="b.xml"/>
             <types><struct name="A"><member name="x" type="int32"/></struct></types>
           </dds>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("b.xml"),
            r#"<dds><include file="a.xml"/>
             <types><struct name="B"><member name="y" type="int32"/></struct></types>
           </dds>"#,
        )
        .unwrap();
        let registry = XmlTypeRegistry::from_file(dir.join("a.xml")).unwrap();
        assert!(registry.get_type_object("A").is_some());
        assert!(registry.get_type_object("B").is_some());
    }

    #[test]
    fn include_diamond_loads_each_file_once() {
        let dir = temp_xml_dir("diamond");
        std::fs::write(
            dir.join("common.xml"),
            r#"<types><struct name="Common"><member name="v" type="int32"/></struct></types>"#,
        )
        .unwrap();
        for (file, name) in [("left.xml", "Left"), ("right.xml", "Right")] {
            std::fs::write(
                dir.join(file),
                format!(
                    r#"<dds><include file="common.xml"/>
                     <types><struct name="{name}">
                       <member name="c" type="nonBasic" nonBasicTypeName="Common"/>
                     </struct></types>
                   </dds>"#
                ),
            )
            .unwrap();
        }
        std::fs::write(
            dir.join("top.xml"),
            r#"<dds><include file="left.xml"/><include file="right.xml"/>
             <types><struct name="Top">
               <member name="l" type="nonBasic" nonBasicTypeName="Left"/>
             </struct></types>
           </dds>"#,
        )
        .unwrap();
        let registry = XmlTypeRegistry::from_file(dir.join("top.xml")).unwrap();
        for name in ["Common", "Left", "Right", "Top"] {
            assert!(registry.get_type_object(name).is_some(), "{name} missing");
        }
        // The shared file is loaded once despite being included from two parents.
        assert_eq!(registry.type_names().iter().filter(|n| n.as_str() == "Common").count(), 1);
    }

    #[test]
    fn load_str_ignores_include() {
        let registry = load(
            r#"<types><include file="other.xml"/>
             <struct name="T"><member name="x" type="int32"/></struct>
           </types>"#,
        );
        assert!(registry.get_type_object("T").is_some());
    }
}
