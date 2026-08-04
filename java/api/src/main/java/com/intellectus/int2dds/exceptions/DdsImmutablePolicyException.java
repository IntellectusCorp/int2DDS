package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS immutable policy cannot be changed. */
public class DdsImmutablePolicyException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsImmutablePolicyException() {
        super("DDS immutable policy cannot be changed.", RET_IMMUTABLE_POLICY);
    }
}
