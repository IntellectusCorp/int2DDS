package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.conditions.QueryCondition;
import com.intellectus.int2dds.conditions.ReadCondition;
import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.ConditionHandleAccess;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.listeners.DataReaderListener;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.status.LivelinessChangedStatus;
import com.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import com.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import com.intellectus.int2dds.status.RequestedIncompatibleTypeStatus;
import com.intellectus.int2dds.status.SampleLostStatus;
import com.intellectus.int2dds.status.SampleRejectedStatus;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.status.SubscriptionMatchedStatus;
import com.intellectus.int2dds.types.IDdsType;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;
import java.util.function.Supplier;

/**
 * Receives samples of type {@code T} from a {@link Topic}. Structural twin of
 * {@link DataWriter}: created through {@link Subscriber#createDataReader}, it
 * takes/reads one serialized sample into a pooled direct buffer and rebuilds
 * {@code T} with the CDR layer — no loan, no copy beyond the core's own.
 *
 * <p><b>Listener lifecycle:</b> a listener installed with {@link #setListener}
 * is held by a binding-owned native context whose pointer this reader tracks.
 * {@link NativeEntity#close()} is final and does not clear it, so a caller that
 * installed a listener must call {@code setListener(null, null)} before closing
 * to release that context; automatic teardown is deferred to a later branch.
 *
 * @param <T> the DDS data type this reader receives.
 */
public final class DataReader<T extends IDdsType> extends NativeEntity {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private static final boolean LITTLE_ENDIAN_HOST =
            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN;

    private static final int DEFAULT_CAPACITY = 4096;

    /** Largest power-of-two direct buffer size an int capacity can hold (2^30). */
    private static final long MAX_SAMPLE_BYTES = 1 << 30;

    // Exactly one of topic/cft is non-null, matching which constructor built
    // this reader. Both exist purely to keep the entity that was created
    // against reachable -- see topic()'s own doc for why the accessor stays
    // Topic<T>-typed even though a CFT-backed reader has no such value.
    private final Topic<T> topic;
    private final ContentFilteredTopic<T> cft;
    private final Supplier<T> factory;

    /**
     * Resolved once at construction: used only by {@link #lookupInstance},
     * which must serialize {@code sample} identically to how a matching
     * {@link DataWriter#write} on this topic would.
     *
     * <p>Resolved from the QoS arguments rather than from the reader itself,
     * unlike {@link DataWriter}'s, because there is no {@code
     * int2dds_datareader_data_representation} to read — adding one moves the
     * exported-symbol count {@code java.yml} gates on. Harmless for now: {@link
     * #lookupInstance} returns the NIL handle whatever this resolves to, for
     * the reason its own doc gives.
     */
    private final boolean xcdr2;

    // Pointer to the binding-owned native listener context, or 0 when none is
    // installed. Guarded by setListener's own synchronization.
    private long listenerCtx = 0L;

    // Reused across take/read on this reader. Not thread-safe by design: a
    // DataReader is used from one consumer thread at a time, as in the C#
    // reference. Grown on BUFFER_TOO_SMALL, never shrunk.
    private ByteBuffer payload =
            ByteBuffer.allocateDirect(DEFAULT_CAPACITY).order(ByteOrder.nativeOrder());
    private final ByteBuffer sizeSlot =
            ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
    private final ByteBuffer infoSlot =
            ByteBuffer.allocateDirect(SampleInfo.STRUCT_SIZE).order(ByteOrder.nativeOrder());
    private final ByteBuffer validSlot =
            ByteBuffer.allocateDirect(1).order(ByteOrder.nativeOrder());

    DataReader(Subscriber subscriber, Topic<T> topic, Supplier<T> factory, DataReaderQos qos) {
        super(Objects.requireNonNull(subscriber, "subscriber"),
                create(subscriber, Objects.requireNonNull(topic, "topic"), qos),
                FfiAccess::deleteDataReader);
        this.topic = topic;
        this.cft = null;
        this.factory = Objects.requireNonNull(factory, "factory");
        this.xcdr2 = resolveXcdr2(qos);
    }

    /**
     * Parallel construction path for a reader created on a {@link
     * ContentFilteredTopic} rather than a plain {@link Topic} -- {@code
     * int2dds_create_datareader_cft} in place of {@code
     * int2dds_create_datareader}, everything else (QoS build-apply-destroy,
     * {@link com.intellectus.int2dds.internal.NativeCleaner NativeCleaner}
     * registration via the inherited constructor,
     * keep-alive fences) identical. Package-private, reached only through
     * {@link Subscriber#createDataReader(ContentFilteredTopic, Supplier)}.
     */
    private DataReader(Subscriber subscriber, ContentFilteredTopic<T> cft, Supplier<T> factory,
            DataReaderQos qos) {
        super(Objects.requireNonNull(subscriber, "subscriber"),
                createCft(subscriber, Objects.requireNonNull(cft, "cft"), qos),
                FfiAccess::deleteDataReader);
        this.topic = null;
        this.cft = cft;
        this.factory = Objects.requireNonNull(factory, "factory");
        this.xcdr2 = resolveXcdr2(qos);
    }

