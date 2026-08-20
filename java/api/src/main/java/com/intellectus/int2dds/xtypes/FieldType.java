package com.intellectus.int2dds.xtypes;

/**
 * Field kind constants for {@link TypeInfo#addField}, transcribed from
 * {@code INT2DDS_FIELD_*} in {@code ffi/src/type_info.rs} — the single
 * source of truth for these numbers.
 */
public final class FieldType {

    private FieldType() {}

    public static final int BOOL = 0;
    public static final int BYTE = 1;
    public static final int CHAR8 = 2;
    public static final int INT8 = 3;
    public static final int INT16 = 4;
    public static final int INT32 = 5;
    public static final int INT64 = 6;
    public static final int UINT8 = 7;
    public static final int UINT16 = 8;
    public static final int UINT32 = 9;
    public static final int UINT64 = 10;
    public static final int FLOAT32 = 11;
    public static final int FLOAT64 = 12;
    public static final int STRING = 13;
    public static final int CHAR16 = 14;
    public static final int WSTRING = 15;
    public static final int NESTED = 16;
    public static final int SEQUENCE = 17;
    public static final int ARRAY = 18;
    public static final int MAP = 19;
}
