package com.intellectus.int2dds.status;

/**
 * Immutable snapshot of a DataWriter's LIVELINESS_LOST status.
 *
 * <p>Constructed from the native trampoline via the {@code (II)V}
 * constructor; the field order matches the core's {@code Int2DdsLivelinessLostStatus}.
 */
public final class LivelinessLostStatus {

    private final int totalCount;
    private final int totalCountChange;

    public LivelinessLostStatus(int totalCount, int totalCountChange) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
    }

    /** Cumulative count of times liveliness was lost. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }
}
