package com.intellectus.int2dds.cdr;

/** Thrown when a CDR writer's buffer would grow past its allowed cap. */
public class CdrOverflowException extends CdrException {
    private static final long serialVersionUID = 1L;

    public CdrOverflowException() {
        super("CDR writer buffer overflow.");
    }

    public CdrOverflowException(String message) {
        super(message);
    }
}
