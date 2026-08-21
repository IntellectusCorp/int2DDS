package com.intellectus.int2dds.status;

/**
 * Immutable snapshot of a DataWriter's OFFERED_INCOMPATIBLE_TYPE status.
 *
 * <p>Constructed from the native trampoline via the {@code (II)V}
 * constructor; the field order matches the core's {@code Int2DdsOfferedIncompatibleTypeStatus}.
 */
public final class OfferedIncompatibleTypeStatus {

    private final int totalCount;
    private final int totalCountChange;

    public OfferedIncompatibleTypeStatus(int totalCount, int totalCountChange) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
    }

    /** Cumulative count of times this writer offered a type incompatible with a requesting reader. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }
}
