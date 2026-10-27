package kr.co.intellectus.int2dds.internal.ffi;

import static java.lang.foreign.ValueLayout.ADDRESS;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import kr.co.intellectus.int2dds.internal.StatusDecoder;
import kr.co.intellectus.int2dds.listeners.DataReaderListener;
import kr.co.intellectus.int2dds.listeners.DataWriterListener;

/**
 * Bridges DDS listener callbacks — fired on native DDS background threads — into a Java {@code
 * DataReaderListener} or {@code DataWriterListener} through FFM upcalls. The JDK 22+ replacement
 * for the JNI trampolines in {@code java/native/src/trampoline.rs}, with the same contract: every
 * callback is installed and {@code mask} decides which ones DDS fires, the entity argument is null
 * (the wrapper listener in {@code DataReader}/{@code DataWriter} substitutes the Java entity), and
 * a Java exception is reported and swallowed rather than allowed onto the DDS thread.
 *
 * <h2>Teardown safety — id registry, never-freed stubs</h2>
 *
 * The core clones its listener and releases its lock <em>before</em> invoking a callback, and
 * holds {@code user_context} only as an opaque value. So a clear on another thread can race a
 * callback that is already on its way in. Two things make that race harmless:
 *
 * <ul>
 *   <li>The upcall stubs are created once, in the global arena, and never freed. A stub freed on
 *       clear could be entered after it was freed, which crashes the process.
 *   <li>{@code user_context} carries an integer id into a registry, not a reference. A clear
 *       removes the id; a callback that finds no id returns without calling anything.
 * </ul>
 */
final class FfmListeners {
    private FfmListeners() {}

    private static final ConcurrentHashMap<Long, DataReaderListener> READERS =
            new ConcurrentHashMap<>();
    private static final ConcurrentHashMap<Long, DataWriterListener> WRITERS =
            new ConcurrentHashMap<>();

    /** Next id to hand out. Starts at 1 so 0 can mean "no listener" on the Java side. */
    private static final AtomicLong NEXT_ID = new AtomicLong(1);

    static long setReader(long reader, DataReaderListener listener, int mask) {
        long id = NEXT_ID.getAndIncrement();
        READERS.put(id, listener);
        int rc;
        try (Arena arena = Arena.ofConfined()) {
            // Int2DdsDataReaderListener: seven callbacks then user_context. The C ABI copies
            // the struct, so it only has to outlive this call.
            MemorySegment[] callbacks = ReaderStubs.ALL;
            MemorySegment s = arena.allocate(ADDRESS, callbacks.length + 1);
            for (int i = 0; i < callbacks.length; i++) {
                s.setAtIndex(ADDRESS, i, callbacks[i]);
            }
            s.setAtIndex(ADDRESS, callbacks.length, MemorySegment.ofAddress(id));
            rc = Ffi.int2dds_datareader_set_listener(reader, s.address(), mask);
        }
        if (rc != 0) {
            // Registration failed: forget the listener so it does not leak.
            READERS.remove(id);
            return 0L;
        }
        return id;
    }

    static int clearReader(long reader, long id) {
        int rc = Ffi.int2dds_datareader_set_listener(reader, 0L, 0);
        if (id != 0L) {
            READERS.remove(id);
        }
        return rc;
    }

    static long setWriter(long writer, DataWriterListener listener, int mask) {
        long id = NEXT_ID.getAndIncrement();
        WRITERS.put(id, listener);
        int rc;
        try (Arena arena = Arena.ofConfined()) {
            // Int2DdsDataWriterListener: four callbacks then user_context.
            MemorySegment[] callbacks = WriterStubs.ALL;
            MemorySegment s = arena.allocate(ADDRESS, callbacks.length + 1);
            for (int i = 0; i < callbacks.length; i++) {
                s.setAtIndex(ADDRESS, i, callbacks[i]);
            }
            s.setAtIndex(ADDRESS, callbacks.length, MemorySegment.ofAddress(id));
            rc = Ffi.int2dds_datawriter_set_listener(writer, s.address(), mask);
        }
        if (rc != 0) {
            WRITERS.remove(id);
            return 0L;
        }
        return id;
    }

    static int clearWriter(long writer, long id) {
        int rc = Ffi.int2dds_datawriter_set_listener(writer, 0L, 0);
        if (id != 0L) {
            WRITERS.remove(id);
        }
        return rc;
    }

    // --- DataReader upcalls. Field order of Int2DdsDataReaderListener. ---

