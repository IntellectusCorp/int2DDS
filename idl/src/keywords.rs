//! Reserved keyword lists for each target language.
//!
//! Used by `naming::escape_keyword` to detect and escape identifiers
//! that collide with language keywords.

#[rustfmt::skip]
pub const RUST: &[&str] = &[
    "as", "break", "const", "continue", "crate", "else", "enum", "extern", "false", "fn", "for",
    "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref", "return",
    "self", "Self", "static", "struct", "super", "trait", "true", "type", "unsafe", "use", "where",
    "while", "async", "await", "dyn", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield", "try", "union", "gen",
];

/// Rust keywords that cannot be written as raw identifiers (`r#self` etc. do not compile).
/// These are escaped with a trailing underscore instead.
#[rustfmt::skip]
pub const RUST_NON_RAW: &[&str] = &["self", "Self", "super", "crate"];

#[rustfmt::skip]
pub const C: &[&str] = &[
    "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else",
    "enum", "extern", "float", "for", "goto", "if", "inline", "int", "long", "register", "restrict",
    "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned",
    "void", "volatile", "while", "bool", "true", "false", "alignas", "alignof", "constexpr", "nullptr",
    "_Bool", "_Complex", "_Imaginary", "_Alignas", "_Alignof", "_Atomic", "_Generic", "_Noreturn",
    "_Static_assert", "_Thread_local", "_BitInt", "_Decimal32", "_Decimal64", "_Decimal128",
    "static_assert", "thread_local", "typeof", "typeof_unqual", "NULL", "EOF",
];

#[rustfmt::skip]
pub const CSHARP: &[&str] = &[
    "abstract", "as", "base", "bool", "break", "byte", "case", "catch", "char", "checked", "class",
    "const", "continue", "decimal", "default", "delegate", "do", "double", "else", "enum", "event", "explicit",
    "extern", "false", "finally", "fixed", "float", "for", "foreach", "goto", "if", "implicit", "in", "int",
    "interface", "internal", "is", "lock", "long", "namespace", "new", "null", "object", "operator", "out", "override",
    "params", "private", "protected", "public", "readonly", "ref", "return", "sbyte", "sealed", "short", "sizeof",
    "stackalloc", "static", "string", "struct", "switch", "this", "throw", "true", "try", "typeof", "uint", "ulong",
    "unchecked", "unsafe", "ushort", "using", "virtual", "void", "volatile", "while", "async", "await", "var", "dynamic",
    "yield", "record", "required", "nint", "nuint", "scoped", "global", "nameof", "partial", "when", "where", "not",
    "and", "or", "with", "file", "init", "value",
];

#[rustfmt::skip]
pub const PYTHON: &[&str] = &[
    "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class", "continue", "def",
    "del", "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "type", "match",
    "case", "id", "list", "dict", "set", "map", "str", "int", "float", "bool", "bytes", "range", "print",
    "object", "property", "super", "tuple", "len", "enumerate", "zip", "filter", "sorted", "reversed", "hash",
    "repr", "open", "input", "frozenset", "isinstance", "issubclass",
];

/// Java SE 17 reserved words, plus the three literals (`true`/`false`/`null`)
/// and the lone `_`, none of which are legal identifiers either. Contextual
/// keywords (`var`, `record`, `sealed`, `permits`, `yield`) are legal
/// identifiers and are deliberately absent.
#[rustfmt::skip]
pub const JAVA: &[&str] = &[
    "abstract", "assert", "boolean", "break", "byte", "case", "catch", "char", "class", "const",
    "continue", "default", "do", "double", "else", "enum", "extends", "final", "finally", "float",
    "for", "goto", "if", "implements", "import", "instanceof", "int", "interface", "long", "native",
    "new", "package", "private", "protected", "public", "return", "short", "static", "strictfp",
    "super", "switch", "synchronized", "this", "throw", "throws", "transient", "try", "void",
    "volatile", "while",
    "true", "false", "null", "_",
];
