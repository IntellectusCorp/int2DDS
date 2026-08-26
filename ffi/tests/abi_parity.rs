//! The C header against the two hand-maintained bindings.
//!
//! `include/int2dds-ffi.h` is the ABI. Measured with `llvm-readobj --coff-exports` on
//! the built cdylib: the committed header's function set and the export table are
//! equal in both directions, nothing exported but undeclared and nothing declared but
//! unexported (456 either way when last measured). That includes the block `build.rs`
//! adds for the macro-generated entry points cbindgen cannot see, which it builds from
//! the invocation list in `src/dynamic.rs` and `src/dynamic_value.rs` rather than
//! holding a copy of the declarations — while it did hold one, an invocation added
//! there was exported by the library, absent from the header, and green here, because
//! an export the header does not mention is not an export this test can see.
//! `build.rs` regenerates the header when this crate's sources change, which is what
//! keeps the two equal — note that a change to `csharp/` or `python/` alone does not
//! trigger it, so what this test reads on such a branch is the committed file. Keeping
//! that file honest is `build.rs`'s job, not this test's.
//!
//! What the bindings do not have is any such check of their own. C# `[DllImport]` is
//! written by hand against the header, and nothing noticed when an export landed in
//! one binding and not the other: 12 exports had no C# declaration when this test was
//! written, of which 8 were oversights and are now bound. Two directions
//! to pin, and they fail differently. A declaration naming an export that does not
//! exist is a runtime fault in the binding's own language
//! (`EntryPointNotFoundException`, cffi `AttributeError`) at whatever moment the call
//! is first made, so that is a hard failure here with no way to record an exception.
//! An export with no declaration is only a gap, so those are listed below and the
//! list is checked in both directions — a new export left unbound fails, and so does
//! one that gets bound without being struck off.
//!
//! Both extractors count what they matched and fail if a construct went unparsed,
//! because the failure mode of a loose pattern is a silent undercount that reads as
//! a clean sweep. Writing this test, `static extern` missed every
//! `static unsafe extern` declaration and reported 273 of 441.
//!
//! What the function comparison cannot catch: it compares names, so a declaration
//! with the wrong arity or the wrong parameter types passes every check here and then
//! corrupts the stack at the call. That is the widest gap between this being green and
//! a binding being right, and closing it needs the header's signatures parsed and
//! matched against the binding's, not just its identifiers. Python no longer sits in
//! that gap — its `cdef` is generated from this header by
//! `python/tools/generate_bindings.py`, so every signature is the header's, and the
//! empty unbound list below is what fails when the generated file goes stale. C# still
//! does: a `[DllImport]` is a marshalling decision the header does not carry, so it
//! stays hand-written and name-checked only.
//!
//! The header's structs and enums are compared field by field instead, and only
//! against C#, for the same reason: Python's are the header's, so comparing them
//! would only restate that the generator ran. A struct is the harder half to get
//! right by hand, because nothing about a wrong field is visible at the call — every
//! field's position depends on the width of the ones before it, so one type off by a
//! byte moves the rest, and the caller reads or writes a neighbour with no fault to
//! show for it. Two ways to be wrong that compile cleanly and are checked here: a
//! C# array field without `fixed`, which marshals as a reference rather than as the
//! inline storage the header declares, and a `bool` without
//! `[MarshalAs(UnmanagedType.U1)]`, which marshals as the 4-byte Win32 `BOOL`. The
//! enum comparison is what found `QosPolicyId` missing its last enumerator, a value
//! the library can return and C# could not name.
//!
//! What that comparison cannot catch, and it is the same shape of gap as the
//! function names: it reads declaration text through the mapping table below, so the
//! table is the oracle. A wrong entry in it would make this test confidently wrong,
//! and nothing here would notice — C#'s marshalled layout is never measured, only
//! predicted. The header's own layout does have an independent oracle, a C compiler,
//! which is how the enum widths were checked; C# has none.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Exports with no C# `[DllImport]`, and every one of them is deliberate: C# has no
/// ContentFilteredTopic class, so the filter's create/delete pair, the reader
/// constructor that takes one, and the field-descriptor topic they need have nothing
/// to bind to.
const UNBOUND_CSHARP: &[&str] = &[
    "int2dds_create_contentfilteredtopic",
    "int2dds_create_datareader_cft",
    "int2dds_create_topic_with_field_descriptors",
    "int2dds_delete_contentfilteredtopic",
];

/// Exports with no Python `cdef`, and there are none: the `cdef` is the header,
/// generated by `python/tools/generate_bindings.py`. Keep this empty. An entry
/// appearing here does not mean a binding is missing — it means the generated file
/// is older than the header and needs regenerating.
const UNBOUND_PYTHON: &[&str] = &[];

const PREFIX: &[u8] = b"int2dds_";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the ffi crate sits in the repo").into()
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|e| {
        panic!("{} carries part of the ABI surface this test compares: {e}", path.display())
    })
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn find_from(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|offset| from + offset)
}

