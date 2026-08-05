package com.intellectus.int2dds.core;

import java.lang.ref.Reference;

/**
 * JDK 9+ multi-release override of {@code src/main/java}'s {@code
 * NativeKeepAlive}: on a runtime that actually has it, {@link #keepAlive}
 * is exactly {@link Reference#reachabilityFence}, the real, formally
 * specified intrinsic the 8-only fallback exists to approximate.
 *
 * <p>Same fully-qualified class name, same package-private visibility, same
 * method signature as the base-version class this replaces — that identity
 * is what makes a multi-release JAR's {@code META-INF/versions/9} entry
 * override the base one at all: a 9+ runtime opening this module's jar
 * resolves {@code com.intellectus.int2dds.core.NativeKeepAlive} to this
 * class instead, transparently to every caller, none of which needs to
 * know which version loaded. See the base version's Javadoc for why the
 * hazard this guards against is real and why Tasks 5+'s constructors need
 * it; this override changes only how the fence is implemented, not when or
 * why callers reach for it.
 *
 * <p>Unlike the base version's single static store — which, of necessity,
 * keeps {@code obj} strongly reachable for as long as it remains the most
 * recent argument any caller anywhere passed in, since nothing else ever
 * overwrites it — the intrinsic has no such retention: once this call
 * returns, {@code obj} is not kept reachable on its account at all. It is a
 * directive to the compiler about a program-order fact ("keep {@code obj}
 * reachable up to here"), not a store of any kind.
 */
final class NativeKeepAlive {

    private NativeKeepAlive() {}

    static void keepAlive(Object obj) {
        Reference.reachabilityFence(obj);
    }
}
