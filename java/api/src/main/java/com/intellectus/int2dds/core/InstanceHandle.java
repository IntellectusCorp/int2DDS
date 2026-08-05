package com.intellectus.int2dds.core;

import java.util.Arrays;

/**
 * A DDS instance handle: a 16-byte, core-assigned key identifying one
 * instance of a keyed topic.
 *
 * <p>A value type: two handles are equal exactly when their 16 bytes are
 * equal, regardless of identity. {@code bytes} is copied on the way in and
 * out, the same defensive-copy convention {@link
 * com.intellectus.int2dds.qos.UserData} uses for its own byte array, so a
 * later mutation of the caller's array cannot change a handle already
 * constructed, and a caller mutating a returned array cannot change this
 * instance.
 *
 * <p>Not used by the write path in this branch — {@link DataWriter#write}
 * always passes a null key, since keys for keyed topics are derived by the
 * core from field descriptors registered at topic creation, which this
 * branch does not do. It is part of the C# reference binding's Core surface
 * ({@code csharp/src/Int2Dds/Core/InstanceHandle.cs}) and the read branch
 * needs it, so it is added here alongside the rest of Core rather than
 * deferred to that later task.
 */
public final class InstanceHandle {

    private static final int LENGTH = 16;

    private final byte[] bytes;

    /**
     * @param bytes exactly 16 bytes; copied, so later mutation of the
     *     caller's array does not affect this instance.
     * @throws IllegalArgumentException if {@code bytes.length != 16}
     */
    public InstanceHandle(byte[] bytes) {
        if (bytes.length != LENGTH) {
            throw new IllegalArgumentException(
                    "InstanceHandle requires exactly " + LENGTH + " bytes, got " + bytes.length);
        }
        this.bytes = bytes.clone();
    }

    /** A copy of the 16-byte key; mutating the returned array does not affect this instance. */
    public byte[] bytes() {
        return bytes.clone();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof InstanceHandle)) {
            return false;
        }
        return Arrays.equals(bytes, ((InstanceHandle) o).bytes);
    }

    @Override
    public int hashCode() {
        return Arrays.hashCode(bytes);
    }

    /** Lowercase hex, no separators — 32 characters for this class's fixed 16-byte key. */
    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) {
            sb.append(Character.forDigit((b >> 4) & 0xF, 16));
            sb.append(Character.forDigit(b & 0xF, 16));
        }
        return sb.toString();
    }
}
