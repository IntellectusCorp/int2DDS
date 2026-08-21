package com.intellectus.int2dds.core;

import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.PublisherQos;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.IDdsType;
import com.intellectus.int2dds.xtypes.DynamicDataWriter;
import com.intellectus.int2dds.xtypes.DynamicTopic;
import com.intellectus.int2dds.xtypes.DynamicTypeSupport;
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

    /** This publisher's own DDS instance handle. */
    public InstanceHandle getInstanceHandle() {
        byte[] h = new byte[16];
        int rc = FfiAccess.publisherGetInstanceHandle(handle(), h);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new InstanceHandle(h);
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
     * Creates a datawriter for {@code topic}, backed by {@code support} (an
     * XTypes dynamic type support), with the core's default QoS.
     */
    public DynamicDataWriter createDynamicDataWriter(DynamicTopic topic, DynamicTypeSupport support) {
        Objects.requireNonNull(topic, "topic");
        Objects.requireNonNull(support, "support");
        long p = handle();
        long t = topic.handle();
        long s = support.handle();
        long[] out = new long[1];
        int rc = FfiAccess.createDataWriterDynamic(p, t, s, out);
        // p, t and s are bare longs read moments before the native call that
        // consumes them; keep this publisher, topic and support reachable
        // across it -- see NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(topic);
        NativeKeepAlive.keepAlive(support);
        ReturnCodes.check(rc);
        return DynamicDataWriter.fromHandle(out[0]);
    }

    /**
     * Creates a datawriter for {@code topic}, backed by {@code support} (an
     * XTypes dynamic type support), with an explicit QoS.
     *
     * <p>Builds a native {@link DataWriterQos} handle, applies {@code qos}'s
     * policies onto it, and destroys it again once the create call returns —
     * the same shape as {@link DataWriter}'s own QoS-taking constructor path.
     */
    public DynamicDataWriter createDynamicDataWriter(
            DynamicTopic topic, DynamicTypeSupport support, DataWriterQos qos) {
        Objects.requireNonNull(topic, "topic");
        Objects.requireNonNull(support, "support");
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createDataWriterQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataWriterQos handle for dynamic datawriter creation");
        }
        try {
            QosMarshal.applyWriterQos(qosHandle, qos);
            long p = handle();
            long t = topic.handle();
            long s = support.handle();
            long[] out = new long[1];
            int rc = FfiAccess.createDataWriterDynamic(p, t, s, qosHandle, out);
            // Same keep-alive reasoning as the no-QoS overload above.
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(topic);
            NativeKeepAlive.keepAlive(support);
            ReturnCodes.check(rc);
            return DynamicDataWriter.fromHandle(out[0]);
        } finally {
            FfiAccess.destroyDataWriterQos(qosHandle);
        }
    }

    /**
     * Applies {@code qos} to this publisher at runtime. Builds a native
     * {@link PublisherQos} handle, applies {@code qos}'s policies onto it,
     * and destroys it again once the native {@code set_qos} call returns —
     * success or failure, thrown or not, the same build-apply-destroy shape
     * {@link #create} uses for the create path. The core rejects a change to
     * {@code Presentation} once this publisher is enabled, surfaced here
     * through {@link ReturnCodes#check}; {@code Partition}, {@code
     * GroupData} and {@code EntityFactory} remain mutable.
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(PublisherQos qos) {
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createPublisherQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native PublisherQos handle for setQos");
        }
        try {
            QosMarshal.applyPublisherQos(qosHandle, qos);
            int rc = FfiAccess.publisherSetQos(handle(), qosHandle);
            // handle() is a bare long read moments before the native call
            // that consumes it; keep this publisher reachable across it --
            // see NativeKeepAlive's own doc for the full argument.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        } finally {
            FfiAccess.destroyPublisherQos(qosHandle);
        }
    }

    /**
     * Blocks until every matched reliable {@link DataReader} of every writer
     * belonging to this publisher has acknowledged all samples sent so far,
     * or {@code timeoutMillis} elapses. Returns {@code true} if
     * acknowledgment completed, {@code false} if the timeout expired first.
     */
    public boolean waitForAcknowledgments(long timeoutMillis) {
        int rc = FfiAccess.publisherWaitForAcknowledgments(handle(), timeoutMillis);
        NativeKeepAlive.keepAlive(this);
        if (rc == DdsException.RET_TIMEOUT) {
            return false;
        }
        ReturnCodes.check(rc);
        return true;
    }

    /**
     * A fresh StatusCondition for this publisher's status changes; attach it
     * to a WaitSet to wait on status transitions.
     */
    public StatusCondition getStatusCondition() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.publisherGetStatusCondition(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new StatusCondition(out[0]);
    }

    /**
     * The set of statuses that have changed since last read (per DDS, reading
     * a status via its getter or here clears it). Attach a StatusCondition to
     * a WaitSet to block on these.
     */
    public StatusMask getStatusChanges() {
        int[] out = new int[1];
        int rc = FfiAccess.publisherGetStatusChanges(handle(), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return StatusMask.of(out[0]);
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
