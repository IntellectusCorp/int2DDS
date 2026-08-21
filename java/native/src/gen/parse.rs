//! Extracts exported C-ABI function signatures from the `int2dds-ffi` source.
//!
//! Parsing the Rust source rather than the cbindgen header gives exact Rust
//! types, which the type mapper needs to distinguish `*const c_char` from
//! other pointers.
//!
//! `syn` does not expand macros, but 59 of the 454 exported functions are
//! produced by five `macro_rules!` macros in `dynamic.rs` and
//! `dynamic_value.rs`. Skipping them would drop the whole dynamic-data API
//! from the Java binding, so this module expands those invocations itself.
//! The expander deliberately handles only the shape those macros use — a
//! single-arm rule with positional `$name:frag` metavariables and no
//! repetitions — and returns an error rather than a short list when it meets
//! an exporting macro it cannot expand.

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};
use std::collections::HashMap;
use std::path::Path;
use syn::{FnArg, Item, Pat, ReturnType, Visibility};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfiFn {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: String,
    /// The `int2dds_ffi` module the function lives in, i.e. the source file
    /// stem. `int2dds-ffi` re-exports nothing at its crate root, so the
    /// generated forwarders must call `int2dds_ffi::<module>::<name>`.
    pub module: String,
}

/// Collapse a token-rendered type into a stable single-spaced string.
///
/// `ToTokens` puts a space between every token, so `*const u8` arrives as
/// `"* const u8"`. These rules put the punctuation back against what it binds
/// to, giving the same spelling a human would write in the source.
fn norm(s: &str) -> String {
    let mut out = s.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [
        ("* const ", "*const "),
        ("* mut ", "*mut "),
        ("& ", "&"),
        (" :: ", "::"),
        (":: ", "::"),
        (" ::", "::"),
        (" <", "<"),
        ("< ", "<"),
        (" >", ">"),
        (" ,", ","),
        (" ;", ";"),
    ] {
        out = out.replace(from, to);
    }
    out
}

fn is_no_mangle(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("no_mangle"))
}

/// One `(matcher) => { body }` arm of a `macro_rules!` definition.
struct MacroArm {
    /// Metavariable names in declaration order, without the `$`.
    params: Vec<String>,
    body: TokenStream,
}

/// True if the token stream mentions `no_mangle`, i.e. expanding it would
/// produce exported symbols that the binding must not miss.
fn mentions_no_mangle(ts: &TokenStream) -> bool {
    ts.clone().into_iter().any(|t| match t {
        TokenTree::Ident(i) => i == "no_mangle",
        TokenTree::Group(g) => mentions_no_mangle(&g.stream()),
        _ => false,
    })
}

/// Read the metavariable names out of a rule matcher such as
/// `($fn_name:ident, $rust_ty:ty)`.
fn matcher_params(matcher: &Group, macro_name: &str) -> Result<Vec<String>, String> {
    let mut params = Vec::new();
    let mut it = matcher.stream().into_iter().peekable();
    while let Some(tt) = it.next() {
        let TokenTree::Punct(p) = &tt else { continue };
        if p.as_char() != '$' {
            continue;
        }
        match it.next() {
            Some(TokenTree::Ident(name)) => {
                params.push(name.to_string());
                // Skip `: fragment_specifier`; the expander is purely textual
                // so the specifier does not affect substitution.
                if matches!(it.peek(), Some(TokenTree::Punct(p)) if p.as_char() == ':') {
                    it.next();
                    it.next();
                }
            }
            // `$(...)` is a repetition. Supporting it would mean implementing
            // real macro matching; refuse instead of guessing.
            other => {
                return Err(format!(
                    "{macro_name}: unsupported metavariable form {other:?}; \
                     the expander handles only `$name:frag`"
                ))
            }
        }
    }
    Ok(params)
}

