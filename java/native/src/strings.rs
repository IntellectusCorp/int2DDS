//! UTF-8 boundary handling.
//!
//! No `String` crosses the JNI boundary. JNI's `NewStringUTF` and
//! `GetStringUTFChars` use *modified* UTF-8, which encodes NUL as two bytes and
//! supplementary-plane characters as surrogate pairs. Topic names, type names
//! and profile paths containing non-ASCII would silently corrupt. Instead all C
//! strings cross as `byte[]` holding standard UTF-8.

use std::ffi::CString;

/// Convert standard UTF-8 bytes (without a trailing NUL) into a `CString`.
/// An interior NUL is an error rather than a silent truncation.
pub fn bytes_to_cstring(bytes: &[u8]) -> Result<CString, std::ffi::NulError> {
    CString::new(bytes)
}

/// Copy as much of `src` into `dst` as fits, returning the *untruncated*
/// length of `src`. Mirrors the FFI convention used by
/// `int2dds_last_error_message`, which returns the full length even when the
/// caller's buffer was too small.
pub fn clamp_copy(dst: &mut [u8], src: &[u8]) -> usize {
    let n = dst.len().min(src.len());
    dst[..n].copy_from_slice(&src[..n]);
    src.len()
}
