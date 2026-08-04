package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation has no data available. */
public class DdsNoDataException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsNoDataException() {
        super("DDS no data available.", ReturnCodeValues.NO_DATA);
    }
}
