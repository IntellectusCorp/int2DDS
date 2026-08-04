package com.intellectus.int2dds.exceptions;

/** Thrown for a general DDS operation failure ({@code INT2DDS_RET_ERROR}). */
public class DdsErrorException extends DdsException {
    private static final long serialVersionUID = 1L;

    private static final String DEFAULT_MESSAGE = "DDS operation failed.";

    public DdsErrorException() {
        super(DEFAULT_MESSAGE, ReturnCodeValues.ERROR);
    }

    /** Uses the native error message when present, falling back to the
     *  default when {@code message} is null or empty. */
    public DdsErrorException(String message) {
        super((message == null || message.isEmpty()) ? DEFAULT_MESSAGE : message,
                ReturnCodeValues.ERROR);
    }
}