    private static void onDataAvailable(MemorySegment reader, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null) {
                l.onDataAvailable(null);
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onSubscriptionMatched(
            MemorySegment reader, MemorySegment status, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onSubscriptionMatched(
                        null,
                        StatusDecoder.subscriptionMatched(
                                view(status, StatusDecoder.SUBSCRIPTION_MATCHED_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onSampleRejected(
            MemorySegment reader, MemorySegment status, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onSampleRejected(
                        null,
                        StatusDecoder.sampleRejected(
                                view(status, StatusDecoder.SAMPLE_REJECTED_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onLivelinessChanged(
            MemorySegment reader, MemorySegment status, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onLivelinessChanged(
                        null,
                        StatusDecoder.livelinessChanged(
                                view(status, StatusDecoder.LIVELINESS_CHANGED_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onRequestedDeadlineMissed(
            MemorySegment reader, MemorySegment status, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onRequestedDeadlineMissed(
                        null,
                        StatusDecoder.requestedDeadlineMissed(
                                view(status, StatusDecoder.DEADLINE_MISSED_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onRequestedIncompatibleQos(
            MemorySegment reader, MemorySegment status, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onRequestedIncompatibleQos(
                        null,
                        StatusDecoder.requestedIncompatibleQos(
                                view(status, StatusDecoder.INCOMPATIBLE_QOS_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onSampleLost(MemorySegment reader, MemorySegment status, MemorySegment ctx) {
        try {
            DataReaderListener l = READERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onSampleLost(
                        null,
                        StatusDecoder.sampleLost(view(status, StatusDecoder.COUNT_PAIR_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    // --- DataWriter upcalls. Field order of Int2DdsDataWriterListener. ---

    private static void onPublicationMatched(
            MemorySegment writer, MemorySegment status, MemorySegment ctx) {
        try {
            DataWriterListener l = WRITERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onPublicationMatched(
                        null,
                        StatusDecoder.publicationMatched(
                                view(status, StatusDecoder.PUBLICATION_MATCHED_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onOfferedDeadlineMissed(
            MemorySegment writer, MemorySegment status, MemorySegment ctx) {
        try {
            DataWriterListener l = WRITERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onOfferedDeadlineMissed(
                        null,
                        StatusDecoder.offeredDeadlineMissed(
                                view(status, StatusDecoder.DEADLINE_MISSED_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onOfferedIncompatibleQos(
            MemorySegment writer, MemorySegment status, MemorySegment ctx) {
        try {
            DataWriterListener l = WRITERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onOfferedIncompatibleQos(
                        null,
                        StatusDecoder.offeredIncompatibleQos(
                                view(status, StatusDecoder.INCOMPATIBLE_QOS_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    private static void onLivelinessLost(
            MemorySegment writer, MemorySegment status, MemorySegment ctx) {
        try {
            DataWriterListener l = WRITERS.get(ctx.address());
            if (l != null && status.address() != 0L) {
                l.onLivelinessLost(
                        null,
                        StatusDecoder.livelinessLost(view(status, StatusDecoder.COUNT_PAIR_SIZE)));
            }
        } catch (Throwable t) {
            report(t);
        }
    }

    /** The {@code size}-byte struct at {@code ptr}, borrowed for the callback's duration. */
    private static ByteBuffer view(MemorySegment ptr, int size) {
        return ptr.reinterpret(size).asByteBuffer().order(ByteOrder.nativeOrder());
    }

    /**
     * An exception escaping an upcall terminates the JVM, and it must never reach the DDS thread
     * either; report it the way the JNI trampolines do and keep the thread running.
     */
    private static void report(Throwable t) {
        try {
            t.printStackTrace();
        } catch (Throwable ignored) {
            // Nothing left to do safely.
        }
    }

    private static MemorySegment stub(String name, FunctionDescriptor descriptor) {
        try {
            MethodType type = descriptor.toMethodType();
            return Linker.nativeLinker()
                    .upcallStub(
                            MethodHandles.lookup().findStatic(FfmListeners.class, name, type),
                            descriptor,
                            Arena.global());
        } catch (ReflectiveOperationException e) {
            throw new ExceptionInInitializerError(e);
        }
    }

    private static final FunctionDescriptor NO_STATUS = FunctionDescriptor.ofVoid(ADDRESS, ADDRESS);
    private static final FunctionDescriptor WITH_STATUS =
            FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, ADDRESS);

    /** Created on the first reader listener install, then kept for the life of the process. */
    private static final class ReaderStubs {
        static final MemorySegment[] ALL = {
            stub("onDataAvailable", NO_STATUS),
            stub("onSubscriptionMatched", WITH_STATUS),
            stub("onSampleRejected", WITH_STATUS),
            stub("onLivelinessChanged", WITH_STATUS),
            stub("onRequestedDeadlineMissed", WITH_STATUS),
            stub("onRequestedIncompatibleQos", WITH_STATUS),
            stub("onSampleLost", WITH_STATUS),
        };
    }

    /** Created on the first writer listener install, then kept for the life of the process. */
    private static final class WriterStubs {
        static final MemorySegment[] ALL = {
            stub("onPublicationMatched", WITH_STATUS),
            stub("onOfferedDeadlineMissed", WITH_STATUS),
            stub("onOfferedIncompatibleQos", WITH_STATUS),
            stub("onLivelinessLost", WITH_STATUS),
        };
    }
}
