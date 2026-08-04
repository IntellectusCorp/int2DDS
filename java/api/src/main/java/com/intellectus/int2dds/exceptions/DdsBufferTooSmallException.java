package com.intellectus.int2dds.exceptions;

/** Thrown when a caller-supplied buffer was too small for the native result. */
public class DdsBufferTooSmallException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsBufferTooSmallException() {
        super("DDS buffer too small.", RET_BUFFER_TOO_SMALL);
    }
}
