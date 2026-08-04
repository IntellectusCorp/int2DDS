package com.intellectus.int2dds.exceptions;

/**
 * The {@code Int2DdsRet} values, transcribed from {@code ffi/src/error.rs}.
 *
 * <p>Public so that {@code internal.ReturnCodes} can share these constants
 * instead of duplicating the numbers; callers of the public API still work
 * with the exception types, not the numbers.
 */
public final class ReturnCodeValues {
    private ReturnCodeValues() {}

    public static final int OK = 0;

    // General errors
    public static final int ERROR = 1;
    public static final int TIMEOUT = 2;
    public static final int UNSUPPORTED = 3;
    public static final int INVALID_ARGUMENT = 11;

    // DDS-specific errors
    public static final int ALREADY_DELETED = 20;
    public static final int NOT_ENABLED = 21;
    public static final int IMMUTABLE_POLICY = 22;
    public static final int INCONSISTENT_POLICY = 23;
    public static final int PRECONDITION_NOT_MET = 24;
    public static final int OUT_OF_RESOURCES = 25;
    public static final int ILLEGAL_OPERATION = 26;
    public static final int NO_DATA = 27;

    // FFI-boundary errors
    public static final int NULL_POINTER = 100;
    public static final int BUFFER_TOO_SMALL = 101;
}
