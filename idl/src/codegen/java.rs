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
    reject_unsupported(model)?;
    let mut files = Vec::new();
    for s in &model.structs {
        files.push(emit_struct(model, s, idl_filename, opts)?);
    }
    for e in &model.enums {
        files.push(emit_enum(e, idl_filename, opts));
    }
    Ok(files)
}

/// Out-of-scope constructs are refused by name. A silently dropped type is a
/// wire mismatch nobody sees until runtime.
fn reject_unsupported(model: &IdlModel) -> Result<(), String> {
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
        }
    }
    Ok(())
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
            parts.push(base.to_string());
        }
    }
    let mut segs: Vec<&str> = qualified_name.split("::").collect();
    segs.pop(); // drop the leaf type name
    for s in segs {
        parts.push(s.to_lowercase());
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
        ResolvedType::Enum(n) => enum_default(model, n),
        _ => None,
    })
}

/// `Type.FIRST_VARIANT` — a null enum field would NPE inside serializeCdr.
fn enum_default(model: &IdlModel, name: &str) -> Option<String> {
    let leaf = name.rsplit("::").next().unwrap_or(name);
    let e = model.enums.iter().find(|e| e.name == leaf || e.qualified_name == name)?;
    let first = e.variants.first()?;
    Some(format!(
        "{}.{}",
        java_type_name(&e.name),
        naming::escape_keyword(&first.name, TargetLang::Java)
    ))
}

// ---- serialization ---------------------------------------------------------

/// Emits the statement that writes `expr` (a Java expression of `t`'s type).
fn emit_write(
    out: &mut String,
    ind: &str,
    expr: &str,
    t: &ResolvedType,
    model: &IdlModel,
) -> Result<(), String> {
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
                out.push_str(&format!(
                    "{ind}if ({expr}.length() > {b}) {{\n\
                     {ind}    throw new IllegalStateException(\
                     \"{expr} exceeds its IDL bound of {b}\");\n\
                     {ind}}}\n",
                    ind = ind,
                    expr = expr,
                    b = b
                ));
            }
            format!("writer.writeString({});", expr)
        }
        ResolvedType::WString { .. } => format!("writer.writeWString({});", expr),
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
    model: &IdlModel,
) -> Result<(), String> {
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

    let mut out = String::new();
    out.push_str(&format!(
        "// Auto-generated by int2dds-idl from {}\n// DO NOT EDIT\n\n",
        idl_filename
    ));
    if let Some(p) = &package {
        out.push_str(&format!("package {};\n\n", p));
    }
    out.push_str("import com.intellectus.int2dds.cdr.CdrReader;\n");
    out.push_str("import com.intellectus.int2dds.cdr.CdrWriter;\n");
    out.push_str("import com.intellectus.int2dds.cdr.Extensibility;\n");
    out.push_str("import com.intellectus.int2dds.types.IDdsType;\n\n");
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

    out.push_str("    @Override\n");
    out.push_str(&format!(
        "    public String typeName() {{\n        return \"{}\";\n    }}\n\n",
        s.name
    ));
    out.push_str("    @Override\n");
    out.push_str(&format!(
        "    public Extensibility extensibility() {{\n        return Extensibility.{};\n    }}\n\n",
        extensibility_const(s.extensibility)
    ));

    // serializeCdr
    out.push_str("    @Override\n    public void serializeCdr(CdrWriter writer) {\n");
    if appendable {
        out.push_str("        int token = writer.dheaderBegin();\n");
    }
    for m in &s.members {
        emit_write(&mut out, "        ", &java_field_name(&m.name), &m.resolved_type, model)?;
    }
    if appendable {
        out.push_str("        writer.dheaderFinalize(token);\n");
    }
    out.push_str("    }\n\n");

    // deserializeCdr
    out.push_str("    @Override\n    public void deserializeCdr(CdrReader reader) {\n");
    if appendable {
        out.push_str("        CdrReader.Dheader d = reader.readDheader();\n");
    }
    for m in &s.members {
        emit_read(&mut out, "        ", &java_field_name(&m.name), &m.resolved_type, model)?;
    }
    if appendable {
        out.push_str("        reader.readDheaderEnd(d);\n");
    }
    out.push_str("    }\n");

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
        // 기본 extensibility 는 APPENDABLE 이므로 DHEADER 를 감싼다.
        assert!(src.contains("return Extensibility.APPENDABLE;"), "{}", src);
        assert!(src.contains("int token = writer.dheaderBegin();"), "{}", src);
        assert!(src.contains("writer.writeU32(index);"), "{}", src);
        assert!(src.contains("writer.writeString(message);"), "{}", src);
        assert!(src.contains("writer.dheaderFinalize(token);"), "{}", src);
        assert!(src.contains("CdrReader.Dheader d = reader.readDheader();"), "{}", src);
        assert!(src.contains("index = reader.readU32();"), "{}", src);
        assert!(src.contains("message = reader.readString();"), "{}", src);
        assert!(src.contains("reader.readDheaderEnd(d);"), "{}", src);
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
            "writer.writeBool(b);",
            "writer.writeU8(o & 0xFF);",
            "writer.writeU8(u8 & 0xFF);",
            "writer.writeI8(i8);",
            "writer.writeU8(c & 0xFF);",
            "writer.writeI16(i16);",
            "writer.writeU16(u16 & 0xFFFF);",
            "writer.writeI32(i32);",
            "writer.writeU32(u32);",
            "writer.writeI64(i64);",
            "writer.writeU64(u64);",
            "writer.writeF32(f);",
            "writer.writeF64(d);",
            "o = (byte) reader.readU8();",
            "u16 = (short) reader.readU16();",
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
    fn bounded_string_is_length_checked_on_write() {
        let files =
            gen(r#"@extensibility(FINAL) struct S { string<256> s; };"#, &JavaOptions::default());
        assert!(files[0].source.contains("if (s.length() > 256)"), "{}", files[0].source);
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
        // null 이면 serializeCdr 이 NPE 를 낸다 — 첫 변형으로 초기화한다.
        assert!(s.source.contains("public Color color = Color.RED;"), "{}", s.source);
        assert!(s.source.contains("writer.writeEnum(color.value());"), "{}", s.source);
        assert!(s.source.contains("color = Color.fromValue(reader.readEnum());"), "{}", s.source);
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
    fn zero_variant_enum_is_refused() {
        // An empty Java enum body has no `;` before the class members, so javac
        // rejects it -- and enum_default would leave the field null besides.
        let defs =
            parse_idl(r#"enum Color { }; @extensibility(FINAL) struct S { long a; };"#).unwrap();
        let model = resolve(defs).unwrap();
        let err = generate(&model, "X.idl", &JavaOptions::default()).unwrap_err();
        assert!(err.contains("Color"), "{}", err);
    }
}
