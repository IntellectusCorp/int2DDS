package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS entity is not enabled. */
public class DdsNotEnabledException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsNotEnabledException() {
        super("DDS entity not enabled.", ReturnCodeValues.NOT_ENABLED);
    }
}
