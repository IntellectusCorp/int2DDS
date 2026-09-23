package com.intellectus.int2dds.status;

/**
 * Immutable snapshot of a Topic's INCONSISTENT_TOPIC status.
 *
 * <p>Constructed from the native trampoline via the {@code (II)V}
 * constructor; the field order matches the core's {@code Int2DdsInconsistentTopicStatus}.
 */
public final class InconsistentTopicStatus {

    private final int totalCount;
    private final int totalCountChange;

    public InconsistentTopicStatus(int totalCount, int totalCountChange) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
    }

    /** Cumulative count of times the topic was found inconsistent. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }
}
