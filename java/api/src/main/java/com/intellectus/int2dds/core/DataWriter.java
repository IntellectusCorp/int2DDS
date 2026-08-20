package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.listeners.DataWriterListener;
import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.IDdsType;
import java.nio.ByteOrder;
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
