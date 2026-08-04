package com.intellectus.int2dds.cdr;

/** Extensibility kinds that select the encapsulation encoding. */
public enum Extensibility {
    /** PLAIN_CDR2 — no DHEADER, no EMHEADER. */
    FINAL,
    /** DELIMITED_CDR2 — DHEADER around each aggregate. */
    APPENDABLE,
    /** PL_CDR2 — EMHEADER per member plus a sentinel. */
    MUTABLE
}
