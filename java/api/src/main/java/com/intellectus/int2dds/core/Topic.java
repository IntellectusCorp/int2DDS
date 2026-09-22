package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsUnsupportedException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.status.InconsistentTopicStatus;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.IDdsType;
import com.intellectus.int2dds.xtypes.TypeInfo;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.util.Objects;

/**
 * Associates a name with a data type for publish/subscribe.
 *
 * <p>Created through {@link DomainParticipant#createTopic}, which registers
 * this instance in the participant's weak child list — matching the C#
 * reference binding's shape, where {@code Topic}'s constructor
 * (csharp/src/Int2Dds/Core/Topic.cs:29) is internal to the assembly rather
 * than public; this one is package-private for the same reason.
 *
 * @param <T> the DDS data type this topic carries. Not read reflectively —
 *     {@link #typeName()} and {@link #extensibility()} come from the
 *     prototype instance passed to {@code createTopic}, not from {@code T}
 *     itself — but it keeps a {@code Topic<Foo>} from being handed to a
 *     {@code DataWriter<Bar>} once that type exists.
 */
public final class Topic<T extends IDdsType> extends NativeEntity {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final String name;
    private final String typeName;
    private final Extensibility extensibility;

    /**
     * @param qos may be null for the core's default Topic QoS; {@link
     *     DomainParticipant#createTopic(String, IDdsType, TopicQos)} is
     *     responsible for rejecting an explicit null before reaching here.
     */
    Topic(DomainParticipant participant, String name, T prototype, TopicQos qos) {
        super(Objects.requireNonNull(participant, "participant"),
                create(participant, Objects.requireNonNull(name, "name"),
                        Objects.requireNonNull(prototype, "prototype"), qos),
                FfiAccess::deleteTopic);
        this.name = name;
        this.typeName = prototype.typeName();
        this.extensibility = prototype.extensibility();
    }

    /**
     * Package-private profile-create path, reached only through {@link
     * DomainParticipant#createTopic(String, IDdsType, String)}: a normal
     * topic whose QoS comes from the named profile at {@code profilePath} (a
     * {@code "LibraryName::ProfileName"} path previously loaded via {@link
     * DomainParticipantFactory#loadProfiles}), released the same way as the
     * default-QoS path ({@code FfiAccess::deleteTopic}).
     */
    Topic(DomainParticipant participant, String name, T prototype, String profilePath) {
        super(Objects.requireNonNull(participant, "participant"),
                createWithProfile(participant, Objects.requireNonNull(name, "name"),
                        Objects.requireNonNull(prototype, "prototype"),
                        Objects.requireNonNull(profilePath, "profilePath")),
                FfiAccess::deleteTopic);
        this.name = name;
        this.typeName = prototype.typeName();
        this.extensibility = prototype.extensibility();
    }

    private Topic(DomainParticipant participant, String name, T prototype,
            NativeCleaner.Deleter deleter) {
        super(Objects.requireNonNull(participant, "participant"),
                create(participant, Objects.requireNonNull(name, "name"),
                        Objects.requireNonNull(prototype, "prototype"), null),
                deleter);
        this.name = name;
        this.typeName = prototype.typeName();
        this.extensibility = prototype.extensibility();
    }

    /**
     * Package-private construction seam, the same pattern as {@link
     * DomainParticipant#createForTest}: the same default-QoS creation path
     * as the public factory, but with an explicit deleter in place of the
     * fixed {@code FfiAccess::deleteTopic}, for tests that need to observe
     * exactly how many times the deleter is actually invoked. A
     * test-supplied deleter should still delegate to the real delete — this
     * seam changes who gets to count the calls, not what actually happens
     * to the underlying native topic.
     */
    static <T extends IDdsType> Topic<T> createForTest(DomainParticipant participant,
            String name, T prototype, NativeCleaner.Deleter deleter) {
        return new Topic<T>(participant, name, prototype, deleter);
    }

