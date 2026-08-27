/// Java code generation backend.
///
/// Emits one file per top-level type: Java forces a public top-level class per
/// file, so this backend returns a Vec where the others return one String.
use crate::naming::{self, TargetLang};
use crate::types::*;

/// Options for Java code generation.
#[derive(Debug, Clone, Default)]
pub struct JavaOptions {
    /// `None` = unnamed package: no `package` line, files land flat.
    pub package: Option<String>,
}

/// One emitted source file, path relative to the output directory.
#[derive(Debug, Clone)]
pub struct GeneratedFile {
    pub relative_path: String,
    pub source: String,
}

pub fn generate(
    model: &IdlModel,
    idl_filename: &str,
    opts: &JavaOptions,
) -> Result<Vec<GeneratedFile>, String> {
    validate_package(opts)?;
    reject_unsupported(model, opts)?;
    reject_name_collisions(model)?;
    let mut files = Vec::new();
    let mut owners: Vec<&str> = Vec::new(); // the IDL type each file came from
    for s in &model.structs {
        files.push(emit_struct(model, s, idl_filename, opts)?);
        owners.push(&s.qualified_name);
    }
    for e in &model.enums {
        files.push(emit_enum(e, idl_filename, opts));
        owners.push(&e.qualified_name);
    }
    // `foo_bar` and `FooBar` both map to FooBar.java. Without this, one would
    // silently overwrite the other.
    for i in 0..files.len() {
        for j in 0..i {
            if files[i].relative_path == files[j].relative_path {
                return Err(format!(
                    "Java backend maps IDL types '{}' and '{}' to the same output file '{}'",
                    owners[j], owners[i], files[i].relative_path
                ));
            }
        }
    }
    Ok(files)
}