/// Drops `//` and `/* */` so that a name mentioned in prose is not read as a
/// declaration. Byte-wise is safe for non-ASCII source: a UTF-8 continuation byte
/// never equals `/`, `*`, or `\n`.
fn without_comments(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {
        if src[i..].starts_with(b"/*") {
            match find_from(src, b"*/", i + 2) {
                Some(end) => i = end + 2,
                None => break,
            }
        } else if src[i..].starts_with(b"//") {
            match find_from(src, b"\n", i + 2) {
                Some(end) => i = end,
                None => break,
            }
        } else {
            out.push(src[i]);
            i += 1;
        }
    }
    out
}

/// Every `int2dds_*` that is applied to an argument list, which in a C header or a
/// `cdef` block means every declaration and in C# means the declaration plus its
/// call sites — so C# is scanned by declaration span, never whole-file.
fn names_in(src: &[u8]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut i = 0;
    while let Some(start) = find_from(src, PREFIX, i) {
        i = start + PREFIX.len();
        if start > 0 && is_ident(src[start - 1]) {
            continue;
        }
        let mut end = i;
        while end < src.len() && is_ident(src[end]) {
            end += 1;
        }
        let mut paren = end;
        while paren < src.len() && src[paren].is_ascii_whitespace() {
            paren += 1;
        }
        if src.get(paren) == Some(&b'(') {
            names.insert(String::from_utf8_lossy(&src[start..end]).into_owned());
        }
        i = end;
    }
    names
}

fn header_exports() -> BTreeSet<String> {
    names_in(&without_comments(&read(&repo_root().join("ffi/include/int2dds-ffi.h"))))
}

fn cs_sources(dir: &Path, into: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("the C# binding lives at {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            cs_sources(&path, into);
        } else if path.extension().is_some_and(|e| e == "cs") {
            into.push(path);
        }
    }
}

/// Each `[DllImport]`/`[LibraryImport]` up to the `;` that ends the declaration it
/// decorates. One name per attribute, or the attribute went unparsed.
fn csharp_declarations() -> BTreeSet<String> {
    let mut sources = Vec::new();
    cs_sources(&repo_root().join("csharp/src/Int2Dds"), &mut sources);
    sources.sort();
    assert!(!sources.is_empty(), "no C# sources found to check");

    let mut names = BTreeSet::new();
    let mut attributes = 0;
    for path in &sources {
        let src = without_comments(&read(path));
        let mut i = 0;
        while let Some(start) = [b"[DllImport".as_slice(), b"[LibraryImport".as_slice()]
            .iter()
            .filter_map(|needle| find_from(&src, needle, i))
            .min()
        {
            let end = find_from(&src, b";", start).unwrap_or(src.len());
            let declared = names_in(&src[start..end]);
            assert_eq!(
                declared.len(),
                1,
                "{}: the declaration at byte {start} does not name its entry point where this \
                 test looks for it, and was read as {declared:?}. `EntryPoint = \"...\"` puts the \
                 name in the attribute instead; teach the extractor that form rather than \
                 leaving the declaration uncounted",
                path.display()
            );
            names.extend(declared);
            attributes += 1;
            i = end;
        }
    }
    assert_eq!(
        names.len(),
        attributes,
        "an export has more than one C# declaration. That is legitimate when the overloads \
         marshal differently — widen this check rather than deleting one"
    );
    names
}

/// The `ffi.cdef("""...""")` blocks, plus a check that no declaration sits outside
/// one — cffi ignores anything that does, so it would bind nothing.
fn python_declarations() -> BTreeSet<String> {
    let path = repo_root().join("python/int2dds/_ffi/_bindings.py");
    let src = read(&path);
    let mut names = BTreeSet::new();
    let mut outside = Vec::new();
    let mut blocks = 0;
    let mut i = 0;
    loop {
        let Some(open) = find_from(&src, b"ffi.cdef(\"\"\"", i) else {
            outside.extend_from_slice(&src[i..]);
            break;
        };
        outside.extend_from_slice(&src[i..open]);
        let body = open + b"ffi.cdef(\"\"\"".len();
        let close = find_from(&src, b"\"\"\"", body).expect("cdef block is closed");
        names.extend(names_in(&without_comments(&src[body..close])));
        blocks += 1;
        i = close + 3;
    }
    assert!(blocks > 0, "{} declares nothing to cffi", path.display());
    let stray = names_in(&without_comments(&outside));
    assert!(stray.is_empty(), "{}: declared outside any cdef block: {stray:?}", path.display());
    names
}

fn assert_no_phantoms(binding: &str, declared: &BTreeSet<String>, exports: &BTreeSet<String>) {
    let phantoms: Vec<_> = declared.difference(exports).collect();
    assert!(
        phantoms.is_empty(),
        "{binding} declares entry points the library does not export, which faults at the \
         first call rather than at build time: {phantoms:#?}"
    );
}

