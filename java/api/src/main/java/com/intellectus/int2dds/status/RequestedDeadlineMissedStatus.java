package com.intellectus.int2dds.status;

import java.util.Arrays;

/**
 * Immutable snapshot of a DataReader's REQUESTED_DEADLINE_MISSED status.
 *
 * <p>Constructed from the native trampoline via the {@code (II[B)V}
 * constructor; the field order matches the core's
 * {@code Int2DdsRequestedDeadlineMissedStatus}.
 */
public final class RequestedDeadlineMissedStatus {

    private final int totalCount;
    private final int totalCountChange;
    private final byte[] lastInstanceHandle;

    public RequestedDeadlineMissedStatus(int totalCount, int totalCountChange,
            byte[] lastInstanceHandle) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
        // Defensive copy: the caller's array must not alias this snapshot.
        this.lastInstanceHandle = lastInstanceHandle == null
                ? new byte[0]
                : Arrays.copyOf(lastInstanceHandle, lastInstanceHandle.length);
    }

    /** Cumulative count of missed deadlines. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }

    /** The instance for which a deadline was last missed, as a 16-byte handle (a copy). */
    public byte[] lastInstanceHandle() {
        return Arrays.copyOf(lastInstanceHandle, lastInstanceHandle.length);
    }
}
