package com.intellectus.int2dds.core;

import com.intellectus.int2dds.discovery.ParticipantBuiltinTopicData;
import com.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import com.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.ParticipantQos;
import com.intellectus.int2dds.qos.PublisherQos;
import com.intellectus.int2dds.qos.SubscriberQos;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.types.IDdsType;
import com.intellectus.int2dds.xtypes.DynamicData;
import com.intellectus.int2dds.xtypes.DynamicTopic;
import com.intellectus.int2dds.xtypes.DynamicTypeSupport;
import com.intellectus.int2dds.xtypes.TypeObject;
import java.nio.Buffer;
import java.nio.ByteBuffer;
import java.nio.charset.Charset;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * The local application's membership in a DDS domain, and the factory for
 * {@code Topic} and {@code Publisher} (and, once it exists, {@code
 * DataWriter}).
 *
 * <p>A root {@link NativeEntity}: {@link #parent()} is always null, and
 * {@link #close()} — inherited unchanged — closes every live child of this
 * participant before releasing the participant itself.
 */
public final class DomainParticipant extends NativeEntity {

    private static final Charset UTF8 = Charset.forName("UTF-8");

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
     * Creates a topic named {@code name} for {@code prototype}'s type, with
     * the core's default QoS. {@code prototype} supplies the type name and
     * extensibility that {@link Topic#typeName()} / {@link
     * Topic#extensibility()} report afterward — an instance rather than
     * {@code Class<T>}, since {@code Class.newInstance()} is both deprecated
     * and unnecessary here.
     */
    public <T extends IDdsType> Topic<T> createTopic(String name, T prototype) {
        return new Topic<T>(this, name, prototype, null);
    }

    /** Creates a topic named {@code name} for {@code prototype}'s type, with an explicit QoS. */
    public <T extends IDdsType> Topic<T> createTopic(String name, T prototype, TopicQos qos) {
        return new Topic<T>(this, name, prototype, Objects.requireNonNull(qos, "qos"));
    }

    /**
     * Creates a topic named {@code name} backed by {@code support} (an
     * XTypes dynamic type support, from {@link
     * com.intellectus.int2dds.xtypes.XmlTypeRegistry#getTypeSupport}) rather
     * than a generated {@link IDdsType}, with the core's default QoS.
     */
    public DynamicTopic createDynamicTopic(String name, DynamicTypeSupport support) {
        Objects.requireNonNull(name, "name");
        Objects.requireNonNull(support, "support");
        long h = handle();
        long s = support.handle();
        long[] out = new long[1];
        int rc = FfiAccess.createTopicDynamic(h, name.getBytes(UTF8), s, out);
        // h and s are bare longs read moments before the native call that
        // consumes them; keep this participant and support reachable across
        // it -- see NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(support);
        ReturnCodes.check(rc);
        return DynamicTopic.fromHandle(out[0]);
    }

    /** Creates a publisher with the core's default QoS. */
    public Publisher createPublisher() {
        return new Publisher(this, null);
    }

    /** Creates a publisher with an explicit QoS. */
    public Publisher createPublisher(PublisherQos qos) {
        return new Publisher(this, Objects.requireNonNull(qos, "qos"));
    }

    /** Creates a subscriber with the core's default QoS. */
    public Subscriber createSubscriber() {
        return new Subscriber(this, (SubscriberQos) null);
    }

    /** Creates a subscriber with an explicit QoS. */
    public Subscriber createSubscriber(SubscriberQos qos) {
        return new Subscriber(this, Objects.requireNonNull(qos, "qos"));
    }

    /**
     * Takes a snapshot of currently discovered (alive) publications, blocking
     * up to {@code timeoutMillis} (negative = infinite) for the builtin
     * DCPSPublication reader to have data. Discovery is asynchronous, so an
     * empty list shortly after matching entities are created does not mean
     * discovery failed -- callers on a bounded budget should retry.
     *
     * <p>Materializes each entry into an immutable {@link
     * PublicationBuiltinTopicData} before releasing the native snapshot: the
     * per-entry box the core mints (a clone of its stored data) is destroyed,
     * and the snapshot sequence itself deleted, in {@code finally} blocks, so
     * an exception mid-read still frees everything already allocated.
     */
    public List<PublicationBuiltinTopicData> takeDiscoveredPublications(long timeoutMillis) {
        long h = handle();
        long[] seqOut = new long[1];
        int rc = FfiAccess.takeDiscoveredPublicationsSnapshot(h, (int) timeoutMillis, seqOut);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long seq = seqOut[0];
        List<PublicationBuiltinTopicData> out = new ArrayList<PublicationBuiltinTopicData>();
        try {
            long[] lenOut = new long[1];
            ReturnCodes.check(FfiAccess.pubDataSeqLength(seq, lenOut));
            int n = (int) lenOut[0];
            for (int i = 0; i < n; i++) {
                long[] dataOut = new long[1];
                ReturnCodes.check(FfiAccess.pubDataSeqGet(seq, i, dataOut));
                long data = dataOut[0];
                try {
                    out.add(PublicationBuiltinTopicData.materialize(data));
                } finally {
                    FfiAccess.pubDataDestroy(data);
                }
            }
        } finally {
            FfiAccess.pubDataSeqDelete(seq);
        }
        return out;
    }

    /**
     * Takes a snapshot of currently discovered (alive) subscriptions, blocking
     * up to {@code timeoutMillis} (negative = infinite) for the builtin
     * DCPSSubscription reader to have data. Discovery is asynchronous, so an
     * empty list shortly after matching entities are created does not mean
     * discovery failed -- callers on a bounded budget should retry.
     *
     * <p>Materializes each entry into an immutable {@link
     * SubscriptionBuiltinTopicData} before releasing the native snapshot: the
     * per-entry box the core mints (a clone of its stored data) is destroyed,
     * and the snapshot sequence itself deleted, in {@code finally} blocks, so
     * an exception mid-read still frees everything already allocated.
     */
    public List<SubscriptionBuiltinTopicData> takeDiscoveredSubscriptions(long timeoutMillis) {
        long h = handle();
        long[] seqOut = new long[1];
        int rc = FfiAccess.takeDiscoveredSubscriptionsSnapshot(h, (int) timeoutMillis, seqOut);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long seq = seqOut[0];
        List<SubscriptionBuiltinTopicData> out = new ArrayList<SubscriptionBuiltinTopicData>();
        try {
            long[] lenOut = new long[1];
            ReturnCodes.check(FfiAccess.subDataSeqLength(seq, lenOut));
            int n = (int) lenOut[0];
            for (int i = 0; i < n; i++) {
                long[] dataOut = new long[1];
                ReturnCodes.check(FfiAccess.subDataSeqGet(seq, i, dataOut));
                long data = dataOut[0];
                try {
                    out.add(SubscriptionBuiltinTopicData.materialize(data));
                } finally {
                    FfiAccess.subDataDestroy(data);
                }
            }
        } finally {
            FfiAccess.subDataSeqDelete(seq);
        }
        return out;
    }

    /** Size in bytes of one native instance handle, as used by the discovery handle-list calls. */
    private static final int HANDLE_SIZE = 16;

    /**
     * Lists currently discovered (alive) remote participants. Unlike {@link
     * #takeDiscoveredPublications}/{@link #takeDiscoveredSubscriptions}, this
     * is a handle-list-then-per-handle-lookup call, not a snapshot sequence:
     * first the discovered participants' instance handles are collected (grow-
     * and-retry on {@code byte[]} capacity, since the native call reports the
     * TRUE total even when it exceeds what was copied), then each handle is
     * resolved to a {@link ParticipantBuiltinTopicData} and materialized.
     *
     * <p>This is also the path that exercises the JNI codegen fix for
     * caller-provided sized array out-params ({@code
     * int2dds_participant_get_discovered_participants}): with two or more
     * discovered participants, the native shim must size its buffer from the
     * Java {@code byte[]}'s actual length rather than a fixed 16-byte stack
     * buffer.
     */
    public List<ParticipantBuiltinTopicData> getDiscoveredParticipants() {
        long h = handle();
        int capacity = 8;
        byte[] handles = new byte[capacity * HANDLE_SIZE];
        int count;
        while (true) {
            long[] countOut = new long[1];
            int rc = FfiAccess.getDiscoveredParticipants(h, handles, capacity, countOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            count = (int) countOut[0];
            if (count <= capacity) {
                break;
            }
            capacity = count;
            handles = new byte[capacity * HANDLE_SIZE];
        }

        List<ParticipantBuiltinTopicData> out = new ArrayList<ParticipantBuiltinTopicData>();
        for (int i = 0; i < count; i++) {
            byte[] handle = Arrays.copyOfRange(handles, i * HANDLE_SIZE, (i + 1) * HANDLE_SIZE);
            long[] dataOut = new long[1];
            int rc = FfiAccess.getDiscoveredParticipantData(h, handle, dataOut);
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
            long data = dataOut[0];
            try {
                out.add(ParticipantBuiltinTopicData.materialize(data));
            } finally {
                FfiAccess.participantDataDestroy(data);
            }
        }
        return out;
    }

    /**
     * Decodes {@code serialized} (a full serialized sample, encapsulation
     * header included, e.g. {@link com.intellectus.int2dds.cdr.CdrWriter}'s
     * output) against {@code type} into a live {@link DynamicData}. {@code
     * serialized} is copied into a direct buffer so its address can cross the
     * FFI boundary the same way {@code writer.write(sample)} does.
     */
    public DynamicData dynamicDataFromSample(byte[] serialized, TypeObject type) {
        long h = handle();
        ByteBuffer buf = ByteBuffer.allocateDirect(serialized.length);
        buf.put(serialized);
        ((Buffer) buf).position(0);
        long[] out = new long[1];
        int rc = FfiAccess.dynamicDataFromSample(
                h, FfiAccess.directBufferAddress(buf), serialized.length, type.handle(), out);
        // buf is read by the native call only through the address handed to
        // it above; without this fence the JIT could treat buf as dead before
        // that call finishes using it. Same hazard FfiAccess's own bridges
        // guard against -- see NativeKeepAlive's doc for the full argument.
        // type is fenced too: type.handle() feeds type_obj, which the native
        // call dereferences, so the reaper could otherwise free the
        // TypeObject mid-call -- the same two-handle fencing DataWriter's
        // createNative applies to both publisher and topic.
        NativeKeepAlive.keepAlive(buf);
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(type);
        ReturnCodes.check(rc);
        return DynamicData.fromHandle(out[0]);
    }

    /**
     * Applies {@code qos} to this participant at runtime. Builds a native
     * {@link ParticipantQos} handle, applies {@code qos}'s policies onto it,
     * and destroys it again once the native {@code set_qos} call returns —
     * success or failure, thrown or not, the same build-apply-destroy shape
     * {@link #createWithQos} uses for the create path. {@code ParticipantQos}
     * declares no immutable policies in this core, so a change to {@code
     * Property} succeeds regardless of whether this participant is already
     * enabled; {@code UserData} is this type's one unsupported policy --
     * rejected outright, mutable or not -- see {@code
     * check_unsupported_policies} in dds/src/dcps/domain/qos/mod.rs.
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(ParticipantQos qos) {
        Objects.requireNonNull(qos, "qos");
        long qosHandle = FfiAccess.createParticipantQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native ParticipantQos handle for setQos");
        }
        try {
            QosMarshal.applyParticipantQos(qosHandle, qos);
            int rc = FfiAccess.participantSetQos(handle(), qosHandle);
            // handle() is a bare long read moments before the native call
            // that consumes it; keep this participant reachable across it --
            // see NativeKeepAlive's own doc for the full argument.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        } finally {
            FfiAccess.destroyParticipantQos(qosHandle);
        }
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
