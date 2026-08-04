package com.intellectus.int2dds.exceptions;

/** Thrown when a DDS operation's precondition is not met. */
public class DdsPreconditionNotMetException extends DdsException {
    private static final long serialVersionUID = 1L;

    public DdsPreconditionNotMetException() {
        super("DDS precondition not met.", ReturnCodeValues.PRECONDITION_NOT_MET);
    }
}