fn assert_unbound_is(binding: &str, declared: &BTreeSet<String>, recorded: &[&str]) {
    let exports = header_exports();
    let unbound: BTreeSet<&str> = exports.difference(declared).map(String::as_str).collect();
    let recorded: BTreeSet<&str> = recorded.iter().copied().collect();

    let unrecorded: Vec<_> = unbound.difference(&recorded).collect();
    assert!(
        unrecorded.is_empty(),
        "these exports have no {binding} declaration. Bind them, or record them in the \
         list at the top of this file: {unrecorded:#?}"
    );
    let stale: Vec<_> = recorded.difference(&unbound).collect();
    assert!(
        stale.is_empty(),
        "these are recorded as unbound, but {binding} declares them now — or the export is \
         gone. Either way, strike them off the list at the top of this file: {stale:#?}"
    );
}

// ── The header's composite types against the C# binding ─────────────────────

/// Header composites with no C# declaration, and there are none: all 17 are
/// declared, 16 prefixed `Native` and `Int2DdsMemberInfo` under the header's own
/// name. An entry here would be a struct the ABI passes that C# cannot see.
const UNDECLARED_CSHARP: &[&str] = &[];

/// What each header scalar has to be spelled as in C#. Absent means fail, not
/// skip: how wide a field marshals is the decision `[StructLayout]` exists to
/// make, and this test cannot infer it from the header. A new field of an unlisted
/// type should stop here rather than pass unchecked.
const CSHARP_SCALARS: &[(&str, &str)] =
    &[("bool", "bool"), ("int32_t", "int"), ("uint8_t", "byte"), ("uint32_t", "uint")];

/// The C# names a header type may appear under. Three spellings are in use and
/// there is no rule that picks one, so all three are accepted.
fn csharp_names(header_name: &str) -> Vec<String> {
    let bare = header_name.strip_prefix("Int2Dds").unwrap_or(header_name);
    vec![format!("Native{bare}"), header_name.to_owned(), bare.to_owned()]
}

fn pascal(snake: &str) -> String {
    snake
        .split('_')
        .map(|word| match word.chars().next() {
            Some(first) => first.to_ascii_uppercase().to_string() + &word[first.len_utf8()..],
            None => String::new(),
        })
        .collect()
}

/// One `keyword NAME [: underlying] { body }`, as the underlying type it names --
/// which is the whole point for an enum -- and its body.
struct Composite {
    underlying: Option<String>,
    body: String,
}

/// Composites by name. A declaration with no body is skipped: that is an opaque
/// handle, and having no layout to compare is the point of one.
///
/// The scan steps over preprocessor lines between the name and the body, because
/// that is where an explicitly sized enum puts its width: cbindgen guards the
/// `: int32_t` form for the languages that have it and emits a `typedef` of the
/// same width for the ones that do not.
fn composites(src: &[u8], keyword: &[u8]) -> BTreeMap<String, Composite> {
    let mut out = BTreeMap::new();
    let mut i = 0;
    while let Some(start) = find_from(src, keyword, i) {
        i = start + keyword.len();
        if start > 0 && is_ident(src[start - 1]) {
            continue;
        }
        let mut cursor = i;
        while cursor < src.len() && src[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < src.len() && is_ident(src[cursor]) {
            cursor += 1;
        }
        let name = String::from_utf8_lossy(&src[name_start..cursor]).into_owned();

        let mut underlying = None;
        loop {
            while cursor < src.len() && src[cursor].is_ascii_whitespace() {
                cursor += 1;
            }
            match src.get(cursor) {
                Some(b'#') => cursor = find_from(src, b"\n", cursor).unwrap_or(src.len()),
                Some(b':') if underlying.is_none() => {
                    cursor += 1;
                    while cursor < src.len() && src[cursor].is_ascii_whitespace() {
                        cursor += 1;
                    }
                    let from = cursor;
                    while cursor < src.len() && is_ident(src[cursor]) {
                        cursor += 1;
                    }
                    underlying = Some(String::from_utf8_lossy(&src[from..cursor]).into_owned());
                }
                _ => break,
            }
        }
        if src.get(cursor) != Some(&b'{') {
            continue;
        }

        let open = cursor;
        let mut depth = 0;
        while cursor < src.len() {
            match src[cursor] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                _ => {}
            }
            cursor += 1;
        }
        let body = String::from_utf8_lossy(&src[open + 1..cursor]).into_owned();
        out.insert(name, Composite { underlying, body });
        i = cursor;
    }
    out
}

