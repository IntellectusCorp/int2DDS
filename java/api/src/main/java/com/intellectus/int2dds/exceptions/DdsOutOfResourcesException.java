package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation runs out of resources. */
public class DdsOutOfResourcesException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsOutOfResourcesException() {
        super("DDS out of resources.", ReturnCodeValues.OUT_OF_RESOURCES);
    }
}
