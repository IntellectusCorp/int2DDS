//! Bridges DDS listener callbacks — fired on native DDS background threads —
//! into a Java `DataReaderListener` or `DataWriterListener`.
//!
//! DDS invokes C function pointers with a `user_context` pointer. Java cannot
//! supply C function pointers, so each callback gets a hand-written Rust
//! trampoline that attaches the calling thread to the JVM and invokes the Java
//! listener. The JVM is cached in `crate::jvm()` at `JNI_OnLoad` (see `lib.rs`);
//! trampolines run on threads with no `JNIEnv`, so they attach on demand.
//!
//! The entity argument of every callback is passed as null: `DataReader` and
//! `DataWriter` install a wrapper listener that substitutes the Java entity.
//!
//! # Teardown safety — id registry, not a raw Arc pointer
//! The core clones its `Arc<FfiDataReaderListener>` and RELEASES its lock
//! *before* invoking the trampoline, and holds `user_context` only as an opaque
//! value. So a raw `Arc<ListenerCtx>` pointer in `user_context` cannot be made
//! safe: a clear on another thread could free the `Arc` between the core's clone
//! and the trampoline's entry, and the trampoline would then dereference freed
//! memory. Instead `user_context` carries an integer *id* into a global
//! `REGISTRY` that owns the `Arc<ListenerCtx>`. The trampoline clones the `Arc`
//! out of the map *while holding the registry lock*; a clear removes the id
//! under the same lock. Either the trampoline gets a live clone that outlives
//! the callback, or the id is already gone and it returns without calling —
//! never a use-after-free.
//!
//! # Shared scaffold
//! Every trampoline funnels through [`run_trampoline`]: registry lookup with
//! clone-under-lock, a `catch_unwind` so a Java-triggered or marshaling panic
//! can never unwind across the `unsafe extern "C"` boundary (UB), JVM attach,
//! and a bounded local frame with post-call exception draining. Each trampoline
//! supplies only its status marshaling as a closure.

use std::collections::HashMap;
use std::os::raw::c_void;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use jni::errors::Error as JniError;
use jni::objects::{GlobalRef, JClass, JMethodID, JObject};
use jni::signature::{Primitive, ReturnType};
use jni::sys::{jint, jlong, jvalue};
use jni::JNIEnv;

use int2dds_ffi::listener::{Int2DdsDataReaderListener, Int2DdsDataWriterListener};
use int2dds_ffi::publisher::int2dds_datawriter_set_listener;
use int2dds_ffi::status::{
    Int2DdsLivelinessChangedStatus, Int2DdsLivelinessLostStatus,
    Int2DdsOfferedDeadlineMissedStatus, Int2DdsOfferedIncompatibleQosStatus,
    Int2DdsPublicationMatchedStatus, Int2DdsRequestedDeadlineMissedStatus,
    Int2DdsRequestedIncompatibleQosStatus, Int2DdsSampleLostStatus, Int2DdsSampleRejectedStatus,
    Int2DdsSubscriptionMatchedStatus,
};
use int2dds_ffi::subscriber::int2dds_datareader_set_listener;
use int2dds_ffi::types::{Int2DdsDataReader, Int2DdsDataWriter};

/// Binding-owned callback context. `Send + Sync` because `GlobalRef` is.
struct ListenerCtx {
    listener: GlobalRef,
}

/// Owns every live listener context, keyed by the id carried in `user_context`.
/// The lock mediates lifetime: a trampoline clones its `Arc` out under the lock,
/// a clear removes the id under the same lock, so a callback never observes a
/// half-freed context.
static REGISTRY: LazyLock<Mutex<HashMap<u64, Arc<ListenerCtx>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Next id to hand out. Starts at 1 so 0 can mean "no listener" on the Java side.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

const READER_LISTENER: &str = "kr/co/intellectus/int2dds/listeners/DataReaderListener";
const WRITER_LISTENER: &str = "kr/co/intellectus/int2dds/listeners/DataWriterListener";
const READER: &str = "Lkr/co/intellectus/int2dds/core/DataReader;";
const WRITER: &str = "Lkr/co/intellectus/int2dds/core/DataWriter;";
const STATUS_PACKAGE: &str = "kr/co/intellectus/int2dds/status/";

/// A status class and its constructor.
struct StatusCtor {
    class: GlobalRef,
    ctor: JMethodID,
}

