//! Rust FFI type → (Java type, JNI type) mapping.
//!
//! The table is deterministic and total over the types `int2dds-ffi` actually
//! uses. Anything outside it returns `None` so the generator fails loudly
//! instead of emitting a wrong signature.

use crate::gen::parse::FfiFn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Scalar,
    /// Opaque handle or raw address, carried as a Java `long`.
    Pointer,
    /// NUL-terminated C string, carried as a UTF-8 `byte[]`.
    CString,
    /// Array of C strings, carried as `byte[][]`.
    CStringArray,
    /// Fixed-size byte buffer the FFI reads, such as `*const [u8; 16]`.
    ByteArrayIn,
    /// Fixed-size byte buffer the FFI *writes*, such as `*mut [u8; 16]`.
    ///
    /// Distinct from [`Kind::ByteArrayIn`] because the forwarder must copy the
    /// result back into the caller's Java array after the call. Treating an out
    /// parameter as an in parameter compiles and runs, and silently returns
    /// nothing — 17 functions in the FFI surface take one.
    ByteArrayOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapped {
    pub java: &'static str,
    pub jni: &'static str,
    pub kind: Kind,
    /// Element count for the fixed-size byte-array kinds; `None` otherwise.
    pub len: Option<usize>,
}

const fn m(java: &'static str, jni: &'static str, kind: Kind) -> Mapped {
    Mapped { java, jni, kind, len: None }
}

/// Element count of a `[u8; N]` pointee, or `None` if this is not one.
fn fixed_array_len(pointee: &str) -> Option<usize> {
    let inner = pointee.strip_prefix('[')?.strip_suffix(']')?;
    let (elem, count) = inner.split_once(';')?;
    if elem.trim() != "u8" {
        return None;
    }
    count.trim().parse().ok()
}

