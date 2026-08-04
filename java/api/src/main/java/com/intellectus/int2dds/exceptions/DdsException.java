package com.intellectus.int2dds.exceptions;

/** Base exception for DDS operations. Unchecked: DDS return codes accompany
 *  nearly every call, and a checked type would put {@code throws} on the whole
 *  public API. */
public class DdsException extends RuntimeException {
    private static final long serialVersionUID = 1L;

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
