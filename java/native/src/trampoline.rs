//! Bridges DDS listener callbacks — fired on native DDS background threads —
//! into a Java `DataReaderListener`.
//!
//! DDS invokes C function pointers with a `user_context` pointer. Java cannot
//! supply C function pointers, so each callback gets a hand-written Rust
//! trampoline that attaches the calling thread to the JVM and invokes the Java
//! listener. The JVM is cached in `crate::jvm()` at `JNI_OnLoad` (see `lib.rs`);
//! trampolines run on threads with no `JNIEnv`, so they attach on demand.
//!
//! # Teardown safety
//! The core does not drain callbacks on `set_listener(None)`: it swaps the
//! listener slot under a lock, clones the `Arc`, then releases the lock before
//! invoking, so a callback can still be mid-flight after `set_listener` returns.
//! There is no destructor hook for `user_context`. So the binding owns the
//! lifetime with its own `Arc<ListenerCtx>`: `nativeReaderListenerSet` stores
//! one strong ref as `user_context`; each trampoline reconstructs a *temporary*
//! clone without consuming the stored ref; `nativeReaderListenerClear` drops the
//! one stored ref. The `GlobalRef` frees only when the last in-flight callback
//! finishes. See `.superpowers/sdd/listener-lifecycle-investigation.md`.

use std::os::raw::c_void;
use std::sync::Arc;

use jni::objects::{GlobalRef, JClass, JObject, JValue};
use jni::sys::{jint, jlong};
use jni::JNIEnv;

use int2dds_ffi::listener::Int2DdsDataReaderListener;
use int2dds_ffi::status::Int2DdsSubscriptionMatchedStatus;
use int2dds_ffi::subscriber::int2dds_datareader_set_listener;
use int2dds_ffi::types::Int2DdsDataReader;

/// Binding-owned callback context. `Send + Sync` because `GlobalRef` is.
struct ListenerCtx {
    listener: GlobalRef,
}

/// Attaches the current (DDS background) thread to the JVM as a daemon and
/// returns its env. Daemon attach stays for the thread's life and detaches
/// automatically at thread exit — DDS reuses its threads, so this amortizes.
fn env_for_callback() -> Option<JNIEnv<'static>> {
    crate::jvm()?.attach_current_thread_as_daemon().ok()
}

/// Trampoline for `on_subscription_matched`. Every later reader callback follows
/// this exact shape.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` must be the pointer
/// `nativeReaderListenerSet` stored (an `Arc<ListenerCtx>` raw ref); `status`
/// must point at a valid `Int2DdsSubscriptionMatchedStatus` for the call.
unsafe extern "C" fn tramp_on_subscription_matched(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSubscriptionMatchedStatus,
    user_context: *mut c_void,
) {
    if user_context.is_null() || status.is_null() {
        return;
    }
    // Temporary clone: bump the count, then reconstruct — this reborrows the
    // stored ref rather than consuming it. Dropped at scope end (one decrement).
    Arc::increment_strong_count(user_context as *const ListenerCtx);
    let ctx = Arc::from_raw(user_context as *const ListenerCtx);

    if let Some(mut env) = env_for_callback() {
        let s = &*status;
        if let Ok(handle) = env.byte_array_from_slice(&s.last_publication_handle) {
            let cls = "com/intellectus/int2dds/status/SubscriptionMatchedStatus";
            if let Ok(jstatus) = env.new_object(
                cls,
                "(IIII[B)V",
                &[
                    JValue::Int(s.total_count),
                    JValue::Int(s.total_count_change),
                    JValue::Int(s.current_count),
                    JValue::Int(s.current_count_change),
                    JValue::Object(&handle),
                ],
            ) {
                // v1 passes the reader as Java null: native->entity reverse
                // mapping is out of scope for this branch.
                let _ = env.call_method(
                    ctx.listener.as_obj(),
                    "onSubscriptionMatched",
                    "(Lcom/intellectus/int2dds/core/DataReader;\
                     Lcom/intellectus/int2dds/status/SubscriptionMatchedStatus;)V",
                    &[JValue::Object(&JObject::null()), JValue::Object(&jstatus)],
                );
            }
        }
        // A Java exception must never leak back onto the native DDS thread.
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
    }

    drop(ctx);
}

/// Installs a Java listener on `reader` for `mask`. Returns the stored context
/// pointer (pass it back to clear), or 0 on failure.
///
/// # Safety
/// Invoked by the JVM under JNI conventions. `reader` must be a live
/// `Int2DdsDataReader` handle.
#[no_mangle]
pub extern "system" fn Java_com_intellectus_int2dds_internal_ffi_FfiHandwritten_nativeReaderListenerSet<
    'local,
>(
    env: JNIEnv<'local>,
    _class: JClass<'local>,
    reader: jlong,
    listener: JObject<'local>,
    mask: jint,
) -> jlong {
    let global = match env.new_global_ref(listener) {
        Ok(g) => g,
        Err(_) => return 0,
    };
    let ctx = Arc::new(ListenerCtx { listener: global });
    // The one stored strong ref. Rides in the C struct's user_context.
    let uc = Arc::into_raw(ctx) as *mut c_void;

    let c_listener = Int2DdsDataReaderListener {
        on_data_available: None,
        on_subscription_matched: Some(tramp_on_subscription_matched),
        on_sample_rejected: None,
        on_liveliness_changed: None,
        on_requested_deadline_missed: None,
        on_requested_incompatible_qos: None,
        on_sample_lost: None,
        user_context: uc,
    };

    let rc = unsafe {
        int2dds_datareader_set_listener(
            reader as usize as *mut Int2DdsDataReader,
            &c_listener as *const _,
            mask as u32,
        )
    };
    if rc != 0 {
        // Registration failed: reclaim the stored ref so it does not leak.
        unsafe { drop(Arc::from_raw(uc as *const ListenerCtx)) };
        return 0;
    }
    uc as jlong
}

/// Clears the listener on `reader` and releases the stored context `ctx`.
/// In-flight callbacks hold their own clone, so the `GlobalRef` frees only when
/// the last one finishes.
///
/// # Safety
/// Invoked by the JVM under JNI conventions. `reader` must be a live handle;
/// `ctx` must be a value returned by `nativeReaderListenerSet` and not yet
/// cleared.
#[no_mangle]
pub extern "system" fn Java_com_intellectus_int2dds_internal_ffi_FfiHandwritten_nativeReaderListenerClear<
    'local,
>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    reader: jlong,
    ctx: jlong,
) -> jint {
    let rc = unsafe {
        int2dds_datareader_set_listener(
            reader as usize as *mut Int2DdsDataReader,
            std::ptr::null(),
            0,
        )
    };
    if ctx != 0 {
        // Drop exactly the one stored strong ref.
        unsafe { drop(Arc::from_raw(ctx as usize as *const ListenerCtx)) };
    }
    rc
}
