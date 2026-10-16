package kr.co.intellectus.int2dds.internal.ffi;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_INT;
import static java.lang.foreign.ValueLayout.JAVA_LONG;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import kr.co.intellectus.int2dds.internal.NativeLoader;
import kr.co.intellectus.int2dds.listeners.DataReaderListener;
import kr.co.intellectus.int2dds.listeners.DataWriterListener;

/**
 * JDK 22+ counterpart of the JNI {@code FfiHandwritten}: the same methods, implemented in Java over
 * FFM against {@code int2dds_ffi}, with no JNI symbol behind any of them.
 */
public final class FfiHandwritten {
    private FfiHandwritten() {}

    /**
     * The native backend this runtime selected: {@code "ffm"} here, {@code "jni"} for the JNI class
     * this one replaces. Plain Java, so asking never loads a library.
     */
    public static String backend() {
        return "ffm";
    }

    /**
     * Installs {@code listener} on {@code reader} for the given status {@code mask}. Returns the
     * context id to hand to {@link #nativeReaderListenerClear}, or {@code 0} on failure.
     */
    static long nativeReaderListenerSet(long reader, DataReaderListener listener, int mask) {
        return FfmListeners.setReader(reader, listener, mask);
    }

    /** Clears the listener on {@code reader} and forgets {@code ctx}. Returns the C ABI code. */
    static int nativeReaderListenerClear(long reader, long ctx) {
        return FfmListeners.clearReader(reader, ctx);
    }

    /**
     * Installs {@code listener} on {@code writer} for the given status {@code mask}. Returns the
     * context id to hand to {@link #nativeWriterListenerClear}, or {@code 0} on failure.
     */
    static long nativeWriterListenerSet(long writer, DataWriterListener listener, int mask) {
        return FfmListeners.setWriter(writer, listener, mask);
    }

    /** Clears the listener on {@code writer} and forgets {@code ctx}. Returns the C ABI code. */
    static int nativeWriterListenerClear(long writer, long ctx) {
        return FfmListeners.clearWriter(writer, ctx);
    }

    /**
     * Returns properties whose names start with {@code prefix}, as alternating name and value
     * entries in UTF-8. An empty array means no matches or an error.
     *
     * <p>Entries are positional: an empty value still occupies its slot, so entry {@code 2i} is
     * always a name and {@code 2i + 1} its value. A null {@code prefix} reaches the C ABI as a null
     * pointer, which it rejects, so the result is the empty array rather than every property.
     */
    static byte[][] participantQosPropertiesWithPrefix(long qos, byte[] prefix) {
        Pairs pairs = new Pairs();
        try (Arena arena = Arena.ofConfined()) {
            // The C ABI calls back synchronously, on this thread, before it returns, so a stub
            // scoped to this call is never invoked after the arena closes.
            MemorySegment callback =
                    Linker.nativeLinker()
                            .upcallStub(
                                    Properties.COLLECT.bindTo(pairs),
                                    Properties.COLLECT_DESCRIPTOR,
                                    arena);
            MemorySegment prefixSeg = MemorySegment.NULL;
            if (prefix != null) {
                prefixSeg = arena.allocate(prefix.length + 1);
                MemorySegment.copy(prefix, 0, prefixSeg, JAVA_BYTE, 0, prefix.length);
            }
            int rc =
                    (int)
                            Properties.GET_WITH_PREFIX.invokeExact(
                                    qos, prefixSeg, callback, MemorySegment.NULL);
            if (rc != 0 || pairs.failed) {
                return new byte[0][];
            }
        } catch (Throwable t) {
            return new byte[0][];
        }
        return pairs.entries.toArray(new byte[0][]);
    }

    /** Product version of the loaded native library, as UTF-8 bytes. */
    public static byte[] nativeVersion() {
        return NativeLoader.nativeVersion().getBytes(StandardCharsets.UTF_8);
    }

    /**
     * Wraps the {@code cap}-byte region at native address {@code addr} as a direct {@link
     * ByteBuffer}, the reverse of {@code Ffi.directBufferAddress}. Returns {@code null} on failure;
     * callers must reject a null result. The returned buffer is valid only until the loan it came
     * from is committed or aborted.
     */
    static ByteBuffer addressToDirectByteBuffer(long addr, long cap) {
        if (addr == 0L || cap < 0L || cap > Integer.MAX_VALUE) {
            return null;
        }
        return MemorySegment.ofAddress(addr).reinterpret(cap).asByteBuffer();
    }

    /** Accumulator for one {@link #participantQosPropertiesWithPrefix} call. */
    private static final class Pairs {
        final List<byte[]> entries = new ArrayList<>();
        boolean failed;
    }

    /**
     * Called by the C ABI once per matching property. {@code name} and {@code value} are borrowed
     * for the call only, so both are copied. Nothing may escape an upcall — an exception would
     * terminate the JVM — so a failure aborts the iteration and is reported through {@code pairs}.
     */
    private static int collect(
            Pairs pairs, MemorySegment name, MemorySegment value, MemorySegment userData) {
        try {
            if (name.address() == 0L || value.address() == 0L) {
                return 0;
            }
            pairs.entries.add(cString(name));
            pairs.entries.add(cString(value));
            return 0;
        } catch (Throwable t) {
            pairs.failed = true;
            return 1;
        }
    }

    private static byte[] cString(MemorySegment ptr) {
        MemorySegment s = ptr.reinterpret(Long.MAX_VALUE);
        long len = 0;
        while (s.get(JAVA_BYTE, len) != 0) {
            len++;
        }
        return s.asSlice(0, len).toArray(JAVA_BYTE);
    }

    /** Resolved on first use, so {@link #backend()} never loads the library. */
    private static final class Properties {
        static final MethodHandle GET_WITH_PREFIX =
                Linker.nativeLinker()
                        .downcallHandle(
                                FfmLibrary.LOOKUP
                                        .find("int2dds_participant_qos_get_properties_with_prefix")
                                        .orElseThrow(
                                                () ->
                                                        new UnsatisfiedLinkError(
                                                                "int2DDS C ABI symbol not found:"
                                                                        + " int2dds_participant_qos_get_properties_with_prefix")),
                                FunctionDescriptor.of(JAVA_INT, JAVA_LONG, ADDRESS, ADDRESS, ADDRESS));

        static final FunctionDescriptor COLLECT_DESCRIPTOR =
                FunctionDescriptor.of(JAVA_INT, ADDRESS, ADDRESS, ADDRESS);

        static final MethodHandle COLLECT;

        static {
            try {
                COLLECT =
                        MethodHandles.lookup()
                                .findStatic(
                                        FfiHandwritten.class,
                                        "collect",
                                        MethodType.methodType(
                                                int.class,
                                                Pairs.class,
                                                MemorySegment.class,
                                                MemorySegment.class,
                                                MemorySegment.class));
            } catch (ReflectiveOperationException e) {
                throw new ExceptionInInitializerError(e);
            }
        }
    }
}
