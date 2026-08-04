package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation receives an invalid argument. */
public class DdsInvalidArgumentException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsInvalidArgumentException() {
        super("DDS invalid argument.", ReturnCodeValues.INVALID_ARGUMENT);
    }
}
