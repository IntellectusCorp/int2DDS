package com.intellectus.int2dds.cdr;

/** Extensibility kinds that select the encapsulation encoding. */
public enum Extensibility {
    /** PLAIN_CDR2 — no DHEADER, no EMHEADER. */
    FINAL(0),
    /** DELIMITED_CDR2 — DHEADER around each aggregate. */
    APPENDABLE(1),
    /** PL_CDR2 — EMHEADER per member plus a sentinel. */
    MUTABLE(2);

    private final int value;

    Extensibility(int value) {
        this.value = value;
    }

    /**
     * The value the C ABI uses, from {@code ExtensibilityKind} in
     * {@code dds/src/xtypes/type_object.rs}. Explicit rather than
     * {@code ordinal()} so reordering the constants cannot change the wire.
     */
    public int value() {
        return value;
    }

    /** The constant for a wire value. */
    public static Extensibility fromValue(int value) {
        for (Extensibility e : values()) {
            if (e.value == value) {
                return e;
            }
        }
        throw new IllegalArgumentException("unknown Extensibility value: " + value);
    }
}
