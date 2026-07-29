//! JNI bindings for int2DDS.
//!
//! This crate is an ABI adapter, not a reimplementation. It depends on
//! `int2dds-ffi` as an rlib and forwards to its C-ABI functions as ordinary
//! Rust calls, so the FFI layer and the DDS core link statically into a single
//! `int2dds_java` shared library.

use jni::sys::jint;
use jni::JavaVM;
use std::sync::OnceLock;

pub mod buffer;
pub mod gen;
pub mod generated_support;
pub mod strings;

/// The product version, inherited from the workspace `Cargo.toml`.
/// `NativeLoader` on the Java side compares this against the JAR version.
pub fn crate_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

static JVM: OnceLock<JavaVM> = OnceLock::new();

/// The cached `JavaVM`, available once the library has been loaded by a JVM.
/// The listener bridge uses this to attach DDS callback threads.
pub fn jvm() -> Option<&'static JavaVM> {
    JVM.get()
}

/// Called by the JVM when `System.load` / `System.loadLibrary` succeeds.
///
/// # Safety
/// Invoked by the JVM under JNI conventions.
#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _reserved: *mut std::ffi::c_void) -> jint {
    let _ = JVM.set(vm);
    jni::sys::JNI_VERSION_1_8
}
