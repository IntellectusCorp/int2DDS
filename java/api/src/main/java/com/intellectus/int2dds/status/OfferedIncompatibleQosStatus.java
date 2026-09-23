package com.intellectus.int2dds.status;

/**
 * Immutable snapshot of a DataWriter's OFFERED_INCOMPATIBLE_QOS status.
 *
 * <p>Constructed from the native trampoline via the {@code (IIII)V}
 * constructor; the field order matches the core's
 * {@code Int2DdsOfferedIncompatibleQosStatus}.
 */
public final class OfferedIncompatibleQosStatus {

    private final int totalCount;
    private final int totalCountChange;
    private final int lastPolicyId;
    private final int policiesCount;

    public OfferedIncompatibleQosStatus(int totalCount, int totalCountChange,
            int lastPolicyId, int policiesCount) {
        this.totalCount = totalCount;
        this.totalCountChange = totalCountChange;
        this.lastPolicyId = lastPolicyId;
        this.policiesCount = policiesCount;
    }

    /** Cumulative count of incompatible-QoS matches. */
    public int totalCount() {
        return totalCount;
    }

    /** Change in {@link #totalCount()} since this status was last read. */
    public int totalCountChange() {
        return totalCountChange;
    }

    /** ID of the QoS policy that was last found incompatible (core's {@code QosPolicyId}). */
    public int lastPolicyId() {
        return lastPolicyId;
    }

    /** Count of incompatible policies (always 0 in v1: the policies list is not exposed). */
    public int policiesCount() {
        return policiesCount;
    }
}
