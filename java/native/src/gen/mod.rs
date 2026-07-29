//! Code generator for the JNI binding layer.
//!
//! Emits `java/native/src/generated.rs` and the Java `Ffi` class from a single
//! parse of `ffi/src/**.rs`, so the two sides cannot drift apart.

pub mod emit_java;
pub mod emit_rust;
pub mod parse;
pub mod typemap;