impl StatusCtor {
    fn resolve(env: &mut JNIEnv, name: &str, sig: &str) -> Result<Self, JniError> {
        let local = env.find_class(format!("{STATUS_PACKAGE}{name}"))?;
        let ctor = env.get_method_id(&local, "<init>", sig)?;
        let class = env.new_global_ref(&local)?;
        env.delete_local_ref(local)?;
        Ok(Self { class, ctor })
    }

    /// # Safety
    /// `args` must match the constructor signature given to [`Self::resolve`].
    unsafe fn construct<'local>(
        &self,
        env: &mut JNIEnv<'local>,
        args: &[jvalue],
    ) -> Result<JObject<'local>, JniError> {
        env.new_object_unchecked(&self.class, self.ctor, args)
    }
}

/// Classes and method IDs the trampolines call through, resolved once.
///
/// A `FindClass` on a natively attached DDS thread searches the system class
/// loader, which cannot see these classes when the binding is loaded by an
/// application class loader (a fat JAR, a servlet container). The listener set
/// calls run on a Java thread inside `FfiHandwritten`, whose loader can, so they
/// resolve everything here before the first callback.
struct JniCache {
    subscription_matched: StatusCtor,
    sample_rejected: StatusCtor,
    liveliness_changed: StatusCtor,
    requested_deadline_missed: StatusCtor,
    requested_incompatible_qos: StatusCtor,
    sample_lost: StatusCtor,
    publication_matched: StatusCtor,
    offered_deadline_missed: StatusCtor,
    offered_incompatible_qos: StatusCtor,
    liveliness_lost: StatusCtor,
    on_subscription_matched: JMethodID,
    on_data_available: JMethodID,
    on_sample_rejected: JMethodID,
    on_liveliness_changed: JMethodID,
    on_requested_deadline_missed: JMethodID,
    on_requested_incompatible_qos: JMethodID,
    on_sample_lost: JMethodID,
    on_publication_matched: JMethodID,
    on_offered_deadline_missed: JMethodID,
    on_offered_incompatible_qos: JMethodID,
    on_liveliness_lost: JMethodID,
}

static CACHE: OnceLock<JniCache> = OnceLock::new();

/// A `(entity, status)` callback on `iface`.
fn callback(
    env: &mut JNIEnv,
    iface: &str,
    entity: &str,
    name: &str,
    status: &str,
) -> Result<JMethodID, JniError> {
    env.get_method_id(iface, name, format!("({entity}L{STATUS_PACKAGE}{status};)V"))
}

impl JniCache {
    fn resolve(env: &mut JNIEnv) -> Result<Self, JniError> {
        let (r, w) = (READER_LISTENER, WRITER_LISTENER);
        Ok(Self {
            subscription_matched: StatusCtor::resolve(
                env,
                "SubscriptionMatchedStatus",
                "(IIII[B)V",
            )?,
            sample_rejected: StatusCtor::resolve(env, "SampleRejectedStatus", "(III[B)V")?,
            liveliness_changed: StatusCtor::resolve(env, "LivelinessChangedStatus", "(IIII[B)V")?,
            requested_deadline_missed: StatusCtor::resolve(
                env,
                "RequestedDeadlineMissedStatus",
                "(II[B)V",
            )?,
            requested_incompatible_qos: StatusCtor::resolve(
                env,
                "RequestedIncompatibleQosStatus",
                "(IIII)V",
            )?,
            sample_lost: StatusCtor::resolve(env, "SampleLostStatus", "(II)V")?,
            publication_matched: StatusCtor::resolve(env, "PublicationMatchedStatus", "(IIII[B)V")?,
            offered_deadline_missed: StatusCtor::resolve(
                env,
                "OfferedDeadlineMissedStatus",
                "(II[B)V",
            )?,
            offered_incompatible_qos: StatusCtor::resolve(
                env,
                "OfferedIncompatibleQosStatus",
                "(IIII)V",
            )?,
            liveliness_lost: StatusCtor::resolve(env, "LivelinessLostStatus", "(II)V")?,
            on_subscription_matched: callback(
                env,
                r,
                READER,
                "onSubscriptionMatched",
                "SubscriptionMatchedStatus",
            )?,
            on_data_available: env.get_method_id(r, "onDataAvailable", format!("({READER})V"))?,
            on_sample_rejected: callback(
                env,
                r,
                READER,
                "onSampleRejected",
                "SampleRejectedStatus",
            )?,
            on_liveliness_changed: callback(
                env,
                r,
                READER,
                "onLivelinessChanged",
                "LivelinessChangedStatus",
            )?,
            on_requested_deadline_missed: callback(
                env,
                r,
                READER,
                "onRequestedDeadlineMissed",
                "RequestedDeadlineMissedStatus",
            )?,
            on_requested_incompatible_qos: callback(
                env,
                r,
                READER,
                "onRequestedIncompatibleQos",
                "RequestedIncompatibleQosStatus",
            )?,
            on_sample_lost: callback(env, r, READER, "onSampleLost", "SampleLostStatus")?,
            on_publication_matched: callback(
                env,
                w,
                WRITER,
                "onPublicationMatched",
                "PublicationMatchedStatus",
            )?,
            on_offered_deadline_missed: callback(
                env,
                w,
                WRITER,
                "onOfferedDeadlineMissed",
                "OfferedDeadlineMissedStatus",
            )?,
            on_offered_incompatible_qos: callback(
                env,
                w,
                WRITER,
                "onOfferedIncompatibleQos",
                "OfferedIncompatibleQosStatus",
            )?,
            on_liveliness_lost: callback(
                env,
                w,
                WRITER,
                "onLivelinessLost",
                "LivelinessLostStatus",
            )?,
        })
    }
}

