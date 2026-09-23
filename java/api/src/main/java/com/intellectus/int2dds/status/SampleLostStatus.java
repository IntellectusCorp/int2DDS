package com.intellectus.int2dds.status;

/**
 * Immutable snapshot of a DataReader's SAMPLE_LOST status.
 *
 * <p>Constructed from the native trampoline via the {@code (II)V}
 * constructor; the field order matches the core's {@code Int2DdsSampleLostStatus}.
 */
public final class SampleLostStatus {

    private final int totalCount;
    private final int totalCountChange;

    public SampleLostStatus(int totalCount, int totalCountChange) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
    }

    /** Cumulative count of samples lost. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }
}
