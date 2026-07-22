/// Minimal IDL preprocessor: resolves `#include` directives by loading and
/// merging referenced files into a single translation unit.
///
/// ROS2 (rosidl) IDL files `#include` types from other packages
/// (e.g. `std_msgs/msg/Header.idl`). The lexer otherwise skips `#include`
/// lines, so without this pass those external types are unresolved.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Load `root` and every file it transitively `#include`s, concatenated into one
/// source string with dependencies emitted before their dependents.
///
/// Each include is resolved relative to the including file's directory first,
/// then against each `include_dirs` entry. Unresolvable includes are returned in
/// the second tuple element and skipped (preserving single-file behavior).
pub fn load_with_includes(
    root: &Path,
    include_dirs: &[PathBuf],
) -> std::io::Result<(String, Vec<String>)> {
    let (source, missing, _loaded) = load_with_includes_ex(root, include_dirs)?;
    Ok((source, missing))
}

/// Like [`load_with_includes`] but also returns every file loaded, in dependency
/// order (dependencies before dependents, root last). The CLI uses this to map
/// each `#include`d type to the output module its own file will be emitted into.
pub fn load_with_includes_ex(
    root: &Path,
    include_dirs: &[PathBuf],
) -> std::io::Result<(String, Vec<String>, Vec<PathBuf>)> {
    let mut visited = HashSet::new();
    let mut out = String::new();
    let mut missing = Vec::new();
    let mut loaded = Vec::new();
    load_recursive(root, include_dirs, &mut visited, &mut out, &mut missing, &mut loaded)?;
    Ok((out, missing, loaded))
}

fn load_recursive(
    file: &Path,
    include_dirs: &[PathBuf],
    visited: &mut HashSet<PathBuf>,
    out: &mut String,
    missing: &mut Vec<String>,
    loaded: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let key = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    if !visited.insert(key) {
        return Ok(());
    }

    let source = std::fs::read_to_string(file)?;
    let base_dir = file.parent().unwrap_or_else(|| Path::new("."));

    for inc in source.lines().filter_map(parse_include) {
        match resolve_include(inc, base_dir, include_dirs) {
            Some(path) => load_recursive(&path, include_dirs, visited, out, missing, loaded)?,
            None => missing.push(inc.to_string()),
        }
    }

    out.push_str(&source);
    out.push('\n');
    loaded.push(file.to_path_buf());
    Ok(())
}

/// Extract the path from an `#include "path"` or `#include <path>` line.
fn parse_include(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("#include")?.trim_start();
    let close = match rest.chars().next()? {
        '"' => '"',
        '<' => '>',
        _ => return None,
    };
    let rest = &rest[1..];
    let end = rest.find(close)?;
    Some(&rest[..end])
}

/// Resolve an include path against the including file's directory, then search dirs.
fn resolve_include(inc: &str, base_dir: &Path, include_dirs: &[PathBuf]) -> Option<PathBuf> {
    let candidate = base_dir.join(inc);
    if candidate.is_file() {
        return Some(candidate);
    }
    for dir in include_dirs {
        let candidate = dir.join(inc);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_include() {
        assert_eq!(
            parse_include("#include \"std_msgs/msg/Header.idl\""),
            Some("std_msgs/msg/Header.idl")
        );
        assert_eq!(parse_include("  #include <pkg/T.idl>"), Some("pkg/T.idl"));
        assert_eq!(parse_include("struct Foo {"), None);
        assert_eq!(parse_include("// #include \"x\""), None);
    }
}