    /**
     * Package-private profile-create path, reached only through {@link
     * Subscriber#createDataReader(Topic, Supplier, String)}: a normal typed
     * datareader whose QoS comes from the named profile at {@code
     * profilePath} (a {@code "LibraryName::ProfileName"} path previously
     * loaded via {@link DomainParticipantFactory#loadProfiles}), released the
     * same way as the default-QoS path ({@code FfiAccess::deleteDataReader}).
     * The profile's own data representation is not reflected in {@link
     * #xcdr2} here the way {@link DataWriter}'s profile constructor now
     * reflects it; see that field's doc. Decoding is unaffected either way --
     * {@code CdrReader.of} takes the version from the encapsulation id.
     */
    DataReader(Subscriber subscriber, Topic<T> topic, Supplier<T> factory, String profilePath) {
        super(Objects.requireNonNull(subscriber, "subscriber"),
                createWithProfile(subscriber, Objects.requireNonNull(topic, "topic"),
                        Objects.requireNonNull(profilePath, "profilePath")),
                FfiAccess::deleteDataReader);
        this.topic = topic;
        this.cft = null;
        this.factory = Objects.requireNonNull(factory, "factory");
        this.xcdr2 = resolveXcdr2(null);
    }

    /** See the {@link #xcdr2} field's doc for why this reads {@code qos} and not the reader. */
    private static boolean resolveXcdr2(DataReaderQos qos) {
        DataRepresentationKind kind = (qos != null && qos.getDataRepresentation() != null)
                ? qos.getDataRepresentation().getKind()
                : DataRepresentationKind.fromValue(FfiAccess.defaultDataRepresentation());
        return kind == DataRepresentationKind.XCDR2;
    }

    /** See the private CFT constructor's own doc; {@code qos} may be null for the core's default. */
    static <T extends IDdsType> DataReader<T> forCft(
            Subscriber subscriber, ContentFilteredTopic<T> cft, Supplier<T> factory, DataReaderQos qos) {
        return new DataReader<T>(subscriber, cft, factory, qos);
    }

    /**
     * The topic this reader receives from — the same instance passed to
     * {@code createDataReader}. {@code null} for a reader created on a
     * {@link ContentFilteredTopic} instead; see {@link #contentFilteredTopic()}.
     */
    public Topic<T> topic() {
        return topic;
    }

    /**
     * The ContentFilteredTopic this reader receives from, or {@code null} for
     * a reader created on a plain {@link Topic}; see {@link #topic()}.
     */
    public ContentFilteredTopic<T> contentFilteredTopic() {
        return cft;
    }

    /**
     * Installs {@code listener} for the statuses in {@code mask}, or — with a
     * {@code null} listener — clears any current listener. A {@code null}
     * {@code mask} means all statuses.
     *
     * <p>Any previously installed listener is cleared first, releasing its
     * native context. Callbacks fire on DDS background threads, so a listener
     * must be thread-safe. Because {@link #close()} does not clear listeners, a
     * caller should {@code setListener(null, null)} before closing this reader.
     *
     * @throws com.intellectus.int2dds.exceptions.DdsException if the native
     *     clear or install fails
     */
    public synchronized void setListener(DataReaderListener listener, StatusMask mask) {
        long h = handle();
        long prev = listenerCtx;
        if (prev != 0L) {
            int rc = FfiAccess.readerListenerClear(h, prev);
            NativeKeepAlive.keepAlive(this);
            listenerCtx = 0L;
            ReturnCodes.check(rc);
        }
        if (listener != null) {
            long ctx = FfiAccess.readerListenerSet(
                    h, listener, mask == null ? StatusMask.all().bits() : mask.bits());
            NativeKeepAlive.keepAlive(this);
            if (ctx == 0L) {
                throw new DdsErrorException("failed to install DataReader listener");
            }
            listenerCtx = ctx;
        }
    }

    /** A fresh {@link StatusCondition} for this reader's status changes. */
    public StatusCondition getStatusCondition() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.datareaderGetStatusCondition(h, out);
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
        int rc = FfiAccess.datareaderGetStatusChanges(handle(), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return StatusMask.of(out[0]);
    }

    /** Whether this reader has any samples available to take/read. */
    public boolean hasData() {
        boolean[] out = new boolean[1];
        int rc = FfiAccess.datareaderHasData(handle(), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** This reader's 16-byte DDS entity GUID. */
    public byte[] getGuid() {
        byte[] guid = new byte[16];
        int rc = FfiAccess.datareaderGetGuid(handle(), guid);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return guid;
    }

    /**
     * This reader's SUBSCRIPTION_MATCHED status: how many DataWriters it has
     * matched, and the change since last read. Per DDS, reading this status
     * clears its {@code *Change} fields.
     */
    public SubscriptionMatchedStatus getSubscriptionMatchedStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetSubscriptionMatchedStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        int currentCount = buf.getInt(8);
        int currentCountChange = buf.getInt(12);
        byte[] lastPublicationHandle = new byte[16];
        for (int i = 0; i < 16; i++) {
            lastPublicationHandle[i] = buf.get(16 + i);
        }
        return new SubscriptionMatchedStatus(
                totalCount, totalCountChange, currentCount, currentCountChange,
                lastPublicationHandle);
    }

    /**
     * This reader's LIVELINESS_CHANGED status: how many matched writers are
     * currently alive versus not alive, and the change since last read. Per
     * DDS, reading this status clears its {@code *Change} fields.
     */
    public LivelinessChangedStatus getLivelinessChangedStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetLivelinessChangedStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int aliveCount = buf.getInt(0);
        int notAliveCount = buf.getInt(4);
        int aliveCountChange = buf.getInt(8);
        int notAliveCountChange = buf.getInt(12);
        byte[] lastPublicationHandle = new byte[16];
        for (int i = 0; i < 16; i++) {
            lastPublicationHandle[i] = buf.get(16 + i);
        }
        return new LivelinessChangedStatus(
                aliveCount, notAliveCount, aliveCountChange, notAliveCountChange,
                lastPublicationHandle);
    }

