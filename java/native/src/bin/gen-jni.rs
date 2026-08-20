//! Regenerates the JNI binding layer.
//!
//! Run from anywhere in the repository:
//!     cargo run -p int2dds-java --bin gen-jni
//!
//! Deliberately a separate binary rather than a build script: writing into the
//! source tree from `build.rs` would dirty the working tree on every build.

use int2dds_java::gen::{
    emit_java::emit_java, emit_java_panama::emit_java_panama, emit_rust::emit_rust,
    parse::parse_ffi_dir,
};
use std::path::{Path, PathBuf};

/// Normalise the emitted Rust with the repo's own rustfmt settings.
///
/// The pre-commit hook runs `cargo fmt --all`, so output that is not already
/// rustfmt-canonical would be rewritten the moment it is committed and the
/// drift check would then fail on every run, forever. Formatting here makes
/// `cargo fmt` a no-op on the generated file. A missing rustfmt is a hard error
/// rather than a warning: silently skipping it reintroduces exactly that trap.
fn rustfmt(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("rustfmt")
        .arg("--edition")
        .arg("2021")
        .arg(path)
        .status()
        .map_err(|e| {
            format!("run rustfmt: {e}\nInstall it with `rustup component add rustfmt`.")
        })?;
    if !status.success() {
        return Err(format!("rustfmt failed on {}", path.display()));
    }
    Ok(())
}

fn main() -> Result<(), String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|e| format!("resolve repo root: {e}"))?;

    let fns = parse_ffi_dir(&root.join("ffi/src"))?;
    let total = fns.len();

    let rust_path = root.join("java/native/src/generated.rs");
    let java_path =
        root.join("java/api/src/main/java/com/intellectus/int2dds/internal/ffi/Ffi.java");
    let java_panama_path =
        root.join("java/api/src/main/java22/com/intellectus/int2dds/internal/ffi/Ffi.java");

    if let Some(parent) = java_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    if let Some(parent) = java_panama_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir: {e}"))?;
    }
    std::fs::write(&rust_path, emit_rust(&fns)).map_err(|e| format!("write rust: {e}"))?;
    std::fs::write(&java_path, emit_java(&fns)).map_err(|e| format!("write java: {e}"))?;
    std::fs::write(&java_panama_path, emit_java_panama(&fns))
        .map_err(|e| format!("write java22: {e}"))?;
    rustfmt(&rust_path)?;

    let generated = fns.iter().filter(|f| int2dds_java::gen::typemap::is_generatable(f)).count();
    println!(
        "parsed {total} FFI functions; generated {generated}; hand-written {}",
        total - generated
    );
    println!("  {}", rust_path.display());
    println!("  {}", java_path.display());
    println!("  {}", java_panama_path.display());
    Ok(())
}