/// A declaration split into its type, its name and any array extent, with runs of
/// whitespace collapsed. `None` for the empty tail after the last separator.
fn declaration_parts(decl: &str) -> Option<(String, String, Option<String>)> {
    let decl = decl.trim();
    if decl.is_empty() {
        return None;
    }
    let (head, extent) = match decl.split_once('[') {
        Some((head, rest)) => (head, Some(rest.trim_end_matches(']').trim().to_owned())),
        None => (decl, None),
    };
    let mut words: Vec<&str> = head.split_whitespace().collect();
    let raw_name = words.pop()?;
    // `const char *name` puts the pointer on the name token; it belongs to the type.
    let name = raw_name.trim_start_matches('*');
    if name.is_empty() {
        return None;
    }
    let mut ty = words.join(" ");
    for _ in 0..raw_name.len() - name.len() {
        ty.push('*');
    }
    Some((ty, name.to_owned(), extent))
}

/// The C# declaration a header member requires, as a string to compare against
/// the one C# actually has. `enums` resolves a field whose type is one of the
/// header's enums; it is passed in rather than looked up here so that both tests
/// resolve an enum's C# name from the same reading of the sources.
fn required_csharp(
    ty: &str,
    name: &str,
    extent: Option<&str>,
    enums: &BTreeMap<String, Composite>,
) -> String {
    let name = pascal(name);
    // A pointer is a pointer: C# marshals every one as `IntPtr`, and what it
    // points at is no part of the struct's layout.
    if ty == "Int2DdsUserContext" || (ty.starts_with("Int2DdsOn") && ty.ends_with("Callback")) {
        return format!("IntPtr {name}");
    }
    // A pointer field is a pointer whatever it points at; C# marshals it as `IntPtr`.
    if ty.ends_with('*') {
        return format!("IntPtr {name}");
    }
    let Some(cs) = CSHARP_SCALARS.iter().find(|(c, _)| *c == ty).map(|(_, cs)| *cs) else {
        // Not a scalar and not a pointer, so it is a composite named in a field --
        // an enum -- and it is spelled with whichever of its accepted names C#
        // declares. Whether that declaration agrees with the header is the enum
        // test's job; all this needs is the name.
        let Some(cs) = csharp_name_of(ty, enums) else {
            panic!(
                "{ty} is a field of a header struct, and is neither a scalar this test knows how \
                 to require a width for nor an enum C# declares. Add it to CSHARP_SCALARS with \
                 the C# type it marshals as -- leaving it unchecked would let the field's width \
                 drift, and every field after it moves when it does"
            )
        };
        return format!("{cs} {name}");
    };
    match extent {
        // Inline storage, which in C# is `fixed` and nothing else -- a plain
        // `byte[]` field marshals as a reference and moves every field after it.
        Some(extent) => format!("fixed {cs} {name}[{extent}]"),
        // C# marshals a bare `bool` in a struct as the 4-byte Win32 `BOOL`, so the
        // attribute is not decoration: without it the field is 3 bytes too wide.
        None if ty == "bool" => format!("[MarshalAs(UnmanagedType.U1)] {cs} {name}"),
        None => format!("{cs} {name}"),
    }
}

/// The C# side of the same comparison: members with their C#-only modifiers
/// dropped, but `fixed` and any attribute kept, because both change the layout.
fn declared_csharp(body: &str) -> Vec<String> {
    body.split(';')
        .filter_map(|member| {
            let mut attributes = String::new();
            let mut rest = member.trim();
            while let Some(after) = rest.strip_prefix('[') {
                let (attribute, tail) = after.split_once(']')?;
                attributes.push_str(&format!(
                    "[{}] ",
                    attribute.split_whitespace().collect::<Vec<_>>().join("")
                ));
                rest = tail.trim();
            }
            let kept: Vec<&str> = rest
                .split_whitespace()
                .filter(|word| {
                    !matches!(*word, "public" | "internal" | "private" | "unsafe" | "readonly")
                })
                .collect();
            let (ty, name, extent) = declaration_parts(&kept.join(" "))?;
            Some(match extent {
                Some(extent) => format!("{attributes}{ty} {name}[{extent}]"),
                None => format!("{attributes}{ty} {name}"),
            })
        })
        .collect()
}

/// Every `enum` body reduced to `(name, value)`, with C's implicit numbering
/// applied so that an enumerator without `=` is still compared.
fn enumerators(body: &str) -> Vec<(String, i64)> {
    let mut next = 0;
    body.split(',')
        .filter_map(|entry| {
            let entry = entry.trim();
            if entry.is_empty() {
                return None;
            }
            let (name, value) = match entry.split_once('=') {
                Some((name, value)) => (
                    name.trim(),
                    parse_value(value).unwrap_or_else(|| {
                        panic!("`{entry}` does not give a value this test can read")
                    }),
                ),
                None => (entry, next),
            };
            next = value + 1;
            Some((name.to_owned(), value))
        })
        .collect()
}

fn header_composites(keyword: &[u8]) -> BTreeMap<String, Composite> {
    composites(&without_comments(&read(&repo_root().join("ffi/include/int2dds-ffi.h"))), keyword)
}