/// Resolves [`CACHE`] on first use. Must run on a Java thread; see [`JniCache`].
fn ensure_cache(env: &mut JNIEnv) -> Result<&'static JniCache, JniError> {
    if let Some(cache) = CACHE.get() {
        return Ok(cache);
    }
    let resolved = JniCache::resolve(env)?;
    Ok(CACHE.get_or_init(|| resolved))
}

fn int(v: i32) -> jvalue {
    jvalue { i: v }
}

fn obj(o: &JObject) -> jvalue {
    jvalue { l: o.as_raw() }
}

const NULL: jvalue = jvalue { l: std::ptr::null_mut() };

/// Calls a `void` listener method.
///
/// # Safety
/// `method` must come from [`JniCache`] and `args` must match its signature.
unsafe fn call_void(
    env: &mut JNIEnv,
    listener: &JObject,
    method: JMethodID,
    args: &[jvalue],
) -> Result<(), JniError> {
    env.call_method_unchecked(listener, method, ReturnType::Primitive(Primitive::Void), args)?;
    Ok(())
}

/// Attaches the current (DDS background) thread to the JVM as a daemon and
/// returns its env. Daemon attach stays for the thread's life and detaches
/// automatically at thread exit — DDS reuses its threads, so this amortizes.
fn env_for_callback() -> Option<JNIEnv<'static>> {
    crate::jvm()?.attach_current_thread_as_daemon().ok()
}

/// Shared trampoline scaffold. Looks up the listener context for `user_context`
/// (cloning the `Arc` out under the registry lock; a missing id is a silent
/// no-op), then — inside a `catch_unwind` so nothing can unwind across the
/// `extern "C"` boundary — attaches the calling thread to the JVM and runs
/// `marshal` inside a bounded local frame. Any pending Java exception left by
/// `marshal` is described and cleared before returning; a Rust panic from
/// `marshal` is logged and swallowed the same way.
///
/// `marshal` builds the Java status object (if any) and calls the listener
/// method; it receives the attached env, the resolved cache and the listener.
fn run_trampoline<F>(user_context: *mut c_void, marshal: F)
where
    F: FnOnce(&mut JNIEnv<'_>, &JniCache, &JObject<'static>) -> Result<(), JniError>,
{
    let id = user_context as usize as u64;
    // Clone the Arc out while holding the lock: this keeps the context (and its
    // GlobalRef) alive for the whole callback even if a concurrent clear removes
    // the id right after. If the id is already gone, there is no listener to
    // call, so return.
    let ctx = {
        let guard = REGISTRY.lock().unwrap_or_else(|e| e.into_inner());
        match guard.get(&id) {
            Some(c) => Arc::clone(c),
            None => return,
        }
    };
    // Resolved by the set call that registered `ctx`.
    let Some(cache) = CACHE.get() else {
        return;
    };

    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        if let Some(mut env) = env_for_callback() {
            // Bound the JNI locals this callback allocates: the DDS thread is
            // daemon-attached, not entered through a native-method stub, so
            // nothing else pushes/pops a frame — without this the status
            // object (and any byte arrays it needs) leak one local ref each
            // per callback until the table overflows.
            let _ = env.with_local_frame(8, |env| marshal(env, cache, ctx.listener.as_obj()));
            // A pending Java exception is thread-wide and survives the frame
            // pop, so it is still checked here. It must never leak onto the
            // native DDS thread.
            if env.exception_check().unwrap_or(false) {
                let _ = env.exception_describe();
                let _ = env.exception_clear();
            }
        }
    }));
    if outcome.is_err() {
        // A panic here would be UB unwinding across the extern "C" boundary.
        // Log and swallow: the DDS thread must keep running regardless.
        eprintln!("int2dds: listener trampoline panicked (id={id}), swallowed at FFI boundary");
    }
}

