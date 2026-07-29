package com.intellectus.int2dds.internal.ffi;

import com.intellectus.int2dds.internal.NativeLoader;
import java.nio.ByteBuffer;

/**
 * Package-visible bridge to the generated {@link Ffi} declarations.
 *
 * <p>Exists so callers get the native library loaded automatically and so that
 * hand-written code never has to live inside the generated file.
 */
public final class FfiAccess {

    static {
        NativeLoader.load();
    }

    private FfiAccess() {}

    /** The library's default type extensibility: 0 Final, 1 Appendable, 2 Mutable. */
    public static int defaultExtensibility() {
        return Ffi.int2dds_default_extensibility();
    }

    /** The library's default data representation. */
    public static int defaultDataRepresentation() {
        return Ffi.int2dds_default_data_representation();
    }

    /**
     * Copies the last error message into {@code buf} as UTF-8 and returns the
     * message's full length, which may exceed {@code buf.length}.
     */
    public static int lastErrorMessage(byte[] buf) {
        return Ffi.int2dds_last_error_message(buf, buf.length);
    }

    /** Native address of a direct ByteBuffer, or 0 if the buffer is not direct. */
    public static long directBufferAddress(ByteBuffer buf) {
        return Ffi.directBufferAddress(buf);
    }

    /**
     * Creates a dynamic value holding {@code value}, writing the handle to the
     * native address {@code out}, which must be at least 8 bytes.
     */
    public static int dynamicValueI32(int value, long out) {
        return Ffi.int2dds_dynamic_value_i32(value, out);
    }

    /**
     * Renders a dynamic value into {@code buf} as UTF-8 and writes the text's
     * byte length to the native address {@code outLen}. The buffer must hold the
     * text plus a trailing NUL.
     */
    public static int dynamicValueToString(long value, byte[] buf, long bufLen, long outLen) {
        return Ffi.int2dds_dynamic_value_to_string(value, buf, bufLen, outLen);
    }

    /** Releases a dynamic value handle. */
    public static void dynamicValueDestroy(long value) {
        Ffi.int2dds_dynamic_value_destroy(value);
    }
}