/// Out-of-scope constructs are refused by name. A silently dropped type is a
/// wire mismatch nobody sees until runtime.
fn reject_unsupported(model: &IdlModel, opts: &JavaOptions) -> Result<(), String> {
    if let Some(u) = model.unions.first() {
        return Err(format!("Java backend does not support union '{}'", u.name));
    }
    if let Some(b) = model.bitmasks.first() {
        return Err(format!("Java backend does not support bitmask '{}'", b.name));
    }
    if let Some(b) = model.bitsets.first() {
        return Err(format!("Java backend does not support bitset '{}'", b.name));
    }
    for e in &model.enums {
        if e.variants.is_empty() {
            return Err(format!(
                "Java backend does not support enum '{}' with no variants",
                e.name
            ));
        }
    }
    for s in &model.structs {
        if s.extensibility == ExtensibilityKind::Mutable {
            return Err(format!("Java backend does not support MUTABLE struct '{}'", s.name));
        }
        if let Some(base) = &s.base_type {
            return Err(format!(
                "Java backend does not support struct inheritance ('{}' extends '{}')",
                s.name, base
            ));
        }
        for m in &s.members {
            if m.is_optional {
                return Err(format!(
                    "Java backend does not support @optional member '{}.{}'",
                    s.name, m.name
                ));
            }
            if matches!(m.resolved_type, ResolvedType::Map { .. }) {
                return Err(format!(
                    "Java backend does not support map member '{}.{}'",
                    s.name, m.name
                ));
            }
            if let ResolvedType::Sequence { element, .. } | ResolvedType::Array { element, .. } =
                &m.resolved_type
            {
                if matches!(
                    element.as_ref(),
                    ResolvedType::Sequence { .. } | ResolvedType::Array { .. }
                ) {
                    return Err(format!(
                        "Java backend does not support a nested collection member '{}.{}'",
                        s.name, m.name
                    ));
                }
            }
            // References go out as bare leaf names with no import, so a different
            // package means `cannot find symbol` from javac.
            if let Some(referenced) = referenced_type_name(&m.resolved_type) {
                let here = package_for(opts, &s.qualified_name);
                let there = package_for(opts, &resolve_reference(model, referenced));
                if here != there {
                    return Err(format!(
                        "Java backend does not support the cross-package reference '{}.{}': \
                         '{}' lands in package {} but '{}' lands in package {}",
                        s.name,
                        m.name,
                        s.qualified_name,
                        package_label(&here),
                        referenced,
                        package_label(&there)
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Two IDL names that map to one Java name. The names survive `resolve` intact
/// and only merge in this backend, so javac is the first thing to notice —
/// report it here with the IDL names the user can act on.
fn reject_name_collisions(model: &IdlModel) -> Result<(), String> {
    for s in &model.structs {
        let java: Vec<String> = s.members.iter().map(|m| java_field_name(&m.name)).collect();
        for i in 0..java.len() {
            for j in 0..i {
                if java[i] == java[j] {
                    return Err(format!(
                        "Java backend maps IDL members '{}.{}' and '{}.{}' to the same \
                         Java field '{}'",
                        s.name, s.members[j].name, s.name, s.members[i].name, java[i]
                    ));
                }
            }
        }
    }
    for e in &model.enums {
        let java: Vec<String> =
            e.variants.iter().map(|v| naming::escape_keyword(&v.name, TargetLang::Java)).collect();
        for i in 0..java.len() {
            // A `value` constant collides with the generated `value` field.
            // `values`/`fromValue` are methods, a different namespace.
            if java[i] == "value" {
                return Err(format!(
                    "Java backend cannot emit enumerator '{}.{}': it collides with the \
                     generated 'value' field; rename it in the IDL",
                    e.name, e.variants[i].name
                ));
            }
            for j in 0..i {
                if java[i] == java[j] {
                    return Err(format!(
                        "Java backend maps IDL enumerators '{}.{}' and '{}.{}' to the same \
                         Java constant '{}'",
                        e.name, e.variants[j].name, e.name, e.variants[i].name, java[i]
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The struct/enum a member refers to, looking through one collection level.
fn referenced_type_name(t: &ResolvedType) -> Option<&str> {
    match t {
        ResolvedType::Struct(n) | ResolvedType::Enum(n) => Some(n),
        ResolvedType::Sequence { element, .. } | ResolvedType::Array { element, .. } => {
            referenced_type_name(element)
        }
        _ => None,
    }
}

/// The qualified name a reference resolves to. An `#include`d type has
/// already been moved into `model.imported` by the resolver, so it must be
/// searched too or its reference falls through to the bare name.
fn resolve_reference(model: &IdlModel, name: &str) -> String {
    let leaf = name.rsplit("::").next().unwrap_or(name);
    let declared = || {
        model
            .structs
            .iter()
            .map(|s| (s.name.as_str(), s.qualified_name.as_str()))
            .chain(model.enums.iter().map(|e| (e.name.as_str(), e.qualified_name.as_str())))
            .chain(
                model.imported.structs.iter().map(|s| (s.name.as_str(), s.qualified_name.as_str())),
            )
            .chain(
                model.imported.enums.iter().map(|e| (e.name.as_str(), e.qualified_name.as_str())),
            )
    };
    // Exact qualified name first, then leaf-only references.
    if let Some((_, q)) = declared().find(|(_, q)| *q == name) {
        return q.to_string();
    }
    if let Some((_, q)) = declared().find(|(n, _)| *n == leaf) {
        return q.to_string();
    }
    name.to_string()
}

fn package_label(p: &Option<String>) -> &str {
    p.as_deref().unwrap_or("<unnamed>")
}

/// `--java-package` becomes the `package` line and the directory path verbatim,
/// so unchecked ".a..b." emits `package .a..b.;` and "com. x" a directory with
/// a space in it.
pub fn validate_package(opts: &JavaOptions) -> Result<(), String> {
    let Some(base) = &opts.package else { return Ok(()) };
    let base = base.trim();
    if base.is_empty() {
        return Ok(()); // empty is the same as no package
    }
    for seg in base.split('.') {
        if !is_java_identifier(seg) {
            return Err(format!(
                "invalid --java-package '{}': segment '{}' is not a Java identifier",
                base, seg
            ));
        }
    }
    Ok(())
}

fn is_java_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '$') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '$')
}

// ---- naming ----------------------------------------------------------------

/// PascalCase leaf name, keyword-escaped. `A::B::Inner` -> `Inner`.
fn java_type_name(name: &str) -> String {
    let leaf = name.rsplit("::").next().unwrap_or(name);
    naming::escape_keyword(&naming::to_pascal_case(leaf), TargetLang::Java)
}

/// camelCase field name, keyword-escaped.
fn java_field_name(name: &str) -> String {
    naming::escape_keyword(&naming::to_camel_case(name), TargetLang::Java)
}

/// The `--java-package` base with the type's IDL module path appended,
/// lowercased. `None` = unnamed package.
fn package_for(opts: &JavaOptions, qualified_name: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(base) = &opts.package {
        let base = base.trim();
        if !base.is_empty() {
            for seg in base.split('.') {
                parts.push(naming::escape_keyword(seg, TargetLang::Java));
            }
        }
    }
    let mut segs: Vec<&str> = qualified_name.split("::").collect();
    segs.pop(); // drop the leaf type name
    for s in segs {
        parts.push(naming::escape_keyword(&s.to_lowercase(), TargetLang::Java));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("."))
    }
}

fn relative_path(package: &Option<String>, class_name: &str) -> String {
    match package {
        Some(p) => format!("{}/{}.java", p.replace('.', "/"), class_name),
        None => format!("{}.java", class_name),
    }
}

// ---- type mapping ----------------------------------------------------------

fn type_to_java(t: &ResolvedType) -> Result<String, String> {
    Ok(match t {
        ResolvedType::Bool => "boolean".to_string(),
        ResolvedType::U8 | ResolvedType::UInt8 | ResolvedType::I8 | ResolvedType::Char => {
            "byte".to_string()
        }
        ResolvedType::I16 | ResolvedType::U16 => "short".to_string(),
        ResolvedType::I32 | ResolvedType::U32 => "int".to_string(),
        ResolvedType::I64 | ResolvedType::U64 => "long".to_string(),
        ResolvedType::F32 => "float".to_string(),
        ResolvedType::F64 => "double".to_string(),
        ResolvedType::WChar => "char".to_string(),
        ResolvedType::String { .. } | ResolvedType::WString { .. } => "String".to_string(),
        ResolvedType::Struct(n) | ResolvedType::Enum(n) => java_type_name(n),
        ResolvedType::Sequence { element, .. } | ResolvedType::Array { element, .. } => {
            format!("{}[]", type_to_java(element)?)
        }
        ResolvedType::Bitmask(n) => {
            return Err(format!("Java backend does not support bitmask '{}'", n))
        }
        ResolvedType::Map { .. } => return Err("Java backend does not support map".to_string()),
    })
}

/// A non-null initializer for reference-typed fields, so `serializeCdr` on a
/// freshly constructed instance never sees null. Primitives return None.
fn field_init(t: &ResolvedType, model: &IdlModel) -> Result<Option<String>, String> {
    Ok(match t {
        ResolvedType::String { .. } | ResolvedType::WString { .. } => Some("\"\"".to_string()),
        ResolvedType::Struct(n) => Some(format!("new {}()", java_type_name(n))),
        ResolvedType::Sequence { element, .. } => {
            Some(format!("new {}[0]", type_to_java(element)?))
        }
        ResolvedType::Array { element, size } => {
            Some(format!("new {}[{}]", type_to_java(element)?, size))
        }
        ResolvedType::Enum(n) => enum_default(model, n),
        _ => None,
    })
}

/// `Type.FIRST_VARIANT` — a null enum field would NPE inside serializeCdr.
///
/// Like `resolve_reference`, this must search `model.imported` as well: an
/// `#include`d enum has been moved out of `model.enums` by the resolver, so
/// searching only there leaves the field null.
fn enum_default(model: &IdlModel, name: &str) -> Option<String> {
    let leaf = name.rsplit("::").next().unwrap_or(name);
    let declared = || model.enums.iter().chain(model.imported.enums.iter());
    // Exact qualified name first, then leaf-only references.
    let e = declared()
        .find(|e| e.qualified_name == name)
        .or_else(|| declared().find(|e| e.name == leaf))?;
    let first = e.variants.first()?;
    Some(format!(
        "{}.{}",
        java_type_name(&e.name),
        naming::escape_keyword(&first.name, TargetLang::Java)
    ))
}

/// A fixed array of a reference type comes back null-filled from `new T[N]`,
/// so serializeCdr would throw. Such fields get filled in the constructor.
fn array_fill(t: &ResolvedType, model: &IdlModel) -> Result<Option<String>, String> {
    let ResolvedType::Array { element, .. } = t else { return Ok(None) };
    Ok(match element.as_ref() {
        ResolvedType::Struct(n) => Some(format!("new {}()", java_type_name(n))),
        ResolvedType::String { .. } | ResolvedType::WString { .. } => Some("\"\"".to_string()),
        ResolvedType::Enum(n) => enum_default(model, n),
        _ => None,
    })
}

// ---- serialization ---------------------------------------------------------

/// `octet` and `uint8` both land on Java byte, so a sequence of either takes
/// the bulk writeBytes/readBytes path.
fn is_byte(t: &ResolvedType) -> bool {
    matches!(t, ResolvedType::U8 | ResolvedType::UInt8)
}

/// The `this.`-less form of a field expression, for exception text only.
fn display_expr(expr: &str) -> &str {
    expr.strip_prefix("this.").unwrap_or(expr)
}

/// Emits the statement that writes `expr` (a Java expression of `t`'s type).
fn emit_write(
    out: &mut String,
    ind: &str,
    expr: &str,
    t: &ResolvedType,
    depth: usize,
) -> Result<(), String> {
    match t {
        ResolvedType::Sequence { element, bound } => {
            if let Some(b) = bound {
                out.push_str(&format!(
                    "{ind}if ({expr}.length > {b}) {{\n\
                     {ind}    throw new IllegalStateException(\
                     \"{name} exceeds its IDL bound of {b}\");\n\
                     {ind}}}\n",
                    ind = ind,
                    expr = expr,
                    name = display_expr(expr),
                    b = b
                ));
            }
            out.push_str(&format!("{}writer.writeSeqHeader({}.length);\n", ind, expr));
            if is_byte(element) {
                out.push_str(&format!("{}writer.writeBytes({});\n", ind, expr));
            } else {
                let i = format!("i{}", depth);
                out.push_str(&format!(
                    "{ind}for (int {i} = 0; {i} < {expr}.length; {i}++) {{\n",
                    ind = ind,
                    i = i,
                    expr = expr
                ));
                let inner = format!("{}    ", ind);
                emit_write(out, &inner, &format!("{}[{}]", expr, i), element, depth + 1)?;
                out.push_str(&format!("{}}}\n", ind));
            }
            return Ok(());
        }
        ResolvedType::Array { element, size } => {
            out.push_str(&format!(
                "{ind}if ({expr}.length != {size}) {{\n\
                 {ind}    throw new IllegalStateException(\
                 \"{name} must hold exactly {size} elements\");\n\
                 {ind}}}\n",
                ind = ind,
                expr = expr,
                name = display_expr(expr),
                size = size
            ));
            let i = format!("i{}", depth);
            out.push_str(&format!(
                "{ind}for (int {i} = 0; {i} < {size}; {i}++) {{\n",
                ind = ind,
                i = i,
                size = size
            ));
            let inner = format!("{}    ", ind);
            emit_write(out, &inner, &format!("{}[{}]", expr, i), element, depth + 1)?;
            out.push_str(&format!("{}}}\n", ind));
            return Ok(());
        }
        _ => {}
    }
    if let ResolvedType::Struct(_) = t {
        out.push_str(&format!("{}{}.serializeCdr(writer);\n", ind, expr));
        return Ok(());
    }
    let line = match t {
        ResolvedType::Bool => format!("writer.writeBool({});", expr),
        ResolvedType::U8 | ResolvedType::UInt8 | ResolvedType::Char => {
            format!("writer.writeU8({} & 0xFF);", expr)
        }
        ResolvedType::I8 => format!("writer.writeI8({});", expr),
        ResolvedType::I16 => format!("writer.writeI16({});", expr),
        ResolvedType::U16 => format!("writer.writeU16({} & 0xFFFF);", expr),
        ResolvedType::I32 => format!("writer.writeI32({});", expr),
        ResolvedType::U32 => format!("writer.writeU32({});", expr),
        ResolvedType::I64 => format!("writer.writeI64({});", expr),
        ResolvedType::U64 => format!("writer.writeU64({});", expr),
        ResolvedType::F32 => format!("writer.writeF32({});", expr),
        ResolvedType::F64 => format!("writer.writeF64({});", expr),
        ResolvedType::WChar => format!("writer.writeU16({});", expr),
        ResolvedType::String { bound } => {
            if let Some(b) = bound {
                // writeString puts UTF-8 bytes on the wire and the core checks the
                // bound in bytes, so length() (UTF-16 units) would pass strings the
                // core then refuses.
                out.push_str(&format!(
                    "{ind}if (CdrWriter.utf8Length({expr}) > {b}) {{\n\
                     {ind}    throw new IllegalStateException(\
                     \"{name} exceeds its IDL bound of {b}\");\n\
                     {ind}}}\n",
                    ind = ind,
                    expr = expr,
                    name = display_expr(expr),
                    b = b
                ));
            }
            format!("writer.writeString({});", expr)
        }
        ResolvedType::WString { bound } => {
            if let Some(b) = bound {
                // length() is the UTF-16 unit count writeWString puts on the
                // wire, so the bound is checked in the unit it is declared in.
                out.push_str(&format!(
                    "{ind}if ({expr}.length() > {b}) {{\n\
                     {ind}    throw new IllegalStateException(\
                     \"{name} exceeds its IDL bound of {b}\");\n\
                     {ind}}}\n",
                    ind = ind,
                    expr = expr,
                    name = display_expr(expr),
                    b = b
                ));
            }
            format!("writer.writeWString({});", expr)
        }
        ResolvedType::Enum(_) => format!("writer.writeEnum({}.value());", expr),
        other => return Err(format!("unsupported member type in write: {:?}", other)),
    };
    out.push_str(&format!("{}{}\n", ind, line));
    Ok(())
}

/// Emits the statement that assigns the decoded value into `target`.
fn emit_read(
    out: &mut String,
    ind: &str,
    target: &str,
    t: &ResolvedType,
    depth: usize,
) -> Result<(), String> {
    match t {
        ResolvedType::Sequence { element, .. } => {
            if is_byte(element) {
                out.push_str(&format!(
                    "{}{} = reader.readBytes(reader.readSeqHeader());\n",
                    ind, target
                ));
            } else {
                let elem_ty = type_to_java(element)?;
                out.push_str(&format!(
                    "{}{} = new {}[reader.readSeqHeader()];\n",
                    ind, target, elem_ty
                ));
                let i = format!("i{}", depth);
                out.push_str(&format!(
                    "{ind}for (int {i} = 0; {i} < {t}.length; {i}++) {{\n",
                    ind = ind,
                    i = i,
                    t = target
                ));
                let inner = format!("{}    ", ind);
                emit_read(out, &inner, &format!("{}[{}]", target, i), element, depth + 1)?;
                out.push_str(&format!("{}}}\n", ind));
            }
            return Ok(());
        }
        ResolvedType::Array { element, size } => {
            let i = format!("i{}", depth);
            out.push_str(&format!(
                "{ind}for (int {i} = 0; {i} < {size}; {i}++) {{\n",
                ind = ind,
                i = i,
                size = size
            ));
            let inner = format!("{}    ", ind);
            emit_read(out, &inner, &format!("{}[{}]", target, i), element, depth + 1)?;
            out.push_str(&format!("{}}}\n", ind));
            return Ok(());
        }
        _ => {}
    }
    if let ResolvedType::Struct(n) = t {
        // Array/sequence elements are still null here. The field is already
        // initialized, but reconstructing it is safe since read overwrites everything.
        out.push_str(&format!("{}{} = new {}();\n", ind, target, java_type_name(n)));
        out.push_str(&format!("{}{}.deserializeCdr(reader);\n", ind, target));
        return Ok(());
    }
    let rhs = match t {
        ResolvedType::Bool => "reader.readBool()".to_string(),
        ResolvedType::U8 | ResolvedType::UInt8 | ResolvedType::Char => {
            "(byte) reader.readU8()".to_string()
        }
        ResolvedType::I8 => "reader.readI8()".to_string(),
        ResolvedType::I16 => "reader.readI16()".to_string(),
        ResolvedType::U16 => "(short) reader.readU16()".to_string(),
        ResolvedType::I32 => "reader.readI32()".to_string(),
        ResolvedType::U32 => "reader.readU32()".to_string(),
        ResolvedType::I64 => "reader.readI64()".to_string(),
        ResolvedType::U64 => "reader.readU64()".to_string(),
        ResolvedType::F32 => "reader.readF32()".to_string(),
        ResolvedType::F64 => "reader.readF64()".to_string(),
        ResolvedType::WChar => "(char) reader.readU16()".to_string(),
        ResolvedType::String { .. } => "reader.readString()".to_string(),
        ResolvedType::WString { .. } => "reader.readWString()".to_string(),
        ResolvedType::Enum(n) => format!("{}.fromValue(reader.readEnum())", java_type_name(n)),
        other => return Err(format!("unsupported member type in read: {:?}", other)),
    };
    out.push_str(&format!("{}{} = {};\n", ind, target, rhs));
    Ok(())
}

// ---- topic field descriptors ------------------------------------------------

/// The `FieldType` constant a member maps to, or None when this native path
/// cannot represent it. Mirrors DomainParticipant.nativeFieldTypeCode, which
/// rejects floats, BYTE, the char kinds, wide strings, and every aggregate.
fn field_type_const(t: &ResolvedType) -> Option<&'static str> {
    Some(match t {
        ResolvedType::Bool => "BOOL",
        ResolvedType::I8 => "INT8",
        ResolvedType::UInt8 => "UINT8",
        ResolvedType::I16 => "INT16",
        ResolvedType::U16 => "UINT16",
        ResolvedType::I32 => "INT32",
        ResolvedType::U32 => "UINT32",
        ResolvedType::I64 => "INT64",
        ResolvedType::U64 => "UINT64",
        ResolvedType::String { .. } => "STRING",
        _ => return None,
    })
}

// ---- file emission ---------------------------------------------------------

fn extensibility_const(k: ExtensibilityKind) -> &'static str {
    match k {
        ExtensibilityKind::Final => "FINAL",
        ExtensibilityKind::Appendable => "APPENDABLE",
        ExtensibilityKind::Mutable => "MUTABLE",
    }
}

fn emit_struct(
    model: &IdlModel,
    s: &ResolvedStruct,
    idl_filename: &str,
    opts: &JavaOptions,
) -> Result<GeneratedFile, String> {
    let class = java_type_name(&s.name);
    let package = package_for(opts, &s.qualified_name);
    let appendable = s.extensibility == ExtensibilityKind::Appendable;

    // The prefix runs from field 0 through the last @key field: the core's
    // flat parser walks descriptors in declaration order, so a gap before a
    // needed field misaligns everything after it.
    let last_key = s.members.iter().rposition(|m| m.is_key);
    let mut descriptors: Vec<String> = Vec::new();
    if let Some(last) = last_key {
        for m in &s.members[..=last] {
            let Some(kind) = field_type_const(&m.resolved_type) else {
                return Err(format!(
                    "Java backend cannot describe '{}.{}' to the keyed-topic path, \
                     and it sits at or before the last @key field; \
                     the core's flat parser needs an unbroken prefix",
                    s.name, m.name
                ));
            };
            descriptors.push(format!(
                "                new TopicFieldDescriptor(\"{}\", FieldType.{}, {})",
                m.name, kind, m.is_key
            ));
        }
    }

    let mut out = String::new();
    out.push_str(&format!(
        "// Auto-generated by int2dds-idl from {}\n// DO NOT EDIT\n\n",
        idl_filename
    ));
    if let Some(p) = &package {
        out.push_str(&format!("package {};\n\n", p));
    }
    let mut imports = vec![
        "import com.intellectus.int2dds.cdr.CdrReader;".to_string(),
        "import com.intellectus.int2dds.cdr.CdrWriter;".to_string(),
        "import com.intellectus.int2dds.cdr.Extensibility;".to_string(),
        "import com.intellectus.int2dds.types.IDdsType;".to_string(),
    ];
    if !descriptors.is_empty() {
        imports.push("import com.intellectus.int2dds.core.TopicFieldDescriptor;".to_string());
        imports.push("import com.intellectus.int2dds.xtypes.FieldType;".to_string());
        imports.push("import java.util.Arrays;".to_string());
        imports.push("import java.util.List;".to_string());
    }
    imports.sort();
    for imp in &imports {
        out.push_str(imp);
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&format!("public final class {} implements IDdsType {{\n\n", class));

    // fields
    for m in &s.members {
        let ty = type_to_java(&m.resolved_type)?;
        let name = java_field_name(&m.name);
        match field_init(&m.resolved_type, model)? {
            Some(init) => out.push_str(&format!("    public {} {} = {};\n", ty, name, init)),
            None => out.push_str(&format!("    public {} {};\n", ty, name)),
        }
    }
    out.push('\n');

    let fills: Vec<(String, String)> = s
        .members
        .iter()
        .filter_map(|m| {
            array_fill(&m.resolved_type, model)
                .ok()
                .flatten()
                .map(|init| (java_field_name(&m.name), init))
        })
        .collect();
    if !fills.is_empty() {
        out.push_str(&format!("    public {}() {{\n", class));
        for (name, init) in &fills {
            out.push_str(&format!(
                "        for (int i = 0; i < this.{name}.length; i++) {{\n\
                 \x20           this.{name}[i] = {init};\n        }}\n",
                name = name,
                init = init
            ));
        }
        out.push_str("    }\n\n");
    }

    out.push_str("    @Override\n");
    // The registered type name must be the qualified name the Rust and C# backends
    // use, or participants built from the same IDL will not match in discovery.
    out.push_str(&format!(
        "    public String typeName() {{\n        return \"{}\";\n    }}\n\n",
        s.qualified_name
    ));
    out.push_str("    @Override\n");
    out.push_str(&format!(
        "    public Extensibility extensibility() {{\n        return Extensibility.{};\n    }}\n\n",
        extensibility_const(s.extensibility)
    ));

    // Fields are read as `this.x` so members named writer/reader/token/d are not
    // shadowed by the locals. A member named `token` compiles either way and would
    // write the DHEADER token instead of the field.
    out.push_str("    @Override\n    public void serializeCdr(CdrWriter writer) {\n");
    if appendable {
        out.push_str("        int token = writer.dheaderBegin();\n");
    }
    for m in &s.members {
        let field = format!("this.{}", java_field_name(&m.name));
        emit_write(&mut out, "        ", &field, &m.resolved_type, 0)?;
    }
    if appendable {
        out.push_str("        writer.dheaderFinalize(token);\n");
    }
    out.push_str("    }\n\n");

    out.push_str("    @Override\n    public void deserializeCdr(CdrReader reader) {\n");
    if appendable {
        out.push_str("        CdrReader.Dheader d = reader.readDheader();\n");
    }
    for m in &s.members {
        let field = format!("this.{}", java_field_name(&m.name));
        emit_read(&mut out, "        ", &field, &m.resolved_type, 0)?;
    }
    if appendable {
        out.push_str("        reader.readDheaderEnd(d);\n");
    }
    out.push_str("    }\n");

    if !descriptors.is_empty() {
        out.push_str(&format!(
            "\n    /**\n\
             \x20    * Field descriptors for {{@link com.intellectus.int2dds.core.DomainParticipant\n\
             \x20    * #createTopic(String, com.intellectus.int2dds.types.IDdsType, List)}}.\n\
             \x20    * Covers every field up to and including the last {{@code @key}} one:\n\
             \x20    * the core's flat parser walks them in order and a gap misaligns\n\
             \x20    * everything after it.\n\
             \x20    */\n\
             \x20   public static List<TopicFieldDescriptor> ddsFields() {{\n\
             \x20       return Arrays.asList(\n{});\n    }}\n",
            descriptors.join(",\n")
        ));
    }

    out.push_str("}\n");

    Ok(GeneratedFile { relative_path: relative_path(&package, &class), source: out })
}

fn emit_enum(e: &ResolvedEnum, idl_filename: &str, opts: &JavaOptions) -> GeneratedFile {
    let class = java_type_name(&e.name);
    let package = package_for(opts, &e.qualified_name);

    let mut out = String::new();
    out.push_str(&format!(
        "// Auto-generated by int2dds-idl from {}\n// DO NOT EDIT\n\n",
        idl_filename
    ));
    if let Some(p) = &package {
        out.push_str(&format!("package {};\n\n", p));
    }
    out.push_str(&format!("public enum {} {{\n\n", class));

    for (i, v) in e.variants.iter().enumerate() {
        let name = naming::escape_keyword(&v.name, TargetLang::Java);
        let sep = if i + 1 == e.variants.len() { ";" } else { "," };
        out.push_str(&format!("    {}({}){}\n", name, v.value, sep));
    }

    // The wire value is explicit, not ordinal(), so reordering the IDL cannot
    // silently change what goes on the wire.
    out.push_str(&format!(
        "\n    private final int value;\n\n\
         \x20   {class}(int value) {{\n        this.value = value;\n    }}\n\n\
         \x20   public int value() {{\n        return value;\n    }}\n\n\
         \x20   public static {class} fromValue(int value) {{\n\
         \x20       for ({class} v : values()) {{\n\
         \x20           if (v.value == value) {{\n                return v;\n            }}\n\
         \x20       }}\n\
         \x20       throw new IllegalArgumentException(\"unknown {class} value: \" + value);\n\
         \x20   }}\n}}\n",
        class = class
    ));

    GeneratedFile { relative_path: relative_path(&package, &class), source: out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_idl;
    use crate::resolver::resolve;

    fn gen(src: &str, opts: &JavaOptions) -> Vec<GeneratedFile> {
        let defs = parse_idl(src).unwrap();
        let model = resolve(defs).unwrap();
        generate(&model, "HelloWorld.idl", opts).unwrap()
    }

    #[test]
    fn every_corpus_file_generates_one_java_file_per_top_level_type() {
        // Paths are relative to the crate root (idl/).
        let expected: &[(&str, &[&str])] = &[
            ("Arrays.idl", &["ArraysType.java"]),
            ("CdrGolden.idl", &["CdrGolden.java"]),
            (
                "Complex_Arrays.idl",
                &["Point2D.java", "ArrayElement.java", "ComplexArraysType.java"],
            ),
            ("Enum.idl", &["Color.java", "StatusKind.java", "EnumType.java"]),
            ("Floats.idl", &["FloatsType.java"]),
            ("HelloWorld.idl", &["HelloWorld.java"]),
            ("Integers.idl", &["IntegersType.java"]),
            ("LatencyTestData.idl", &["LatencyTestData.java"]),
            (
                "Nested_Structs.idl",
                &[
                    "InnerStruct.java",
                    "DeepInnerStruct.java",
                    "MiddleStruct.java",
                    "NestedStructType.java",
                ],
            ),
            ("PerformanceTestData.idl", &["PerformanceTestData.java"]),
            ("Primitives.idl", &["PrimitivesType.java"]),
            ("Sequence_Enum.idl", &["Color.java", "SequenceEnumType.java"]),
            ("Sequences.idl", &["SequencesType.java"]),
            ("Strings.idl", &["StringsType.java"]),
            ("Tuple_Structs.idl", &["Point2D.java", "Point3D.java", "TupleStructType.java"]),
        ];

        for (file, want) in expected {
            let src = std::fs::read_to_string(format!("input/{}", file))
                .unwrap_or_else(|e| panic!("cannot read input/{}: {}", file, e));
            let defs = parse_idl(&src).unwrap_or_else(|e| panic!("{}: parse: {:?}", file, e));
            let model = resolve(defs).unwrap_or_else(|e| panic!("{}: resolve: {:?}", file, e));
            let files = generate(&model, file, &JavaOptions::default())
                .unwrap_or_else(|e| panic!("{}: generate: {}", file, e));

            let mut got: Vec<&str> = files.iter().map(|f| f.relative_path.as_str()).collect();
            let mut want: Vec<&str> = want.to_vec();
            got.sort();
            want.sort();
            assert_eq!(got, want, "{}", file);
        }
    }

    #[test]
    fn unnamed_package_emits_flat_file_with_no_package_line() {
        let files = gen(
            r#"struct HelloWorld { unsigned long index; string message; };"#,
            &JavaOptions::default(),
        );
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].relative_path, "HelloWorld.java");
        assert!(!files[0].source.contains("package "), "{}", files[0].source);
        assert!(
            files[0].source.starts_with(
                "// Auto-generated by int2dds-idl from HelloWorld.idl\n// DO NOT EDIT\n"
            ),
            "{}",
            files[0].source
        );
    }

    #[test]
    fn named_package_nests_the_path_and_declares_the_package() {
        let files = gen(
            r#"struct HelloWorld { unsigned long index; };"#,
            &JavaOptions { package: Some("com.intellectus.int2dds.examples".to_string()) },
        );
        assert_eq!(files[0].relative_path, "com/intellectus/int2dds/examples/HelloWorld.java");
        assert!(files[0].source.contains("package com.intellectus.int2dds.examples;"));
    }

    #[test]
    fn package_segment_matching_a_keyword_gets_escaped() {
        // "generated.enum" would compile as a syntax error: `enum` is reserved.
        let files = gen(
            r#"struct HelloWorld { unsigned long index; };"#,
            &JavaOptions { package: Some("generated.enum".to_string()) },
        );
        assert_eq!(files[0].relative_path, "generated/enum_/HelloWorld.java");
        assert!(files[0].source.contains("package generated.enum_;"), "{}", files[0].source);
    }

    #[test]
    fn scalar_struct_round_trips_through_the_cdr_api() {
        let files = gen(
            r#"struct HelloWorld { unsigned long index; string message; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("public final class HelloWorld implements IDdsType"), "{}", src);
        assert!(src.contains("public int index;"), "{}", src);
        assert!(src.contains("public String message = \"\";"), "{}", src);
        assert!(src.contains("return \"HelloWorld\";"), "{}", src);
        // The default extensibility is APPENDABLE, so a DHEADER wraps the body.
        assert!(src.contains("return Extensibility.APPENDABLE;"), "{}", src);
        assert!(src.contains("int token = writer.dheaderBegin();"), "{}", src);
        assert!(src.contains("writer.writeU32(this.index);"), "{}", src);
        assert!(src.contains("writer.writeString(this.message);"), "{}", src);
        assert!(src.contains("writer.dheaderFinalize(token);"), "{}", src);
        assert!(src.contains("CdrReader.Dheader d = reader.readDheader();"), "{}", src);
        assert!(src.contains("this.index = reader.readU32();"), "{}", src);
        assert!(src.contains("this.message = reader.readString();"), "{}", src);
        assert!(src.contains("reader.readDheaderEnd(d);"), "{}", src);
    }

    #[test]
    fn module_scoped_struct_advertises_the_qualified_type_name() {
        // The Rust and C# backends register the qualified name. A leaf-only name
        // here would stop participants built from the same IDL from matching.
        let files = gen(
            r#"module app { @extensibility(FINAL) struct Use { long v; }; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("return \"app::Use\";"), "{}", src);
        // The class name stays the leaf; the package carries the module.
        assert!(src.contains("public final class Use implements IDdsType"), "{}", src);
    }

    #[test]
    fn module_less_struct_keeps_the_bare_type_name() {
        // The corpus and java/examples have no module, so this path must not change.
        let files = gen(r#"struct HelloWorld { unsigned long index; };"#, &JavaOptions::default());
        assert!(files[0].source.contains("return \"HelloWorld\";"), "{}", files[0].source);
    }

    #[test]
    fn final_struct_writes_no_dheader() {
        let files =
            gen(r#"@extensibility(FINAL) struct Flat { long a; };"#, &JavaOptions::default());
        let src = &files[0].source;
        assert!(src.contains("return Extensibility.FINAL;"), "{}", src);
        assert!(!src.contains("dheaderBegin"), "{}", src);
        assert!(!src.contains("readDheader"), "{}", src);
    }

    #[test]
    fn every_scalar_maps_to_its_cdr_method() {
        let files = gen(
            r#"@extensibility(FINAL) struct S {
                boolean b; octet o; uint8 u8; int8 i8; char c;
                short i16; unsigned short u16; long i32; unsigned long u32;
                long long i64; unsigned long long u64; float f; double d;
            };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        for expected in [
            "public boolean b;",
            "public byte o;",
            "public byte u8;",
            "public byte i8;",
            "public byte c;",
            "public short i16;",
            "public short u16;",
            "public int i32;",
            "public int u32;",
            "public long i64;",
            "public long u64;",
            "public float f;",
            "public double d;",
            "writer.writeBool(this.b);",
            "writer.writeU8(this.o & 0xFF);",
            "writer.writeU8(this.u8 & 0xFF);",
            "writer.writeI8(this.i8);",
            "writer.writeU8(this.c & 0xFF);",
            "writer.writeI16(this.i16);",
            "writer.writeU16(this.u16 & 0xFFFF);",
            "writer.writeI32(this.i32);",
            "writer.writeU32(this.u32);",
            "writer.writeI64(this.i64);",
            "writer.writeU64(this.u64);",
            "writer.writeF32(this.f);",
            "writer.writeF64(this.d);",
            "this.o = (byte) reader.readU8();",
            "this.u16 = (short) reader.readU16();",
        ] {
            assert!(src.contains(expected), "missing {:?} in\n{}", expected, src);
        }
    }

    #[test]
    fn field_names_become_camel_case_and_dodge_keywords() {
        let files = gen(
            r#"@extensibility(FINAL) struct S { long bool_seq; long class; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("public int boolSeq;"), "{}", src);
        assert!(src.contains("public int class_;"), "{}", src);
    }

    #[test]
    fn bounded_string_is_byte_checked_on_write() {
        // The core measures string<N> in UTF-8 bytes, so length() would pass a
        // non-ASCII string the core then rejects. wstring is the opposite.
        let files =
            gen(r#"@extensibility(FINAL) struct S { string<256> s; };"#, &JavaOptions::default());
        let src = &files[0].source;
        assert!(src.contains("if (CdrWriter.utf8Length(this.s) > 256)"), "{}", src);
        assert!(!src.contains("this.s.length()"), "{}", src);
    }

    #[test]
    fn enum_becomes_a_java_enum_with_explicit_values() {
        let files = gen(
            r#"enum Color { RED, GREEN, BLUE };
               @extensibility(FINAL) struct S { Color color; };"#,
            &JavaOptions::default(),
        );
        assert_eq!(files.len(), 2);
        let e = files.iter().find(|f| f.relative_path == "Color.java").unwrap();
        assert!(e.source.contains("public enum Color {"), "{}", e.source);
        assert!(e.source.contains("RED(0),"), "{}", e.source);
        assert!(e.source.contains("GREEN(1),"), "{}", e.source);
        assert!(e.source.contains("BLUE(2);"), "{}", e.source);
        assert!(e.source.contains("public int value()"), "{}", e.source);
        assert!(e.source.contains("public static Color fromValue(int value)"), "{}", e.source);
    }

    #[test]
    fn enum_member_defaults_to_the_first_variant_and_uses_write_enum() {
        let files = gen(
            r#"enum Color { RED, GREEN };
               @extensibility(FINAL) struct S { Color color; };"#,
            &JavaOptions::default(),
        );
        let s = files.iter().find(|f| f.relative_path == "S.java").unwrap();
        // A null field would NPE in serializeCdr, so it starts at the first variant.
        assert!(s.source.contains("public Color color = Color.RED;"), "{}", s.source);
        assert!(s.source.contains("writer.writeEnum(this.color.value());"), "{}", s.source);
        assert!(
            s.source.contains("this.color = Color.fromValue(reader.readEnum());"),
            "{}",
            s.source
        );
    }

    #[test]
    fn enum_shares_the_struct_package() {
        let files = gen(
            r#"enum Color { RED };
               @extensibility(FINAL) struct S { Color color; };"#,
            &JavaOptions { package: Some("a.b".to_string()) },
        );
        assert!(files.iter().any(|f| f.relative_path == "a/b/Color.java"));
        assert!(files.iter().any(|f| f.relative_path == "a/b/S.java"));
    }

    #[test]
    fn out_of_scope_constructs_are_refused_by_name() {
        let cases = [
            (r#"union U switch (long) { case 1: long a; };"#, "union 'U'"),
            (r#"@bit_bound(8) bitmask M { FLAG_A };"#, "bitmask 'M'"),
            (r#"@extensibility(MUTABLE) struct M { long a; };"#, "MUTABLE struct 'M'"),
            (r#"struct S { @optional long a; };"#, "@optional member 'S.a'"),
        ];
        for (src, needle) in cases {
            let defs = parse_idl(src).unwrap();
            let model = resolve(defs).unwrap();
            let err = generate(&model, "X.idl", &JavaOptions::default())
                .expect_err(&format!("should reject: {}", src));
            assert!(err.contains(needle), "error {:?} should mention {:?}", err, needle);
        }
    }

    #[test]
    fn malformed_java_packages_are_refused() {
        let src = r#"@extensibility(FINAL) struct S { long a; };"#;
        let defs = parse_idl(src).unwrap();
        let model = resolve(defs).unwrap();
        // These used to emit `package .a..b.;` or a directory with a space, exit 0.
        for bad in [".a..b.", "com. x", "1st.pkg", "a.b-c", "a..b"] {
            let err = generate(&model, "S.idl", &JavaOptions { package: Some(bad.to_string()) })
                .expect_err(&format!("{:?} should be refused", bad));
            assert!(err.contains("--java-package"), "{}", err);
            assert!(err.contains(bad), "{}", err);
        }
        // Valid packages and the empty (unnamed) case still pass.
        for ok in ["com.intellectus.int2dds.examples", "generated.enum", "_a.$b.c1", ""] {
            generate(&model, "S.idl", &JavaOptions { package: Some(ok.to_string()) })
                .unwrap_or_else(|e| panic!("{:?} should be accepted: {}", ok, e));
        }
    }

    #[test]
    fn colliding_generated_names_are_refused() {
        // foo_bar and FooBar both map to FooBar.java. One used to vanish silently
        // and fields referencing it bound to the survivor, sending its layout.
        for src in [
            r#"@extensibility(FINAL) struct foo_bar { long a; };
               @extensibility(FINAL) struct FooBar { double b; };"#,
            r#"enum foo_bar { RED };
               @extensibility(FINAL) struct FooBar { long a; };"#,
        ] {
            let defs = parse_idl(src).unwrap();
            let model = resolve(defs).unwrap();
            let err = generate(&model, "X.idl", &JavaOptions::default())
                .expect_err("should reject the colliding path");
            assert!(err.contains("foo_bar"), "{}", err);
            assert!(err.contains("FooBar"), "{}", err);
            assert!(err.contains("FooBar.java"), "{}", err);
        }

        // Names that collide within one file. Each used to emit code javac rejects.
        let cases = [
            // Two members that merge under camelCase. The resolver does not reject
            // duplicate member names, so identical ones reach here too.
            (r#"struct S { long my_type; long myType; };"#, "same Java field 'myType'"),
            (r#"struct S { long a; long a; };"#, "same Java field 'a'"),
            // A collision created by keyword escaping: int -> int_.
            (r#"enum E { int, int_ };"#, "same Java constant 'int_'"),
            // An enumerator colliding with the generated value field.
            (r#"enum E { value, OTHER };"#, "collides with the generated 'value' field"),
        ];
        for (src, needle) in cases {
            let defs = parse_idl(src).unwrap();
            let model = resolve(defs).unwrap();
            let err = generate(&model, "X.idl", &JavaOptions::default())
                .expect_err(&format!("should reject: {}", src));
            assert!(err.contains(needle), "error {:?} should mention {:?}", err, needle);
        }

        // values/fromValue are methods, a different namespace from the constants.
        let defs = parse_idl(r#"enum E { values, fromValue, E };"#).unwrap();
        let model = resolve(defs).unwrap();
        generate(&model, "X.idl", &JavaOptions::default()).expect("method names do not collide");
    }

    #[test]
    fn cross_package_references_are_refused() {
        // References go out as bare leaf names with no import, so javac reports
        // `cannot find symbol`. Generation used to succeed and only the compile broke.
        for src in [
            r#"module a { @extensibility(FINAL) struct X { long v; }; };
               module b { @extensibility(FINAL) struct Y { a::X x; long t; }; };"#,
            r#"module a { enum Color { RED }; };
               module b { @extensibility(FINAL) struct Y { a::Color x; }; };"#,
            // Collection and array elements must be caught too.
            r#"module a { @extensibility(FINAL) struct X { long v; }; };
               module b { @extensibility(FINAL) struct Y { sequence<a::X> x; }; };"#,
            r#"module a { @extensibility(FINAL) struct X { long v; }; };
               module b { @extensibility(FINAL) struct Y { a::X x[2]; }; };"#,
        ] {
            let defs = parse_idl(src).unwrap();
            let model = resolve(defs).unwrap();
            let err = generate(&model, "AB.idl", &JavaOptions::default())
                .expect_err("should reject the cross-package reference");
            assert!(err.contains("cross-package reference 'Y.x'"), "{}", err);
            assert!(err.contains("package a"), "{}", err);
            assert!(err.contains("package b"), "{}", err);
        }
    }

    #[test]
    fn same_module_references_still_generate() {
        // The same module is the same package, so this must not be rejected.
        let files = gen(
            r#"module a {
                 @extensibility(FINAL) struct X { long v; };
                 @extensibility(FINAL) struct Y { X x; };
               };"#,
            &JavaOptions::default(),
        );
        assert!(files.iter().any(|f| f.relative_path == "a/Y.java"), "{:?}", files);
        let y = files.iter().find(|f| f.relative_path == "a/Y.java").unwrap();
        assert!(y.source.contains("public X x = new X();"), "{}", y.source);
    }

    #[test]
    fn wstring_maps_to_string_and_uses_the_wide_accessors() {
        // The spec maps both string and wstring to java.lang.String.
        let files = gen(
            r#"@extensibility(FINAL) struct W { wstring ws; wstring<4> bounded; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("public String ws = \"\";"), "{}", src);
        assert!(src.contains("writer.writeWString(this.ws);"), "{}", src);
        assert!(src.contains("this.ws = reader.readWString();"), "{}", src);
        assert!(src.contains("if (this.bounded.length() > 4) {"), "{}", src);
    }

    #[test]
    fn wstring_collections_recurse_into_the_element() {
        for (src, decl) in [
            (r#"@extensibility(FINAL) struct W { sequence<wstring> ws; };"#, "public String[] ws"),
            (r#"@extensibility(FINAL) struct W { wstring ws[2]; };"#, "public String[] ws"),
        ] {
            let files = gen(src, &JavaOptions::default());
            let out = &files[0].source;
            assert!(out.contains(decl), "{}", out);
            assert!(out.contains("writer.writeWString(this.ws[i0]);"), "{}", out);
            assert!(out.contains("this.ws[i0] = reader.readWString();"), "{}", out);
        }
    }

    #[test]
    fn wchar_still_round_trips() {
        let files = gen(r#"@extensibility(FINAL) struct S { wchar c; };"#, &JavaOptions::default());
        let src = &files[0].source;
        assert!(src.contains("public char c;"), "{}", src);
        assert!(src.contains("writer.writeU16(this.c);"), "{}", src);
        assert!(src.contains("this.c = (char) reader.readU16();"), "{}", src);
    }

    #[test]
    fn byte_sequence_uses_the_bulk_path() {
        let files = gen(
            r#"@extensibility(FINAL) struct S { sequence<octet> data; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("public byte[] data = new byte[0];"), "{}", src);
        assert!(src.contains("writer.writeSeqHeader(this.data.length);"), "{}", src);
        assert!(src.contains("writer.writeBytes(this.data);"), "{}", src);
        assert!(src.contains("this.data = reader.readBytes(reader.readSeqHeader());"), "{}", src);
    }

    #[test]
    fn scalar_sequence_loops_without_boxing() {
        let files = gen(
            r#"@extensibility(FINAL) struct S { sequence<long> v; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("public int[] v = new int[0];"), "{}", src);
        assert!(src.contains("writer.writeSeqHeader(this.v.length);"), "{}", src);
        assert!(src.contains("for (int i0 = 0; i0 < this.v.length; i0++) {"), "{}", src);
        assert!(src.contains("writer.writeI32(this.v[i0]);"), "{}", src);
        assert!(src.contains("this.v = new int[reader.readSeqHeader()];"), "{}", src);
        assert!(src.contains("this.v[i0] = reader.readI32();"), "{}", src);
    }

    #[test]
    fn string_sequence_allocates_and_fills_on_read() {
        let files = gen(
            r#"@extensibility(FINAL) struct S { sequence<string> v; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("public String[] v = new String[0];"), "{}", src);
        assert!(src.contains("writer.writeString(this.v[i0]);"), "{}", src);
        assert!(src.contains("this.v[i0] = reader.readString();"), "{}", src);
    }

    #[test]
    fn bounded_sequence_is_length_checked_on_write() {
        let files = gen(
            r#"@extensibility(FINAL) struct S { sequence<long, 10> v; };"#,
            &JavaOptions::default(),
        );
        assert!(files[0].source.contains("if (this.v.length > 10)"), "{}", files[0].source);
    }

    #[test]
    fn fixed_array_has_no_seq_header_and_checks_its_length() {
        let files =
            gen(r#"@extensibility(FINAL) struct S { long m[4]; };"#, &JavaOptions::default());
        let src = &files[0].source;
        assert!(src.contains("public int[] m = new int[4];"), "{}", src);
        assert!(src.contains("if (this.m.length != 4)"), "{}", src);
        assert!(!src.contains("writeSeqHeader(this.m"), "{}", src);
        assert!(src.contains("for (int i0 = 0; i0 < 4; i0++) {"), "{}", src);
        assert!(src.contains("writer.writeI32(this.m[i0]);"), "{}", src);
        assert!(src.contains("this.m[i0] = reader.readI32();"), "{}", src);
    }

    #[test]
    fn enum_sequence_round_trips() {
        let files = gen(
            r#"enum Color { RED, GREEN };
               @extensibility(FINAL) struct S { sequence<Color> v; };"#,
            &JavaOptions::default(),
        );
        let s = files.iter().find(|f| f.relative_path == "S.java").unwrap();
        assert!(s.source.contains("public Color[] v = new Color[0];"), "{}", s.source);
        assert!(s.source.contains("writer.writeEnum(this.v[i0].value());"), "{}", s.source);
        assert!(
            s.source.contains("this.v[i0] = Color.fromValue(reader.readEnum());"),
            "{}",
            s.source
        );
    }

    #[test]
    fn nested_collections_are_refused() {
        // Java array creation needs the sized dimension first (`new int[0][]`),
        // and this backend has no test that a nested encoding round-trips.
        for src in [
            r#"@extensibility(FINAL) struct S { sequence<sequence<long> > v; };"#,
            r#"@extensibility(FINAL) struct S { long m[3][4]; };"#,
        ] {
            let defs = parse_idl(src).unwrap();
            let model = resolve(defs).unwrap();
            let err = generate(&model, "S.idl", &JavaOptions::default()).unwrap_err();
            assert!(err.contains("S."), "{}", err);
        }
    }

    #[test]
    fn zero_variant_enum_is_refused() {
        // An empty Java enum body has no `;` before the class members, so javac
        // rejects it -- and enum_default would leave the field null besides.
        let defs =
            parse_idl(r#"enum Color { }; @extensibility(FINAL) struct S { long a; };"#).unwrap();
        let model = resolve(defs).unwrap();
        let err = generate(&model, "X.idl", &JavaOptions::default()).unwrap_err();
        assert!(err.contains("Color"), "{}", err);
    }

    #[test]
    fn nested_struct_delegates_and_never_starts_null() {
        let files = gen(
            r#"struct Inner { long x; };
               struct Outer { Inner inner; };"#,
            &JavaOptions::default(),
        );
        assert_eq!(files.len(), 2);
        let o = files.iter().find(|f| f.relative_path == "Outer.java").unwrap();
        assert!(o.source.contains("public Inner inner = new Inner();"), "{}", o.source);
        assert!(o.source.contains("inner.serializeCdr(writer);"), "{}", o.source);
        assert!(o.source.contains("inner.deserializeCdr(reader);"), "{}", o.source);
    }

    #[test]
    fn nested_struct_carries_its_own_dheader() {
        // The default extensibility is APPENDABLE, so a nested type with no
        // @extensibility writes its own DHEADER. The parent must not write it.
        let files = gen(
            r#"struct Inner { long x; };
               struct Outer { Inner inner; };"#,
            &JavaOptions::default(),
        );
        let i = files.iter().find(|f| f.relative_path == "Inner.java").unwrap();
        assert!(i.source.contains("return Extensibility.APPENDABLE;"), "{}", i.source);
        assert!(i.source.contains("int token = writer.dheaderBegin();"), "{}", i.source);
        let o = files.iter().find(|f| f.relative_path == "Outer.java").unwrap();
        // Outer opens exactly one DHEADER of its own.
        assert_eq!(o.source.matches("dheaderBegin()").count(), 1, "{}", o.source);
    }

    #[test]
    fn struct_sequence_constructs_each_element_on_read() {
        let files = gen(
            r#"struct Inner { long x; };
               @extensibility(FINAL) struct Outer { sequence<Inner> items; };"#,
            &JavaOptions::default(),
        );
        let o = files.iter().find(|f| f.relative_path == "Outer.java").unwrap();
        assert!(o.source.contains("public Inner[] items = new Inner[0];"), "{}", o.source);
        assert!(o.source.contains("items[i0] = new Inner();"), "{}", o.source);
        assert!(o.source.contains("items[i0].deserializeCdr(reader);"), "{}", o.source);
    }

    #[test]
    fn reference_element_fixed_array_is_filled_by_a_constructor() {
        let files = gen(
            r#"struct Inner { long x; };
               @extensibility(FINAL) struct Outer { Inner cells[4]; string names[3]; };"#,
            &JavaOptions::default(),
        );
        let o = files.iter().find(|f| f.relative_path == "Outer.java").unwrap();
        // new Inner[4] fills with null, which would NPE in serializeCdr.
        assert!(o.source.contains("public Outer() {"), "{}", o.source);
        assert!(o.source.contains("cells[i] = new Inner();"), "{}", o.source);
        assert!(o.source.contains("names[i] = \"\";"), "{}", o.source);
    }

    #[test]
    fn primitive_fixed_array_needs_no_constructor() {
        let files =
            gen(r#"@extensibility(FINAL) struct S { long m[4]; };"#, &JavaOptions::default());
        assert!(!files[0].source.contains("public S() {"), "{}", files[0].source);
    }

    #[test]
    fn keyed_struct_emits_the_descriptor_prefix() {
        let files = gen(
            r#"@extensibility(APPENDABLE) struct S {
                 @key long id;
                 sequence<long> payload;
               };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(
            src.contains("import com.intellectus.int2dds.core.TopicFieldDescriptor;"),
            "{}",
            src
        );
        assert!(src.contains("import com.intellectus.int2dds.xtypes.FieldType;"), "{}", src);
        assert!(src.contains("public static List<TopicFieldDescriptor> ddsFields()"), "{}", src);
        assert!(src.contains("new TopicFieldDescriptor(\"id\", FieldType.INT32, true)"), "{}", src);
        // The prefix ends at the last key, so the sequence after it cannot ride along.
        assert!(!src.contains("\"payload\""), "{}", src);
    }

    #[test]
    fn unkeyed_struct_emits_no_descriptors() {
        let files = gen(
            r#"struct HelloWorld { unsigned long index; string message; };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(!src.contains("ddsFields"), "{}", src);
        assert!(!src.contains("TopicFieldDescriptor"), "{}", src);
    }

    #[test]
    fn descriptor_prefix_spans_every_field_up_to_the_last_key() {
        let files = gen(
            r#"@extensibility(FINAL) struct S {
                 long a; @key string b; @key long c;
               };"#,
            &JavaOptions::default(),
        );
        let src = &files[0].source;
        assert!(src.contains("new TopicFieldDescriptor(\"a\", FieldType.INT32, false)"), "{}", src);
        assert!(src.contains("new TopicFieldDescriptor(\"b\", FieldType.STRING, true)"), "{}", src);
        assert!(src.contains("new TopicFieldDescriptor(\"c\", FieldType.INT32, true)"), "{}", src);
    }

    #[test]
    fn an_undescribable_field_before_a_key_fails_generation() {
        // This native path cannot describe float or wstring. Dropping the key
        // silently would leave no way to see why keying stopped working.
        for src in [
            r#"@extensibility(FINAL) struct S { float bad; @key long id; };"#,
            r#"@extensibility(FINAL) struct S { wstring bad; @key long id; };"#,
        ] {
            let defs = parse_idl(src).unwrap();
            let model = resolve(defs).unwrap();
            let err = generate(&model, "S.idl", &JavaOptions::default()).unwrap_err();
            assert!(err.contains("S.bad"), "{}", err);
            assert!(err.contains("key"), "{}", err);
        }
    }
}