fn csharp_composites(keyword: &[u8]) -> BTreeMap<String, Composite> {
    let mut sources = Vec::new();
    cs_sources(&repo_root().join("csharp/src/Int2Dds"), &mut sources);
    sources.sort();
    let mut out = BTreeMap::new();
    for path in &sources {
        out.extend(composites(&without_comments(&read(path)), keyword));
    }
    out
}

/// Which of a header composite's accepted names C# declares it under.
fn csharp_name_of<'a>(
    header_name: &str,
    declared: &'a BTreeMap<String, Composite>,
) -> Option<&'a String> {
    csharp_names(header_name).iter().find_map(|name| declared.get_key_value(name)).map(|(k, _)| k)
}

fn assert_declared_or_recorded(
    header: &BTreeMap<String, Composite>,
    declared: &BTreeMap<String, Composite>,
) {
    let missing: Vec<&str> = header
        .keys()
        .filter(|name| csharp_name_of(name, declared).is_none())
        .map(String::as_str)
        .filter(|name| !UNDECLARED_CSHARP.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "the C# binding declares no counterpart for these header composites. Declare them, \
         or record them in the list at the top of this file: {missing:#?}"
    );
}

#[test]
fn the_csharp_structs_have_the_header_layout() {
    let header = header_composites(b"typedef struct");
    let declared = csharp_composites(b"struct");
    let enums = csharp_composites(b"enum");
    assert_eq!(
        header.len(),
        19,
        "the header's field-bearing struct count moved. That is not a failure by itself, but \
         a struct this test never saw is one it never checked"
    );
    assert_declared_or_recorded(&header, &declared);

    for (name, composite) in &header {
        let Some(csharp) = csharp_name_of(name, &declared).map(|name| &declared[name]) else {
            continue;
        };
        let required: Vec<String> = composite
            .body
            .split(';')
            .filter_map(declaration_parts)
            .map(|(ty, field, extent)| required_csharp(&ty, &field, extent.as_deref(), &enums))
            .collect();
        assert_eq!(
            declared_csharp(&csharp.body),
            required,
            "the C# declaration of {name} does not have the header's layout. Every field's \
             position depends on the width of the ones before it, so one wrong type silently \
             moves the rest -- the C# side is the left column"
        );
    }
}

#[test]
fn the_csharp_enums_have_the_header_enumerators() {
    let header = header_composites(b"enum");
    let declared = csharp_composites(b"enum");
    assert_eq!(
        header.len(),
        3,
        "the header's enum count moved. That is not a failure by itself, but an enum this test \
         never saw is one it never checked"
    );
    assert_declared_or_recorded(&header, &declared);

    for (name, composite) in &header {
        let Some(csharp) = csharp_name_of(name, &declared).map(|name| &declared[name]) else {
            continue;
        };
        // C# spells an unstated underlying type `int`; the header states its own,
        // because these enums are struct fields and their width is part of a layout.
        let header_width = composite.underlying.as_deref().unwrap_or("int");
        let width = csharp.underlying.as_deref().unwrap_or("int");
        assert_eq!(
            Some(width),
            CSHARP_SCALARS.iter().find(|(c, _)| *c == header_width).map(|(_, cs)| *cs),
            "the C# declaration of {name} is {width}-wide where the header says {header_width}"
        );
        assert_eq!(
            enumerators(&csharp.body),
            enumerators(&composite.body),
            "the C# declaration of {name} does not have the header's enumerators. A value the \
             library returns and C# cannot name arrives in managed code as a number no `switch` \
             matches -- the C# side is the left column"
        );
    }
}

// ---------------------------------------------------------------------------
// The header's `#define` constants against the two bindings.
//
// This is the axis the comparisons above do not reach. cbindgen writes these
// constants as `#define`, and `generate_bindings.py` drops every `#define` line,
// so unlike the functions and the structs there is no generated copy anywhere:
// both bindings write the values out by hand and nothing read them back. The
// survey that added this found the status bits in four hand-written copies -- two
// in C#, two in Python -- each missing a different subset of the header, and
// `FieldType.Enum = 14` in C# where the header's 14 is `CHAR16`, a public
// constant that gave a char16 field to anyone who named it.
//
// Two comparisons, because they fail differently. Values are compared for every
// name both sides have, which cannot false-fire. Names are compared as sets,
// which is what catches a constant a binding never learned, and needs the two
// lists below wherever a binding deliberately names fewer or more than the
// header. Both lists are folded into the expected set rather than filtered out
// of the actual one, so recording a name that turns out to be present fails just
// as loudly as omitting one that is missing -- neither list can go stale.
//
// What this cannot catch is a wrong entry in the table below, the same shape of
// gap as the struct comparison's: the table says where a family is mirrored, and
// a family pointed at the wrong type would compare something real against
// something else real and pass.
// ---------------------------------------------------------------------------