    /**
     * Package-private construction seam for a topic whose native
     * RawTypeSupport carries explicit CDR field descriptors -- the {@code
     * int2dds_create_topic_with_field_descriptors} path -- rather than the
     * name-only registration the public constructors above use. A {@link
     * ContentFilteredTopic} built against a name-only topic cannot evaluate
     * its filter expression against sample fields: the core's {@code
     * has_field} always returns false, filter evaluation errors on every
     * sample, and {@code DataReader::passes_content_filter} treats that
     * error as "passes" -- so the filter silently becomes a no-op. This seam
     * exists for {@code ContentFilteredTopicTest}, which needs a topic that
     * actually filters; it is not reachable from the public API.
     *
     * <p>{@code fieldTypeCodes}/{@code fieldIsKey} follow {@link
     * FfiAccess#createTopicWithFieldDescriptors}'s own doc for the type
     * codes and the "fields before the filtered one must still be declared"
     * ordering rule.
     */
    static <T extends IDdsType> Topic<T> createWithFieldDescriptors(DomainParticipant participant,
            String name, T prototype, String[] fieldNames, int[] fieldTypeCodes,
            boolean[] fieldIsKey) {
        return new Topic<T>(participant, name, prototype, fieldNames, fieldTypeCodes, fieldIsKey);
    }

    private Topic(DomainParticipant participant, String name, T prototype, String[] fieldNames,
            int[] fieldTypeCodes, boolean[] fieldIsKey) {
        super(Objects.requireNonNull(participant, "participant"),
                createWithFields(participant, Objects.requireNonNull(name, "name"),
                        Objects.requireNonNull(prototype, "prototype"),
                        fieldNames, fieldTypeCodes, fieldIsKey),
                FfiAccess::deleteTopic);
        this.name = name;
        this.typeName = prototype.typeName();
        this.extensibility = prototype.extensibility();
    }

    /**
     * Wraps an already-obtained OWNED handle (an Arc clone from {@code
     * int2dds_participant_find_topic}) rather than minting one -- the
     * super-constructor is handed {@code precomputedHandle} directly instead
     * of a {@code create*()} call, but the handle is released the same way
     * ({@code FfiAccess::deleteTopic}) as any created topic. {@code name}/
     * {@code typeName}/{@code extensibility} are set from {@code prototype}
     * the same way every create-path constructor above does.
     */
    private Topic(DomainParticipant participant, String name, T prototype, long precomputedHandle) {
        super(Objects.requireNonNull(participant, "participant"), precomputedHandle,
                FfiAccess::deleteTopic);
        this.name = Objects.requireNonNull(name, "name");
        this.typeName = Objects.requireNonNull(prototype, "prototype").typeName();
        this.extensibility = prototype.extensibility();
    }

    /**
     * Package-private lookup path, reached only through {@link
     * DomainParticipant#findTopic}: finds an existing topic named {@code
     * name} (created elsewhere in this participant, or discovered), waiting
     * up to {@code timeoutMs} milliseconds (negative = wait indefinitely).
     * {@code prototype} supplies the DDS type name the native lookup matches
     * against, and the compile-time type {@code T} for the returned {@link
     * Topic}. The returned {@code Topic<T>} owns its handle and is released
     * the same way ({@code FfiAccess::deleteTopic}) as a topic {@code
     * createTopic} builds.
     */
    static <T extends IDdsType> Topic<T> find(
            DomainParticipant participant, String name, T prototype, int timeoutMs) {
        Objects.requireNonNull(participant, "participant");
        Objects.requireNonNull(name, "name");
        Objects.requireNonNull(prototype, "prototype");
        long[] out = new long[1];
        int rc = FfiAccess.participantFindTopic(participant.handle(), utf8(name),
                utf8(prototype.typeName()), timeoutMs, out);
        // participant.handle() above returns a bare long, disconnected from
        // `participant` the moment it is read -- same fence as createNative.
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return new Topic<T>(participant, name, prototype, out[0]);
    }

    /** The topic name this instance was created with. */
    public String name() {
        return name;
    }

