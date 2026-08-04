package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS entity has already been deleted. */
public class DdsAlreadyDeletedException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsAlreadyDeletedException() {
        super("DDS entity already deleted.", ReturnCodeValues.ALREADY_DELETED);
    }
}
