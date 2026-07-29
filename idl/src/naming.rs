//! Name conversion utilities for IDL code generation.

/// Split an identifier into its constituent words.
///
/// Word boundaries (spec §7.1.1):
/// - an underscore separates words (and is dropped);
/// - a lowercase/digit -> uppercase transition starts a new word;
/// - within a run of uppercase letters followed by a lowercase letter, the final
///   uppercase letter starts a new word ("HTTPServer" -> "HTTP", "Server").
fn split_words(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut words = Vec::new();
    let mut cur = String::new();

    for i in 0..chars.len() {
        let c = chars[i];
        if c == '_' {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if cur.is_empty() {
            cur.push(c);
            continue;
        }
        let prev = chars[i - 1];
        if (prev.is_lowercase() || prev.is_ascii_digit()) && c.is_uppercase() {
            // lower/digit -> upper: new word
            words.push(std::mem::take(&mut cur));
            cur.push(c);
        } else if c.is_lowercase() && prev.is_uppercase() && i >= 2 && chars[i - 2].is_uppercase() {
            // run of uppercase then lowercase: the last uppercase starts a new word
            let last = cur.pop().unwrap();
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            cur.push(last);
            cur.push(c);
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// Convert to PascalCase for Rust type names.
/// "hello_world" -> "HelloWorld"
/// "SENSOR_KIND" -> "SensorKind"
/// "SensorData" -> "SensorData" (already PascalCase)
/// "HTTPServer" -> "HttpServer"
pub fn to_pascal_case(name: &str) -> String {
    split_words(name)
        .iter()
        .map(|word| {
            let lower = word.to_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Convert to snake_case for Rust field/module names.
/// "HelloWorld" -> "hello_world"
/// "SensorData" -> "sensor_data"
/// "sensor_id" -> "sensor_id" (already snake_case)
/// "HTTPServer" -> "http_server"
pub fn to_snake_case(name: &str) -> String {
    split_words(name).iter().map(|w| w.to_lowercase()).collect::<Vec<_>>().join("_")
}

/// Convert to SCREAMING_SNAKE_CASE for C enum prefixes.
/// "SensorKind" -> "SENSOR_KIND"
pub fn to_screaming_snake(name: &str) -> String {
    to_snake_case(name).to_uppercase()
}

/// Convert IDL filename to output base name.
/// "HelloWorld.idl" -> "hello_world"
pub fn idl_to_output_name(filename: &str) -> String {
    let stem = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename)
        .strip_suffix(".idl")
        .unwrap_or(filename);
    to_snake_case(stem)
}

/// Generate C include guard from filename.
/// "hello_world.h" -> "HELLO_WORLD_H"
pub fn to_include_guard(filename: &str) -> String {
    let stem = filename.rsplit(['/', '\\']).next().unwrap_or(filename);
    stem.replace('.', "_").to_uppercase()
}

/// Mangle a qualified type name to the ROS2-over-DDS naming convention.
/// `pkg::msg::Type` -> `pkg::msg::dds_::Type_` (scope taken from the name itself)
/// Flat `Type` with `flat_scope` "pkg::msg" -> `pkg::msg::dds_::Type_`
/// Flat `Type` with no `flat_scope` -> `Type`.
pub fn ros2_type_name(qualified_name: &str, flat_scope: Option<&str>) -> String {
    // Idempotent: an already-mangled `scope::dds_::Name_` is returned unchanged.
    if is_ros2_mangled(qualified_name) {
        return qualified_name.to_string();
    }
    if let Some(idx) = qualified_name.rfind("::") {
        let scope = &qualified_name[..idx];
        let name = &qualified_name[idx + 2..];
        format!("{}::dds_::{}_", scope, name)
    } else if let Some(scope) = flat_scope {
        format!("{}::dds_::{}_", scope, qualified_name)
    } else {
        qualified_name.to_string()
    }
}

/// Whether a qualified name already follows the ROS2-over-DDS shape `…::dds_::Name_`.
fn is_ros2_mangled(name: &str) -> bool {
    match name.rfind("::") {
        Some(idx) => {
            let scope = &name[..idx];
            let leaf = &name[idx + 2..];
            leaf.ends_with('_') && (scope == "dds_" || scope.ends_with("::dds_"))
        }
        None => false,
    }
}

/// Determine the ROS2 scope (`package::kind`) to apply to flat, module-less IDL.
/// An explicit package wins (defaulting the interface kind to `msg`). Otherwise the
/// scope is inferred from a ROS2-standard input path `<package>/{msg,srv,action}/<file>.idl`.
pub fn ros2_flat_scope(
    input_file: &str,
    explicit_package: Option<&str>,
    explicit_kind: Option<&str>,
) -> Option<String> {
    if let Some(pkg) = explicit_package {
        return Some(format!("{}::{}", pkg, explicit_kind.unwrap_or("msg")));
    }
    // Inspect only the trailing path segments: <package>/<kind>/<file>.idl
    let mut rev = input_file.rsplit(['/', '\\']).filter(|s| !s.is_empty());
    let _file = rev.next()?;
    let kind = rev.next()?;
    let package = rev.next()?;
    if matches!(kind, "msg" | "srv" | "action") {
        Some(format!("{}::{}", package, kind))
    } else {
        None
    }
}

/// Inverse of [`ros2_type_name`] for an already-mangled name: strip the inserted
/// `dds_` scope and trailing `_`. `pkg::msg::dds_::Type_` -> `pkg::msg::Type`.
/// Names that are not in the mangled shape are returned unchanged. Used to compare
/// member references (never mangled) against a type's `qualified_name` (mangled
/// under `--ros2`).
pub fn ros2_unmangle(name: &str) -> String {
    if let Some(idx) = name.rfind("::dds_::") {
        let scope = &name[..idx];
        let leaf = &name[idx + "::dds_::".len()..];
        let leaf = leaf.strip_suffix('_').unwrap_or(leaf);
        return format!("{}::{}", scope, leaf);
    }
    name.to_string()
}

/// Rust path reference for an external (cross-package, `#include`d) type.
/// `std_msgs::msg::Header` -> `std_msgs::msg::Header`: scope segments are kept
/// verbatim and only the leaf is PascalCased, so the reference points at the
/// dependency's own generated type instead of a bare (possibly colliding) leaf.
/// The consuming build must expose each package's types at that path.
pub fn rust_external_path(qualified: &str) -> String {
    let mut parts: Vec<&str> = qualified.split("::").filter(|s| !s.is_empty()).collect();
    match parts.pop() {
        Some(leaf) if !parts.is_empty() => {
            format!("{}::{}", parts.join("::"), to_pascal_case(leaf))
        }
        Some(leaf) => to_pascal_case(leaf),
        None => qualified.to_string(),
    }
}

use crate::keywords;

/// Target language for keyword escaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLang {
    Rust,
    C,
    CSharp,
    Python,
}

/// Escape an identifier if it collides with a reserved keyword in the target language.
/// Returns the escaped name, or the original name unchanged if no escaping is needed.
pub fn escape_keyword(name: &str, lang: TargetLang) -> String {
    let kw_list = match lang {
        TargetLang::Rust => keywords::RUST,
        TargetLang::C => keywords::C,
        TargetLang::CSharp => keywords::CSHARP,
        TargetLang::Python => keywords::PYTHON,
    };

    if kw_list.contains(&name) {
        match lang {
            // `self`, `Self`, `super` and `crate` cannot be written as raw identifiers
            // (`r#self` etc. do not compile), so fall back to a trailing underscore.
            TargetLang::Rust if keywords::RUST_NON_RAW.contains(&name) => format!("{}_", name),
            TargetLang::Rust => format!("r#{}", name),
            TargetLang::C => format!("{}_", name),
            TargetLang::CSharp => format!("@{}", name),
            TargetLang::Python => format!("{}_", name),
        }
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("SENSOR_KIND"), "SensorKind");
        assert_eq!(to_pascal_case("SensorData"), "SensorData");
        assert_eq!(to_pascal_case("RED"), "Red");
    }

    #[test]
    fn test_snake_case() {
        assert_eq!(to_snake_case("HelloWorld"), "hello_world");
        assert_eq!(to_snake_case("SensorData"), "sensor_data");
        assert_eq!(to_snake_case("sensor_id"), "sensor_id");
    }

    #[test]
    fn test_acronym_word_boundary() {
        // Spec §7.1.1: a run of uppercase letters followed by a lowercase letter is a
        // word that ends before the final uppercase letter. Acronyms map as one word.
        assert_eq!(to_pascal_case("HTTPServer"), "HttpServer");
        assert_eq!(to_snake_case("HTTPServer"), "http_server");
        assert_eq!(to_pascal_case("id"), "Id");
        assert_eq!(to_snake_case("fooBar"), "foo_bar");
        assert_eq!(to_pascal_case("fooBar"), "FooBar");
    }

    #[test]
    fn test_screaming_snake() {
        assert_eq!(to_screaming_snake("SensorKind"), "SENSOR_KIND");
        assert_eq!(to_screaming_snake("Color"), "COLOR");
    }

    #[test]
    fn test_output_name() {
        assert_eq!(idl_to_output_name("HelloWorld.idl"), "hello_world");
        assert_eq!(idl_to_output_name("path/to/SensorData.idl"), "sensor_data");
    }

    #[test]
    fn test_include_guard() {
        assert_eq!(to_include_guard("hello_world.h"), "HELLO_WORLD_H");
    }

    #[test]
    fn test_ros2_type_name() {
        // Scoped name: insert dds_ before the last segment, append _
        assert_eq!(ros2_type_name("pkg::msg::Type", None), "pkg::msg::dds_::Type_");
        assert_eq!(ros2_type_name("a::b::c::Foo", None), "a::b::c::dds_::Foo_");
        // Flat name with a flat scope
        assert_eq!(ros2_type_name("Type", Some("my_pkg::msg")), "my_pkg::msg::dds_::Type_");
        assert_eq!(ros2_type_name("Type", Some("my_pkg::srv")), "my_pkg::srv::dds_::Type_");
        // Flat name without scope: unchanged
        assert_eq!(ros2_type_name("Type", None), "Type");
    }

    #[test]
    fn test_ros2_type_name_idempotent() {
        // Re-mangling an already-mangled name is a no-op.
        assert_eq!(ros2_type_name("pkg::msg::dds_::Type_", None), "pkg::msg::dds_::Type_");
        assert_eq!(ros2_type_name("a::b::c::dds_::Foo_", None), "a::b::c::dds_::Foo_");
        // A name that merely ends with '_' (no `dds_` scope) is NOT considered mangled.
        assert_eq!(ros2_type_name("pkg::msg::Foo_", None), "pkg::msg::dds_::Foo__");
    }

    #[test]
    fn test_ros2_unmangle() {
        assert_eq!(ros2_unmangle("pkg::msg::dds_::Type_"), "pkg::msg::Type");
        assert_eq!(ros2_unmangle("a::b::c::dds_::Foo_"), "a::b::c::Foo");
        // Round-trips with ros2_type_name for scoped names.
        assert_eq!(ros2_unmangle(&ros2_type_name("pkg::msg::Type", None)), "pkg::msg::Type");
        // Not in mangled shape -> unchanged.
        assert_eq!(ros2_unmangle("pkg::msg::Type"), "pkg::msg::Type");
        assert_eq!(ros2_unmangle("Type"), "Type");
    }

    #[test]
    fn test_rust_external_path() {
        assert_eq!(rust_external_path("std_msgs::msg::Header"), "std_msgs::msg::Header");
        assert_eq!(rust_external_path("pkg::msg::sensor_data"), "pkg::msg::SensorData");
        assert_eq!(rust_external_path("Header"), "Header");
    }

    #[test]
    fn test_ros2_flat_scope() {
        // Explicit package -> defaults interface kind to msg
        assert_eq!(
            ros2_flat_scope("HelloWorld.idl", Some("my_pkg"), None).as_deref(),
            Some("my_pkg::msg")
        );
        // Explicit package + explicit kind
        assert_eq!(
            ros2_flat_scope("AddTwo.idl", Some("my_pkg"), Some("srv")).as_deref(),
            Some("my_pkg::srv")
        );
        // Inferred from ROS2-standard layout
        assert_eq!(
            ros2_flat_scope("a/b/my_pkg/msg/HelloWorld.idl", None, None).as_deref(),
            Some("my_pkg::msg")
        );
        assert_eq!(
            ros2_flat_scope("robot_msgs/srv/AddTwo.idl", None, None).as_deref(),
            Some("robot_msgs::srv")
        );
        assert_eq!(
            ros2_flat_scope("pkg\\action\\Fib.idl", None, None).as_deref(),
            Some("pkg::action")
        );
        // No package and non-standard path -> none
        assert_eq!(ros2_flat_scope("input/HelloWorld.idl", None, None), None);
    }

    // ---- Keyword escaping tests ----

    #[test]
    fn test_escape_rust_keywords() {
        assert_eq!(escape_keyword("type", TargetLang::Rust), "r#type");
        assert_eq!(escape_keyword("match", TargetLang::Rust), "r#match");
        assert_eq!(escape_keyword("struct", TargetLang::Rust), "r#struct");
        assert_eq!(escape_keyword("async", TargetLang::Rust), "r#async");
        assert_eq!(escape_keyword("gen", TargetLang::Rust), "r#gen");
        assert_eq!(escape_keyword("index", TargetLang::Rust), "index");
    }

    #[test]
    fn test_escape_rust_non_raw_keywords() {
        // `self`, `Self`, `super`, `crate` cannot be written as raw identifiers in Rust,
        // so they must be escaped with a trailing underscore, not `r#` (spec §7.1.2).
        assert_eq!(escape_keyword("self", TargetLang::Rust), "self_");
        assert_eq!(escape_keyword("Self", TargetLang::Rust), "Self_");
        assert_eq!(escape_keyword("super", TargetLang::Rust), "super_");
        assert_eq!(escape_keyword("crate", TargetLang::Rust), "crate_");
    }

    #[test]
    fn test_escape_c_keywords() {
        assert_eq!(escape_keyword("int", TargetLang::C), "int_");
        assert_eq!(escape_keyword("return", TargetLang::C), "return_");
        assert_eq!(escape_keyword("struct", TargetLang::C), "struct_");
        // C23 keywords
        assert_eq!(escape_keyword("nullptr", TargetLang::C), "nullptr_");
        assert_eq!(escape_keyword("alignas", TargetLang::C), "alignas_");
        assert_eq!(escape_keyword("constexpr", TargetLang::C), "constexpr_");
        // Macros
        assert_eq!(escape_keyword("NULL", TargetLang::C), "NULL_");
        assert_eq!(escape_keyword("EOF", TargetLang::C), "EOF_");
        // Non-keyword
        assert_eq!(escape_keyword("data", TargetLang::C), "data");
    }

    #[test]
    fn test_escape_csharp_keywords() {
        assert_eq!(escape_keyword("class", TargetLang::CSharp), "@class");
        assert_eq!(escape_keyword("event", TargetLang::CSharp), "@event");
        assert_eq!(escape_keyword("string", TargetLang::CSharp), "@string");
        // Contextual keywords
        assert_eq!(escape_keyword("async", TargetLang::CSharp), "@async");
        assert_eq!(escape_keyword("record", TargetLang::CSharp), "@record");
        assert_eq!(escape_keyword("var", TargetLang::CSharp), "@var");
        assert_eq!(escape_keyword("yield", TargetLang::CSharp), "@yield");
        assert_eq!(escape_keyword("value", TargetLang::CSharp), "@value");
        // Non-keyword
        assert_eq!(escape_keyword("data", TargetLang::CSharp), "data");
    }

    #[test]
    fn test_escape_python_keywords() {
        assert_eq!(escape_keyword("class", TargetLang::Python), "class_");
        assert_eq!(escape_keyword("type", TargetLang::Python), "type_");
        assert_eq!(escape_keyword("def", TargetLang::Python), "def_");
        assert_eq!(escape_keyword("list", TargetLang::Python), "list_");
        // Soft keywords (3.10+)
        assert_eq!(escape_keyword("match", TargetLang::Python), "match_");
        assert_eq!(escape_keyword("case", TargetLang::Python), "case_");
        // Builtins
        assert_eq!(escape_keyword("tuple", TargetLang::Python), "tuple_");
        assert_eq!(escape_keyword("len", TargetLang::Python), "len_");
        assert_eq!(escape_keyword("enumerate", TargetLang::Python), "enumerate_");
        // Non-keyword
        assert_eq!(escape_keyword("data", TargetLang::Python), "data");
    }

    #[test]
    fn test_escape_non_keywords() {
        assert_eq!(escape_keyword("hello", TargetLang::Rust), "hello");
        assert_eq!(escape_keyword("world", TargetLang::C), "world");
        assert_eq!(escape_keyword("foo", TargetLang::CSharp), "foo");
        assert_eq!(escape_keyword("bar", TargetLang::Python), "bar");
    }

    #[test]
    fn test_keyword_prefix_not_escaped() {
        // Identifiers that start with a keyword but are not exact matches
        assert_eq!(escape_keyword("type_name", TargetLang::Rust), "type_name");
        assert_eq!(escape_keyword("class_id", TargetLang::Python), "class_id");
        assert_eq!(escape_keyword("return_value", TargetLang::C), "return_value");
        assert_eq!(escape_keyword("interface_impl", TargetLang::CSharp), "interface_impl");
    }
}
