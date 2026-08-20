package com.intellectus.int2dds.discovery;

import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * An immutable snapshot of one remote subscription's builtin topic data, as
 * discovered through the {@code DCPSSubscription} builtin topic.
 *
 * <p>Holds no native handle: {@link #materialize} reads every field off a
 * native {@code SubscriptionBuiltinTopicData} box while it is still alive, so
 * an instance stays valid forever after, including past the box's own
 * destroy. See {@link com.intellectus.int2dds.core.DomainParticipant#takeDiscoveredSubscriptions}
 * for the materialize-then-destroy lifecycle that produces these.
 *
 * <p>{@code reliabilityKind}: 0 = BEST_EFFORT, 1 = RELIABLE.
 * {@code durabilityKind}: 0 = VOLATILE, 1 = TRANSIENT_LOCAL, 2 = TRANSIENT,
 * 3 = PERSISTENT. {@code livelinessKind}: 0 = AUTOMATIC,
 * 1 = MANUAL_BY_PARTICIPANT, 2 = MANUAL_BY_TOPIC (matching
 * {@code ffi/src/discovery.rs}). Every duration reports an infinite value as
 * (0x7fffffff, 0x7fffffff). Unlike {@link PublicationBuiltinTopicData}, there
 * is no {@code lifespan} -- that QoS policy applies to writers only.
 */
public final class SubscriptionBuiltinTopicData {

    private final byte[] key;
    private final byte[] endpointGuid;
    private final byte[] participantKey;
    private final String topicName;
    private final String typeName;
    private final int reliabilityKind;
    private final int durabilityKind;
    private final int livelinessKind;
    private final int deadlineSeconds;
    private final int deadlineNanos;
    private final int livelinessLeaseSeconds;
    private final int livelinessLeaseNanos;
    private final byte[] userData;

    private SubscriptionBuiltinTopicData(byte[] key, byte[] endpointGuid, byte[] participantKey,
            String topicName, String typeName, int reliabilityKind, int durabilityKind,
            int livelinessKind, int deadlineSeconds, int deadlineNanos,
            int livelinessLeaseSeconds, int livelinessLeaseNanos, byte[] userData) {
        this.key = key;
        this.endpointGuid = endpointGuid;
        this.participantKey = participantKey;
        this.topicName = topicName;
        this.typeName = typeName;
        this.reliabilityKind = reliabilityKind;
        this.durabilityKind = durabilityKind;
        this.livelinessKind = livelinessKind;
        this.deadlineSeconds = deadlineSeconds;
        this.deadlineNanos = deadlineNanos;
        this.livelinessLeaseSeconds = livelinessLeaseSeconds;
        this.livelinessLeaseNanos = livelinessLeaseNanos;
        this.userData = userData;
    }

    /** The 12-byte instance key (defensively copied). */
    public byte[] key() {
        return key.clone();
    }

    /** The 16-byte GUID of the remote endpoint (defensively copied). */
    public byte[] endpointGuid() {
        return endpointGuid.clone();
    }

    /** The 12-byte key of the owning remote participant (defensively copied). */
    public byte[] participantKey() {
        return participantKey.clone();
    }

    public String topicName() {
        return topicName;
    }

    public String typeName() {
        return typeName;
    }

    public int reliabilityKind() {
        return reliabilityKind;
    }

    public int durabilityKind() {
        return durabilityKind;
    }

    public int livelinessKind() {
        return livelinessKind;
    }

    public long deadlineSeconds() {
        return deadlineSeconds;
    }

    public int deadlineNanos() {
        return deadlineNanos;
    }

    public long livelinessLeaseSeconds() {
        return livelinessLeaseSeconds;
    }

    public int livelinessLeaseNanos() {
        return livelinessLeaseNanos;
    }

    /** The raw {@code user_data} bytes (defensively copied), possibly empty. */
    public byte[] userData() {
        return userData.clone();
    }

    /**
     * Reads every field off a live native {@code SubscriptionBuiltinTopicData}
     * box into an immutable snapshot. {@code data} must still be valid when
     * this is called; the caller retains ownership and is responsible for
     * destroying it afterward -- this never does.
     */
    public static SubscriptionBuiltinTopicData materialize(long data) {
        byte[] key = new byte[12];
        ReturnCodes.check(FfiAccess.subDataGetKey(data, key));

        byte[] endpointGuid = new byte[16];
        ReturnCodes.check(FfiAccess.subDataGetEndpointGuid(data, endpointGuid));

        byte[] participantKey = new byte[12];
        ReturnCodes.check(FfiAccess.subDataGetParticipantKey(data, participantKey));

        final long dataHandle = data;
        byte[][] topicNameBytes = new byte[1][];
        ReturnCodes.check(FfiAccess.readGrowableString(
                new FfiAccess.GrowableStringGetter() {
                    @Override
                    public int get(long buf, long capacity, long sizeOut) {
                        return FfiAccess.subDataGetTopicName(dataHandle, buf, capacity, sizeOut);
                    }
                },
                topicNameBytes));
        String topicName = new String(topicNameBytes[0], StandardCharsets.UTF_8);

        byte[][] typeNameBytes = new byte[1][];
        ReturnCodes.check(FfiAccess.readGrowableString(
                new FfiAccess.GrowableStringGetter() {
                    @Override
                    public int get(long buf, long capacity, long sizeOut) {
                        return FfiAccess.subDataGetTypeName(dataHandle, buf, capacity, sizeOut);
                    }
                },
                typeNameBytes));
        String typeName = new String(typeNameBytes[0], StandardCharsets.UTF_8);

        int[] reliabilityKind = new int[1];
        ReturnCodes.check(FfiAccess.subDataGetReliabilityKind(data, reliabilityKind));

        int[] durabilityKind = new int[1];
        ReturnCodes.check(FfiAccess.subDataGetDurabilityKind(data, durabilityKind));

        int[] livelinessKind = new int[1];
        ReturnCodes.check(FfiAccess.subDataGetLivelinessKind(data, livelinessKind));

        int[] deadlineSeconds = new int[1];
        int[] deadlineNanos = new int[1];
        ReturnCodes.check(FfiAccess.subDataGetDeadline(data, deadlineSeconds, deadlineNanos));

        int[] leaseSeconds = new int[1];
        int[] leaseNanos = new int[1];
        ReturnCodes.check(
                FfiAccess.subDataGetLivelinessLeaseDuration(data, leaseSeconds, leaseNanos));

        byte[][] userDataBytes = new byte[1][];
        ReturnCodes.check(FfiAccess.readGrowableBytes(
                new FfiAccess.GrowableBytesGetter() {
                    @Override
                    public int get(long buf, long capacity, long sizeOut) {
                        return FfiAccess.subDataGetUserData(dataHandle, buf, capacity, sizeOut);
                    }
                },
                userDataBytes));

        return new SubscriptionBuiltinTopicData(key, endpointGuid, participantKey, topicName,
                typeName, reliabilityKind[0], durabilityKind[0], livelinessKind[0],
                deadlineSeconds[0], deadlineNanos[0], leaseSeconds[0], leaseNanos[0],
                userDataBytes[0]);
    }

    @Override
    public String toString() {
        return "SubscriptionBuiltinTopicData{topicName=" + topicName + ", typeName=" + typeName
                + ", endpointGuid=" + Arrays.toString(endpointGuid) + "}";
    }
}
