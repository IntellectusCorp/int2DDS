package com.intellectus.int2dds.internal.ffi;

import com.intellectus.int2dds.listeners.DataReaderListener;
import com.intellectus.int2dds.listeners.DataWriterListener;

/**
 * Native declarations that the generator cannot express.
 *
 * <p>Kept in a separate class from {@link Ffi} so the generator can overwrite
 * {@code Ffi.java} wholesale without touching hand-written code.
 */
public final class FfiHandwritten {
    private FfiHandwritten() {}

    /**
     * Installs {@code listener} on {@code reader} for the given status
     * {@code mask}. The native side wraps the listener in a global reference
     * held by a binding-owned context and returns that context's pointer, or
     * {@code 0} on failure. The returned pointer must later be handed to
     * {@link #nativeReaderListenerClear} to release the global reference.
     */
    static native long nativeReaderListenerSet(long reader, DataReaderListener listener, int mask);

    /**
     * Clears the listener on {@code reader} and releases the context pointer
     * {@code ctx} returned by {@link #nativeReaderListenerSet}. The held global
     * reference is freed only once any in-flight callback finishes. Returns the
     * C ABI status code of the underlying clear.
     */
    static native int nativeReaderListenerClear(long reader, long ctx);

    /**
     * Installs {@code listener} on {@code writer} for the given status
     * {@code mask}. The native side wraps the listener in a global reference
     * held by a binding-owned context and returns that context's pointer, or
     * {@code 0} on failure. The returned pointer must later be handed to
     * {@link #nativeWriterListenerClear} to release the global reference.
     */
    static native long nativeWriterListenerSet(long writer, DataWriterListener listener, int mask);

    /**
     * Clears the listener on {@code writer} and releases the context pointer
     * {@code ctx} returned by {@link #nativeWriterListenerSet}. The held global
     * reference is freed only once any in-flight callback finishes. Returns the
     * C ABI status code of the underlying clear.
     */
    static native int nativeWriterListenerClear(long writer, long ctx);

    /**
     * Returns properties whose names start with {@code prefix}, as alternating
     * name and value entries in UTF-8. An empty array means no matches or an
     * error.
     *
     * <p>Entries are positional: an empty value still occupies its slot, so
     * entry {@code 2i} is always a name and {@code 2i + 1} its value.
     */
    static native byte[][] participantQosPropertiesWithPrefix(long qos, byte[] prefix);

    /**
     * Product version of the loaded native library, as UTF-8 bytes.
     *
     * <p>Public because {@code NativeLoader} lives in the parent package and
     * needs it to verify the library against the JAR. Everything else here
     * stays package-private.
     */
    public static native byte[] nativeVersion();

    /**
     * Wraps the {@code cap}-byte region at native address {@code addr} as a
     * direct {@link java.nio.ByteBuffer}, the reverse of {@code
     * Ffi.directBufferAddress}. Used to expose the DDS-owned buffer {@code
     * int2dds_datawriter_prepare_serialized_write} returns as an address
     * rather than a {@code JByteBuffer} -- the FFM backend can do this in
     * pure Java, the JNI backend cannot, hence this hand-written helper.
     * Returns {@code null} on failure; callers must reject a null result. The
     * returned buffer is valid only until the loan it came from is committed
     * or aborted.
     */
    static native java.nio.ByteBuffer addressToDirectByteBuffer(long addr, long cap);
}
