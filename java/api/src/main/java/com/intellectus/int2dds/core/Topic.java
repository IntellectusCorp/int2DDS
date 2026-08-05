package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.types.IDdsType;
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
        byte[] typeNameBytes = utf8(prototype.typeName());
        int extensibility = prototype.extensibility().value();
        if (qos == null) {
            return createNative(participant, nameBytes, typeNameBytes, extensibility, 0L);
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
            return createNative(participant, nameBytes, typeNameBytes, extensibility, qosHandle);
        } finally {
            FfiAccess.destroyTopicQos(qosHandle);
        }
    }

    private static long createNative(DomainParticipant participant, byte[] nameBytes,
            byte[] typeNameBytes, int extensibility, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createTopic(
                participant.handle(), nameBytes, typeNameBytes, extensibility, qos, handleOut);
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

    /** UTF-8 bytes for a name that crosses as byte[]. Never a String. */
    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }
}
