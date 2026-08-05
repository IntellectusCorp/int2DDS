package com.intellectus.int2dds.core;

/**
 * The Java 8 stand-in for {@link java.lang.ref.Reference#reachabilityFence},
 * which does not exist before Java 9 and so cannot be called from this
 * module's {@code --release 8} source set.
 *
 * <p>{@link NativeEntity#handle()} returns a bare {@code long}, disconnected
 * from the entity that produced it. Once a caller holds that value, nothing
 * about it keeps the entity — or the underlying native object it names —
 * reachable: if nothing else in the calling code still references the
 * entity, the JIT is free to treat it as unreachable from that point on,
 * including for the duration of a native call still using the raw handle.
 * {@link com.intellectus.int2dds.internal.NativeCleaner}'s reaper can then
 * observe the entity as phantom-reachable and race that in-flight call to
 * delete the very object the call is using — the same class of hazard that
 * makes the reaper safe to begin with (see that class's doc), now applied to
 * a handle a native call is actively using rather than one sitting idle in a
 * field.
 *
 * <p>{@link #keepAlive} closes that window: it forces the object to be
 * considered reachable up to the point it is called, with no other effect on
 * the program — exactly what {@code Reference.reachabilityFence} does on 9+.
 * Task 4's own code has no call site that needs this (its one native create
 * is {@code static} and does not read a handle off a live entity, and
 * {@link NativeEntity#close()} does not either — the {@code NativeHandle}
 * reference it calls through, once loaded, keeps the state it needs
 * reachable independently of {@code this}). Tasks 5-7 will: a
 * {@code Topic}/{@code Publisher}/{@code DataWriter} constructor reads its
 * parent's handle via {@link NativeEntity#handle()} and then makes a native
 * create call with that bare value, with nothing else necessarily keeping
 * the parent reachable for the call's duration.
 *
 * <h2>Why a plain static store works as a fence</h2>
 *
 * <p>The implementation below is a single, deliberately non-volatile, store
 * into a static field. A static field is global, cross-thread-visible state:
 * the JIT cannot prove, in general, that no other thread will ever read it
 * (that would require whole-program closed-world analysis HotSpot does not
 * perform), so the write can never be recognised as dead and eliminated —
 * which is all a fence needs here. This is deliberately not a volatile
 * store: volatility would add a memory barrier that neither correctness nor
 * any fence semantics this class needs actually requires (this is about
 * compiler liveness analysis, not cross-thread visibility of the sink's
 * value, which is never read by anything), and the write path this guards is
 * performance-sensitive.
 *
 * <p>This is a best-effort stand-in, not a formally specified guarantee the
 * way the JDK 9+ intrinsic is — which is exactly why the JDK added a real
 * one rather than leaving this trick as the answer forever. If this module's
 * baseline ever moves past 8, the correct refinement is a multi-release
 * override that delegates to {@code Reference.reachabilityFence} directly on
 * 9+ (see the {@code java22} source set already wired in
 * {@code api/build.gradle.kts} for how this module splits by release when it
 * is warranted); not built now, since standing up that MR-JAR wiring for one
 * call is premature before anything needs it.
 */
final class NativeKeepAlive {

    /** Never read. Only the act of writing into it matters. */
    private static Object sink;

    private NativeKeepAlive() {}

    /**
     * Keeps {@code obj} reachable up to this call. Place immediately after a
     * native call that consumed a raw handle read from {@code obj} via
     * {@link NativeEntity#handle()} — directly, or via a value derived
     * through it, such as a parent's handle read for a child's create call —
     * so neither {@code obj} nor the native object its handle names can be
     * reaped while that call is still in flight.
     */
    static void keepAlive(Object obj) {
        sink = obj;
    }
}
