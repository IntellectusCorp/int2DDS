/// Name conversion utilities for IDL code generation.

/// Convert to PascalCase for Rust type names.
/// "hello_world" -> "HelloWorld"
/// "SENSOR_KIND" -> "SensorKind"
/// "SensorData" -> "SensorData" (already PascalCase)
pub fn to_pascal_case(name: &str) -> String {
    if name.contains('_') {
        name.split('_')
            .filter(|s| !s.is_empty())
            .map(|part| {
                let lower = part.to_lowercase();
                let mut chars = lower.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect()
    } else if name.chars().all(|c| c.is_uppercase() || c.is_ascii_digit()) {
        // ALL_CAPS -> Allcaps
        let lower = name.to_lowercase();
        let mut chars = lower.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
    } else {
        // Already PascalCase or camelCase - ensure first char is uppercase
        let mut chars = name.chars();
        match chars.next() {
            Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            None => String::new(),
        }
    }
}

/// Convert to snake_case for Rust field/module names.
/// "HelloWorld" -> "hello_world"
/// "SensorData" -> "sensor_data"
/// "sensor_id" -> "sensor_id" (already snake_case)
pub fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    let mut prev_upper = false;
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 && !prev_upper {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
            prev_upper = true;
        } else {
            prev_upper = false;
            result.push(c);
        }
    }
    result
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
    let stem = filename
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(filename);
    stem.replace('.', "_").to_uppercase()
}

/// Target language for keyword escaping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetLang {
    Rust,
    C,
    CSharp,
    Python,
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use",
    "where", "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final",
    "macro", "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "union",
];

const C_KEYWORDS: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register",
    "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
    "union", "unsigned", "void", "volatile", "while", "_Bool", "_Complex", "_Imaginary",
    "_Alignas", "_Alignof", "_Atomic", "_Generic", "_Noreturn", "_Static_assert",
    "_Thread_local", "bool", "true", "false",
];

const CSHARP_KEYWORDS: &[&str] = &[
    "abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked",
    "class", "const", "continue", "decimal", "default", "delegate", "do", "double", "else",
    "enum", "event", "explicit", "extern", "false", "finally", "fixed", "float", "for",
    "foreach", "goto", "if", "implicit", "in", "int", "interface", "internal", "is", "lock",
    "long", "namespace", "new", "null", "object", "operator", "out", "override", "params",
    "private", "protected", "public", "readonly", "ref", "return", "sbyte", "sealed", "short",
    "sizeof", "stackalloc", "static", "string", "struct", "switch", "this", "throw", "true",
    "try", "typeof", "uint", "ulong", "unchecked", "unsafe", "ushort", "using", "virtual",
    "void", "volatile", "while",
];

const PYTHON_KEYWORDS: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
    "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
    "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
    "try", "while", "with", "yield", "type", "id", "list", "dict", "set", "map", "str", "int",
    "float", "bool", "bytes", "range", "print", "object", "property", "super",
];

/// Escape an identifier if it collides with a reserved keyword in the target language.
/// Returns the escaped name, or the original name unchanged if no escaping is needed.
pub fn escape_keyword(name: &str, lang: TargetLang) -> String {
    let keywords = match lang {
        TargetLang::Rust => RUST_KEYWORDS,
        TargetLang::C => C_KEYWORDS,
        TargetLang::CSharp => CSHARP_KEYWORDS,
        TargetLang::Python => PYTHON_KEYWORDS,
    };

    if keywords.contains(&name) {
        match lang {
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
    fn test_escape_rust_keywords() {
        assert_eq!(escape_keyword("type", TargetLang::Rust), "r#type");
        assert_eq!(escape_keyword("match", TargetLang::Rust), "r#match");
        assert_eq!(escape_keyword("struct", TargetLang::Rust), "r#struct");
        assert_eq!(escape_keyword("async", TargetLang::Rust), "r#async");
        assert_eq!(escape_keyword("index", TargetLang::Rust), "index");
    }

    #[test]
    fn test_escape_c_keywords() {
        assert_eq!(escape_keyword("int", TargetLang::C), "int_");
        assert_eq!(escape_keyword("return", TargetLang::C), "return_");
        assert_eq!(escape_keyword("struct", TargetLang::C), "struct_");
        assert_eq!(escape_keyword("data", TargetLang::C), "data");
    }

    #[test]
    fn test_escape_csharp_keywords() {
        assert_eq!(escape_keyword("class", TargetLang::CSharp), "@class");
        assert_eq!(escape_keyword("event", TargetLang::CSharp), "@event");
        assert_eq!(escape_keyword("string", TargetLang::CSharp), "@string");
        assert_eq!(escape_keyword("data", TargetLang::CSharp), "data");
    }

    #[test]
    fn test_escape_python_keywords() {
        assert_eq!(escape_keyword("class", TargetLang::Python), "class_");
        assert_eq!(escape_keyword("type", TargetLang::Python), "type_");
        assert_eq!(escape_keyword("def", TargetLang::Python), "def_");
        assert_eq!(escape_keyword("list", TargetLang::Python), "list_");
        assert_eq!(escape_keyword("data", TargetLang::Python), "data");
    }

    #[test]
    fn test_escape_non_keywords() {
        assert_eq!(escape_keyword("hello", TargetLang::Rust), "hello");
        assert_eq!(escape_keyword("world", TargetLang::C), "world");
        assert_eq!(escape_keyword("foo", TargetLang::CSharp), "foo");
        assert_eq!(escape_keyword("bar", TargetLang::Python), "bar");
    }
}