/// How C# spells a family: an `enum`, or a `static class` of `const` fields.
#[derive(Clone, Copy)]
enum CsKind {
    Enum,
    Class,
}

/// How Python spells one header suffix: at module level under a pattern, where
/// `{}` stands for the suffix, or as a member of a class named exactly it.
#[derive(Clone, Copy)]
enum PyScope {
    Module(&'static str),
    Class(&'static str),
}

/// One `#define` family and the single place each binding mirrors it. Both
/// bindings are required to have one: the family that had no C# mirror when this
/// was written turned out to be a gap rather than a decision -- C# handed the
/// member flags back as a bare `int` and named the bits only in a doc comment,
/// where nothing could compare them.
struct Family {
    prefix: &'static str,
    csharp: (&'static str, CsKind),
    python: (&'static str, PyScope),
}

/// Every family the header defines. A constant whose name matches none of these
/// fails: an unmapped family is one nothing compares, which is the state this
/// whole section exists to end.
const FAMILIES: &[Family] = &[
    Family {
        prefix: "INT2DDS_RET_",
        csharp: ("ReturnCode", CsKind::Class),
        python: ("python/int2dds/exceptions.py", PyScope::Module("INT2DDS_RET_{}")),
    },
    Family {
        prefix: "INT2DDS_STATUS_",
        csharp: ("StatusMask", CsKind::Enum),
        python: ("python/int2dds/core/conditions.py", PyScope::Module("STATUS_{}")),
    },
    Family {
        prefix: "INT2DDS_SAMPLE_STATE_",
        csharp: ("SampleState", CsKind::Class),
        python: ("python/int2dds/core/conditions.py", PyScope::Module("{}_SAMPLE_STATE")),
    },
    Family {
        prefix: "INT2DDS_VIEW_STATE_",
        csharp: ("ViewState", CsKind::Class),
        python: ("python/int2dds/core/conditions.py", PyScope::Module("{}_VIEW_STATE")),
    },
    Family {
        prefix: "INT2DDS_INSTANCE_STATE_",
        csharp: ("InstanceState", CsKind::Class),
        python: ("python/int2dds/core/conditions.py", PyScope::Module("{}_INSTANCE_STATE")),
    },
    Family {
        prefix: "INT2DDS_FIELD_",
        csharp: ("FieldType", CsKind::Class),
        python: ("python/int2dds/types/dynamic.py", PyScope::Module("FIELD_{}")),
    },
    Family {
        prefix: "INT2DDS_VALUE_KIND_",
        csharp: ("DynamicValueKind", CsKind::Enum),
        python: ("python/int2dds/types/dynamic.py", PyScope::Module("VALUE_KIND_{}")),
    },
    Family {
        prefix: "INT2DDS_MEMBER_",
        csharp: ("MemberFlags", CsKind::Class),
        python: ("python/int2dds/types/dynamic.py", PyScope::Module("MEMBER_{}")),
    },
    Family {
        prefix: "INT2DDS_QOS_RELIABILITY_",
        csharp: ("ReliabilityKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("ReliabilityKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_DURABILITY_",
        csharp: ("DurabilityKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("DurabilityKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_HISTORY_",
        csharp: ("HistoryKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("HistoryKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_DATA_REPR_",
        csharp: ("DataRepresentationKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("DataRepresentationKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_LIVELINESS_",
        csharp: ("LivelinessKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("LivelinessKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_OWNERSHIP_",
        csharp: ("OwnershipKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("OwnershipKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_DEST_ORDER_",
        csharp: ("DestinationOrderKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("DestinationOrderKind")),
    },
    Family {
        prefix: "INT2DDS_QOS_LIFESPAN_REF_",
        csharp: ("LifespanReferenceKind", CsKind::Enum),
        python: ("python/int2dds/core/qos.py", PyScope::Class("LifespanReferenceKind")),
    },
];

/// Header constants a binding names nowhere, as `(language, header name)`.
///
/// The dynamic return codes are the whole list, and both bindings leave them out
/// for the same stated reason: an unrecognised code falls through to the generic
/// error carrying its own number (`check_ret` in `exceptions.py`, the `_` arm of
/// `ReturnCodeHelper` in `DdsException.cs`), so a caller can still tell 202 from
/// 204 without either binding having a name for it.
const UNNAMED: &[(&str, &str)] = &[
    ("C#", "INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND"),
    ("C#", "INT2DDS_RET_DYNAMIC_TYPE_MISMATCH"),
    ("C#", "INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE"),
    ("C#", "INT2DDS_RET_DYNAMIC_TIMEOUT"),
    ("C#", "INT2DDS_RET_DYNAMIC_DECODE_ERROR"),
    ("Python", "INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND"),
    ("Python", "INT2DDS_RET_DYNAMIC_TYPE_MISMATCH"),
    ("Python", "INT2DDS_RET_DYNAMIC_UNSUPPORTED_TYPE"),
    ("Python", "INT2DDS_RET_DYNAMIC_TIMEOUT"),
    ("Python", "INT2DDS_RET_DYNAMIC_DECODE_ERROR"),
];

/// Members a binding has that the header does not define, as `(language,
/// family prefix, member name, value)`. The empty and full masks are not status
/// bits, so the header has nothing to say about them, but their values are still
/// worth pinning here rather than nowhere.
const EXTRA: &[(&str, &str, &str, i64)] =
    &[("C#", "INT2DDS_STATUS_", "None", 0), ("C#", "INT2DDS_STATUS_", "All", 0xFFFF_FFFF)];

/// A C or C# or Python integer literal: decimal, hex, or a shift, with the
/// parentheses and any width suffix the three languages spell differently.
fn parse_value(text: &str) -> Option<i64> {
    let text = text.trim();
    let text = match text.strip_prefix('(').and_then(|inner| inner.strip_suffix(')')) {
        Some(inner) => inner.trim(),
        None => text,
    };
    if let Some((lhs, rhs)) = text.split_once("<<") {
        return Some(parse_value(lhs)? << parse_value(rhs)?);
    }
    let text = text.trim_end_matches(['u', 'U', 'l', 'L']);
    match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        Some(hex) => i64::from_str_radix(hex, 16).ok(),
        None => text.parse().ok(),
    }
}

/// `#define NAME value` for every name that has a value this can read. The
/// include guard has no value and `INT2DDS_DEPRECATED` is function-like, so both
/// fall out here without needing to be named.
fn header_defines() -> BTreeMap<String, i64> {
    let src = read(&repo_root().join("ffi/include/int2dds-ffi.h"));
    String::from_utf8_lossy(&src)
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("#define ")?;
            let (name, value) = rest.split_once(' ')?;
            if !name.starts_with("INT2DDS_") || name.contains('(') {
                return None;
            }
            Some((name.to_owned(), parse_value(value)?))
        })
        .collect()
}

/// The family a header constant belongs to, by longest matching prefix so that
/// one family's prefix cannot swallow another's.
fn family_of(name: &str) -> Option<&'static Family> {
    FAMILIES
        .iter()
        .filter(|family| name.starts_with(family.prefix))
        .max_by_key(|family| family.prefix.len())
}

/// Underscores dropped and lowercased, which is how a header's `UINT8` and C#'s
/// `UInt8` are recognised as the same name. Python keeps the header's spelling
/// and is compared as written.
fn squashed(name: &str) -> String {
    name.chars().filter(|c| *c != '_').flat_map(char::to_lowercase).collect()
}

/// `(name, value)` for one C# mirror.
fn csharp_members(kind: CsKind, body: &str) -> Vec<(String, i64)> {
    match kind {
        CsKind::Enum => enumerators(body),
        CsKind::Class => body
            .split(';')
            .filter_map(|member| {
                let (declaration, value) = member.split_once('=')?;
                Some((declaration.split_whitespace().last()?.to_owned(), parse_value(value)?))
            })
            .collect(),
    }
}

/// Module-level and class-scoped `NAME = value` from one Python module, the
/// latter keyed `Class::NAME`. A class body runs until the next line that starts
/// at column zero, which is what separates `qos.py`'s enums from each other.
///
/// This reads lines, not syntax: a docstring containing `NAME = 5` at a matching
/// indent is read as a constant. Harmless only because a stray name has to match
/// a family's pattern to reach a comparison at all -- if one ever does, the fix
/// is a real parse here, not a wider pattern.
fn python_constants(path: &Path) -> BTreeMap<String, i64> {
    let src = read(path);
    let mut out = BTreeMap::new();
    let mut class = None;
    for line in String::from_utf8_lossy(&src).lines() {
        let indented = line.starts_with([' ', '\t']);
        if !indented && !line.trim().is_empty() {
            class = line
                .strip_prefix("class ")
                .map(|rest| rest.split([':', '(']).next().unwrap_or(rest).trim().to_owned());
        }
        let Some((name, value)) = line.trim().split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit())
        {
            continue;
        }
        let Some(value) = parse_value(value) else {
            continue;
        };
        match (indented, &class) {
            (true, Some(class)) => out.insert(format!("{class}::{name}"), value),
            (false, _) => out.insert(name.to_owned(), value),
            (true, None) => continue,
        };
    }
    out
}

/// The name a header suffix is expected to have in each binding.
fn expected_names(family: &Family, suffix: &str) -> (String, String) {
    let csharp = squashed(suffix);
    let python = match family.python.1 {
        PyScope::Module(pattern) => pattern.replace("{}", suffix),
        PyScope::Class(class) => format!("{class}::{suffix}"),
    };
    (csharp, python)
}

/// Every header constant, grouped by the family it belongs to.
fn defines_by_family() -> BTreeMap<&'static str, Vec<(String, i64)>> {
    let mut out: BTreeMap<&'static str, Vec<(String, i64)>> = BTreeMap::new();
    for (name, value) in header_defines() {
        let family = family_of(&name).unwrap_or_else(|| {
            panic!(
                "{name} is a header constant belonging to no family in FAMILIES, so nothing \
                 compares it against either binding. Add its family with the C# type and the \
                 Python module that mirror it"
            )
        });
        out.entry(family.prefix).or_default().push((name, value));
    }
    out
}

#[test]
fn every_header_constant_belongs_to_a_mapped_family() {
    let by_family = defines_by_family();
    assert_eq!(
        by_family.values().map(Vec::len).sum::<usize>(),
        113,
        "the header's constant count moved. That is not a failure by itself, but it is worth \
         knowing which way it went before trusting the comparisons below"
    );
    let unmapped: Vec<&str> =
        FAMILIES.iter().map(|f| f.prefix).filter(|p| !by_family.contains_key(p)).collect();
    assert!(
        unmapped.is_empty(),
        "these families are mapped to a binding but the header defines nothing under them, so \
         the mapping is describing something that no longer exists: {unmapped:#?}"
    );
}

#[test]
fn the_csharp_constants_have_the_header_values() {
    let declared_enums = csharp_composites(b"enum");
    let declared_classes = csharp_composites(b"class");

    for (prefix, defines) in defines_by_family() {
        let family = FAMILIES.iter().find(|f| f.prefix == prefix).expect("grouped by prefix");
        let (type_name, kind) = family.csharp;
        let declared = match kind {
            CsKind::Enum => &declared_enums,
            CsKind::Class => &declared_classes,
        };
        let composite = declared.get(type_name).unwrap_or_else(|| {
            panic!(
                "FAMILIES maps {prefix} to the C# {type_name}, which the binding does not \
                 declare. Either it moved and the table needs the new name, or the mirror is \
                 gone and every constant in the family is now unchecked"
            )
        });

        let mut expected: BTreeMap<String, i64> = defines
            .iter()
            .filter(|(name, _)| !UNNAMED.contains(&("C#", name.as_str())))
            .map(|(name, value)| (expected_names(family, &name[prefix.len()..]).0, *value))
            .collect();
        for (language, family_prefix, name, value) in EXTRA {
            if *language == "C#" && *family_prefix == prefix {
                expected.insert(squashed(name), *value);
            }
        }
        let actual: BTreeMap<String, i64> = csharp_members(kind, &composite.body)
            .into_iter()
            .map(|(name, value)| (squashed(&name), value))
            .collect();
        assert_eq!(
            actual, expected,
            "the C# {type_name} does not mirror the header's {prefix} constants. A missing name \
             is one the library can return and C# cannot spell; a wrong value is one it spells \
             and means something else by -- the C# side is the left column"
        );
    }
}

#[test]
fn the_python_constants_have_the_header_values() {
    let root = repo_root();
    let mut modules: BTreeMap<&str, BTreeMap<String, i64>> = BTreeMap::new();

    for (prefix, defines) in defines_by_family() {
        let family = FAMILIES.iter().find(|f| f.prefix == prefix).expect("grouped by prefix");
        let (module, scope) = family.python;
        let constants =
            modules.entry(module).or_insert_with(|| python_constants(&root.join(module))).clone();

        let expected: BTreeMap<String, i64> = defines
            .iter()
            .filter(|(name, _)| !UNNAMED.contains(&("Python", name.as_str())))
            .map(|(name, value)| (expected_names(family, &name[prefix.len()..]).1, *value))
            .collect();
        // Only the names this family owns: one module holds several families,
        // and the states share `conditions.py` with the status bits.
        let actual: BTreeMap<String, i64> = constants
            .into_iter()
            .filter(|(name, _)| match scope {
                PyScope::Module(pattern) => match pattern.split_once("{}") {
                    Some((head, tail)) => {
                        name.starts_with(head)
                            && name.ends_with(tail)
                            && name.len() > head.len() + tail.len()
                    }
                    None => false,
                },
                PyScope::Class(class) => name.starts_with(&format!("{class}::")),
            })
            .collect();
        assert_eq!(
            actual, expected,
            "{module} does not mirror the header's {prefix} constants -- the Python side is the \
             left column"
        );
    }
}

#[test]
fn csharp_declares_only_real_exports() {
    assert_no_phantoms("The C# binding", &csharp_declarations(), &header_exports());
}

#[test]
fn python_declares_only_real_exports() {
    assert_no_phantoms("The Python binding", &python_declarations(), &header_exports());
}

#[test]
fn the_csharp_binding_leaves_exactly_the_recorded_exports_unbound() {
    assert_unbound_is("C#", &csharp_declarations(), UNBOUND_CSHARP);
}

#[test]
fn the_python_binding_leaves_exactly_the_recorded_exports_unbound() {
    assert_unbound_is("Python", &python_declarations(), UNBOUND_PYTHON);
}
