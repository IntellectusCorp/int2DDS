package com.intellectus.int2dds.core;

/**
 * The Java 8 stand-in for {@link java.lang.ref.Reference#reachabilityFence},
 * which does not exist before Java 9 and so cannot be called from this
 * module's {@code --release 8} source set. This is the base version of a
 * multi-release class: {@code src/main/java9}'s {@code NativeKeepAlive}
 * overrides it on a 9+ runtime with the real intrinsic — see that class's
 * Javadoc — so this fallback's behavior below only matters on 8 itself.
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
 * considered reachable up to the point it is called, with no other
 * <em>required</em> effect on the program — exactly what {@code
 * Reference.reachabilityFence} does on 9+. {@code Topic}/{@code
 * Publisher}/{@code DataWriter} constructors are exactly the call sites that
 * need it: each reads its parent's handle via {@link NativeEntity#handle()}
 * and then makes a native create call with that bare value, with nothing
 * else necessarily keeping the parent reachable for the call's duration.
 *
 * <h2>Why a plain static store works as a fence — and why it must not stop there</h2>
 *
 * <p>Forcing the write in {@link #keepAlive} to survive at all relies on the
 * same reasoning either way: a static field is global, cross-thread-visible
 * state, and the JIT cannot prove, in general, that no other thread will
 * ever read it (that would require whole-program closed-world analysis
 * HotSpot does not perform), so a store into it can never be recognised as
 * dead and eliminated in isolation. That much was, and still is, correct.
 *
 * <p>What an earlier version of this class got wrong was stopping at a
 * single store with nothing to undo it. {@code sink} is a {@code static}
 * field with process lifetime: whatever object a call last passed to {@link
 * #keepAlive} stays strongly reachable through it — a genuine, if
 * unintended, GC root — for as long as nothing else in the process happens
 * to call {@link #keepAlive} again with something else. That is not a
 * theoretical concern: it was caught directly by a test that built a
 * {@code DomainParticipant}/{@code Topic}/{@code Publisher} tree and
 * abandoned all three with no further activity — the last constructor
 * call's {@code keepAlive(parent)} left {@code sink} holding the parent
 * indefinitely, and the reaper never even got a chance to attempt its
 * delete: not refused, not failed, simply never enqueued, for as long as
 * the process kept running. See {@code Topic}/{@code Publisher}'s own
 * Javadoc and Task 5's report for the full account.
 *
 * <p>The fix is for {@link #keepAlive} to null {@code sink} out again
 * immediately after setting it, so the retention window is just the gap
 * between the two statements rather than indefinite. That second store is
 * exactly why {@code sink} must be {@code volatile} now, which it
 * deliberately was not before: two plain, non-volatile writes to the same
 * field with no intervening read between them, inside the same compiled
 * method, are legally eligible for the JIT to merge into just the last one
 * — {@code sink = obj; sink = null;} collapsing to {@code sink = null;} —
 * which would silently delete the fence for exactly the call it exists to
 * protect, the opposite failure from the one a single store already
 * guarded against. A {@code volatile} write cannot be coalesced with
 * another {@code volatile} write or reordered relative to it: the JLS
 * requires each one to be individually significant, so both stores are
 * guaranteed to actually happen, in order, closing that hole. This is a
 * real, non-zero memory barrier on every call, unlike the single-store
 * design's plain write — but the cost is confined to the JDK 8 floor: any 9+
 * <em>consumer of this module's packaged, multi-release jar</em> loads
 * {@code src/main/java9}'s override instead, which has no store and no
 * barrier at all. Paying a barrier here, on the compatibility fallback
 * nothing performance-sensitive is meant to run under, is the right side of
 * that trade to be on.
 *
 * <p>"Consumer of the packaged jar" is doing real work in that sentence, not
 * a formality: this module's own {@code :api:test} runs against the
 * compiled classes directly (Gradle's normal, exploded-classpath test
 * wiring), never the packaged jar those classes are also built into, so it
 * exercises this fallback specifically — on every JDK the suite happens to
 * run under, 8 through 25 alike — never the multi-release override.
 * Confirmed directly, not assumed: reading back {@code NativeKeepAlive}'s
 * own class file bytes mid-test reports major version 52 (Java 8) even
 * under a JDK 17 test run, while doing the same read through a
 * {@code URLClassLoader} opened on the actual built jar
 * ({@code build/libs/int2dds-api-*.jar}) reports major version 53 and
 * resolves the {@code META-INF/versions/9} class, as it should. That split
 * is exactly why this fallback still needs to be correct on its own, not
 * merely present as a fallback nothing exercises: this test suite is real
 * evidence for the 8-only code path above, not for the override, and the
 * override's correctness rests on the jar's structure and the JVM's own
 * multi-release resolution, verified the same direct way.
 *
 * <p>This fallback is still a best-effort stand-in, not a formally
 * specified guarantee the way the JDK 9+ intrinsic is. {@code
 * src/main/java9}'s override (wired into {@code api/build.gradle.kts}
 * alongside the {@code java22} source set that already established the
 * pattern) is that formally specified guarantee, for whichever consumer of
 * the packaged artifact is in a position to load it.
 */
final class NativeKeepAlive {

    /**
     * Volatile so the store-then-clear pair in {@link #keepAlive} cannot be
     * coalesced into just the clear — see this class's doc. Read only by
     * {@link #keepAlive} itself, to clear it again; nothing ever reads it
     * for its value.
     */
    private static volatile Object sink;

    private NativeKeepAlive() {}

    /**
     * Keeps {@code obj} reachable up to this call. Place immediately after a
     * native call that consumed a raw handle read from {@code obj} via
     * {@link NativeEntity#handle()} — directly, or via a value derived
     * through it, such as a parent's handle read for a child's create call —
     * so neither {@code obj} nor the native object its handle names can be
     * reaped while that call is still in flight.
     *
     * <p>Clears the fence again immediately after setting it, so {@code obj}
     * is not left strongly reachable through {@code sink} beyond this call —
     * see this class's doc for why an earlier version that skipped this step
     * was a real, observed defect, not a hypothetical one.
     */
    static void keepAlive(Object obj) {
        sink = obj;
        sink = null;
    }
}
