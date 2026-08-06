package com.intellectus.int2dds.internal;

import java.lang.ref.Reference;

/**
 * JDK 9+ multi-release override of {@code src/main/java}'s {@code
 * NativeKeepAlive}: on a runtime that actually has it, {@link #keepAlive}
 * is exactly {@link Reference#reachabilityFence}, the real, formally
 * specified intrinsic the 8-only fallback exists to approximate.
 *
 * <p>Same fully-qualified class name, same {@code public} visibility, same
 * method signature as the base-version class this replaces — that identity
 * is what makes a multi-release JAR's {@code META-INF/versions/9} entry
 * override the base one at all: a 9+ runtime opening this module's jar
 * resolves {@code com.intellectus.int2dds.internal.NativeKeepAlive} to this
 * class instead, transparently to every caller in {@code core} or {@code
 * internal.ffi}, none of which needs to know which version loaded. {@code
 * public} here is for the same reason it is on the base version: this class
 * lives in {@code internal} because two packages need to call it, not
 * because it is meant for use outside this module. See the base version's
 * Javadoc for why the hazard this guards against is real, why Tasks 5+'s
 * entity constructors need it, and why every {@code FfiAccess} bridge that
 * hands a direct buffer's address to native code needs the identical fence;
 * this override changes only how the fence is implemented, not when or why
 * callers reach for it.
 *
 * <p>Unlike the base version's store-then-clear pair — which retains {@code
 * obj} strongly reachable only for the brief window between the two writes,
 * not indefinitely; see the base version's own Javadoc for the earlier,
 * single-store design that did retain indefinitely, and why it was
 * retracted — the intrinsic has no retention window at all: once this call
 * returns, {@code obj} was never kept reachable on its account in the first
 * place. It is a directive to the compiler about a program-order fact
 * ("keep {@code obj} reachable up to here"), not a store of any kind.
 */
public final class NativeKeepAlive {

    private NativeKeepAlive() {}

    public static void keepAlive(Object obj) {
        Reference.reachabilityFence(obj);
    }
}
