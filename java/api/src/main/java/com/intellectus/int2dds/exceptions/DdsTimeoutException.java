package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation times out. */
public class DdsTimeoutException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsTimeoutException() {
        super("DDS operation timed out.", RET_TIMEOUT);
    }
}
