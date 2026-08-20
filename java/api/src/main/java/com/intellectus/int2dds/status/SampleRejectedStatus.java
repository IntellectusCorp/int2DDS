package com.intellectus.int2dds.status;

import java.util.Arrays;

/**
 * Immutable snapshot of a DataReader's SAMPLE_REJECTED status.
 *
 * <p>Constructed from the native trampoline via the {@code (III[B)V}
 * constructor; the field order matches the core's
 * {@code Int2DdsSampleRejectedStatus}.
 */
public final class SampleRejectedStatus {

    /** {@link #lastReason()} value: no sample has been rejected. */
    public static final int REASON_NOT_REJECTED = 0;
    /** {@link #lastReason()} value: rejected by the max-instances limit. */
    public static final int REASON_REJECTED_BY_INSTANCES_LIMIT = 1;
    /** {@link #lastReason()} value: rejected by the max-samples limit. */
    public static final int REASON_REJECTED_BY_SAMPLES_LIMIT = 2;
    /** {@link #lastReason()} value: rejected by the max-samples-per-instance limit. */
    public static final int REASON_REJECTED_BY_SAMPLES_PER_INSTANCE_LIMIT = 3;

    private final int totalCount;
    private final int totalCountChange;
    private final int lastReason;
    private final byte[] lastInstanceHandle;

    public SampleRejectedStatus(int totalCount, int totalCountChange,
            int lastReason, byte[] lastInstanceHandle) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
        this.lastReason = lastReason;
        // Defensive copy: the caller's array must not alias this snapshot.
        this.lastInstanceHandle = lastInstanceHandle == null
                ? new byte[0]
                : Arrays.copyOf(lastInstanceHandle, lastInstanceHandle.length);
    }

    /** Cumulative count of samples ever rejected. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }

    /** Reason the last sample was rejected; one of the {@code REASON_*} constants. */
    public int lastReason() {
        return lastReason;
    }

    /** The last rejected sample's 16-byte instance handle (a copy). */
    public byte[] lastInstanceHandle() {
        return Arrays.copyOf(lastInstanceHandle, lastInstanceHandle.length);
    }
}
