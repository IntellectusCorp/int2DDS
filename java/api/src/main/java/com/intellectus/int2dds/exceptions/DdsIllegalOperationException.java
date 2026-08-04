package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation is illegal in the current state. */
public class DdsIllegalOperationException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsIllegalOperationException() {
        super("DDS illegal operation.", ReturnCodeValues.ILLEGAL_OPERATION);
    }
}