    /** The DDS type name, taken from the prototype's {@link IDdsType#typeName()}. */
    public String typeName() {
        return typeName;
    }

    /**
     * The wire extensibility, taken from the prototype's {@link
     * IDdsType#extensibility()}.
     */
    public Extensibility extensibility() {
        return extensibility;
    }

    /**
     * Applies {@code qos} to this topic at runtime. Reads this topic's
     * current QoS into a native handle, applies {@code qos}'s non-null
     * policies onto it (a null policy keeps its current value), and destroys
     * it again once the native {@code set_qos} call returns — success or
     * failure, thrown or not, the same build-apply-destroy shape {@link
     * #create} uses for the create path. The core rejects a change to an
     * immutable policy (e.g. {@code Reliability}, {@code Durability}, {@code
     * History}) once a writer or reader on this topic is enabled, surfaced
     * here through {@link ReturnCodes#check}; {@code Deadline} and {@code
     * Lifespan} remain mutable. ({@code TopicData}, {@code LatencyBudget}
     * and {@code TransportPriority} are rejected outright by this core
     * regardless of mutability -- see {@code check_unsupported_policies} in
     * dds/src/dcps/topic/qos/mod.rs -- so none of the three is a useful
     * example of a runtime-mutable policy here.)
     *
     * @throws NullPointerException if {@code qos} is null
     */
    public void setQos(TopicQos qos) {
        Objects.requireNonNull(qos, "qos");
        long[] qosOut = new long[1];
        ReturnCodes.check(FfiAccess.getTopicQos(handle(), qosOut));
        long qosHandle = qosOut[0];
        try {
            QosMarshal.applyTopicQos(qosHandle, qos);
            int rc = FfiAccess.topicSetQos(handle(), qosHandle);
            // handle() is a bare long read moments before the native call
            // that consumes it; keep this topic reachable across it -- see
            // NativeKeepAlive's own doc for the full argument.
            NativeKeepAlive.keepAlive(this);
            ReturnCodes.check(rc);
        } finally {
            FfiAccess.destroyTopicQos(qosHandle);
        }
    }

    /**
     * A fresh StatusCondition for this topic's status changes; attach it to
     * a WaitSet to wait on status transitions.
     */
    public StatusCondition getStatusCondition() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.topicGetStatusCondition(h, out);
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
        int rc = FfiAccess.topicGetStatusChanges(handle(), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return StatusMask.of(out[0]);
    }

