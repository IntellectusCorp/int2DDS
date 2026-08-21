package com.intellectus.int2dds.core;

import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.SubscriberQos;
import com.intellectus.int2dds.types.IDdsType;
import com.intellectus.int2dds.xtypes.DynamicDataReader;
import com.intellectus.int2dds.xtypes.DynamicTopic;
import com.intellectus.int2dds.xtypes.DynamicTypeSupport;
import java.util.Objects;
import java.util.function.Supplier;

/**
 * Groups DataReaders for coordinated subscription.
 *
 * <p>Created through {@link DomainParticipant#createSubscriber}, which
 * registers this instance in the participant's weak child list — the
 * structural twin of {@link Publisher}. DataReader factory methods land on
 * this class in a later task, once {@code DataReader} itself exists.
 */
public final class Subscriber extends NativeEntity {

    /**
     * @param qos may be null for the core's default Subscriber QoS; {@link
     *     DomainParticipant#createSubscriber(SubscriberQos)} is responsible
     *     for rejecting an explicit null before reaching here.
     */
    Subscriber(DomainParticipant participant, SubscriberQos qos) {
        super(Objects.requireNonNull(participant, "participant"), create(participant, qos),
                FfiAccess::deleteSubscriber);
    }

    private Subscriber(DomainParticipant participant, NativeCleaner.Deleter deleter) {
        super(Objects.requireNonNull(participant, "participant"), create(participant, null),
                deleter);
    }

    /**
     * Package-private construction seam, the same pattern as {@link
     * Publisher#createForTest}: the same default-QoS creation path as the
     * public factory, but with an explicit deleter in place of the fixed
     * {@code FfiAccess::deleteSubscriber}, for tests that need to observe
     * exactly how many times the deleter is actually invoked. A
     * test-supplied deleter should still delegate to the real delete.
     */
    static Subscriber createForTest(DomainParticipant participant, NativeCleaner.Deleter deleter) {
        return new Subscriber(participant, deleter);
    }

    /** Creates a datareader for {@code topic} with the core's default QoS. */
    public <T extends IDdsType> DataReader<T> createDataReader(Topic<T> topic, Supplier<T> factory) {
        return new DataReader<T>(this, topic, factory, null);
    }

    /** Creates a datareader for {@code topic} with an explicit QoS. */
    public <T extends IDdsType> DataReader<T> createDataReader(
            Topic<T> topic, Supplier<T> factory, DataReaderQos qos) {
        return new DataReader<T>(this, topic, factory, Objects.requireNonNull(qos, "qos"));
    }

    /**
     * Creates a datareader for {@code topic}, backed by {@code support} (an
     * XTypes dynamic type support), with the core's default QoS.
     */
    public DynamicDataReader createDynamicDataReader(DynamicTopic topic, DynamicTypeSupport support) {
        Objects.requireNonNull(topic, "topic");
        Objects.requireNonNull(support, "support");
        long sub = handle();
        long t = topic.handle();
        long s = support.handle();
        long[] out = new long[1];
        int rc = FfiAccess.createDataReaderDynamic(sub, t, s, out);
        // sub, t and s are bare longs read moments before the native call
        // that consumes them; keep this subscriber, topic and support
        // reachable across it -- see NativeKeepAlive's own doc for the full
        // argument.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(topic);
        NativeKeepAlive.keepAlive(support);
        ReturnCodes.check(rc);
        return DynamicDataReader.fromHandle(out[0]);
    }

    /**
     * Creates a datareader for {@code topic}, backed by {@code support} (an
     * XTypes dynamic type support), with an explicit QoS.
     *
     * <p>Builds a native {@link DataReaderQos} handle, applies {@code qos}'s
     * policies onto it, and destroys it again once the create call returns —
     * the same shape as {@link DataReader}'s own QoS-taking constructor path.
     */
    public DynamicDataReader createDynamicDataReader(
            DynamicTopic topic, DynamicTypeSupport support, DataReaderQos qos) {
        Objects.requireNonNull(topic, "topic");
        Objects.requireNonNull(support, "support");
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createDataReaderQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataReaderQos handle for dynamic datareader creation");
        }
        try {
            QosMarshal.applyReaderQos(qosHandle, qos);
            long sub = handle();
            long t = topic.handle();
            long s = support.handle();
            long[] out = new long[1];
            int rc = FfiAccess.createDataReaderDynamic(sub, t, s, qosHandle, out);
            // Same keep-alive reasoning as the no-QoS overload above.
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(topic);
            NativeKeepAlive.keepAlive(support);
            ReturnCodes.check(rc);
            return DynamicDataReader.fromHandle(out[0]);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    /**
     * Applies {@code qos} to this subscriber at runtime. Builds a native
     * {@link SubscriberQos} handle, applies {@code qos}'s policies onto it,
     * and destroys it again once the native {@code set_qos} call returns —
     * success or failure, thrown or not, the same build-apply-destroy shape
     * {@link #create} uses for the create path. The core rejects a change to
     * {@code Presentation} once this subscriber is enabled, surfaced here
     * through {@link ReturnCodes#check}; {@code Partition}, {@code
     * GroupData} and {@code EntityFactory} remain mutable.
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(SubscriberQos qos) {
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createSubscriberQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native SubscriberQos handle for setQos");
        }
        try {
            QosMarshal.applySubscriberQos(qosHandle, qos);
            int rc = FfiAccess.subscriberSetQos(handle(), qosHandle);
            // handle() is a bare long read moments before the native call
            // that consumes it; keep this subscriber reachable across it --
            // see NativeKeepAlive's own doc for the full argument.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        } finally {
            FfiAccess.destroySubscriberQos(qosHandle);
        }
    }

    /**
     * Resolves the create argument and, when {@code qos} is supplied, builds
     * a native QoS handle, applies the policies onto it, and destroys it
     * again once the create call returns — success or failure, thrown or
     * not, the same shape as {@link Publisher#create}. {@code qos == null}
     * passes {@code 0L}, engaging the same core-side default resolution as
     * an explicit {@link SubscriberQos} with every policy left null.
     */
    private static long create(DomainParticipant participant, SubscriberQos qos) {
        if (qos == null) {
            return createNative(participant, 0L);
        }
        long qosHandle = FfiAccess.createSubscriberQos();
        if (qosHandle == 0L) {
            // Same failure-hiding hazard Publisher.create guards against:
            // falling through to createNative(..., 0L) here would silently
            // downgrade a QoS-allocation failure into a successful
            // default-QoS create whenever qos itself has no policy set.
            throw new DdsErrorException(
                    "failed to allocate a native SubscriberQos handle for subscriber creation");
        }
        try {
            QosMarshal.applySubscriberQos(qosHandle, qos);
            return createNative(participant, qosHandle);
        } finally {
            FfiAccess.destroySubscriberQos(qosHandle);
        }
    }

    private static long createNative(DomainParticipant participant, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createSubscriber(participant.handle(), qos, handleOut);
        // See Publisher.createNative: participant.handle() is a bare long,
        // disconnected from `participant` once read, so this keeps
        // `participant` reachable for the duration of the native call above.
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
