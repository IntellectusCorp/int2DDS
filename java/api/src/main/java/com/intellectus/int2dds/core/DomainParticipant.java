package com.intellectus.int2dds.core;

import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.ParticipantQos;
import java.util.Objects;

/**
 * The local application's membership in a DDS domain, and (once the
 * remaining write-path entities exist) the factory for {@code Topic}, {@code
 * Publisher} and {@code DataWriter}.
 *
 * <p>A root {@link NativeEntity}: {@link #parent()} is always null, and
 * {@link #close()} — inherited unchanged — closes every live child of this
 * participant before releasing the participant itself.
 */
public final class DomainParticipant extends NativeEntity {

    private final int domainId;

    /**
     * Creates a participant in {@code domainId} with the core's default QoS
     * (registered default → configured default profile → spec default).
     */
    public DomainParticipant(int domainId) {
        super(null, create(domainId, 0L), FfiAccess::deleteParticipant);
        this.domainId = domainId;
    }

    /** Creates a participant in {@code domainId} with an explicit QoS. */
    public DomainParticipant(int domainId, ParticipantQos qos) {
        super(null, createWithQos(domainId, Objects.requireNonNull(qos, "qos")),
                FfiAccess::deleteParticipant);
        this.domainId = domainId;
    }

    private DomainParticipant(int domainId, NativeCleaner.Deleter deleter) {
        super(null, create(domainId, 0L), deleter);
        this.domainId = domainId;
    }

    /**
     * Package-private construction seam: the same default-QoS creation path
     * as {@link #DomainParticipant(int)}, but with an explicit deleter in
     * place of the fixed {@code FfiAccess::deleteParticipant}. A separate
     * static method rather than a second package-visible constructor
     * overload, because {@code new DomainParticipant(domainId, null)} — the
     * null-qos test elsewhere in this package — would otherwise be ambiguous
     * between this and the public {@code (int, ParticipantQos)} constructor:
     * neither parameter type is a subtype of the other, so a bare
     * {@code null} argument cannot disambiguate them, and that would be a
     * compile error, not a runtime one.
     *
     * <p>Exists for tests that need to observe exactly how many times the
     * deleter is actually invoked (see {@code DomainParticipantTest}) without
     * loosening the public constructors' contract to allow it generally. A
     * test-supplied deleter should still delegate to the real delete — this
     * seam changes who gets to count the calls, not what actually happens to
     * the underlying native participant.
     */
    static DomainParticipant createForTest(int domainId, NativeCleaner.Deleter deleter) {
        return new DomainParticipant(domainId, deleter);
    }

    /** The domain id this participant was constructed with. */
    public int domainId() {
        return domainId;
    }

    /**
     * Builds a native QoS handle, applies {@code qos} onto it, and destroys it
     * again once {@link #create} has returned — success or failure, thrown or
     * not. The native create call reads the QoS synchronously and does not
     * retain the handle past that call, so nothing needs it to survive any
     * longer than this method's body.
     */
    private static long createWithQos(int domainId, ParticipantQos qos) {
        long qosHandle = FfiAccess.createParticipantQos();
        if (qosHandle == 0L) {
            // Without this guard, a QoS-allocation failure with no policy
            // actually set (applyParticipantQos then calls no native setter
            // at all) would fall straight through to create(domainId, 0L) —
            // 0L being exactly this class's own spelling of "no QoS", i.e.
            // the core's default — silently downgrading a failure into a
            // successful default-QoS create. The same failure with any
            // policy set would instead have surfaced as a null-pointer
            // rejection from the first native setter applyParticipantQos
            // does call. Failing the same way regardless of which policies
            // happen to be set is the point of checking here explicitly.
            throw new DdsErrorException(
                    "failed to allocate a native ParticipantQos handle for participant creation");
        }
        try {
            QosMarshal.applyParticipantQos(qosHandle, qos);
            return create(domainId, qosHandle);
        } finally {
            FfiAccess.destroyParticipantQos(qosHandle);
        }
    }

    /**
     * Calls the create bridge and turns a non-zero status into the mapped
     * exception. {@code qos} is {@code 0L} (the Java-side spelling of a null
     * native pointer — the equivalent of C#'s {@code IntPtr.Zero}) for the
     * default-QoS constructor, engaging the same core-side resolution chain as
     * an explicit {@link ParticipantQos} with every policy left null.
     *
     * <p>The factory handle is passed for ABI fidelity only:
     * {@code int2dds_create_participant} (ffi/src/participant.rs) ignores its
     * {@code _factory} argument and calls
     * {@code DomainParticipantFactory::get_instance()} itself, so a wrong or
     * even zero factory handle cannot affect this call.
     */
    private static long create(int domainId, long qos) {
        long factory = DomainParticipantFactory.getInstance().handle();
        long[] handleOut = new long[1];
        int rc = FfiAccess.createParticipant(factory, domainId, qos, handleOut);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