    /**
     * This topic's INCONSISTENT_TOPIC status: how many times a topic of the
     * same name but an incompatible type or QoS was discovered, and the
     * change since last read. Per DDS, reading this status clears its
     * {@code *Change} field.
     */
    public InconsistentTopicStatus getInconsistentTopicStatus() {
        ByteBuffer buf = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.topicGetInconsistentTopicStatus(
                handle(), FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int totalCount = buf.getInt(0);
        int totalCountChange = buf.getInt(4);
        return new InconsistentTopicStatus(totalCount, totalCountChange);
    }

    /**
     * Resolves the create arguments and, when {@code qos} is supplied,
     * builds a native QoS handle, applies the policies onto it, and destroys
     * it again once the create call returns — success or failure, thrown or
     * not. The native create call reads the QoS synchronously and does not
     * retain the handle past that call, so nothing needs it to survive any
     * longer than this method's body. {@code qos == null} passes {@code 0L}
     * (the Java-side spelling of a null native pointer), engaging the same
     * core-side default resolution as an explicit {@link TopicQos} with
     * every policy left null.
     */
    private static long create(
            DomainParticipant participant, String name, IDdsType prototype, TopicQos qos) {
        byte[] nameBytes = utf8(name);
        if (qos == null) {
            return createNative(participant, nameBytes, prototype, 0L);
        }
        long qosHandle = FfiAccess.createTopicQos();
        if (qosHandle == 0L) {
            // Without this guard, a QoS-allocation failure with no policy
            // actually set (applyTopicQos then calls no native setter at
            // all) would fall straight through to createNative(..., 0L) --
            // 0L being exactly this class's own spelling of "no QoS" --
            // silently downgrading a failure into a successful default-QoS
            // create. See DomainParticipant.createWithQos for the same
            // reasoning spelled out at greater length.
            throw new DdsErrorException(
                    "failed to allocate a native TopicQos handle for topic creation");
        }
        try {
            QosMarshal.applyTopicQos(qosHandle, qos);
            return createNative(participant, nameBytes, prototype, qosHandle);
        } finally {
            FfiAccess.destroyTopicQos(qosHandle);
        }
    }

    private static long createNative(DomainParticipant participant, byte[] nameBytes,
            IDdsType prototype, long qos) {
        long[] handleOut = new long[1];
        int rc;
        TypeInfo typeInfo = prototype.typeInfo();
        if (typeInfo != null) {
            // The type name, extensibility, keys and the advertised TypeObject
            // all come from the description, as in the C# and Python bindings.
            try {
                rc = typeInfo.createTopic(participant.handle(), nameBytes, qos, handleOut);
            } finally {
                typeInfo.close();
            }
        } else {
            rc = FfiAccess.createTopic(participant.handle(), nameBytes,
                    utf8(prototype.typeName()), prototype.extensibility().value(), qos, handleOut);
        }
        // participant.handle() above returns a bare long, disconnected from
        // `participant` the moment it is read: nothing else in this method
        // still references `participant`, so without this the reaper could
        // observe it as phantom-reachable and race the native call above,
        // which is still using the handle that call read. See
        // NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return handleOut[0];
    }

    /**
     * The profile-create path: derives {@code nameBytes}/{@code
     * typeNameBytes}/{@code extensibility} from {@code prototype} the same
     * way {@link #create} does, then routes through {@link
     * FfiAccess#createTopicWithProfile} instead of building a QoS handle.
     */
    private static long createWithProfile(
            DomainParticipant participant, String name, IDdsType prototype, String profilePath) {
        // The C ABI has no profile variant of create_topic_with_type_info, and a
        // keyed type created without one would be a key-less topic that never
        // matches its keyed peers -- so refuse it, as the C# binding does.
        TypeInfo typeInfo = prototype.typeInfo();
        if (typeInfo != null) {
            boolean keyed = typeInfo.hasKey();
            typeInfo.close();
            if (keyed) {
                throw new DdsUnsupportedException("keyed topic '" + name
                        + "' cannot be created from a QoS profile; create it with a TopicQos instead");
            }
        }
        byte[] nameBytes = utf8(name);
        byte[] typeNameBytes = utf8(prototype.typeName());
        int extensibility = prototype.extensibility().value();
        long[] handleOut = new long[1];
        int rc = FfiAccess.createTopicWithProfile(participant.handle(), nameBytes, typeNameBytes,
                extensibility, utf8(profilePath), handleOut);
        // Same fence as createNative.
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return handleOut[0];
    }

    private static long createWithFields(DomainParticipant participant, String name,
            IDdsType prototype, String[] fieldNames, int[] fieldTypeCodes, boolean[] fieldIsKey) {
        byte[] nameBytes = utf8(name);
        byte[] typeNameBytes = utf8(prototype.typeName());
        int extensibility = prototype.extensibility().value();
        byte[][] fieldNameBytes = new byte[fieldNames.length][];
        for (int i = 0; i < fieldNames.length; i++) {
            fieldNameBytes[i] = utf8(fieldNames[i]);
        }
        long[] handleOut = new long[1];
        int rc = FfiAccess.createTopicWithFieldDescriptors(participant.handle(), nameBytes,
                typeNameBytes, extensibility, 0L, fieldNameBytes, fieldTypeCodes, fieldIsKey,
                fieldNames.length, handleOut);
        NativeKeepAlive.keepAlive(participant);
        ReturnCodes.check(rc);
        return handleOut[0];
    }

    /** UTF-8 bytes for a name that crosses as byte[]. Never a String. */
    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }
}
