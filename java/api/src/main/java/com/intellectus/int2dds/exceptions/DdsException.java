package com.intellectus.int2dds.exceptions;

/** Base exception for DDS operations. Unchecked: DDS return codes accompany
 *  nearly every call, and a checked type would put {@code throws} on the whole
 *  public API. */
public class DdsException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    /**
     * The {@code Int2DdsRet} values, transcribed from {@code ffi/src/error.rs} —
     * the single source of truth for this mapping.
     */
    public static final int RET_OK = 0;

    // General errors
    public static final int RET_ERROR = 1;
    public static final int RET_TIMEOUT = 2;
    public static final int RET_UNSUPPORTED = 3;
    public static final int RET_INVALID_ARGUMENT = 11;

    // DDS-specific errors
    public static final int RET_ALREADY_DELETED = 20;
    public static final int RET_NOT_ENABLED = 21;
    public static final int RET_IMMUTABLE_POLICY = 22;
    public static final int RET_INCONSISTENT_POLICY = 23;
    public static final int RET_PRECONDITION_NOT_MET = 24;
    public static final int RET_OUT_OF_RESOURCES = 25;
    public static final int RET_ILLEGAL_OPERATION = 26;
    public static final int RET_NO_DATA = 27;

    // FFI-boundary errors
    public static final int RET_NULL_POINTER = 100;
    public static final int RET_BUFFER_TOO_SMALL = 101;

    private final int code;

    public DdsException(String message, int code) {
        super(message);
        this.code = code;
    }

    public DdsException(String message, int code, Throwable cause) {
        super(message, cause);
        this.code = code;
    }

    /** The {@code Int2DdsRet} value that produced this exception. */
    public int getCode() {
        return code;
    }
}
