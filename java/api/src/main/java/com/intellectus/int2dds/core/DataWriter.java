package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.listeners.DataWriterListener;
import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.status.LivelinessLostStatus;
import com.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import com.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import com.intellectus.int2dds.status.OfferedIncompatibleTypeStatus;
import com.intellectus.int2dds.status.PublicationMatchedStatus;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.IDdsType;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * Publishes samples of type {@code T} to a {@link Topic}.
 *
 * <p>Created through {@link Publisher#createDataWriter}, which registers
 * this instance in the publisher's weak child list — matching the C#
 * reference binding's shape, where {@code DataWriter}'s constructor
 * (csharp/src/Int2Dds/Core/DataWriter.cs:33) is internal to the assembly
 * rather than public; this one is package-private for the same reason.
 *
 * <p>{@link #write} is this branch's first real use of the CDR layer: it
 * borrows a pooled direct {@link CdrWriter}, has {@code T} serialize into it,
 * and hands the writer's address and length straight to the C ABI. Nothing
 * is allocated and nothing is copied — the reason {@link IDdsType} takes a
 * writer instead of returning a {@code byte[]}, and the reason {@code
 * CdrWriter}'s buffer is direct.
 *
 * <p><b>Listener lifecycle:</b> a listener installed with {@link #setListener}
 * is held by a binding-owned native context whose pointer this writer tracks.
 * {@link NativeEntity#close()} is final and does not clear it, so a caller that
 * installed a listener must call {@code setListener(null, null)} before closing
 * to release that context; automatic teardown is deferred to a later branch.
 *
 * @param <T> the DDS data type this writer publishes.
 */
public final class DataWriter<T extends IDdsType> extends NativeEntity {

    private static final boolean LITTLE_ENDIAN_HOST =
            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN;

    /** Largest power-of-two direct buffer size an int capacity can hold (2^30); see {@link #getKeyValue}. */
    private static final long MAX_SAMPLE_BYTES = 1 << 30;

    // Pointer to the binding-owned native listener context, or 0 when none is
    // installed. Guarded by setListener's own synchronization.
    private long listenerCtx = 0L;

    /**
     * Held strongly — not only so {@link #topic()} has something to return,
     * but because the native writer refers to this topic, so the topic must
     * not be released while this writer is in use.
     *
     * <p>This reference does not, and cannot, order the writer's and the
     * topic's own deleters relative to each other: reachability propagates,
     * so a writer and its topic that die together become phantom-reachable
     * in the very same GC pass, and nothing orders two references discovered
     * together (see {@link NativeEntity}'s and {@link NativeCleaner}'s class
     * docs for the full argument). What actually makes a reap of the topic
     * before this writer safe is the native layer's refuse-and-restore
     * contract on {@code int2dds_delete_topic}, retried with backoff by
     * {@link NativeCleaner} until this writer is gone — not the direction of
     * this field.
     */
    private final Topic<T> topic;

    /**
     * Resolved once at construction: {@code true} for XCDR2, {@code false}
     * for XCDR1. See {@link #resolveXcdr2} for how it is chosen.
     */
    private final boolean xcdr2;

    DataWriter(Publisher publisher, Topic<T> topic, DataWriterQos qos) {
        super(Objects.requireNonNull(publisher, "publisher"),
                create(publisher, Objects.requireNonNull(topic, "topic"), qos),
                FfiAccess::deleteDataWriter);
        this.topic = topic;
        this.xcdr2 = resolveXcdr2(qos);
    }

    private DataWriter(Publisher publisher, Topic<T> topic, NativeCleaner.Deleter deleter) {
        super(Objects.requireNonNull(publisher, "publisher"),
                create(publisher, Objects.requireNonNull(topic, "topic"), null),
                deleter);
        this.topic = topic;
        this.xcdr2 = resolveXcdr2(null);
    }

    /**
     * Package-private construction seam, the same pattern as {@link
     * DomainParticipant#createForTest}, {@link Topic#createForTest} and
     * {@link Publisher#createForTest}: the same default-QoS creation path as
     * the public factory, but with an explicit deleter in place of the fixed
     * {@code FfiAccess::deleteDataWriter}, for tests that need to observe
     * exactly how many times the deleter is actually invoked. A
     * test-supplied deleter should still delegate to the real delete — this
     * seam changes who gets to count the calls, not what actually happens to
     * the underlying native writer.
     */
    static <T extends IDdsType> DataWriter<T> createForTest(
            Publisher publisher, Topic<T> topic, NativeCleaner.Deleter deleter) {
        return new DataWriter<T>(publisher, topic, deleter);
    }

    /** The topic this writer publishes to — the same instance passed to {@code createDataWriter}. */
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
     * caller should {@code setListener(null, null)} before closing this writer.
     *
     * @throws com.intellectus.int2dds.exceptions.DdsException if the native
     *     clear or install fails
     */
    public synchronized void setListener(DataWriterListener listener, StatusMask mask) {
        long h = handle();
        long prev = listenerCtx;
        if (prev != 0L) {
            int rc = FfiAccess.writerListenerClear(h, prev);
            NativeKeepAlive.keepAlive(this);
            listenerCtx = 0L;
            ReturnCodes.check(rc);
        }
        if (listener != null) {
            long ctx = FfiAccess.writerListenerSet(
                    h, listener, mask == null ? StatusMask.all().bits() : mask.bits());
            NativeKeepAlive.keepAlive(this);
            if (ctx == 0L) {
                throw new DdsErrorException("failed to install DataWriter listener");
            }
            listenerCtx = ctx;
        }
    }

    /** A fresh {@link StatusCondition} for this writer's status changes. */
    public StatusCondition getStatusCondition() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.datawriterGetStatusCondition(h, out);
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
        int rc = FfiAccess.datawriterGetStatusChanges(handle(), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return StatusMask.of(out[0]);
    }

    /** This writer's 16-byte DDS entity GUID. */
    public byte[] getGuid() {
        byte[] guid = new byte[16];
        int rc = FfiAccess.datawriterGetGuid(handle(), guid);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return guid;
    }

    /**
     * This writer's PUBLICATION_MATCHED status: how many DataReaders it has
     * matched, and the change since last read. Per DDS, reading this status
     * clears its {@code *Change} fields.
     */
    public PublicationMatchedStatus getPublicationMatchedStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(32).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datawriterGetPublicationMatchedStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        int currentCount = buf.getInt(8);
        int currentCountChange = buf.getInt(12);
        byte[] lastSubscriptionHandle = new byte[16];
        for (int i = 0; i < 16; i++) {
            lastSubscriptionHandle[i] = buf.get(16 + i);
        }
        return new PublicationMatchedStatus(
                totalCount, totalCountChange, currentCount, currentCountChange,
                lastSubscriptionHandle);
    }

    /**
     * This writer's LIVELINESS_LOST status: how many times this writer
     * failed to assert its liveliness within its offered liveliness period,
     * and the change since last read. Per DDS, reading this status clears
     * its {@code *Change} field.
     */
    public LivelinessLostStatus getLivelinessLostStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datawriterGetLivelinessLostStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        return new LivelinessLostStatus(totalCount, totalCountChange);
    }

    /**
     * This writer's OFFERED_DEADLINE_MISSED status: how many times this
     * writer missed the deadline it offered for an instance, and the change
     * since last read. Per DDS, reading this status clears its {@code
     * *Change} field.
     */
    public OfferedDeadlineMissedStatus getOfferedDeadlineMissedStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(24).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datawriterGetOfferedDeadlineMissedStatus(
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
        return new OfferedDeadlineMissedStatus(totalCount, totalCountChange, lastInstanceHandle);
    }

    /**
     * This writer's OFFERED_INCOMPATIBLE_QOS status: how many times this
     * writer discovered an offered QoS incompatible with a requesting
     * reader's QoS, and the change since last read. Per DDS, reading this
     * status clears its {@code *Change} field.
     */
    public OfferedIncompatibleQosStatus getOfferedIncompatibleQosStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(16).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datawriterGetOfferedIncompatibleQosStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        int lastPolicyId = buf.getInt(8);
        int policiesCount = buf.getInt(12);
        return new OfferedIncompatibleQosStatus(
                totalCount, totalCountChange, lastPolicyId, policiesCount);
    }

    /**
     * This writer's OFFERED_INCOMPATIBLE_TYPE status: how many times this
     * writer's type was found incompatible with a requesting reader's type,
     * and the change since last read. Per DDS, reading this status clears
     * its {@code *Change} field.
     */
    public OfferedIncompatibleTypeStatus getOfferedIncompatibleTypeStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.datawriterGetOfferedIncompatibleTypeStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        return new OfferedIncompatibleTypeStatus(totalCount, totalCountChange);
    }

    /**
     * Serializes {@code sample} as CDR and publishes it.
     *
     * <p>Allocates nothing and copies nothing: {@code sample.serializeCdr}
     * writes straight into a pooled direct buffer, and only that buffer's
     * native address and length cross into the C ABI. No key is passed: the
     * core derives the instance key and KeyHash canonically from {@code data},
     * the full serialized sample this method already builds, the same way for
     * a keyed topic as an unkeyed one.
     *
     * @throws NullPointerException if {@code sample} is null
     */
    public void write(T sample) {
        Objects.requireNonNull(sample, "sample");
        // Read before the try block, not inside it: a closed writer should
        // fail here, before this method does the (wasted) work of borrowing
        // a pooled CdrWriter and serializing into it.
        long h = handle();
        try (CdrWriter w = CdrWriter.acquire(topic.extensibility(), LITTLE_ENDIAN_HOST, xcdr2)) {
            sample.serializeCdr(w);
            int rc = FfiAccess.datawriterWriteSerialized(h, w.address(), w.length());
            // `h = handle()` above this try block reads this writer's own
            // handle; nothing else in this method touches `this` again
            // before the try-with-resources closes `w`, so without this
            // fence the reaper could treat this writer as phantom-reachable
            // and race the native call above, freeing the very writer that
            // call is using -- the same hazard NativeKeepAlive's own doc
            // describes for a parent's handle read in a constructor, here
            // applied to a handle an instance method reads off itself. That
            // is exactly the canonical shape
            // java.lang.ref.Reference#reachabilityFence's own javadoc
            // illustrates (a receiver fenced inside its own instance
            // method), not a new hazard this class introduces. A fence
            // makes every use of `this` strictly before it non-eliminable,
            // so placing it here still covers the earlier `handle()` read
            // too, not just this method's tail end.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        }
    }

    /** 16 zero bytes: the NIL instance handle, used to let the core derive an instance from its key. */
    private static final byte[] NIL_HANDLE = new byte[16];

    /**
     * Registers the instance {@code sample} belongs to, returning its
     * instance handle. Serializes {@code sample} the same way {@link #write}
     * does and hands the buffer to the core as the key it derives the
     * instance from; requires a keyed topic ({@link
     * com.intellectus.int2dds.exceptions.DdsException#RET_PRECONDITION_NOT_MET}
     * on a plain topic).
     *
     * @throws NullPointerException if {@code sample} is null
     */
    public InstanceHandle registerInstance(T sample) {
        Objects.requireNonNull(sample, "sample");
        long h = handle();
        byte[] handleOut = new byte[16];
        try (CdrWriter w = CdrWriter.acquire(topic.extensibility(), LITTLE_ENDIAN_HOST, xcdr2)) {
            sample.serializeCdr(w);
            int rc = FfiAccess.datawriterRegisterInstance(h, w.address(), w.length(), handleOut);
            // Same fence as write(): h was read off this writer just above,
            // and w's address crosses into the native call below it.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        }
        return new InstanceHandle(handleOut);
    }

    /**
     * Unregisters the instance {@code sample} belongs to: this writer no
     * longer has anything to say about it, but the instance itself is not
     * disposed (a reader still sees it ALIVE until every writer unregisters
     * it, per {@code NOT_ALIVE_NO_WRITERS}). Serializes {@code sample} as the
     * key and passes a NIL handle, letting the core derive the instance from
     * the key. Requires a keyed topic.
     *
     * @throws NullPointerException if {@code sample} is null
     */
    public void unregisterInstance(T sample) {
        Objects.requireNonNull(sample, "sample");
        long h = handle();
        try (CdrWriter w = CdrWriter.acquire(topic.extensibility(), LITTLE_ENDIAN_HOST, xcdr2)) {
            sample.serializeCdr(w);
            int rc = FfiAccess.datawriterUnregisterInstance(
                    h, w.address(), w.length(), NIL_HANDLE);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        }
    }

    /**
     * Disposes the instance {@code sample} belongs to: a matched reader that
     * later takes/reads this instance observes {@code
     * instanceState() == }{@link com.intellectus.int2dds.conditions.InstanceState#NOT_ALIVE_DISPOSED}.
     * Serializes {@code sample} as the key and passes a NIL handle, letting
     * the core derive the instance from the key. Requires a keyed topic.
     *
     * @throws NullPointerException if {@code sample} is null
     */
    public void disposeInstance(T sample) {
        Objects.requireNonNull(sample, "sample");
        long h = handle();
        try (CdrWriter w = CdrWriter.acquire(topic.extensibility(), LITTLE_ENDIAN_HOST, xcdr2)) {
            sample.serializeCdr(w);
            int rc = FfiAccess.datawriterDispose(h, w.address(), w.length(), NIL_HANDLE);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        }
    }

    /**
     * Looks up the instance handle for {@code sample}'s key, without
     * registering it. Returns an {@link InstanceHandle} wrapping 16 zero
     * bytes when the instance is unknown to this writer — callers that need
     * to distinguish "unknown" from a real handle should compare against
     * {@code new InstanceHandle(new byte[16])}. Requires a keyed topic.
     *
     * @throws NullPointerException if {@code sample} is null
     */
    public InstanceHandle lookupInstance(T sample) {
        Objects.requireNonNull(sample, "sample");
        long h = handle();
        byte[] handleOut = new byte[16];
        try (CdrWriter w = CdrWriter.acquire(topic.extensibility(), LITTLE_ENDIAN_HOST, xcdr2)) {
            sample.serializeCdr(w);
            int rc = FfiAccess.datawriterLookupInstance(h, w.address(), w.length(), handleOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        }
        return new InstanceHandle(handleOut);
    }

    /**
     * The serialized key representation for the instance {@code handle}
     * identifies, as this writer's core copy holds it: a key-only CDR the
     * core produces via its own key deserializer, not a full sample -- {@code
     * T}'s CDR codec has no key-only decoder and cannot rebuild a sample from
     * these bytes. Round-trips with {@link #registerInstance} and {@link
     * #lookupInstance}.
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
            int rc = FfiAccess.datawriterGetKeyValue(h, handle.bytes(),
                    FfiAccess.directBufferAddress(buf), buf.capacity(),
                    FfiAccess.directBufferAddress(sizeSlot));
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

    /**
     * Reads this writer's current QoS off the native side — not the {@code
     * DataWriterQos} it was constructed with, which this class does not
     * retain. Policies with no native getter come back null; see {@link
     * QosMarshal#readWriterQos}'s own doc for the full list.
     */
    public DataWriterQos getQos() {
        long[] qosOut = new long[1];
        int rc = FfiAccess.getWriterQos(handle(), qosOut);
        // Same reasoning as write(): handle() reads this writer's own handle
        // right before the native call that consumes it, with nothing else
        // touching `this` in between.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long qosHandle = qosOut[0];
        try {
            return QosMarshal.readWriterQos(qosHandle);
        } finally {
            FfiAccess.destroyDataWriterQos(qosHandle);
        }
    }

    /**
     * Applies {@code qos} to this writer at runtime. Builds a native {@link
     * DataWriterQos} handle, applies {@code qos}'s policies onto it, and
     * destroys it again once the native {@code set_qos} call returns —
     * success or failure, thrown or not, the same build-apply-destroy shape
     * {@link #create} uses for the create path. The core rejects a change to
     * an immutable policy (e.g. {@code Reliability}, {@code Durability},
     * {@code History}) once this writer is enabled, surfaced here through
     * {@link ReturnCodes#check}; {@code Deadline}, {@code Lifespan}, {@code
     * UserData}, {@code OwnershipStrength} and {@code WriterDataLifecycle}
     * remain mutable. ({@code LatencyBudget} and {@code TransportPriority}
     * are rejected outright by this core regardless of mutability -- see
     * {@code check_unsupported_policies} in
     * dds/src/dcps/publication/qos/mod.rs -- so neither is a useful example
     * of a runtime-mutable policy here.)
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(DataWriterQos qos) {
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createDataWriterQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataWriterQos handle for setQos");
        }
        try {
            QosMarshal.applyWriterQos(qosHandle, qos);
            int rc = FfiAccess.datawriterSetQos(handle(), qosHandle);
            // handle() is a bare long read moments before the native call
            // that consumes it; keep this writer reachable across it -- see
            // NativeKeepAlive's own doc for the full argument.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        } finally {
            FfiAccess.destroyDataWriterQos(qosHandle);
        }
    }

    /** Size in bytes of one native instance handle, as used by the matched-endpoint handle-list call. */
    private static final int HANDLE_SIZE = 16;

    /**
     * Lists the subscriptions currently matched to this writer: a
     * handle-list-then-per-handle-lookup call, the same shape as {@link
     * DomainParticipant#getDiscoveredParticipants()}. First the matched
     * subscriptions' instance handles are collected (grow-and-retry on {@code
     * byte[]} capacity, since the native call reports the TRUE total even
     * when it exceeds what was copied), then each handle is resolved to a
     * {@link SubscriptionBuiltinTopicData} and materialized.
     */
    public List<SubscriptionBuiltinTopicData> getMatchedSubscriptions() {
        long h = handle();
        int capacity = 8;
        byte[] handles = new byte[capacity * HANDLE_SIZE];
        int count;
        while (true) {
            long[] countOut = new long[1];
            int rc = FfiAccess.getMatchedSubscriptions(h, handles, capacity, countOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            count = (int) countOut[0];
            if (count <= capacity) {
                break;
            }
            capacity = count;
            handles = new byte[capacity * HANDLE_SIZE];
        }

        List<SubscriptionBuiltinTopicData> out = new ArrayList<SubscriptionBuiltinTopicData>();
        for (int i = 0; i < count; i++) {
            byte[] handle = Arrays.copyOfRange(handles, i * HANDLE_SIZE, (i + 1) * HANDLE_SIZE);
            long[] dataOut = new long[1];
            int rc = FfiAccess.getMatchedSubscriptionData(h, handle, dataOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            long data = dataOut[0];
            try {
                out.add(SubscriptionBuiltinTopicData.materialize(data));
            } finally {
                FfiAccess.subDataDestroy(data);
            }
        }
        return out;
    }

    /**
     * Blocks until every matched reliable {@link DataReader} has acknowledged
     * all samples this writer has sent so far, or {@code timeoutMillis}
     * elapses. Returns {@code true} if acknowledgment completed, {@code
     * false} if the timeout expired first.
     */
    public boolean waitForAcknowledgments(long timeoutMillis) {
        int rc = FfiAccess.datawriterWaitForAcknowledgments(handle(), timeoutMillis);
        NativeKeepAlive.keepAlive(this);
        if (rc == DdsException.RET_TIMEOUT) {
            return false;
        }
        ReturnCodes.check(rc);
        return true;
    }

    /**
     * Manually asserts this writer's liveliness, for use with {@code
     * MANUAL_BY_TOPIC} (or {@code MANUAL_BY_PARTICIPANT}) liveliness QoS.
     */
    public void assertLiveliness() {
        int rc = FfiAccess.datawriterAssertLiveliness(handle());
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * {@code true} for XCDR2, {@code false} for XCDR1. {@code qos}'s own
     * {@link com.intellectus.int2dds.qos.DataRepresentation} wins when it was
     * set; a {@code qos} with no representation set is treated exactly like
     * {@code qos == null} — both ask the core for its compiled-in default via
     * {@link FfiAccess#defaultDataRepresentation()} rather than guessing,
     * matching the C# reference binding's {@code effectiveRepr} resolution
     * (csharp/src/Int2Dds/Core/DataWriter.cs:40-42). The Java {@code
     * DataRepresentation} policy's own no-arg-constructor default is {@code
     * XCDR1} (see that class), matching what the core itself reports here —
     * confirmed at runtime for this task, not merely read off the Rust
     * source; see the task report.
     */
    private static boolean resolveXcdr2(DataWriterQos qos) {
        DataRepresentationKind kind = (qos != null && qos.getDataRepresentation() != null)
                ? qos.getDataRepresentation().getKind()
                : DataRepresentationKind.fromValue(FfiAccess.defaultDataRepresentation());
        return kind == DataRepresentationKind.XCDR2;
    }

    /**
     * Resolves the create arguments and, when {@code qos} is supplied,
     * builds a native QoS handle, applies the policies onto it, and destroys
     * it again once the create call returns — success or failure, thrown or
     * not — the same shape as {@link Topic#create} and {@link
     * Publisher#create}. {@code qos == null} passes {@code 0L}, engaging the
     * same core-side default resolution as an explicit {@link DataWriterQos}
     * with every policy left null.
     */
    private static long create(Publisher publisher, Topic<?> topic, DataWriterQos qos) {
        if (qos == null) {
            return createNative(publisher, topic, 0L);
        }
        long qosHandle = FfiAccess.createDataWriterQos();
        if (qosHandle == 0L) {
            // Same failure-hiding hazard Topic.create and Publisher.create
            // guard against: falling through to createNative(..., 0L) here
            // would silently downgrade a QoS-allocation failure into a
            // successful default-QoS create whenever qos itself has no
            // policy set.
            throw new DdsErrorException(
                    "failed to allocate a native DataWriterQos handle for datawriter creation");
        }
        try {
            QosMarshal.applyWriterQos(qosHandle, qos);
            return createNative(publisher, topic, qosHandle);
        } finally {
            FfiAccess.destroyDataWriterQos(qosHandle);
        }
    }

    private static long createNative(Publisher publisher, Topic<?> topic, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createDataWriter(
                publisher.handle(), topic.handle(), qos, 0L, 0, handleOut);
        // publisher.handle() and topic.handle() above are bare longs,
        // disconnected from the objects that produced them the moment they
        // are read: nothing else in this method still references either
        // object, so without this the reaper could observe one as
        // phantom-reachable and race the native call above, which is still
        // using the handle that call read. See NativeKeepAlive's own doc for
        // the full argument.
        NativeKeepAlive.keepAlive(publisher);
        NativeKeepAlive.keepAlive(topic);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
