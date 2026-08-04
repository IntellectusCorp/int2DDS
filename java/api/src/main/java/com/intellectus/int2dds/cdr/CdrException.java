package com.intellectus.int2dds.cdr;

/** Base exception for CDR serialization and deserialization errors. */
public class CdrException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public CdrException(String message) {
        super(message);
    }

    public CdrException(String message, Throwable cause) {
        super(message, cause);
    }
}
