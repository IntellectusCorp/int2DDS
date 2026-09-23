package com.intellectus.int2dds.xtypes;

/**
 * The kind returned by {@link DynamicValue#kind()} -- mirrors the
 * {@code INT2DDS_VALUE_KIND_*} constants in {@code ffi/src/dynamic_value.rs}.
 */
public final class DynamicValueKind {

    private DynamicValueKind() {}

    public static final int BOOLEAN = 0;
    public static final int INT8 = 1;
    public static final int INT16 = 2;
    public static final int INT32 = 3;
    public static final int INT64 = 4;
    public static final int UINT8 = 5;
    public static final int UINT16 = 6;
    public static final int UINT32 = 7;
    public static final int UINT64 = 8;
    public static final int FLOAT32 = 9;
    public static final int FLOAT64 = 10;
    public static final int CHAR8 = 11;
    public static final int BYTE = 12;
    public static final int STRING = 13;
    public static final int WSTRING = 14;
    public static final int ENUM = 15;
    public static final int UNION = 16;
    public static final int BITMASK = 17;
    public static final int BITSET = 18;
    public static final int STRUCT = 19;
    public static final int SEQUENCE = 20;
    public static final int ARRAY = 21;
    public static final int MAP = 22;
    public static final int OPTIONAL = 23;
    public static final int NULL = 24;
}
