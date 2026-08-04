package com.intellectus.int2dds.exceptions;

/** Thrown when the native layer receives a null pointer it cannot accept. */
public class DdsNullPointerException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsNullPointerException() {
        super("DDS null pointer.", ReturnCodeValues.NULL_POINTER);
    }
}
