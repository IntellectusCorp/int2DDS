package com.intellectus.int2dds.core;

import com.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * The process-wide DDS entry point. A singleton: the native factory is not
 * owned by any one Java object, so unlike {@link NativeEntity} subclasses this
 * has no {@code close()} — there is nothing for any particular caller to
 * release.
 *
 * <p>Lazily initialised on first use via the classloader's own
 * once-and-thread-safe guarantee for static nested class initialisation (the
 * initialization-on-demand holder idiom), the same effect as C#'s {@code
 * Lazy<T>} used by the reference binding.
 */
public final class DomainParticipantFactory {

    private static final class Holder {
        private static final DomainParticipantFactory INSTANCE = new DomainParticipantFactory();
    }

    private final long handle;

    private DomainParticipantFactory() {
        this.handle = FfiAccess.participantFactoryGetInstance();
    }

    /** The singleton factory instance. */
    public static DomainParticipantFactory getInstance() {
        return Holder.INSTANCE;
    }

    /** The native factory handle, for entities built on top of this class. */
    public long handle() {
        return handle;
    }
}
