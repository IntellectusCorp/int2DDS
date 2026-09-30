package kr.co.intellectus.int2dds.qos;

/** Lifespan reference kinds. The value is the wire encoding, not the ordinal. */
public enum LifespanReferenceKind {
    BY_SOURCE(0),
    BY_RECEPTION(1);

    private final int value;

    LifespanReferenceKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static LifespanReferenceKind fromValue(int value) {
        for (LifespanReferenceKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown LifespanReferenceKind value: " + value);
    }
}
