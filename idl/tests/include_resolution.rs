//! Integration tests for `#include` resolution (ROS2 cross-package references).

use std::fs;
use std::path::PathBuf;

use int2dds_idl::{parser, preprocess, resolver};

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
