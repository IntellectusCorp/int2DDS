package com.intellectus.int2dds.qos;

/**
 * Data representation kinds. The value is the wire encoding, not the ordinal.
 *
 * <p>This is the enum where ordinal and wire value genuinely disagree:
 * {@code XCDR2} skips value 1, so its ordinal is 1 but its wire value is 2.
 */
public enum DataRepresentationKind {
    XCDR1(0),
    XCDR2(2);

    private final int value;

    DataRepresentationKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static DataRepresentationKind fromValue(int value) {
        for (DataRepresentationKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown DataRepresentationKind value: " + value);
    }
}
