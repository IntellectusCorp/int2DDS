package com.intellectus.int2dds.discovery;

import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.util.Arrays;

/**
 * An immutable snapshot of one remote participant's builtin topic data, as
 * discovered through the {@code DCPSParticipant} builtin topic.
 *
 * <p>Holds no native handle: {@link #materialize} reads every field off a
 * native {@code ParticipantBuiltinTopicData} box while it is still alive, so
 * an instance stays valid forever after, including past the box's own
 * destroy. See {@link com.intellectus.int2dds.core.DomainParticipant#getDiscoveredParticipants}
 * for the handle-list-then-per-handle-lookup lifecycle that produces these.
 */
public final class ParticipantBuiltinTopicData {

    private final byte[] key;
    private final byte[] userData;

    private ParticipantBuiltinTopicData(byte[] key, byte[] userData) {
        this.key = key;
        this.userData = userData;
    }

    /** The 12-byte instance key (defensively copied). */
    public byte[] key() {
        return key.clone();
    }

    /** The raw {@code user_data} bytes (defensively copied), possibly empty. */
    public byte[] userData() {
        return userData.clone();
    }

    /**
     * Reads every field off a live native {@code ParticipantBuiltinTopicData}
     * box into an immutable snapshot. {@code data} must still be valid when
     * this is called; the caller retains ownership and is responsible for
     * destroying it afterward -- this never does.
     */
    public static ParticipantBuiltinTopicData materialize(long data) {
        byte[] key = new byte[12];
        ReturnCodes.check(FfiAccess.participantDataGetKey(data, key));

        final long dataHandle = data;
        byte[][] userDataBytes = new byte[1][];
        ReturnCodes.check(FfiAccess.readGrowableBytes(
                new FfiAccess.GrowableBytesGetter() {
                    @Override
                    public int get(long buf, long capacity, long sizeOut) {
                        return FfiAccess.participantDataGetUserData(dataHandle, buf, capacity, sizeOut);
                    }
                },
                userDataBytes));

        return new ParticipantBuiltinTopicData(key, userDataBytes[0]);
    }

    @Override
    public String toString() {
        return "ParticipantBuiltinTopicData{key=" + Arrays.toString(key) + "}";
    }
}
