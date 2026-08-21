package com.intellectus.int2dds.status;

/**
 * Immutable snapshot of a DataReader's REQUESTED_INCOMPATIBLE_TYPE status.
 *
 * <p>Constructed from the native trampoline via the {@code (II)V}
 * constructor; the field order matches the core's {@code Int2DdsRequestedIncompatibleTypeStatus}.
 */
public final class RequestedIncompatibleTypeStatus {

    private final int totalCount;
    private final int totalCountChange;

    public RequestedIncompatibleTypeStatus(int totalCount, int totalCountChange) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
    }

    /** Cumulative count of times this reader discovered an incompatible offered type. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }
}
