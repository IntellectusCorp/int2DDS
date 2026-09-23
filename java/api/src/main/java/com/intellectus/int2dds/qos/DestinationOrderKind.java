package com.intellectus.int2dds.qos;

/** Destination order kinds. The value is the wire encoding, not the ordinal. */
public enum DestinationOrderKind {
    BY_RECEPTION(0),
    BY_SOURCE(1);

    private final int value;

    DestinationOrderKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static DestinationOrderKind fromValue(int value) {
        for (DestinationOrderKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown DestinationOrderKind value: " + value);
    }
}
