package com.intellectus.int2dds.cdr;

/** Thrown when a CDR encapsulation header carries an unrecognized id. */
public class CdrInvalidEncapsulationException extends CdrException {
    private static final long serialVersionUID = 1L;

    public CdrInvalidEncapsulationException() {
        super("Unrecognized CDR encapsulation ID.");
    }

    public CdrInvalidEncapsulationException(String message) {
        super(message);
    }
}