/// Split a `macro_rules!` body into its arms.
fn collect_arms(tokens: TokenStream, macro_name: &str) -> Result<Vec<MacroArm>, String> {
    let mut arms = Vec::new();
    let mut it = tokens.into_iter().peekable();
    while let Some(tt) = it.next() {
        let TokenTree::Group(matcher) = tt else {
            // A stray `;` between arms.
            continue;
        };
        if matcher.delimiter() == Delimiter::Brace {
            continue;
        }
        let params = matcher_params(&matcher, macro_name)?;
        // Consume `=>`.
        for _ in 0..2 {
            match it.next() {
                Some(TokenTree::Punct(_)) => {}
                other => return Err(format!("{macro_name}: expected `=>`, found {other:?}")),
            }
        }
        match it.next() {
            Some(TokenTree::Group(body)) => arms.push(MacroArm { params, body: body.stream() }),
            other => return Err(format!("{macro_name}: expected arm body, found {other:?}")),
        }
    }
    Ok(arms)
}

/// Split a macro invocation's arguments on top-level commas.
fn split_args(tokens: TokenStream) -> Vec<TokenStream> {
    let mut args = Vec::new();
    let mut current = Vec::new();
    for tt in tokens {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == ',' => {
                args.push(current.drain(..).collect());
            }
            _ => current.push(tt),
        }
    }
    if !current.is_empty() {
        args.push(current.into_iter().collect());
    }
    args
}

/// Replace every `$name` in `body` with its bound tokens, recursing into groups.
fn substitute(body: TokenStream, binds: &HashMap<String, TokenStream>) -> TokenStream {
    let mut out = Vec::new();
    let mut it = body.into_iter().peekable();
    while let Some(tt) = it.next() {
        match tt {
            TokenTree::Punct(ref p) if p.as_char() == '$' => {
                match it.peek() {
                    Some(TokenTree::Ident(name)) if binds.contains_key(&name.to_string()) => {
                        let key = name.to_string();
                        it.next();
                        out.extend(binds[&key].clone());
                    }
                    // Not one of ours (`$crate`, a repetition marker): keep as is.
                    _ => out.push(tt),
                }
            }
            TokenTree::Group(g) => {
                let inner = substitute(g.stream(), binds);
                let mut replaced = Group::new(g.delimiter(), inner);
                replaced.set_span(g.span());
                out.push(TokenTree::Group(replaced));
            }
            other => out.push(other),
        }
    }
    out.into_iter().collect()
}

