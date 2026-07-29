package com.intellectus.int2dds.internal.ffi;

/**
 * Native declarations that the generator cannot express.
 *
 * <p>Kept in a separate class from {@link Ffi} so the generator can overwrite
 * {@code Ffi.java} wholesale without touching hand-written code.
 */
public final class FfiHandwritten {
    private FfiHandwritten() {}

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
}
