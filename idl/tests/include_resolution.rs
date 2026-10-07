//! Integration tests for `#include` resolution (ROS2 cross-package references).

use std::fs;
use std::path::PathBuf;

use int2dds_idl::{codegen, naming, parser, preprocess, resolver};

fn unique_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("int2dds_idl_inc_{}_{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

#[test]
fn test_cross_package_include_resolves() {
    let root = unique_dir("xpkg");
    fs::create_dir_all(root.join("std_msgs/msg")).unwrap();
    fs::create_dir_all(root.join("sensor_msgs/msg")).unwrap();
    fs::write(
        root.join("std_msgs/msg/Header.idl"),
        "module std_msgs { module msg { struct Header { uint32 stamp; }; }; };\n",
    )
    .unwrap();
    let imu = root.join("sensor_msgs/msg/Imu.idl");
    fs::write(
        &imu,
        r#"#include "std_msgs/msg/Header.idl"
module sensor_msgs { module msg {
  struct Imu { std_msgs::msg::Header header; double v[3]; };
}; };
"#,
    )
    .unwrap();

    let (source, missing) =
        preprocess::load_with_includes(&imu, std::slice::from_ref(&root)).unwrap();
    assert!(missing.is_empty(), "unexpected missing includes: {:?}", missing);

    let defs = parser::parse_idl(&source).expect("parse");
    let model = resolver::resolve(defs).expect("resolve");

    // Both the included Header and the including Imu are present and resolved.
    assert!(model.structs.iter().any(|s| s.name == "Header"));
    let imu_struct = model.structs.iter().find(|s| s.name == "Imu").unwrap();
    assert!(matches!(
        &imu_struct.members[0].resolved_type,
        int2dds_idl::types::ResolvedType::Struct(n) if n.ends_with("Header")
    ));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn test_scoped_resolve_does_not_emit_included_types() {
    // Mirrors the CLI pipeline: full TU resolves cross-package refs, but the
    // generated model contains only the root file's own types (resolve-only).
    let root = unique_dir("scoped");
    fs::create_dir_all(root.join("std_msgs/msg")).unwrap();
    fs::create_dir_all(root.join("sensor_msgs/msg")).unwrap();
    fs::write(
        root.join("std_msgs/msg/Header.idl"),
        "module std_msgs { module msg { struct Header { uint32 stamp; }; }; };\n",
    )
    .unwrap();
    let imu = root.join("sensor_msgs/msg/Imu.idl");
    fs::write(
        &imu,
        r#"#include "std_msgs/msg/Header.idl"
module sensor_msgs { module msg {
  struct Imu { std_msgs::msg::Header header; double v[3]; };
}; };
"#,
    )
    .unwrap();

    let (merged, _missing) =
        preprocess::load_with_includes(&imu, std::slice::from_ref(&root)).unwrap();
    let all_defs = parser::parse_idl(&merged).expect("parse all");
    let root_src = fs::read_to_string(&imu).unwrap();
    let root_defs = parser::parse_idl(&root_src).expect("parse root");

    let model = resolver::resolve_scoped(&root_defs, all_defs).expect("resolve scoped");

    // Only Imu is emitted; the included Header is gone but still resolved as a member.
    assert_eq!(model.structs.len(), 1);
    assert_eq!(model.structs[0].name, "Imu");
    assert!(matches!(
        &model.structs[0].members[0].resolved_type,
        int2dds_idl::types::ResolvedType::Struct(n) if n.ends_with("Header")
    ));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn test_cross_file_nested_key_codegen() {
    // A `@key` field whose type is a struct from an `#include`d file must produce
    // working Python and C#: the imported type is referenced by its leaf name
    // (import in Python; shared namespace in C#) and resolves for (de)serialization.
    let root = unique_dir("xkey");
    fs::create_dir_all(&root).unwrap();
    let header = root.join("header.idl");
    fs::write(
        &header,
        "module dep { module msg {
            @final struct Header { uint32 stamp; uint32 seq; };
        }; };\n",
    )
    .unwrap();
    let msg = root.join("msg.idl");
    fs::write(
        &msg,
        r#"#include "header.idl"
module app { module msg {
  @final struct Msg { @key dep::msg::Header header; long x; };
}; };
"#,
    )
    .unwrap();

    // Mirror the CLI pipeline: full TU resolves the cross-file ref; only the root
    // file's types are emitted; imported defs are retained for key recursion.
    let (merged, _m) = preprocess::load_with_includes(&msg, std::slice::from_ref(&root)).unwrap();
    let all_defs = parser::parse_idl(&merged).unwrap();
    let root_defs = parser::parse_idl(&fs::read_to_string(&msg).unwrap()).unwrap();
    let mut model = resolver::resolve_scoped(&root_defs, all_defs).unwrap();

    // Map the imported type to the module its own file is emitted into (as the CLI
    // does): every name declared in header.idl -> `header`.
    let hdr_defs = parser::parse_idl(&fs::read_to_string(&header).unwrap()).unwrap();
    for q in resolver::declared_qualified_names(&hdr_defs).unwrap() {
        let leaf = q.rsplit("::").next().unwrap_or(&q).to_string();
        model.imported.modules.entry(leaf).or_insert_with(|| "header".to_string());
        model.imported.modules.insert(q, "header".to_string());
    }
    assert_eq!(naming::idl_to_output_name("header.idl"), "header");

    let py = codegen::python::generate(
        &model,
        "msg.idl",
        &codegen::python::PythonOptions { int2dds_module: "int2dds".to_string() },
    );
    // Import emitted and leaf-named references resolve the imported nested type.
    assert!(py.contains("from header import Header"), "missing import:\n{py}");
    assert!(py.contains("field(default_factory=lambda: Header())"), "{py}");
    assert!(py.contains("Header._deserialize_cdr_inline(r)"), "{py}");
    assert!(!py.contains("dep::msg::Header"), "leaked qualified name:\n{py}");

    let cs = codegen::csharp::generate(
        &model,
        "msg.idl",
        &codegen::csharp::CSharpOptions { namespace: "GeneratedTypes".to_string() },
    );
    // Shared namespace resolves the leaf name for the imported nested type.
    assert!(cs.contains("public Header Header"), "{cs}");
    assert!(cs.contains("Header.DeserializeCdrInline(r)"), "{cs}");
    assert!(!cs.contains("Dep::msg::Header"), "leaked qualified name:\n{cs}");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn test_java_same_module_include_generates() {
    // An #include'd type declared in the SAME module as the referencing struct
    // must resolve into that module's package and generate successfully.
    let root = unique_dir("java_same_mod");
    fs::create_dir_all(&root).unwrap();
    let base = root.join("base.idl");
    fs::write(
        &base,
        "module a { enum Color { RED, GREEN }; struct Point { double x; double y; }; };\n",
    )
    .unwrap();
    let same_mod = root.join("same_mod.idl");
    fs::write(
        &same_mod,
        "#include \"base.idl\"\nmodule a { struct Wrap { Point p; Color c; long t; }; };\n",
    )
    .unwrap();

    let (merged, _m) =
        preprocess::load_with_includes(&same_mod, std::slice::from_ref(&root)).unwrap();
    let all_defs = parser::parse_idl(&merged).unwrap();
    let root_defs = parser::parse_idl(&fs::read_to_string(&same_mod).unwrap()).unwrap();
    let model = resolver::resolve_scoped(&root_defs, all_defs).unwrap();

    let files =
        codegen::java::generate(&model, "same_mod.idl", &codegen::java::JavaOptions::default())
            .expect("same-module included reference must generate");
    let wrap = files.iter().find(|f| f.relative_path.ends_with("Wrap.java")).unwrap();
    assert!(wrap.source.contains("package a;"), "{}", wrap.source);
    assert!(wrap.source.contains("public Point p"), "{}", wrap.source);
    // The included enum must still yield a default, or the field stays null and
    // serializeCdr throws NPE on the first write.
    assert!(wrap.source.contains("public Color c = Color.RED;"), "{}", wrap.source);

    fs::remove_dir_all(&root).ok();
}

#[test]
fn test_java_cross_module_include_still_refused() {
    // An #include'd type declared in a DIFFERENT module than the referencing
    // struct must still be refused: the guard's whole point is to catch this.
    let root = unique_dir("java_cross_mod");
    fs::create_dir_all(&root).unwrap();
    let base = root.join("base.idl");
    fs::write(&base, "module a { struct Point { double x; double y; }; };\n").unwrap();
    let cross_mod = root.join("cross_mod.idl");
    fs::write(
        &cross_mod,
        "#include \"base.idl\"\nmodule b { struct Wrap { a::Point p; long t; }; };\n",
    )
    .unwrap();

    let (merged, _m) =
        preprocess::load_with_includes(&cross_mod, std::slice::from_ref(&root)).unwrap();
    let all_defs = parser::parse_idl(&merged).unwrap();
    let root_defs = parser::parse_idl(&fs::read_to_string(&cross_mod).unwrap()).unwrap();
    let model = resolver::resolve_scoped(&root_defs, all_defs).unwrap();

    let err =
        codegen::java::generate(&model, "cross_mod.idl", &codegen::java::JavaOptions::default())
            .expect_err("cross-module included reference must still be refused");
    assert!(err.contains("cross-package reference"), "{err}");

    fs::remove_dir_all(&root).ok();
}

#[test]
fn test_missing_include_is_reported_not_fatal() {
    let root = unique_dir("missing");
    fs::create_dir_all(&root).unwrap();
    let f = root.join("A.idl");
    fs::write(&f, "#include \"does/not/Exist.idl\"\nstruct A { long x; };\n").unwrap();

    let (source, missing) = preprocess::load_with_includes(&f, &[]).unwrap();
    assert_eq!(missing, vec!["does/not/Exist.idl".to_string()]);
    // The local definition still parses/resolves.
    let defs = parser::parse_idl(&source).expect("parse");
    let model = resolver::resolve(defs).expect("resolve");
    assert!(model.structs.iter().any(|s| s.name == "A"));

    fs::remove_dir_all(&root).ok();
}

#[test]
fn test_include_cycle_terminates() {
    let root = unique_dir("cycle");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("A.idl"), "#include \"B.idl\"\nstruct A { long a; };\n").unwrap();
    fs::write(root.join("B.idl"), "#include \"A.idl\"\nstruct B { long b; };\n").unwrap();

    let (source, missing) =
        preprocess::load_with_includes(&root.join("A.idl"), std::slice::from_ref(&root)).unwrap();
    assert!(missing.is_empty());
    let defs = parser::parse_idl(&source).expect("parse");
    let model = resolver::resolve(defs).expect("resolve");
    assert!(model.structs.iter().any(|s| s.name == "A"));
    assert!(model.structs.iter().any(|s| s.name == "B"));

    fs::remove_dir_all(&root).ok();
}
