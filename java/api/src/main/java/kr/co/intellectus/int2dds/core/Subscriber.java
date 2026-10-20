package kr.co.intellectus.int2dds.core;

import java.nio.charset.Charset;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.function.Supplier;
import kr.co.intellectus.int2dds.conditions.InstanceState;
import kr.co.intellectus.int2dds.conditions.SampleState;
import kr.co.intellectus.int2dds.conditions.StatusCondition;
import kr.co.intellectus.int2dds.conditions.ViewState;
import kr.co.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import kr.co.intellectus.int2dds.exceptions.DdsErrorException;
import kr.co.intellectus.int2dds.exceptions.DdsException;
import kr.co.intellectus.int2dds.internal.NativeCleaner;
import kr.co.intellectus.int2dds.internal.NativeKeepAlive;
import kr.co.intellectus.int2dds.internal.QosMarshal;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.SubscriberQos;
import kr.co.intellectus.int2dds.status.StatusMask;
import kr.co.intellectus.int2dds.types.IDdsType;
import kr.co.intellectus.int2dds.xtypes.DynamicDataReader;
import kr.co.intellectus.int2dds.xtypes.DynamicTopic;
import kr.co.intellectus.int2dds.xtypes.DynamicTypeSupport;

/**
 * Groups DataReaders for coordinated subscription.
 *
 * <p>Created through {@link DomainParticipant#createSubscriber}, which registers this instance in
 * the participant's weak child list — the structural twin of {@link Publisher}. DataReader factory
 * methods land on this class in a later task, once {@code DataReader} itself exists.
 */
public final class Subscriber extends NativeEntity {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private static final int HANDLE_SIZE = 16;

    /**
     * @param qos may be null for the core's default Subscriber QoS; {@link
     *     DomainParticipant#createSubscriber(SubscriberQos)} is responsible for rejecting an
     *     explicit null before reaching here.
     */
    Subscriber(DomainParticipant participant, SubscriberQos qos) {
        super(
                Objects.requireNonNull(participant, "participant"),
                create(participant, qos),
                FfiAccess::deleteSubscriber);
    }

    /**
     * Package-private profile-create path, reached only through {@link
     * DomainParticipant#createSubscriber(String)}: a normal subscriber whose QoS comes from the
     * named profile at {@code profilePath} (a {@code "LibraryName::ProfileName"} path previously
     * loaded via {@link DomainParticipantFactory#loadProfiles}), released the same way as the
     * default-QoS path ({@code FfiAccess::deleteSubscriber}).
     */
    Subscriber(DomainParticipant participant, String profilePath) {
        super(
                Objects.requireNonNull(participant, "participant"),
                createWithProfile(participant, Objects.requireNonNull(profilePath, "profilePath")),
                FfiAccess::deleteSubscriber);
    }

    private Subscriber(DomainParticipant participant, NativeCleaner.Deleter deleter) {
        super(
                Objects.requireNonNull(participant, "participant"),
                create(participant, null),
                deleter);
    }

    /**
     * Package-private construction seam, the same pattern as {@link Publisher#createForTest}: the
     * same default-QoS creation path as the public factory, but with an explicit deleter in place
     * of the fixed {@code FfiAccess::deleteSubscriber}, for tests that need to observe exactly how
     * many times the deleter is actually invoked. A test-supplied deleter should still delegate to
     * the real delete.
     */
    static Subscriber createForTest(DomainParticipant participant, NativeCleaner.Deleter deleter) {
        return new Subscriber(participant, deleter);
    }

    private Subscriber(DomainParticipant participant, long builtinHandle) {
        super(participant, builtinHandle, Subscriber::releaseBuiltin);
    }

    /**
     * Wraps the participant's builtin subscriber, reached through {@link
     * DomainParticipant#getBuiltinSubscriber}. {@code int2dds_delete_subscriber} refuses a builtin
     * subscriber, so this wrapper's deleter releases nothing.
     */
    static Subscriber builtin(DomainParticipant participant, long handle) {
        return new Subscriber(participant, handle);
    }

    private static int releaseBuiltin(long ignored) {
        return 0;
    }

    /**
     * Takes one DCPSPublication discovery sample, blocking up to {@code timeoutMs} (negative =
     * indefinitely) for one whose topic name equals {@code topicNameFilter} ({@code null} accepts
     * any). Returns null on timeout. Meaningful only on the builtin subscriber. The result carries
     * the TypeObject when the publication sent one inline; see {@link
     * PublicationBuiltinTopicData#typeObject()} for when that is the case.
     */
    public PublicationBuiltinTopicData takePublicationData(String topicNameFilter, int timeoutMs) {
        long[] out = new long[1];
        int rc =
                FfiAccess.subscriberTakePublicationData(
                        handle(),
                        topicNameFilter == null ? null : topicNameFilter.getBytes(UTF8),
                        timeoutMs,
                        out);
        NativeKeepAlive.keepAlive(this);
        if (rc == DdsException.RET_DYNAMIC_TIMEOUT) {
            return null;
        }
        ReturnCodes.check(rc);
        long data = out[0];
        try {
            return PublicationBuiltinTopicData.materialize(data, InstanceState.ALIVE, true);
        } finally {
            FfiAccess.pubDataDestroy(data);
        }
    }

