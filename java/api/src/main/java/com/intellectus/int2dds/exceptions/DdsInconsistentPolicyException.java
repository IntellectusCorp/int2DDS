package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS policy combination is inconsistent. */
public class DdsInconsistentPolicyException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsInconsistentPolicyException() {
        super("DDS inconsistent policy.", ReturnCodeValues.INCONSISTENT_POLICY);
    }
}
