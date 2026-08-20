package com.intellectus.int2dds.status;

import java.util.Arrays;

/**
 * Immutable snapshot of a DataReader's SUBSCRIPTION_MATCHED status: how many
 * DataWriters it has matched, and the change since the status was last read.
 *
 * <p>Constructed from the native trampoline via the {@code (IIII[B)V}
 * constructor; the field order matches the core's
 * {@code Int2DdsSubscriptionMatchedStatus}.
 */
public final class SubscriptionMatchedStatus {

    private final int totalCount;
    private final int totalCountChange;
    private final int currentCount;
    private final int currentCountChange;
    private final byte[] lastPublicationHandle;

    public SubscriptionMatchedStatus(int totalCount, int totalCountChange,
            int currentCount, int currentCountChange, byte[] lastPublicationHandle) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
        this.currentCount = currentCount;
        this.currentCountChange = currentCountChange;
        // Defensive copy: the caller's array must not alias this snapshot.
        this.lastPublicationHandle = lastPublicationHandle == null
                ? new byte[0]
                : Arrays.copyOf(lastPublicationHandle, lastPublicationHandle.length);
    }

    /** Cumulative count of DataWriters ever matched. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }

    /** Number of DataWriters currently matched. */
    public int currentCount() {
        return currentCount;
    }

    /** Change in {@link #currentCount()} since this status was last read. */
    public int currentCountChange() {
        return currentCountChange;
    }

    /** The last matched DataWriter's 16-byte instance handle (a copy). */
    public byte[] lastPublicationHandle() {
        return Arrays.copyOf(lastPublicationHandle, lastPublicationHandle.length);
    }
}
