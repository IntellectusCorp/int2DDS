//! Helpers referenced by the generated forwarders in `generated.rs`.
//!
//! Four shapes of byte-oriented parameter cross the boundary, and each needs
//! different handling. Getting any of them wrong is silent rather than loud —
//! the code still compiles and runs, it just loses data — so they are separated
//! deliberately instead of funnelled through one helper:
//!
//! | FFI parameter        | Java      | Handling                                |
//! |----------------------|-----------|-----------------------------------------|
//! | `*const c_char`      | `byte[]`  | copy in, append NUL, null stays null     |
//! | `*const *const c_char` | `byte[][]` | copy each in, build a pointer vector  |
//! | `*const [u8; N]`     | `byte[]`  | copy into a fixed array, reject bad size |
//! | `*mut [u8; N]`       | `byte[]`  | fixed scratch, **copy back after the call** |
//!
//! The last row is the one that cannot be folded into the others: the FFI
//! writes the instance handle or GUID into the caller's buffer, so a forwarder
//! that only copies Java → native returns nothing to Java at all.

use jni::objects::{JByteArray, JObjectArray};
use jni::JNIEnv;
use std::os::raw::c_char;

/// Copy a Java `byte[]` into a NUL-terminated owned buffer.
///
/// Returns `None` for a null array. The caller passes a null pointer in that
/// case: in this FFI a null `const char*` means "absent", which an empty string
/// does not — `int2dds_create_participant_with_profile(.., NULL, ..)` and
/// `(.., "", ..)` do different things.
pub fn take_bytes(env: &mut JNIEnv, arr: &JByteArray) -> Option<Vec<u8>> {
    if arr.is_null() {
        return None;
    }
    let mut v = env.convert_byte_array(arr).ok()?;
    v.push(0);
    Some(v)
}

/// The address of a buffer produced by [`take_bytes`], or null when absent.
pub fn ptr_or_null(buf: &Option<Vec<u8>>) -> *const u8 {
    match buf {
        Some(v) => v.as_ptr(),
        None => std::ptr::null(),
    }
}

/// Copy a Java `byte[]` into a fixed-size array such as a 16-byte GUID.
///
/// Returns `None` for a null array or one shorter than `N`, so the forwarder
/// passes null and the FFI reports an invalid argument, rather than reading off
/// the end of a short array.
pub fn take_fixed<const N: usize>(env: &mut JNIEnv, arr: &JByteArray) -> Option<[u8; N]> {
    if arr.is_null() {
        return None;
    }
    let v = env.convert_byte_array(arr).ok()?;
    if v.len() < N {
        return None;
    }
    let mut out = [0u8; N];
    out.copy_from_slice(&v[..N]);
    Some(out)
}

/// The address of a fixed array produced by [`take_fixed`], or null when absent.
pub fn fixed_ptr<const N: usize>(buf: &Option<[u8; N]>) -> *const [u8; N] {
    match buf {
        Some(a) => a as *const [u8; N],
        None => std::ptr::null(),
    }
}

/// The address of a scratch array the FFI is about to write into.
pub fn fixed_ptr_mut<const N: usize>(buf: &mut [u8; N]) -> *mut [u8; N] {
    buf as *mut [u8; N]
}

/// Copy an out-parameter scratch buffer back into the caller's Java array.
///
/// Without this the FFI's write lands in a Rust-owned buffer and is dropped;
/// the Java caller sees its array unchanged. A null array is ignored, matching
/// the FFI convention that a null out pointer means "not interested".
pub fn write_back(env: &mut JNIEnv, arr: &JByteArray, src: &[u8]) {
    if arr.is_null() {
        return;
    }
    // `jbyte` is `i8`; the reinterpretation is layout-identical.
    let signed = unsafe { std::slice::from_raw_parts(src.as_ptr() as *const i8, src.len()) };
    let _ = env.set_byte_array_region(arr, 0, signed);
}

/// A Java `byte[][]` marshalled into a C `const char *const *`.
///
/// Holds the NUL-terminated copies alive alongside the pointer vector that
/// refers to them, so the FFI call cannot outlive its own arguments.
pub struct CStrArray {
    _owned: Vec<Vec<u8>>,
    ptrs: Vec<*const c_char>,
}

impl CStrArray {
    /// The `const char *const *` the FFI expects, or null when absent.
    pub fn as_ptr(&self) -> *const *const c_char {
        if self.ptrs.is_empty() {
            std::ptr::null()
        } else {
            self.ptrs.as_ptr()
        }
    }

    /// Element count, for the paired length parameter.
    pub fn len(&self) -> usize {
        self.ptrs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ptrs.is_empty()
    }
}

/// Copy a Java `byte[][]` into NUL-terminated C strings plus a pointer vector.
/// A null array, or an element that cannot be read, yields an empty result
/// whose `as_ptr()` is null.
pub fn take_string_array(env: &mut JNIEnv, arr: &JObjectArray) -> CStrArray {
    let empty = CStrArray { _owned: Vec::new(), ptrs: Vec::new() };
    if arr.is_null() {
        return empty;
    }
    let len = match env.get_array_length(arr) {
        Ok(n) => n,
        Err(_) => return empty,
    };
    let mut owned: Vec<Vec<u8>> = Vec::with_capacity(len as usize);
    for i in 0..len {
        let Ok(obj) = env.get_object_array_element(arr, i) else {
            return empty;
        };
        let bytes = JByteArray::from(obj);
        match take_bytes(env, &bytes) {
            Some(v) => owned.push(v),
            None => owned.push(vec![0]),
        }
    }
    let ptrs = owned.iter().map(|v| v.as_ptr() as *const c_char).collect();
    CStrArray { _owned: owned, ptrs }
}