/// Strip the `std::os::raw::` prefix so `c_char` spellings unify.
fn canonical(ty: &str) -> String {
    ty.replace("std :: os :: raw :: ", "")
        .replace("std::os::raw::", "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn map_type(rust_ty: &str) -> Option<Mapped> {
    let t = canonical(rust_ty);
    let scalar = match t.as_str() {
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "Int2DdsRet" => {
            Some(m("int", "jint", Kind::Scalar))
        }
        "i64" | "u64" | "usize" | "isize" => Some(m("long", "jlong", Kind::Scalar)),
        "bool" => Some(m("boolean", "jboolean", Kind::Scalar)),
        "f32" => Some(m("float", "jfloat", Kind::Scalar)),
        "f64" => Some(m("double", "jdouble", Kind::Scalar)),
        _ => None,
    };
    if scalar.is_some() {
        return scalar;
    }
    if !t.starts_with('*') {
        return None;
    }
    // Reject anything containing a function type; those need hand-writing.
    if t.contains("fn ") || t.contains("Option") {
        return None;
    }
    // Strip every leading `*const ` / `*mut ` to reach the pointee, and count
    // how many levels we stripped.
    let mut pointee = t.as_str();
    let mut depth = 0usize;
    loop {
        if let Some(rest) = pointee.strip_prefix("*const ") {
            pointee = rest;
            depth += 1;
        } else if let Some(rest) = pointee.strip_prefix("*mut ") {
            pointee = rest;
            depth += 1;
        } else {
            break;
        }
    }
    if pointee == "c_char" {
        return match depth {
            1 => Some(m("byte[]", "JByteArray<'local>", Kind::CString)),
            2 => Some(m("byte[][]", "JObjectArray<'local>", Kind::CStringArray)),
            _ => None,
        };
    }
    if depth == 1 {
        if let Some(n) = fixed_array_len(pointee) {
            let kind = if t.starts_with("*mut ") { Kind::ByteArrayOut } else { Kind::ByteArrayIn };
            return Some(Mapped { java: "byte[]", jni: "JByteArray<'local>", kind, len: Some(n) });
        }
    }
    // Every other pointer, including double pointers to opaque structs, is a
    // raw address. Java obtains addresses via Ffi.directBufferAddress().
    Some(m("long", "jlong", Kind::Pointer))
}

/// A function is generatable when every parameter type and the return type map.
pub fn is_generatable(f: &FfiFn) -> bool {
    if f.ret != "()" && map_type(&f.ret).is_none() {
        return false;
    }
    f.params.iter().all(|p| map_type(&p.ty).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen::parse::{parse_ffi_dir, FfiFn, Param};
    use std::path::Path;

    fn p(name: &str, ty: &str) -> Param {
        Param { name: name.into(), ty: ty.into() }
    }

    #[test]
    fn scalars_map_to_java_primitives() {
        assert_eq!(map_type("i32").unwrap().java, "int");
        assert_eq!(map_type("u32").unwrap().java, "int");
        assert_eq!(map_type("u16").unwrap().java, "int");
        assert_eq!(map_type("u8").unwrap().java, "int");
        assert_eq!(map_type("i64").unwrap().java, "long");
        assert_eq!(map_type("usize").unwrap().java, "long");
        assert_eq!(map_type("bool").unwrap().java, "boolean");
        assert_eq!(map_type("Int2DdsRet").unwrap().java, "int");
        assert_eq!(map_type("f32").unwrap().java, "float");
        assert_eq!(map_type("f64").unwrap().java, "double");
    }

    #[test]
    fn opaque_pointers_map_to_long() {
        assert_eq!(map_type("*const Int2DdsDataWriter").unwrap().java, "long");
        assert_eq!(map_type("*mut *mut Int2DdsSampleSeq").unwrap().java, "long");
        assert_eq!(map_type("*const u8").unwrap().java, "long");
        assert_eq!(map_type("*mut c_void").unwrap().java, "long");
        // A path-qualified struct is still just an address.
        assert_eq!(map_type("*mut *mut crate::dynamic::Int2DdsTypeObject").unwrap().java, "long");
    }

    #[test]
    fn c_strings_map_to_byte_arrays_never_string() {
        // JNI's NewStringUTF uses modified UTF-8, which corrupts NUL bytes and
        // supplementary-plane characters. C strings must cross as UTF-8 byte[].
        for t in [
            "*const c_char",
            "*mut c_char",
            "*const std::os::raw::c_char",
            "*mut std::os::raw::c_char",
        ] {
            let m = map_type(t).unwrap_or_else(|| panic!("{t} must map"));
            assert_eq!(m.java, "byte[]", "{t}");
            assert_eq!(m.kind, Kind::CString, "{t}");
        }
        assert_eq!(map_type("*const *const c_char").unwrap().java, "byte[][]");
        assert_eq!(map_type("*const *const std::os::raw::c_char").unwrap().java, "byte[][]");
    }

    #[test]
    fn fixed_size_byte_arrays_map_to_byte_array() {
        // GUIDs are [u8; 16]; built-in topic keys are [u8; 12].
        assert_eq!(map_type("*mut [u8; 16]").unwrap().java, "byte[]");
        assert_eq!(map_type("*const [u8; 16]").unwrap().java, "byte[]");
        assert_eq!(map_type("*mut [u8; 12]").unwrap().java, "byte[]");
    }

    #[test]
    fn mutable_fixed_arrays_are_out_parameters() {
        // The direction decides whether the forwarder copies the result back to
        // Java. Conflating them loses every instance handle and GUID the FFI
        // writes, without any compile or runtime error.
        let out = map_type("*mut [u8; 16]").unwrap();
        assert_eq!(out.kind, Kind::ByteArrayOut);
        assert_eq!(out.len, Some(16));

        let inp = map_type("*const [u8; 16]").unwrap();
        assert_eq!(inp.kind, Kind::ByteArrayIn);
        assert_eq!(inp.len, Some(16));

        assert_eq!(map_type("*mut [u8; 12]").unwrap().len, Some(12));
        // A plain pointer carries no length.
        assert_eq!(map_type("*mut Int2DdsTopic").unwrap().len, None);
    }

    #[test]
    fn the_real_ffi_surface_has_both_array_directions() {
        let fns = parse_ffi_dir(Path::new("../../ffi/src")).unwrap();
        let count = |k: Kind| {
            fns.iter()
                .flat_map(|f| f.params.iter())
                .filter(|p| map_type(&p.ty).map(|m| m.kind) == Some(k))
                .count()
        };
        assert_eq!(count(Kind::ByteArrayOut), 17, "*mut [u8; N] out parameters");
        assert!(count(Kind::ByteArrayIn) > 0, "*const [u8; N] in parameters");
    }

    #[test]
    fn unknown_types_return_none_rather_than_guessing() {
        assert!(map_type("SomeStructByValue").is_none());
        assert!(map_type("Option < unsafe extern \"C\" fn () >").is_none());
    }

    #[test]
    fn function_pointer_parameters_are_not_generatable() {
        let f = FfiFn {
            name: "int2dds_participant_qos_get_properties_with_prefix".into(),
            module: "qos".into(),
            params: vec![
                p("qos", "*const Int2DdsParticipantQos"),
                p("prefix", "*const c_char"),
                p("cb", "Option<unsafe extern \"C\" fn (name : *const c_char) -> i32>"),
                p("user_data", "*mut c_void"),
            ],
            ret: "Int2DdsRet".into(),
        };
        assert!(!is_generatable(&f));
    }

    #[test]
    fn unit_return_is_generatable() {
        let f = FfiFn {
            name: "int2dds_condition_seq_delete".into(),
            module: "condition".into(),
            params: vec![p("seq", "*mut Int2DdsConditionSeq")],
            ret: "()".into(),
        };
        assert!(is_generatable(&f));
    }

    #[test]
    fn exactly_one_real_function_is_not_generatable() {
        let fns = parse_ffi_dir(Path::new("../../ffi/src")).unwrap();
        let skipped: Vec<&str> =
            fns.iter().filter(|f| !is_generatable(f)).map(|f| f.name.as_str()).collect();
        assert_eq!(
            skipped,
            vec!["int2dds_participant_qos_get_properties_with_prefix"],
            "the set of hand-written functions changed"
        );
        assert_eq!(fns.iter().filter(|f| is_generatable(f)).count(), 443);
    }

    #[test]
    fn every_type_in_the_real_ffi_surface_is_accounted_for() {
        // Totality check: the only type in the whole 444-function surface that
        // map_type refuses is the C callback. If a future FFI change introduces
        // another unmapped type this names it, rather than silently shrinking
        // the generated binding.
        let fns = parse_ffi_dir(Path::new("../../ffi/src")).unwrap();
        let mut unmapped: Vec<String> = fns
            .iter()
            .flat_map(|f| {
                let ret = std::iter::once(f.ret.clone()).filter(|r| r != "()");
                f.params.iter().map(|p| p.ty.clone()).chain(ret)
            })
            .filter(|t| map_type(t).is_none())
            .collect();
        unmapped.sort();
        unmapped.dedup();
        assert_eq!(unmapped.len(), 1, "unmapped types: {unmapped:?}");
        assert!(unmapped[0].starts_with("Option<"), "{unmapped:?}");
    }
}
