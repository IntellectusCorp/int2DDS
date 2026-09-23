package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation is not supported. */
public class DdsUnsupportedException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsUnsupportedException() {
        super("DDS operation not supported.", RET_UNSUPPORTED);
    }

    public DdsUnsupportedException(String message) {
        super(message, RET_UNSUPPORTED);
    }
}
