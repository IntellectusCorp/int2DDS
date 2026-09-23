package com.intellectus.int2dds.qos;

/** Durability kinds. The value is the wire encoding, not the ordinal. */
public enum DurabilityKind {
    VOLATILE(0),
    TRANSIENT_LOCAL(1),
    TRANSIENT(2),
    PERSISTENT(3);

    private final int value;

    DurabilityKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static DurabilityKind fromValue(int value) {
        for (DurabilityKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown DurabilityKind value: " + value);
    }
}
