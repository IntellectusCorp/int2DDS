package com.intellectus.int2dds.core;

import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.PublisherQos;
import com.intellectus.int2dds.types.IDdsType;
import java.util.Objects;

/**
 * Groups DataWriters for coordinated publication.
 *
 * <p>Created through {@link DomainParticipant#createPublisher}, which
 * registers this instance in the participant's weak child list — matching
 * the C# reference binding's shape, where {@code Publisher}'s constructor
 * (csharp/src/Int2Dds/Core/Publisher.cs:26) is internal to the assembly
 * rather than public; this one is package-private for the same reason.
 */
public final class Publisher extends NativeEntity {

    /**
     * @param qos may be null for the core's default Publisher QoS; {@link
     *     DomainParticipant#createPublisher(PublisherQos)} is responsible
     *     for rejecting an explicit null before reaching here.
     */
    Publisher(DomainParticipant participant, PublisherQos qos) {
        super(Objects.requireNonNull(participant, "participant"), create(participant, qos),
                FfiAccess::deletePublisher);
    }

    private Publisher(DomainParticipant participant, NativeCleaner.Deleter deleter) {
        super(Objects.requireNonNull(participant, "participant"), create(participant, null),
                deleter);
    }

    /**
     * Package-private construction seam, the same pattern as {@link
     * DomainParticipant#createForTest} and {@link Topic#createForTest}: the
     * same default-QoS creation path as the public factory, but with an
     * explicit deleter in place of the fixed {@code FfiAccess::deletePublisher},
     * for tests that need to observe exactly how many times the deleter is
     * actually invoked. A test-supplied deleter should still delegate to the
     * real delete.
     */
    static Publisher createForTest(DomainParticipant participant, NativeCleaner.Deleter deleter) {
        return new Publisher(participant, deleter);
    }

    /** Creates a datawriter for {@code topic} with the core's default QoS. */
    public <T extends IDdsType> DataWriter<T> createDataWriter(Topic<T> topic) {
        return new DataWriter<T>(this, topic, null);
    }

    /** Creates a datawriter for {@code topic} with an explicit QoS. */
    public <T extends IDdsType> DataWriter<T> createDataWriter(Topic<T> topic, DataWriterQos qos) {
        return new DataWriter<T>(this, topic, Objects.requireNonNull(qos, "qos"));
    }

    /**
     * Resolves the create argument and, when {@code qos} is supplied, builds
     * a native QoS handle, applies the policies onto it, and destroys it
     * again once the create call returns — success or failure, thrown or
     * not, the same shape as {@link Topic#create}. {@code qos == null}
     * passes {@code 0L}, engaging the same core-side default resolution as
     * an explicit {@link PublisherQos} with every policy left null.
     */
    private static long create(DomainParticipant participant, PublisherQos qos) {
        if (qos == null) {
            return createNative(participant, 0L);
        }
        long qosHandle = FfiAccess.createPublisherQos();
        if (qosHandle == 0L) {
            // Same failure-hiding hazard DomainParticipant.createWithQos and
            // Topic.create guard against: falling through to
            // createNative(..., 0L) here would silently downgrade a
            // QoS-allocation failure into a successful default-QoS create
            // whenever qos itself has no policy set.
            throw new DdsErrorException(
                    "failed to allocate a native PublisherQos handle for publisher creation");
        }
        try {
            QosMarshal.applyPublisherQos(qosHandle, qos);
            return createNative(participant, qosHandle);
        } finally {
            FfiAccess.destroyPublisherQos(qosHandle);
        }
    }

    private static long createNative(DomainParticipant participant, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createPublisher(participant.handle(), qos, handleOut);
        // See Topic.createNative: participant.handle() is a bare long,
        // disconnected from `participant` once read, so this keeps
        // `participant` reachable for the duration of the native call above.
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