/// Trampoline for `on_subscription_matched`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored; `status` must point at a valid
/// `Int2DdsSubscriptionMatchedStatus` for the call.
unsafe extern "C" fn tramp_on_subscription_matched(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSubscriptionMatchedStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let handle = env.byte_array_from_slice(&s.last_publication_handle)?;
        let jstatus = cache.subscription_matched.construct(
            env,
            &[
                int(s.total_count),
                int(s.total_count_change),
                int(s.current_count),
                int(s.current_count_change),
                obj(&handle),
            ],
        )?;
        call_void(env, listener, cache.on_subscription_matched, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_data_available`. Unlike the other callbacks this one
/// carries no status argument.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored.
unsafe extern "C" fn tramp_on_data_available(
    _reader: *mut Int2DdsDataReader,
    user_context: *mut c_void,
) {
    run_trampoline(user_context, |env, cache, listener| unsafe {
        call_void(env, listener, cache.on_data_available, &[NULL])
    });
}

/// Trampoline for `on_sample_rejected`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored; `status` must point at a valid
/// `Int2DdsSampleRejectedStatus` for the call.
unsafe extern "C" fn tramp_on_sample_rejected(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSampleRejectedStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let handle = env.byte_array_from_slice(&s.last_instance_handle)?;
        let jstatus = cache.sample_rejected.construct(
            env,
            &[
                int(s.total_count),
                int(s.total_count_change),
                int(s.last_reason as i32),
                obj(&handle),
            ],
        )?;
        call_void(env, listener, cache.on_sample_rejected, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_liveliness_changed`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored; `status` must point at a valid
/// `Int2DdsLivelinessChangedStatus` for the call.
unsafe extern "C" fn tramp_on_liveliness_changed(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsLivelinessChangedStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let handle = env.byte_array_from_slice(&s.last_publication_handle)?;
        let jstatus = cache.liveliness_changed.construct(
            env,
            &[
                int(s.alive_count),
                int(s.not_alive_count),
                int(s.alive_count_change),
                int(s.not_alive_count_change),
                obj(&handle),
            ],
        )?;
        call_void(env, listener, cache.on_liveliness_changed, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_requested_deadline_missed`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored; `status` must point at a valid
/// `Int2DdsRequestedDeadlineMissedStatus` for the call.
unsafe extern "C" fn tramp_on_requested_deadline_missed(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsRequestedDeadlineMissedStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let handle = env.byte_array_from_slice(&s.last_instance_handle)?;
        let jstatus = cache
            .requested_deadline_missed
            .construct(env, &[int(s.total_count), int(s.total_count_change), obj(&handle)])?;
        call_void(env, listener, cache.on_requested_deadline_missed, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_requested_incompatible_qos`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored; `status` must point at a valid
/// `Int2DdsRequestedIncompatibleQosStatus` for the call.
unsafe extern "C" fn tramp_on_requested_incompatible_qos(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsRequestedIncompatibleQosStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let jstatus = cache.requested_incompatible_qos.construct(
            env,
            &[
                int(s.total_count),
                int(s.total_count_change),
                int(s.last_policy_id as i32),
                int(s.policies_count as i32),
            ],
        )?;
        call_void(env, listener, cache.on_requested_incompatible_qos, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_sample_lost`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeReaderListenerSet` stored; `status` must point at a valid
/// `Int2DdsSampleLostStatus` for the call.
unsafe extern "C" fn tramp_on_sample_lost(
    _reader: *mut Int2DdsDataReader,
    status: *const Int2DdsSampleLostStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let jstatus =
            cache.sample_lost.construct(env, &[int(s.total_count), int(s.total_count_change)])?;
        call_void(env, listener, cache.on_sample_lost, &[NULL, obj(&jstatus)])
    });
}

/// Installs a Java listener on `reader` for the given status `mask`. Returns the
/// registry id (pass it back to clear), or 0 on failure.
///
/// # Safety
/// Invoked by the JVM under JNI conventions. `reader` must be a live
/// `Int2DdsDataReader` handle.
#[no_mangle]
pub extern "system" fn Java_kr_co_intellectus_int2dds_internal_ffi_FfiHandwritten_nativeReaderListenerSet<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    reader: jlong,
    listener: JObject<'local>,
    mask: jint,
) -> jlong {
    // On failure the pending NoClassDefFoundError/NoSuchMethodError is thrown
    // to the caller.
    if ensure_cache(&mut env).is_err() {
        return 0;
    }
    let global = match env.new_global_ref(listener) {
        Ok(g) => g,
        Err(_) => return 0,
    };
    let ctx = Arc::new(ListenerCtx { listener: global });
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).insert(id, ctx);

    // Every trampoline whose Java method exists is always installed; the
    // caller's mask decides which ones DDS actually fires, and callers that
    // did not override a method get DataReaderListenerBase's no-op.
    let c_listener = Int2DdsDataReaderListener {
        on_data_available: Some(tramp_on_data_available),
        on_subscription_matched: Some(tramp_on_subscription_matched),
        on_sample_rejected: Some(tramp_on_sample_rejected),
        on_liveliness_changed: Some(tramp_on_liveliness_changed),
        on_requested_deadline_missed: Some(tramp_on_requested_deadline_missed),
        on_requested_incompatible_qos: Some(tramp_on_requested_incompatible_qos),
        on_sample_lost: Some(tramp_on_sample_lost),
        // The id, not a pointer: the core never dereferences user_context.
        user_context: id as usize as *mut c_void,
    };

    let rc = unsafe {
        int2dds_datareader_set_listener(
            reader as usize as *mut Int2DdsDataReader,
            &c_listener as *const _,
            mask as u32,
        )
    };
    if rc != 0 {
        // Registration failed: forget the context so it does not leak.
        REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        return 0;
    }
    id as jlong
}

/// Clears the listener on `reader` and forgets the registry entry `id`. A
/// concurrent in-flight callback already cloned its `Arc`, so the `GlobalRef`
/// frees only when that last clone drops. Removing an absent id is a harmless
/// no-op, so a double clear is safe.
///
/// # Safety
/// Invoked by the JVM under JNI conventions. `reader` must be a live handle;
/// `id` must be a value returned by `nativeReaderListenerSet`.
#[no_mangle]
pub extern "system" fn Java_kr_co_intellectus_int2dds_internal_ffi_FfiHandwritten_nativeReaderListenerClear<
    'local,
>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    reader: jlong,
    id: jlong,
) -> jint {
    let rc = unsafe {
        int2dds_datareader_set_listener(
            reader as usize as *mut Int2DdsDataReader,
            std::ptr::null(),
            0,
        )
    };
    if id != 0 {
        REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).remove(&(id as u64));
    }
    rc
}

