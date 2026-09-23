package com.intellectus.int2dds.qos;

/** Reliability kinds. The value is the wire encoding, not the ordinal. */
public enum ReliabilityKind {
    BEST_EFFORT(0),
    RELIABLE(1);

    private final int value;

    ReliabilityKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static ReliabilityKind fromValue(int value) {
        for (ReliabilityKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown ReliabilityKind value: " + value);
    }
}
