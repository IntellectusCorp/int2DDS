package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.conditions.QueryCondition;
import com.intellectus.int2dds.conditions.ReadCondition;
import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.listeners.DataReaderListener;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.status.SubscriptionMatchedStatus;
import com.intellectus.int2dds.types.IDdsType;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
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

    private static final int DEFAULT_CAPACITY = 4096;

    /** Largest power-of-two direct buffer size an int capacity can hold (2^30). */
    private static final long MAX_SAMPLE_BYTES = 1 << 30;

    private final Topic<T> topic;
    private final Supplier<T> factory;

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

    DataReader(Subscriber subscriber, Topic<T> topic, Supplier<T> factory, DataReaderQos qos) {
        super(Objects.requireNonNull(subscriber, "subscriber"),
                create(subscriber, Objects.requireNonNull(topic, "topic"), qos),
                FfiAccess::deleteDataReader);
        this.topic = topic;
        this.factory = Objects.requireNonNull(factory, "factory");
    }

    /** The topic this reader receives from — the same instance passed to {@code createDataReader}. */
    public Topic<T> topic() {
        return topic;
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

    /** Takes (removes) the next sample, or null if the cache is empty. */
    public Sample<T> take() {
        return next(true);
    }

    /** Reads (without removing) the next sample, or null if the cache is empty. */
    public Sample<T> read() {
        return next(false);
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
}