/// Trampoline for `on_publication_matched`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeWriterListenerSet` stored; `status` must point at a valid
/// `Int2DdsPublicationMatchedStatus` for the call.
unsafe extern "C" fn tramp_on_publication_matched(
    _writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsPublicationMatchedStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let handle = env.byte_array_from_slice(&s.last_subscription_handle)?;
        let jstatus = cache.publication_matched.construct(
            env,
            &[
                int(s.total_count),
                int(s.total_count_change),
                int(s.current_count),
                int(s.current_count_change),
                obj(&handle),
            ],
        )?;
        call_void(env, listener, cache.on_publication_matched, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_offered_deadline_missed`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeWriterListenerSet` stored; `status` must point at a valid
/// `Int2DdsOfferedDeadlineMissedStatus` for the call.
unsafe extern "C" fn tramp_on_offered_deadline_missed(
    _writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsOfferedDeadlineMissedStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let handle = env.byte_array_from_slice(&s.last_instance_handle)?;
        let jstatus = cache
            .offered_deadline_missed
            .construct(env, &[int(s.total_count), int(s.total_count_change), obj(&handle)])?;
        call_void(env, listener, cache.on_offered_deadline_missed, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_offered_incompatible_qos`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeWriterListenerSet` stored; `status` must point at a valid
/// `Int2DdsOfferedIncompatibleQosStatus` for the call.
unsafe extern "C" fn tramp_on_offered_incompatible_qos(
    _writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsOfferedIncompatibleQosStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let jstatus = cache.offered_incompatible_qos.construct(
            env,
            &[
                int(s.total_count),
                int(s.total_count_change),
                int(s.last_policy_id as i32),
                int(s.policies_count as i32),
            ],
        )?;
        call_void(env, listener, cache.on_offered_incompatible_qos, &[NULL, obj(&jstatus)])
    });
}

/// Trampoline for `on_liveliness_lost`.
///
/// # Safety
/// Invoked by the DDS core under the C ABI. `user_context` is the registry id
/// `nativeWriterListenerSet` stored; `status` must point at a valid
/// `Int2DdsLivelinessLostStatus` for the call.
unsafe extern "C" fn tramp_on_liveliness_lost(
    _writer: *mut Int2DdsDataWriter,
    status: *const Int2DdsLivelinessLostStatus,
    user_context: *mut c_void,
) {
    if status.is_null() {
        return;
    }
    let s = &*status;
    run_trampoline(user_context, |env, cache, listener| unsafe {
        let jstatus = cache
            .liveliness_lost
            .construct(env, &[int(s.total_count), int(s.total_count_change)])?;
        call_void(env, listener, cache.on_liveliness_lost, &[NULL, obj(&jstatus)])
    });
}

/// Installs a Java listener on `writer` for the given status `mask`. Returns the
/// registry id (pass it back to clear), or 0 on failure. Same id space and
/// `ListenerCtx`/`REGISTRY` as the reader side.
///
/// # Safety
/// Invoked by the JVM under JNI conventions. `writer` must be a live
/// `Int2DdsDataWriter` handle.
#[no_mangle]
pub extern "system" fn Java_kr_co_intellectus_int2dds_internal_ffi_FfiHandwritten_nativeWriterListenerSet<
    'local,
>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    writer: jlong,
    listener: JObject<'local>,
    mask: jint,
) -> jlong {
    // On failure the pending NoClassDefFoundError/NoSuchMethodError is thrown
    // to the caller.
    if ensure_cache(&mut env).is_err() {
        return 0;
    }
    let global = match env.new_global_ref(listener) {
        Ok(g) => g,
        Err(_) => return 0,
    };
    let ctx = Arc::new(ListenerCtx { listener: global });
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).insert(id, ctx);

    // Every trampoline whose Java method exists is always installed; the
    // caller's mask decides which ones DDS actually fires, and callers that
    // did not override a method get DataWriterListenerBase's no-op.
    let c_listener = Int2DdsDataWriterListener {
        on_publication_matched: Some(tramp_on_publication_matched),
        on_offered_deadline_missed: Some(tramp_on_offered_deadline_missed),
        on_offered_incompatible_qos: Some(tramp_on_offered_incompatible_qos),
        on_liveliness_lost: Some(tramp_on_liveliness_lost),
        // The id, not a pointer: the core never dereferences user_context.
        user_context: id as usize as *mut c_void,
    };

    let rc = unsafe {
        int2dds_datawriter_set_listener(
            writer as usize as *mut Int2DdsDataWriter,
            &c_listener as *const _,
            mask as u32,
        )
    };
    if rc != 0 {
        // Registration failed: forget the context so it does not leak.
        REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
        return 0;
    }
    id as jlong
}

/// Clears the listener on `writer` and forgets the registry entry `id`. A
/// concurrent in-flight callback already cloned its `Arc`, so the `GlobalRef`
/// frees only when that last clone drops. Removing an absent id is a harmless
/// no-op, so a double clear is safe.
///
/// # Safety
/// Invoked by the JVM under JNI conventions. `writer` must be a live handle;
/// `id` must be a value returned by `nativeWriterListenerSet`.
#[no_mangle]
pub extern "system" fn Java_kr_co_intellectus_int2dds_internal_ffi_FfiHandwritten_nativeWriterListenerClear<
    'local,
>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    writer: jlong,
    id: jlong,
) -> jint {
    let rc = unsafe {
        int2dds_datawriter_set_listener(
            writer as usize as *mut Int2DdsDataWriter,
            std::ptr::null(),
            0,
        )
    };
    if id != 0 {
        REGISTRY.lock().unwrap_or_else(|e| e.into_inner()).remove(&(id as u64));
    }
    rc
}
