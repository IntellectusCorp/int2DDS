package com.intellectus.int2dds.core;

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
