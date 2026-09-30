#[test]
fn crate_version_matches_workspace_version() {
    // The workspace Cargo.toml is the single source of truth for the product
    // version; the crate inherits it via `version.workspace = true`.
    assert_eq!(int2dds_java::crate_version(), env!("CARGO_PKG_VERSION"));
    assert!(!int2dds_java::crate_version().is_empty());
}