/// Pull the signature out of a function item, if it is an exported C-ABI one.
fn extract_fn(f: &syn::ItemFn, module: &str) -> Result<Option<FfiFn>, String> {
    if !matches!(f.vis, Visibility::Public(_)) {
        return Ok(None);
    }
    if f.sig.abi.is_none() || !is_no_mangle(&f.attrs) {
        return Ok(None);
    }
    let mut params = Vec::new();
    for arg in &f.sig.inputs {
        let FnArg::Typed(pt) = arg else {
            return Err(format!("{}: unexpected receiver arg", f.sig.ident));
        };
        let Pat::Ident(id) = &*pt.pat else {
            return Err(format!("{}: non-ident parameter pattern", f.sig.ident));
        };
        let ty = &*pt.ty;
        params
            .push(Param { name: id.ident.to_string(), ty: norm(&quote::quote!(#ty).to_string()) });
    }
    let ret = match &f.sig.output {
        ReturnType::Default => "()".to_string(),
        ReturnType::Type(_, t) => norm(&quote::quote!(#t).to_string()),
    };
    Ok(Some(FfiFn { name: f.sig.ident.to_string(), params, ret, module: module.to_string() }))
}

/// Parse every `.rs` file directly inside `dir`, returning exported functions
/// sorted by name so generated output is deterministic across runs.
pub fn parse_ffi_dir(dir: &Path) -> Result<Vec<FfiFn>, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("read_dir {}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "rs").unwrap_or(false))
        .collect();
    paths.sort();

    let mut files = Vec::new();
    for path in paths {
        let src =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let file = syn::parse_file(&src).map_err(|e| format!("parse {}: {e}", path.display()))?;
        let module = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("{}: unreadable file stem", path.display()))?
            .to_string();
        files.push((module, file));
    }

    // Pass 1: collect the macros whose expansion defines exported symbols.
    // `macro_rules!` is textually scoped, but collecting across the whole
    // directory first keeps the result independent of file ordering.
    let mut macros: HashMap<String, Vec<MacroArm>> = HashMap::new();
    for (_, file) in &files {
        for item in &file.items {
            let Item::Macro(m) = item else { continue };
            let Some(name) = &m.ident else { continue };
            if !m.mac.path.is_ident("macro_rules") || !mentions_no_mangle(&m.mac.tokens) {
                continue;
            }
            let name = name.to_string();
            let arms = collect_arms(m.mac.tokens.clone(), &name)?;
            macros.insert(name, arms);
        }
    }

    // Pass 2: collect functions, expanding invocations of those macros.
    let mut out = Vec::new();
    for (module, file) in &files {
        for item in &file.items {
            match item {
                Item::Fn(f) => {
                    if let Some(ffi) = extract_fn(f, module)? {
                        out.push(ffi);
                    }
                }
                Item::Macro(m) if m.ident.is_none() => {
                    let Some(name) = m.mac.path.get_ident().map(|i| i.to_string()) else {
                        continue;
                    };
                    let Some(arms) = macros.get(&name) else { continue };
                    let args = split_args(m.mac.tokens.clone());
                    let arm =
                        arms.iter().find(|a| a.params.len() == args.len()).ok_or_else(|| {
                            format!(
                                "{name}!: invocation with {} arguments matches no rule arm",
                                args.len()
                            )
                        })?;
                    let binds: HashMap<String, TokenStream> =
                        arm.params.iter().cloned().zip(args.into_iter()).collect();
                    let expanded = substitute(arm.body.clone(), &binds);
                    let parsed = syn::parse2::<syn::File>(expanded)
                        .map_err(|e| format!("{name}!: expansion does not parse: {e}"))?;
                    for item in &parsed.items {
                        let Item::Fn(f) = item else { continue };
                        if let Some(ffi) = extract_fn(f, module)? {
                            out.push(ffi);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn ffi_dir() -> &'static Path {
        // Tests run with CWD = the crate root (java/native).
        Path::new("../../ffi/src")
    }

    fn sig_of<'a>(fns: &'a [FfiFn], name: &str) -> (Vec<(&'a str, &'a str)>, &'a str) {
        let f =
            fns.iter().find(|f| f.name == name).unwrap_or_else(|| panic!("{name} must be found"));
        let params = f.params.iter().map(|p| (p.name.as_str(), p.ty.as_str())).collect();
        (params, f.ret.as_str())
    }

    #[test]
    fn finds_every_exported_ffi_function() {
        let fns = parse_ffi_dir(ffi_dir()).expect("parse must succeed");
        // Ground truth is the dynamic symbol table of the built cdylib:
        //   nm -D --defined-only target/release/libint2dds_ffi.so | grep -c ' T int2dds_'
        // reports 454 after adding int2dds_dynamic_reader_get_statuscondition
        // (dynamic-reader StatusCondition access, mirroring the typed reader).
        // This assertion is intentionally exact so an FFI change forces a
        // deliberate update here.
        assert_eq!(fns.len(), 454, "exported FFI function count changed");
    }

    #[test]
    fn extracts_write_serialized_signature_exactly() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let (params, ret) = sig_of(&fns, "int2dds_datawriter_write_serialized");
        assert_eq!(ret, "Int2DdsRet");
        assert_eq!(
            params,
            vec![
                ("writer", "*const Int2DdsDataWriter"),
                ("data", "*const u8"),
                ("data_len", "usize"),
            ]
        );
    }

    #[test]
    fn ignores_private_extern_fns() {
        // ffi/src has 13 non-pub `extern "C" fn` items (callback trampolines).
        // They are not exported symbols and must not appear.
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        assert!(fns.iter().all(|f| f.name.starts_with("int2dds_")));
    }

    #[test]
    fn unit_return_is_normalised() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        assert!(fns.iter().any(|f| f.ret == "()"));
    }

    #[test]
    fn functions_carry_their_ffi_module() {
        // int2dds-ffi re-exports nothing at its crate root, so the generated
        // forwarders need the module to build a resolvable call path.
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let module_of =
            |name: &str| fns.iter().find(|f| f.name == name).map(|f| f.module.as_str()).unwrap();
        assert_eq!(module_of("int2dds_datawriter_write_serialized"), "publisher");
        assert_eq!(module_of("int2dds_default_extensibility"), "qos");
        // Macro-generated functions belong to the file the invocation sits in.
        assert_eq!(module_of("int2dds_dynamic_data_set_i32"), "dynamic");
        assert_eq!(module_of("int2dds_dynamic_value_u64"), "dynamic_value");
        assert!(fns.iter().all(|f| !f.module.is_empty()));
    }

    #[test]
    fn names_are_unique_and_sorted() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let names: Vec<&str> = fns.iter().map(|f| f.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "output must be sorted for deterministic codegen");
        let mut deduped = sorted.clone();
        deduped.dedup();
        assert_eq!(deduped.len(), names.len(), "duplicate function names");
    }

    // ---- macro_rules!-generated functions --------------------------------
    // syn does not expand macros, so these five families would silently vanish
    // from the binding without the expander. All 59 are dynamic-data API that
    // the C# binding already exposes.

    #[test]
    fn expands_value_ctor_macro() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let (params, ret) = sig_of(&fns, "int2dds_dynamic_value_u64");
        assert_eq!(ret, "Int2DdsRet");
        assert_eq!(params, vec![("value", "u64"), ("out", "*mut *mut Int2DdsDynamicValue"),]);
    }

    #[test]
    fn expands_value_as_macro() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let (params, ret) = sig_of(&fns, "int2dds_dynamic_value_as_f64");
        assert_eq!(ret, "Int2DdsRet");
        assert_eq!(params, vec![("value", "*const Int2DdsDynamicValue"), ("out", "*mut f64"),]);
    }

    #[test]
    fn expands_flat_sample_getter_macro() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let (params, ret) = sig_of(&fns, "int2dds_dynamic_sample_get_i32");
        assert_eq!(ret, "Int2DdsRet");
        assert_eq!(
            params,
            vec![
                ("bytes", "*const u8"),
                ("len", "usize"),
                ("type_obj", "*const Int2DdsTypeObject"),
                ("field_name", "*const c_char"),
                ("out", "*mut i32"),
            ]
        );
    }

    #[test]
    fn expands_handle_getter_macro() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let (params, ret) = sig_of(&fns, "int2dds_dynamic_data_get_u16");
        assert_eq!(ret, "Int2DdsRet");
        assert_eq!(
            params,
            vec![
                ("data", "*const Int2DdsDynamicData"),
                ("field_path", "*const c_char"),
                ("out", "*mut u16"),
            ]
        );
    }

    #[test]
    fn expands_handle_setter_macro() {
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let (params, ret) = sig_of(&fns, "int2dds_dynamic_data_set_i32");
        assert_eq!(ret, "Int2DdsRet");
        assert_eq!(
            params,
            vec![("data", "*mut Int2DdsDynamicData"), ("field", "*const c_char"), ("value", "i32"),]
        );
    }

    #[test]
    fn every_macro_family_is_fully_expanded() {
        // Each family is generated once per primitive type. Asserting the
        // suffixes rather than a prefix count keeps hand-written functions that
        // share the prefix (e.g. int2dds_dynamic_data_get_string) out of it.
        const PRIMITIVES: [&str; 11] =
            ["bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64"];
        let fns = parse_ffi_dir(ffi_dir()).unwrap();
        let has = |name: &str| fns.iter().any(|f| f.name == name);

        for family in [
            "int2dds_dynamic_value_as_",
            "int2dds_dynamic_data_get_",
            "int2dds_dynamic_data_set_",
            "int2dds_dynamic_sample_get_",
        ] {
            for prim in PRIMITIVES {
                assert!(has(&format!("{family}{prim}")), "missing {family}{prim}");
            }
        }
        // `int2dds_dynamic_sample_get_byte` is the twelfth member of its family.
        assert!(has("int2dds_dynamic_sample_get_byte"));

        // value_ctor! covers the 11 primitives plus byte, bitmask and bitset.
        for prim in PRIMITIVES {
            assert!(has(&format!("int2dds_dynamic_value_{prim}")));
        }
        for extra in ["byte", "bitmask", "bitset"] {
            assert!(has(&format!("int2dds_dynamic_value_{extra}")));
        }
    }
}
