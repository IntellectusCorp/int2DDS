package com.intellectus.int2dds.qos;

/** Liveliness kinds. The value is the wire encoding, not the ordinal. */
public enum LivelinessKind {
    AUTOMATIC(0),
    MANUAL_BY_PARTICIPANT(1),
    MANUAL_BY_TOPIC(2);

    private final int value;

    LivelinessKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static LivelinessKind fromValue(int value) {
        for (LivelinessKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown LivelinessKind value: " + value);
    }
}
