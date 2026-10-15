package kr.co.intellectus.int2dds.qos;

/** Presentation access scope kinds. The value is the wire encoding, not the ordinal. */
public enum PresentationAccessScopeKind {
    INSTANCE(0),
    TOPIC(1),
    GROUP(2);

    private final int value;

    PresentationAccessScopeKind(int value) {
        this.value = value;
    }

    /** The value the C ABI uses. */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static PresentationAccessScopeKind fromValue(int value) {
        for (PresentationAccessScopeKind k : values()) {
            if (k.value == value) {
                return k;
            }
        }
        throw new IllegalArgumentException("unknown PresentationAccessScopeKind value: " + value);
    }
}