    /**
     * This reader's REQUESTED_DEADLINE_MISSED status: how many times this
     * reader missed the deadline it requested for an instance, and the
     * change since last read. Per DDS, reading this status clears its
     * {@code *Change} field.
     */
    public RequestedDeadlineMissedStatus getRequestedDeadlineMissedStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(24).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetRequestedDeadlineMissedStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        byte[] lastInstanceHandle = new byte[16];
        for (int i = 0; i < 16; i++) {
            lastInstanceHandle[i] = buf.get(8 + i);
        }
        return new RequestedDeadlineMissedStatus(totalCount, totalCountChange, lastInstanceHandle);
    }

    /**
     * This reader's REQUESTED_INCOMPATIBLE_QOS status: how many times this
     * reader discovered a requested QoS incompatible with an offering
     * writer's QoS, and the change since last read. Per DDS, reading this
     * status clears its {@code *Change} field.
     */
    public RequestedIncompatibleQosStatus getRequestedIncompatibleQosStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(16).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetRequestedIncompatibleQosStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        int lastPolicyId = buf.getInt(8);
        int policiesCount = buf.getInt(12);
        return new RequestedIncompatibleQosStatus(
                totalCount, totalCountChange, lastPolicyId, policiesCount);
    }

    /**
     * This reader's REQUESTED_INCOMPATIBLE_TYPE status: how many times this
     * reader's type was found incompatible with an offering writer's type,
     * and the change since last read. Per DDS, reading this status clears
     * its {@code *Change} field.
     */
    public RequestedIncompatibleTypeStatus getRequestedIncompatibleTypeStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetRequestedIncompatibleTypeStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        return new RequestedIncompatibleTypeStatus(totalCount, totalCountChange);
    }

    /**
     * This reader's SAMPLE_LOST status: how many samples were lost (never
     * received) by this reader, and the change since last read. Per DDS,
     * reading this status clears its {@code *Change} field.
     */
    public SampleLostStatus getSampleLostStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetSampleLostStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        return new SampleLostStatus(totalCount, totalCountChange);
    }

    /**
     * This reader's SAMPLE_REJECTED status: how many samples this reader
     * rejected (e.g. resource-limit related), the last reason, and the
     * change since last read. Per DDS, reading this status clears its
     * {@code *Change} field.
     */
    public SampleRejectedStatus getSampleRejectedStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(28).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datareaderGetSampleRejectedStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        int lastReason = buf.getInt(8);
        byte[] lastInstanceHandle = new byte[16];
        for (int i = 0; i < 16; i++) {
            lastInstanceHandle[i] = buf.get(12 + i);
        }
        return new SampleRejectedStatus(totalCount, totalCountChange, lastReason, lastInstanceHandle);
    }

    /**
     * Reads this reader's current QoS off the native side — not the {@code
     * DataReaderQos} it was constructed with, which this class does not
     * retain. Policies with no native getter come back null; see {@link
     * QosMarshal#readReaderQos}'s own doc for the full list.
     */
    public DataReaderQos getQos() {
        long[] qosOut = new long[1];
        int rc = FfiAccess.getReaderQos(handle(), qosOut);
        // Same reasoning as DataWriter.getQos(): handle() reads this reader's
        // own handle right before the native call that consumes it, with
        // nothing else touching `this` in between.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long qosHandle = qosOut[0];
        try {
            return QosMarshal.readReaderQos(qosHandle);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    /**
     * Applies {@code qos} to this reader at runtime. Builds a native {@link
     * DataReaderQos} handle, applies {@code qos}'s policies onto it, and
     * destroys it again once the native {@code set_qos} call returns —
     * success or failure, thrown or not, the same build-apply-destroy shape
     * {@link #create} uses for the create path. The core rejects a change to
     * an immutable policy (e.g. {@code Reliability}, {@code Durability},
     * {@code History}) once this reader is enabled, surfaced here through
     * {@link ReturnCodes#check}; {@code Deadline}, {@code TimeBasedFilter},
     * {@code UserData} and {@code ReaderDataLifecycle} remain mutable.
     * ({@code LatencyBudget} is rejected outright by this core regardless of
     * mutability -- see {@code check_unsupported_policies} in
     * dds/src/dcps/subscription/qos/mod.rs -- so it is not a useful example
     * of a runtime-mutable policy here.)
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(DataReaderQos qos) {
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createDataReaderQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataReaderQos handle for setQos");
        }
        try {
            QosMarshal.applyReaderQos(qosHandle, qos);
            int rc = FfiAccess.datareaderSetQos(handle(), qosHandle);
            // handle() is a bare long read moments before the native call
            // that consumes it; keep this reader reachable across it -- see
            // NativeKeepAlive's own doc for the full argument.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    /** A fresh {@link ReadCondition} filtering this reader's cache by state masks. */
    public ReadCondition createReadCondition(int sampleStates, int viewStates, int instanceStates) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.datareaderCreateReadCondition(
                h, sampleStates, viewStates, instanceStates, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new ReadCondition(out[0]);
    }

    /**
     * A fresh {@link QueryCondition} filtering this reader's cache by state
     * masks and a query expression. {@code queryExpr} and {@code params}
     * cross as UTF-8 {@code byte[]} / {@code byte[][]}, never {@code String}.
     */
    public QueryCondition createQueryCondition(int sampleStates, int viewStates,
            int instanceStates, byte[] queryExpr, byte[][] params) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.datareaderCreateQueryCondition(h, sampleStates, viewStates,
                instanceStates, queryExpr, params, params.length, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new QueryCondition(out[0]);
    }

    /** Size in bytes of one native instance handle, as used by the matched-endpoint handle-list call. */
    private static final int HANDLE_SIZE = 16;

    /**
     * Lists the publications currently matched to this reader: a
     * handle-list-then-per-handle-lookup call, the same shape as {@link
     * DomainParticipant#getDiscoveredParticipants()}. First the matched
     * publications' instance handles are collected (grow-and-retry on {@code
     * byte[]} capacity, since the native call reports the TRUE total even
     * when it exceeds what was copied), then each handle is resolved to a
     * {@link PublicationBuiltinTopicData} and materialized.
     */
    public List<PublicationBuiltinTopicData> getMatchedPublications() {
        long h = handle();
        int capacity = 8;
        byte[] handles = new byte[capacity * HANDLE_SIZE];
        int count;
        while (true) {
            long[] countOut = new long[1];
            int rc = FfiAccess.getMatchedPublications(h, handles, capacity, countOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            count = (int) countOut[0];
            if (count <= capacity) {
                break;
            }
            capacity = count;
            handles = new byte[capacity * HANDLE_SIZE];
        }

        List<PublicationBuiltinTopicData> out = new ArrayList<PublicationBuiltinTopicData>();
        for (int i = 0; i < count; i++) {
            byte[] handle = Arrays.copyOfRange(handles, i * HANDLE_SIZE, (i + 1) * HANDLE_SIZE);
            long[] dataOut = new long[1];
            int rc = FfiAccess.getMatchedPublicationData(h, handle, dataOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            long data = dataOut[0];
            try {
                out.add(PublicationBuiltinTopicData.materialize(data));
            } finally {
                FfiAccess.pubDataDestroy(data);
            }
        }
        return out;
    }

    /**
     * Blocks until historical data (relevant under transient/persistent
     * durability) has been received, or {@code timeoutMillis} elapses.
     * Returns {@code true} if historical data arrived, {@code false} if the
     * timeout expired first.
     *
     * <p><b>Not yet supported by the core:</b> the native implementation
     * currently returns {@code UNSUPPORTED} unconditionally, so this method
     * throws {@link com.intellectus.int2dds.exceptions.DdsException} until the
     * core implements durability-aware historical delivery. The wrapper is in
     * place so callers work unchanged once it does.
     */
    public boolean waitForHistoricalData(long timeoutMillis) {
        int rc = FfiAccess.datareaderWaitForHistoricalData(handle(), timeoutMillis);
        NativeKeepAlive.keepAlive(this);
        if (rc == DdsException.RET_TIMEOUT) {
            return false;
        }
        ReturnCodes.check(rc);
        return true;
    }

    /**
     * Looks up the instance handle for {@code sample}'s key, without taking
     * or reading it. Serializes {@code sample} the same way {@link
     * DataWriter#write} does and hands the buffer to the core as the lookup
     * key. Requires a keyed topic.
     *
     * <p><b>Currently always returns the NIL handle in practice.</b> Unlike
     * the writer-side lookup ({@link DataWriter#lookupInstance}), which
     * projects the key fields out of the full sample before comparing, the
     * native reader-side lookup ({@code int2dds_datareader_lookup_instance})
     * does a raw byte-compare of the full serialized sample this method
     * passes against each instance's stored key, which the core keeps as the
     * canonical key-only CDR (not the full sample) — see
     * {@code dds/src/dcps/subscription/data_reader.rs}'s
     * {@code cache_change_received}. For any type with fields beyond the key
     * (including every generated type in this codebase) those two byte
     * strings never match, so this returns {@code new InstanceHandle(new
     * byte[16])} (NIL) every time, even for an instance the reader has
     * actually seen. Use {@link DataWriter#lookupInstance} when a real
     * handle is needed; this method is kept for API symmetry with the writer
     * side and in case the core's reader-side lookup is later made to
     * project the key the same way.
     *
     * @throws NullPointerException if {@code sample} is null
     */
    public InstanceHandle lookupInstance(T sample) {
        Objects.requireNonNull(sample, "sample");
        long h = handle();
        byte[] handleOut = new byte[16];
        Extensibility ext = topic != null ? topic.extensibility() : cft.relatedTopic().extensibility();
        try (CdrWriter w = CdrWriter.acquire(ext, LITTLE_ENDIAN_HOST, xcdr2)) {
            sample.serializeCdr(w);
            int rc = FfiAccess.datareaderLookupInstance(h, w.address(), w.length(), handleOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        }
        return new InstanceHandle(handleOut);
    }

    /**
     * The serialized key representation for the instance {@code handle}
     * identifies, as this reader's core copy holds it: a key-only CDR the
     * core produces via its own key deserializer, not a full sample -- {@code
     * T}'s CDR codec has no key-only decoder and cannot rebuild a sample from
     * these bytes. Round-trips with {@link DataWriter#registerInstance} and
     * with an instance handle taken off a received {@link Sample}'s {@link
     * SampleInfo}, for the same logical instance.
     *
     * <p>Uses a fresh local buffer, not the reader's shared {@link #payload},
     * so this is safe to call regardless of take/read state.
     *
     * @throws NullPointerException if {@code handle} is null
     * @throws com.intellectus.int2dds.exceptions.DdsException on a nil or
     *     unknown handle
     */
    public byte[] getKeyValue(InstanceHandle handle) {
        Objects.requireNonNull(handle, "handle");
        long h = handle();
        ByteBuffer buf = ByteBuffer.allocateDirect(64).order(ByteOrder.nativeOrder());
        ByteBuffer sizeSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        while (true) {
            int rc = FfiAccess.datareaderGetKeyValue(
                    h, handle.bytes(), addr(buf), buf.capacity(), addr(sizeSlot));
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(buf);
            NativeKeepAlive.keepAlive(sizeSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                long required = sizeSlot.getLong(0);
                if (required > MAX_SAMPLE_BYTES) {
                    throw new DdsErrorException("serialized key too large: " + required + " bytes");
                }
                long cap = buf.capacity();
                while (cap < required) {
                    cap <<= 1;
                }
                buf = ByteBuffer.allocateDirect((int) cap).order(ByteOrder.nativeOrder());
                continue;
            }
            ReturnCodes.check(rc);
            break;
        }
        int n = (int) sizeSlot.getLong(0);
        ((java.nio.Buffer) buf).position(0);
        ((java.nio.Buffer) buf).limit(n);
        byte[] out = new byte[n];
        buf.get(out);
        return out;
    }

    /** Takes (removes) the next sample, or null if the cache is empty. */
    public Sample<T> take() {
        return next(true);
    }

    /** Reads (without removing) the next sample, or null if the cache is empty. */
    public Sample<T> read() {
        return next(false);
    }

    /**
     * Takes (removes) the next sample's raw CDR bytes, encapsulation header
     * included, without decoding it into {@code T}: the primitive a
     * type-agnostic gateway/bridge needs to forward a sample onward — e.g.
     * via {@link DataWriter#writeSerialized} — without knowing the compiled
     * type. Returns {@code null} if the cache is empty, or an empty {@code
     * byte[0]} for an invalid-data (dispose/unregister) sample, which is
     * still removed but carries no bytes to copy.
     */
    public byte[] takeSerialized() {
        return nextSerialized(true);
    }

    /**
     * Non-removing counterpart of {@link #takeSerialized}, same return-value
     * contract — but note the OMG {@code read_next}/{@code take_next}
     * semantics both this and {@link #takeSerialized} follow: each returns the
     * next <em>not-yet-read</em> sample and marks it read. Reading a sample
     * therefore consumes it from that frontier — a later {@code readSerialized}
     * or {@code takeSerialized} will not return the same sample again (it is
     * already read), so this is not a peek you can re-take afterwards.
     */
    public byte[] readSerialized() {
        return nextSerialized(false);
    }

    private byte[] nextSerialized(boolean take) {
        long h = handle();
        while (true) {
            int rc = take
                    ? FfiAccess.datareaderTakeSerialized(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(validSlot))
                    : FfiAccess.datareaderReadSerialized(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(validSlot));
            // handle() and the three buffer addresses were consumed by the
            // native call above; keep them all reachable across it -- same
            // reasoning as next()'s identical fence.
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(payload);
            NativeKeepAlive.keepAlive(sizeSlot);
            NativeKeepAlive.keepAlive(validSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                growPayload(sizeSlot.getLong(0));
                continue;
            }
            if (!ReturnCodes.checkOrNoData(rc)) {
                return null;   // NO_DATA: nothing in the cache.
            }
            boolean validData = validSlot.get(0) != 0;
            if (!validData) {
                return new byte[0];
            }
            int n = (int) sizeSlot.getLong(0);
            ((java.nio.Buffer) payload).position(0);
            ((java.nio.Buffer) payload).limit(n);
            byte[] out = new byte[n];
            payload.get(out);
            ((java.nio.Buffer) payload).clear();
            return out;
        }
    }

    /**
     * State-filtered counterpart of {@link #takeSerialized()}: takes (removes)
     * the next sample matching {@code sampleStateMask}/{@code
     * viewStateMask}/{@code instanceStateMask} as its raw CDR bytes plus its
     * {@link SampleInfo}. Masks are a bitwise-OR of the constants in {@link
     * com.intellectus.int2dds.conditions.SampleState}, {@link
     * com.intellectus.int2dds.conditions.ViewState} and {@link
     * com.intellectus.int2dds.conditions.InstanceState} (e.g. {@code
     * SampleState.ANY, ViewState.ANY, InstanceState.ANY} for "any state").
     * Unlike the no-arg {@link #takeSerialized()} (NOT_READ-only), this can
     * retrieve an already-READ sample via {@code SampleState.READ}. Returns
     * {@code null} if nothing in the cache matches the masks.
     */
    public SerializedSample takeSerialized(int sampleStateMask, int viewStateMask, int instanceStateMask) {
        return nextSerializedWStates(true, sampleStateMask, viewStateMask, instanceStateMask);
    }

    /**
     * Non-removing counterpart of {@link #takeSerialized(int, int, int)}.
     * Same mask semantics; unlike the no-arg {@link #readSerialized()}
     * (NOT_READ-only), this can retrieve an already-READ sample via {@code
     * SampleState.READ}.
     */
    public SerializedSample readSerialized(int sampleStateMask, int viewStateMask, int instanceStateMask) {
        return nextSerializedWStates(false, sampleStateMask, viewStateMask, instanceStateMask);
    }

    private SerializedSample nextSerializedWStates(
            boolean take, int sampleStateMask, int viewStateMask, int instanceStateMask) {
        long h = handle();
        while (true) {
            int rc = take
                    ? FfiAccess.datareaderTakeSerializedWStates(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(infoSlot), sampleStateMask, viewStateMask, instanceStateMask)
                    : FfiAccess.datareaderReadSerializedWStates(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(infoSlot), sampleStateMask, viewStateMask, instanceStateMask);
            // handle() and the three buffer addresses were consumed by the
            // native call above; keep them all reachable across it -- same
            // reasoning as next()'s identical fence.
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(payload);
            NativeKeepAlive.keepAlive(sizeSlot);
            NativeKeepAlive.keepAlive(infoSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                growPayload(sizeSlot.getLong(0));
                continue;
            }
            if (!ReturnCodes.checkOrNoData(rc)) {
                return null;   // NO_DATA: nothing matches the state masks.
            }
            SampleInfo info = SampleInfo.decode(infoSlot);
            int n = (int) sizeSlot.getLong(0);
            byte[] b;
            if (info.validData()) {
                ((java.nio.Buffer) payload).position(0);
                ((java.nio.Buffer) payload).limit(n);
                b = new byte[n];
                payload.get(b);
                ((java.nio.Buffer) payload).clear();
            } else {
                b = new byte[0];
            }
            return new SerializedSample(b, info);
        }
    }

    /**
     * Takes (removes) up to {@code maxSamples} samples' raw CDR bytes plus
     * {@link SampleInfo} in a single native call -- the batch counterpart of
     * {@link #takeSerialized()}. Returns an empty list when the cache has
     * nothing; otherwise up to {@code maxSamples} elements in delivery order.
     * The native sample sequence backing the batch is freed before this
     * method returns.
     *
     * <p><b>Unlike</b> the no-arg {@link #takeSerialized()} (NOT_READ-only,
     * {@code take_next_sample} scoping), this and {@link #readSerializedBatch}
     * match sample/view/instance state {@code ANY}: an already-READ sample
     * (e.g. left behind by a prior {@link #readSerializedBatch}) is eligible
     * too, and reading still flips a sample to READ per ordinary DDS
     * semantics but does not exclude it from a later batch take.
     *
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> takeSerializedBatch(int maxSamples) {
        return nextSerializedBatch(true, maxSamples);
    }

    /**
     * Non-removing counterpart of {@link #takeSerializedBatch}. Same {@code
     * ANY} sample/view/instance state matching -- see that method's doc for
     * how this differs from the no-arg {@link #readSerialized()}. A sample
     * this reads is left in the cache and is still eligible for a later
     * {@link #takeSerializedBatch} even though reading marks it READ.
     *
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> readSerializedBatch(int maxSamples) {
        return nextSerializedBatch(false, maxSamples);
    }

    private List<SerializedSample> nextSerializedBatch(boolean take, int maxSamples) {
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long h = handle();
        long[] seqOut = new long[1];
        int rc = take
                ? FfiAccess.datareaderTakeSerializedBatch(h, maxSamples, seqOut)
                : FfiAccess.datareaderReadSerializedBatch(h, maxSamples, seqOut);
        NativeKeepAlive.keepAlive(this);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * State-filtered counterpart of {@link #takeSerializedBatch(int)}: takes
     * (removes) up to {@code maxSamples} samples matching {@code
     * sampleStateMask}/{@code viewStateMask}/{@code instanceStateMask} as a
     * single native batch call. Masks are a bitwise-OR of the constants in
     * {@link com.intellectus.int2dds.conditions.SampleState}, {@link
     * com.intellectus.int2dds.conditions.ViewState} and {@link
     * com.intellectus.int2dds.conditions.InstanceState} (e.g. {@code
     * SampleState.ANY, ViewState.ANY, InstanceState.ANY} to match the
     * no-arg {@link #takeSerializedBatch(int)}'s own {@code ANY} scoping --
     * see that method's doc for how batch matching differs from the no-arg
     * single-sample {@link #takeSerialized()}). Returns an empty list when
     * nothing in the cache matches the masks. The native sample sequence
     * backing the batch is freed before this method returns.
     *
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> takeSerializedBatch(
            int maxSamples, int sampleStateMask, int viewStateMask, int instanceStateMask) {
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long h = handle();
        long[] seqOut = new long[1];
        int rc = FfiAccess.datareaderTakeSerializedBatchWStates(
                h, maxSamples, seqOut, sampleStateMask, viewStateMask, instanceStateMask);
        NativeKeepAlive.keepAlive(this);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * Non-removing counterpart of {@link #takeSerializedBatch(int, int, int,
     * int)}. Same mask semantics.
     *
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> readSerializedBatch(
            int maxSamples, int sampleStateMask, int viewStateMask, int instanceStateMask) {
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long h = handle();
        long[] seqOut = new long[1];
        int rc = FfiAccess.datareaderReadSerializedBatchWStates(
                h, maxSamples, seqOut, sampleStateMask, viewStateMask, instanceStateMask);
        NativeKeepAlive.keepAlive(this);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * Instance-scoped counterpart of {@link #takeSerializedBatch(int, int,
     * int, int)}: takes (removes) up to {@code maxSamples} samples belonging
     * only to the instance {@code instance} identifies, filtered by the same
     * state masks. Samples of any other instance are left untouched in the
     * cache. {@code instance} typically comes from a received sample's
     * {@link SampleInfo#instanceHandle()} (wrapped in an {@link
     * InstanceHandle}), or from {@link DataWriter#registerInstance}/{@link
     * #lookupInstance}; meaningful only on a keyed topic, since an unkeyed
     * topic has a single (NIL) instance.
     *
     * @throws NullPointerException if {@code instance} is null
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> takeInstanceSerializedBatch(InstanceHandle instance,
            int maxSamples, int sampleStateMask, int viewStateMask, int instanceStateMask) {
        Objects.requireNonNull(instance, "instance");
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long h = handle();
        long[] seqOut = new long[1];
        int rc = FfiAccess.datareaderTakeInstanceSerializedBatch(h, instance.bytes(), maxSamples,
                sampleStateMask, viewStateMask, instanceStateMask, seqOut);
        NativeKeepAlive.keepAlive(this);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * Non-removing counterpart of {@link #takeInstanceSerializedBatch}. Same
     * instance-scoping and mask semantics.
     *
     * @throws NullPointerException if {@code instance} is null
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> readInstanceSerializedBatch(InstanceHandle instance,
            int maxSamples, int sampleStateMask, int viewStateMask, int instanceStateMask) {
        Objects.requireNonNull(instance, "instance");
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long h = handle();
        long[] seqOut = new long[1];
        int rc = FfiAccess.datareaderReadInstanceSerializedBatch(h, instance.bytes(), maxSamples,
                sampleStateMask, viewStateMask, instanceStateMask, seqOut);
        NativeKeepAlive.keepAlive(this);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * ReadCondition/QueryCondition-filtered counterpart of {@link
     * #takeSerializedBatch(int)}: takes (removes) up to {@code maxSamples}
     * samples matching {@code condition} as a single native batch call.
     * {@code condition} may be a plain {@link ReadCondition} (state masks
     * only) or a {@link QueryCondition} (state masks plus a content filter),
     * since {@code QueryCondition} extends {@code ReadCondition}. See {@link
     * #takeSerializedBatch(int)} for how batch matching differs from the
     * no-arg single-sample {@link #takeSerialized()}. Returns an empty list
     * when nothing in the cache matches. The native sample sequence backing
     * the batch is freed before this method returns.
     *
     * <p>{@code condition} must have been created from this same reader
     * (e.g. via {@link #createReadCondition} or {@link
     * #createQueryCondition}); passing a condition from another reader is
     * undefined.
     *
     * @throws NullPointerException if {@code condition} is null
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> takeSerializedBatch(ReadCondition condition, int maxSamples) {
        Objects.requireNonNull(condition, "condition");
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long condH = ConditionHandleAccess.handle(condition);
        long[] seqOut = new long[1];
        int rc = FfiAccess.datareaderTakeSerializedBatchWReadCondition(
                handle(), condH, maxSamples, seqOut);
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(condition);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * Non-removing counterpart of {@link #takeSerializedBatch(ReadCondition,
     * int)}. Same condition-matching semantics.
     *
     * @throws NullPointerException if {@code condition} is null
     * @throws IllegalArgumentException if {@code maxSamples <= 0}
     */
    public List<SerializedSample> readSerializedBatch(ReadCondition condition, int maxSamples) {
        Objects.requireNonNull(condition, "condition");
        if (maxSamples <= 0) {
            throw new IllegalArgumentException("maxSamples must be > 0: " + maxSamples);
        }
        long condH = ConditionHandleAccess.handle(condition);
        long[] seqOut = new long[1];
        int rc = FfiAccess.datareaderReadSerializedBatchWReadCondition(
                handle(), condH, maxSamples, seqOut);
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(condition);
        return drainSeq(rc, seqOut[0]);
    }

    /**
     * Drains a native sample sequence returned by a batch take/read into a
     * {@link SerializedSample} list, freeing the sequence before returning
     * -- shared by every batch variant ({@link #nextSerializedBatch} and the
     * state-filtered/instance-scoped methods above). {@code rc} is the
     * status the batch take/read call itself returned; {@code seq} is the
     * sequence handle it wrote to its {@code seqOut} slot, valid on both
     * {@code RET_OK} and {@code RET_NO_DATA} (an empty sequence on the
     * latter, still requiring the same free).
     */
    private List<SerializedSample> drainSeq(int rc, long seq) {
        if (!ReturnCodes.checkOrNoData(rc)) {
            // NO_DATA: the native side still allocated an empty sequence.
            FfiAccess.sampleSeqDelete(seq);
            return new ArrayList<SerializedSample>();
        }
        try {
            long len = FfiAccess.sampleSeqLength(seq);
            List<SerializedSample> result = new ArrayList<SerializedSample>((int) len);
            for (long i = 0; i < len; i++) {
                int infoRc = FfiAccess.sampleSeqGetInfo(seq, i, addr(infoSlot));
                NativeKeepAlive.keepAlive(infoSlot);
                ReturnCodes.check(infoRc);
                SampleInfo info = SampleInfo.decode(infoSlot);
                byte[] b = info.validData() ? copySampleSeqData(seq, i) : new byte[0];
                result.add(new SerializedSample(b, info));
            }
            return result;
        } finally {
            FfiAccess.sampleSeqDelete(seq);
        }
    }

    /** Copies sample {@code index}'s bytes out of {@code seq} into {@link #payload}, growing on BUFFER_TOO_SMALL. */
    private byte[] copySampleSeqData(long seq, long index) {
        while (true) {
            int rc = FfiAccess.sampleSeqGetData(
                    seq, index, addr(payload), payload.capacity(), addr(sizeSlot));
            NativeKeepAlive.keepAlive(payload);
            NativeKeepAlive.keepAlive(sizeSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                growPayload(sizeSlot.getLong(0));
                continue;
            }
            ReturnCodes.check(rc);
            int n = (int) sizeSlot.getLong(0);
            ((java.nio.Buffer) payload).position(0);
            ((java.nio.Buffer) payload).limit(n);
            byte[] out = new byte[n];
            payload.get(out);
            ((java.nio.Buffer) payload).clear();
            return out;
        }
    }

    private Sample<T> next(boolean take) {
        long h = handle();
        // The core removes the sample only when it fits, so on BUFFER_TOO_SMALL
        // we can grow to the required size and retry without losing data.
        while (true) {
            int rc = take
                    ? FfiAccess.datareaderTakeSerializedWInfo(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(infoSlot))
                    : FfiAccess.datareaderReadSerializedWInfo(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(infoSlot));
            // handle() and the three buffer addresses were consumed by the
            // native call above; keep them all reachable across it.
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(payload);
            NativeKeepAlive.keepAlive(sizeSlot);
            NativeKeepAlive.keepAlive(infoSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                growPayload(sizeSlot.getLong(0));
                continue;
            }
            if (!ReturnCodes.checkOrNoData(rc)) {
                return null;   // NO_DATA: nothing in the cache.
            }
            return decodeSample();
        }
    }

    private Sample<T> decodeSample() {
        SampleInfo info = SampleInfo.decode(infoSlot);
        if (!info.validData()) {
            return new Sample<T>(null, info);
        }
        int n = (int) sizeSlot.getLong(0);
        ((java.nio.Buffer) payload).position(0);
        ((java.nio.Buffer) payload).limit(n);
        CdrReader reader = CdrReader.of(payload);
        T data = factory.get();
        data.deserializeCdr(reader);
        ((java.nio.Buffer) payload).clear();
        return new Sample<T>(data, info);
    }

    private void growPayload(long required) {
        if (required > MAX_SAMPLE_BYTES) {
            throw new DdsErrorException("serialized sample too large: " + required + " bytes");
        }
        long cap = payload.capacity();
        while (cap < required) {
            cap <<= 1;
        }
        payload = ByteBuffer.allocateDirect((int) cap).order(ByteOrder.nativeOrder());
    }

    private static long addr(ByteBuffer b) {
        return FfiAccess.directBufferAddress(b);
    }

    private static long create(Subscriber subscriber, Topic<?> topic, DataReaderQos qos) {
        if (qos == null) {
            return createNative(subscriber, topic, 0L);
        }
        long qosHandle = FfiAccess.createDataReaderQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataReaderQos handle for datareader creation");
        }
        try {
            QosMarshal.applyReaderQos(qosHandle, qos);
            return createNative(subscriber, topic, qosHandle);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    private static long createNative(Subscriber subscriber, Topic<?> topic, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createDataReader(
                subscriber.handle(), topic.handle(), qos, 0L, 0, handleOut);
        NativeKeepAlive.keepAlive(subscriber);
        NativeKeepAlive.keepAlive(topic);
        ReturnCodes.check(rc);
        return handleOut[0];
    }

    /**
     * The profile-create path: {@code listener=0L, mask=0} (no creation-time
     * listener), same keep-alive/check shape as {@link #createNative}.
     */
    private static long createWithProfile(Subscriber subscriber, Topic<?> topic, String profilePath) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createDataReaderWithProfile(subscriber.handle(), topic.handle(),
                profilePath.getBytes(UTF8), 0L, 0, handleOut);
        NativeKeepAlive.keepAlive(subscriber);
        NativeKeepAlive.keepAlive(topic);
        ReturnCodes.check(rc);
        return handleOut[0];
    }

    private static long createCft(Subscriber subscriber, ContentFilteredTopic<?> cft, DataReaderQos qos) {
        if (qos == null) {
            return createNativeCft(subscriber, cft, 0L);
        }
        long qosHandle = FfiAccess.createDataReaderQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataReaderQos handle for CFT datareader creation");
        }
        try {
            QosMarshal.applyReaderQos(qosHandle, qos);
            return createNativeCft(subscriber, cft, qosHandle);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    private static long createNativeCft(Subscriber subscriber, ContentFilteredTopic<?> cft, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createDataReaderCft(
                subscriber.handle(), cft.handle(), qos, 0L, 0, handleOut);
        NativeKeepAlive.keepAlive(subscriber);
        NativeKeepAlive.keepAlive(cft);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
