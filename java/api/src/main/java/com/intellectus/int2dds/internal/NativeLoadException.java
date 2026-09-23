package com.intellectus.int2dds.internal;

/** Thrown when the int2DDS native library cannot be located, loaded, or verified. */
public class NativeLoadException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public NativeLoadException(String message) {
        super(message);
    }

    public NativeLoadException(String message, Throwable cause) {
        super(message, cause);
    }
}