    /**
     * Deletes every datareader created through this subscriber. Their Java wrappers are closed
     * first, so none outlives its native reader; the native call then removes any reader the tree
     * does not track.
     */
    public void deleteContainedEntities() {
        closeChildren();
        int rc = FfiAccess.subscriberDeleteContainedEntities(handle());
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** This subscriber's own DDS instance handle. */
    public InstanceHandle getInstanceHandle() {
        byte[] h = new byte[16];
        int rc = FfiAccess.subscriberGetInstanceHandle(handle(), h);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new InstanceHandle(h);
    }

    /** Creates a datareader for {@code topic} with the core's default QoS. */
    public <T extends IDdsType> DataReader<T> createDataReader(
            Topic<T> topic, Supplier<T> factory) {
        // Cast disambiguates from the (Topic, Supplier, String) profile-create constructor.
        return new DataReader<T>(this, topic, factory, (DataReaderQos) null);
    }

    /** Creates a datareader for {@code topic} with an explicit QoS. */
    public <T extends IDdsType> DataReader<T> createDataReader(
            Topic<T> topic, Supplier<T> factory, DataReaderQos qos) {
        return new DataReader<T>(this, topic, factory, Objects.requireNonNull(qos, "qos"));
    }

    /**
     * Creates a datareader for {@code topic} with QoS from the named profile at {@code profilePath}
     * (a {@code "LibraryName::ProfileName"} path). The profile must already be loaded via {@link
     * DomainParticipantFactory#loadProfiles}.
     */
    public <T extends IDdsType> DataReader<T> createDataReader(
            Topic<T> topic, Supplier<T> factory, String profilePath) {
        return new DataReader<T>(
                this,
                Objects.requireNonNull(topic, "topic"),
                factory,
                Objects.requireNonNull(profilePath, "profilePath"));
    }

    /**
     * Creates a datareader for {@code cft} (a filtered view of a Topic) with the core's default
     * QoS.
     */
    public <T extends IDdsType> DataReader<T> createDataReader(
            ContentFilteredTopic<T> cft, Supplier<T> factory) {
        return DataReader.forCft(this, cft, factory, null);
    }

    /** Creates a datareader for {@code cft} (a filtered view of a Topic) with an explicit QoS. */
    public <T extends IDdsType> DataReader<T> createDataReader(
            ContentFilteredTopic<T> cft, Supplier<T> factory, DataReaderQos qos) {
        return DataReader.forCft(this, cft, factory, Objects.requireNonNull(qos, "qos"));
    }

    /**
     * Creates a datareader for {@code topic}, backed by {@code support} (an XTypes dynamic type
     * support), with the core's default QoS.
     */
    public DynamicDataReader createDynamicDataReader(
            DynamicTopic topic, DynamicTypeSupport support) {
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
        return DynamicDataReader.fromHandle(out[0], this);
    }

    /**
     * Creates a datareader for {@code topic}, backed by {@code support} (an XTypes dynamic type
     * support), with an explicit QoS.
     *
     * <p>Builds a native {@link DataReaderQos} handle, applies {@code qos}'s policies onto it, and
     * destroys it again once the create call returns — the same shape as {@link DataReader}'s own
     * QoS-taking constructor path.
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
            return DynamicDataReader.fromHandle(out[0], this);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    /**
     * Applies {@code qos} to this subscriber at runtime. Reads this subscriber's current QoS into a
     * native handle, applies {@code qos}'s non-null policies onto it (a null policy keeps its
     * current value), and destroys it again once the native {@code set_qos} call returns — success
     * or failure, thrown or not, the same build-apply-destroy shape {@link #create} uses for the
     * create path. The core rejects a change to {@code Presentation} once this subscriber is
     * enabled, surfaced here through {@link ReturnCodes#check}; {@code Partition}, {@code
     * GroupData} and {@code EntityFactory} remain mutable.
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(SubscriberQos qos) {
        Objects.requireNonNull(qos, "qos");
        long[] qosOut = new long[1];
        ReturnCodes.check(FfiAccess.getSubscriberQos(handle(), qosOut));
        long qosHandle = qosOut[0];
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
     * Reads this subscriber's current QoS off the native side. Policies with no native getter come
     * back null; see {@link QosMarshal#readSubscriberQos}.
     */
    public SubscriberQos getQos() {
        long[] qosOut = new long[1];
        int rc = FfiAccess.getSubscriberQos(handle(), qosOut);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long qosHandle = qosOut[0];
        try {
            return QosMarshal.readSubscriberQos(qosHandle);
        } finally {
            FfiAccess.destroySubscriberQos(qosHandle);
        }
    }

    /**
     * Opens a sample access block. Under GROUP access scope it is required before reading from this
     * subscriber's datareaders or calling {@link #getDataReaders}; under other scopes it has no
     * effect. Calls may be nested.
     */
    public void beginAccess() {
        int rc = FfiAccess.subscriberBeginAccess(handle());
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Closes the access block opened by {@link #beginAccess}.
     *
     * @throws kr.co.intellectus.int2dds.exceptions.DdsPreconditionNotMetException under GROUP
     *     access scope if no access block is open
     */
    public void endAccess() {
        int rc = FfiAccess.subscriberEndAccess(handle());
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** {@link #getDataReaders(int, int, int)} with every state mask set to ANY. */
    public List<DataReader<?>> getDataReaders() {
        return getDataReaders(SampleState.ANY, ViewState.ANY, InstanceState.ANY);
    }

    /**
     * The datareaders of this subscriber holding samples that match the state masks. Under GROUP
     * access scope with ordered access the list is in sample order, one entry per sample, so a
     * reader can appear more than once; read one sample from each entry in turn. Datareaders
     * created through {@link #createDynamicDataReader} are not included.
     *
     * @throws kr.co.intellectus.int2dds.exceptions.DdsPreconditionNotMetException under GROUP
     *     access scope outside an access block
     */
    public List<DataReader<?>> getDataReaders(
            int sampleStateMask, int viewStateMask, int instanceStateMask) {
        List<InstanceHandle> readerHandles =
                getDataReaderHandles(sampleStateMask, viewStateMask, instanceStateMask);

        Map<InstanceHandle, DataReader<?>> readersByHandle =
                new HashMap<InstanceHandle, DataReader<?>>();
        for (NativeEntity child : openChildren()) {
            if (child instanceof DataReader) {
                DataReader<?> reader = (DataReader<?>) child;
                readersByHandle.put(new InstanceHandle(reader.getGuid()), reader);
            }
        }

        List<DataReader<?>> readers = new ArrayList<DataReader<?>>(readerHandles.size());
        for (InstanceHandle readerHandle : readerHandles) {
            DataReader<?> reader = readersByHandle.get(readerHandle);
            if (reader != null) {
                readers.add(reader);
            }
        }
        return readers;
    }

    /** Invokes the {@code onDataAvailable} listener of every datareader of this subscriber. */
    public void notifyDataReaders() {
        int rc = FfiAccess.subscriberNotifyDatareaders(handle());
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * A fresh StatusCondition for this subscriber's status changes; attach it to a WaitSet to wait
     * on status transitions.
     */
    public StatusCondition getStatusCondition() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.subscriberGetStatusCondition(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new StatusCondition(out[0]);
    }

    /**
     * The set of statuses that have changed since last read (per DDS, reading a status via its
     * getter or here clears it). Attach a StatusCondition to a WaitSet to block on these.
     */
    public StatusMask getStatusChanges() {
        int[] out = new int[1];
        int rc = FfiAccess.subscriberGetStatusChanges(handle(), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return StatusMask.of(out[0]);
    }

    /**
     * The reader instance handles {@code int2dds_subscriber_get_datareaders} reports, in its order.
     * Grow-and-retry on capacity, the same shape as {@link DataReader#getMatchedPublications}.
     */
    private List<InstanceHandle> getDataReaderHandles(
            int sampleStateMask, int viewStateMask, int instanceStateMask) {
        long h = handle();
        int capacity = 8;
        byte[] handles = new byte[capacity * HANDLE_SIZE];
        int count;
        while (true) {
            long[] countOut = new long[1];
            int rc =
                    FfiAccess.subscriberGetDatareaders(
                            h,
                            sampleStateMask,
                            viewStateMask,
                            instanceStateMask,
                            handles,
                            capacity,
                            countOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            count = (int) countOut[0];
            if (count <= capacity) {
                break;
            }
            capacity = count;
            handles = new byte[capacity * HANDLE_SIZE];
        }

        List<InstanceHandle> readerHandles = new ArrayList<InstanceHandle>(count);
        for (int i = 0; i < count; i++) {
            readerHandles.add(
                    new InstanceHandle(
                            Arrays.copyOfRange(handles, i * HANDLE_SIZE, (i + 1) * HANDLE_SIZE)));
        }
        return readerHandles;
    }

    /**
     * Resolves the create argument and, when {@code qos} is supplied, builds a native QoS handle,
     * applies the policies onto it, and destroys it again once the create call returns — success or
     * failure, thrown or not, the same shape as {@link Publisher#create}. {@code qos == null}
     * passes {@code 0L}, engaging the same core-side default resolution as an explicit {@link
     * SubscriberQos} with every policy left null.
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

    /** The profile-create path, the same keep-alive/check shape as {@link #createNative}. */
    private static long createWithProfile(DomainParticipant participant, String profilePath) {
        long[] handleOut = new long[1];
        int rc =
                FfiAccess.createSubscriberWithProfile(
                        participant.handle(), profilePath.getBytes(UTF8), handleOut);
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
