package com.intellectus.int2dds.status;

import java.util.Arrays;

/**
 * Immutable snapshot of a DataWriter's PUBLICATION_MATCHED status: how many
 * DataReaders it has matched, and the change since the status was last read.
 *
 * <p>Constructed from the native trampoline via the {@code (IIII[B)V}
 * constructor; the field order matches the core's
 * {@code Int2DdsPublicationMatchedStatus}.
 */
public final class PublicationMatchedStatus {

    private final int totalCount;
    private final int totalCountChange;
    private final int currentCount;
    private final int currentCountChange;
    private final byte[] lastSubscriptionHandle;

    public PublicationMatchedStatus(int totalCount, int totalCountChange,
            int currentCount, int currentCountChange, byte[] lastSubscriptionHandle) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
        this.currentCount = currentCount;
        this.currentCountChange = currentCountChange;
        // Defensive copy: the caller's array must not alias this snapshot.
        this.lastSubscriptionHandle = lastSubscriptionHandle == null
                ? new byte[0]
                : Arrays.copyOf(lastSubscriptionHandle, lastSubscriptionHandle.length);
    }

    /** Cumulative count of DataReaders ever matched. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }

    /** Number of DataReaders currently matched. */
    public int currentCount() {
        return currentCount;
    }

    /** Change in {@link #currentCount()} since this status was last read. */
    public int currentCountChange() {
        return currentCountChange;
    }

    /** The last matched DataReader's 16-byte instance handle (a copy). */
    public byte[] lastSubscriptionHandle() {
        return Arrays.copyOf(lastSubscriptionHandle, lastSubscriptionHandle.length);
    }
}
