package com.intellectus.int2dds.cdr;

/** Thrown when a CDR reader runs out of bytes before decoding completes. */
public class CdrUnderflowException extends CdrException {
    private static final long serialVersionUID = 1L;

    public CdrUnderflowException() {
        super("CDR reader buffer underflow.");
    }

    public CdrUnderflowException(String message) {
        super(message);
    }
}
