package com.intellectus.int2dds.internal;

/**
 * Cross-package accessor for {@code conditions.Condition}'s package-private
 * {@code handle()}, so {@code core.DataReader} (a condition-filtered batch
 * take/read needs the condition's native handle) can reach it without that
 * method going public and without {@code internal} depending on {@code
 * conditions} (hence {@link Accessor#handle} taking {@code Object}, cast by
 * the registered lambda). Mirrors the JDK's own {@code SharedSecrets}
 * pattern.
 */
public final class ConditionHandleAccess {
    /** Registered by {@code conditions.Condition}'s static initializer. */
    public interface Accessor {
        long handle(Object condition);
    }

    private static volatile Accessor accessor;

    private ConditionHandleAccess() {}

    public static void setAccessor(Accessor a) {
        accessor = a;
    }

    /** Returns {@code condition}'s native handle. Requires {@code conditions.Condition} to be loaded. */
    public static long handle(Object condition) {
        Accessor a = accessor;
        if (a == null) {
            throw new IllegalStateException("Condition handle accessor not registered");
        }
        return a.handle(condition);
    }
}
