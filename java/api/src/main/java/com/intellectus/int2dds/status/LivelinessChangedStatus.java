package com.intellectus.int2dds.status;

import java.util.Arrays;

/**
 * Immutable snapshot of a DataReader's LIVELINESS_CHANGED status.
 *
 * <p>Constructed from the native trampoline via the {@code (IIII[B)V}
 * constructor; the field order matches the core's
 * {@code Int2DdsLivelinessChangedStatus}.
 */
public final class LivelinessChangedStatus {

    private final int aliveCount;
    private final int notAliveCount;
    private final int aliveCountChange;
    private final int notAliveCountChange;
    private final byte[] lastPublicationHandle;

    public LivelinessChangedStatus(int aliveCount, int notAliveCount,
            int aliveCountChange, int notAliveCountChange, byte[] lastPublicationHandle) {
        this.aliveCount = aliveCount;
        this.notAliveCount = notAliveCount;
        this.aliveCountChange = aliveCountChange;
        this.notAliveCountChange = notAliveCountChange;
        // Defensive copy: the caller's array must not alias this snapshot.
        this.lastPublicationHandle = lastPublicationHandle == null
                ? new byte[0]
                : Arrays.copyOf(lastPublicationHandle, lastPublicationHandle.length);
    }

    /** Current count of alive matched DataWriters. */
    public int aliveCount() {
        return aliveCount;
    }

    /** Current count of not-alive matched DataWriters. */
    public int notAliveCount() {
        return notAliveCount;
    }

    /** Change in {@link #aliveCount()} since this status was last read. */
    public int aliveCountChange() {
        return aliveCountChange;
    }

    /** Change in {@link #notAliveCount()} since this status was last read. */
    public int notAliveCountChange() {
        return notAliveCountChange;
    }

    /** The last DataWriter whose liveliness changed, as a 16-byte handle (a copy). */
    public byte[] lastPublicationHandle() {
        return Arrays.copyOf(lastPublicationHandle, lastPublicationHandle.length);
    }
}
