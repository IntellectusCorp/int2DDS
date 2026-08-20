package com.intellectus.int2dds.status;

/**
 * Immutable DDS status bitmask. The bit values match the core's
 * {@code StatusKind} numbering ({@code dds/src/dcps/infrastructure/status.rs}).
 */
public final class StatusMask {
    public static final int SUBSCRIPTION_MATCHED = 1 << 14;
    public static final int PUBLICATION_MATCHED = 1 << 13;
    public static final int DATA_AVAILABLE = 1 << 10;
    public static final int LIVELINESS_CHANGED = 1 << 12;
    public static final int LIVELINESS_LOST = 1 << 11;
    public static final int REQUESTED_DEADLINE_MISSED = 1 << 2;
    public static final int OFFERED_DEADLINE_MISSED = 1 << 1;
    public static final int REQUESTED_INCOMPATIBLE_QOS = 1 << 6;
    public static final int OFFERED_INCOMPATIBLE_QOS = 1 << 5;
    public static final int SAMPLE_LOST = 1 << 7;
    public static final int SAMPLE_REJECTED = 1 << 8;

    private final int bits;

    private StatusMask(int bits) {
        this.bits = bits;
    }

    /** A mask of exactly {@code bits}. */
    public static StatusMask of(int bits) {
        return new StatusMask(bits);
    }

    /** A mask with every bit set. */
    public static StatusMask all() {
        return new StatusMask(0xFFFFFFFF);
    }

    /** This mask with the additional {@code more} bits set. */
    public StatusMask or(int more) {
        return new StatusMask(bits | more);
    }

    /** The raw bit set. */
    public int bits() {
        return bits;
    }
}
