package com.intellectus.int2dds.qos;

/** Ownership kinds. The value is the wire encoding, not the ordinal. */
public enum OwnershipKind {
    SHARED(0),
    EXCLUSIVE(1);

    private final int value;

    OwnershipKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static OwnershipKind fromValue(int value) {
        for (OwnershipKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown OwnershipKind value: " + value);
    }
}
