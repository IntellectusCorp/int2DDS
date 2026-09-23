package com.intellectus.int2dds.qos;

/** History kinds. The value is the wire encoding, not the ordinal. */
public enum HistoryKind {
    KEEP_LAST(0),
    KEEP_ALL(1);

    private final int value;

    HistoryKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static HistoryKind fromValue(int value) {
        for (HistoryKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown HistoryKind value: " + value);
    }
}
