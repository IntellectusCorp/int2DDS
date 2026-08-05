package com.intellectus.int2dds.internal;

import java.lang.ref.PhantomReference;
import java.lang.ref.ReferenceQueue;
import java.util.Collections;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Releases native handles whose owning Java object has been collected.
 *
 * <p>This is what {@code java.lang.ref.Cleaner} does, written out because the
 * baseline is JDK 8 and {@code Cleaner} arrived in 9. {@code finalize()} is not
 * an option either: it is deprecated for removal and can be disabled at
 * runtime, so a JAR that must run on 8 through 25 cannot rely on it.
 *
 * <p>The registered state deliberately never references the owner. If it did,
 * the owner would be strongly reachable from this class's static set and would
 * never be collected — the cleaner would become the leak it exists to prevent.
 */
public final class NativeCleaner {

    /** Releases one native handle. Returns the C ABI status code. */
    public interface Deleter {
        int delete(long handle);
    }

    private static final ReferenceQueue<Object> QUEUE = new ReferenceQueue<Object>();

    /** Keeps the phantom references themselves alive until they are enqueued. */
    private static final Set<Reaper> LIVE =
            Collections.newSetFromMap(new ConcurrentHashMap<Reaper, Boolean>());

    private static final AtomicLong RELEASED = new AtomicLong();
    private static final AtomicLong REAPED = new AtomicLong();
    private static final AtomicLong FAILED = new AtomicLong();

    static {
        Thread t = new Thread(new Runnable() {
            @Override
            public void run() {
                drain();
            }
        }, "int2dds-native-cleaner");
        // A daemon thread does not hold the JVM open. Handles still queued at
        // exit are not released, which is harmless: the process is ending and
        // the OS reclaims everything.
        t.setDaemon(true);
        t.start();
    }

    private NativeCleaner() {}

    /**
     * Arranges for {@code handle} to be released when {@code owner} becomes
     * unreachable, or earlier if the returned handle is closed.
     */
    public static NativeHandle register(Object owner, long handle, Deleter deleter) {
        State state = new State(handle, deleter);
        Reaper reaper = new Reaper(owner, state);
        LIVE.add(reaper);
        return new NativeHandle(state, reaper);
    }

    /** Deletes performed, by either path. */
    public static long releasedCount() {
        return RELEASED.get();
    }

    /** Deletes performed by the reaper rather than by an explicit close. */
    public static long reapedCount() {
        return REAPED.get();
    }

    /** Deletes that returned a non-zero code or threw. */
    public static long failedCount() {
        return FAILED.get();
    }

    private static void drain() {
        for (;;) {
            Reaper reaper;
            try {
                reaper = (Reaper) QUEUE.remove();
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
            LIVE.remove(reaper);
            try {
                if (reaper.state.release()) {
                    REAPED.incrementAndGet();
                }
            } catch (Throwable t) {
                // An exception escaping here would kill this thread and stop
                // every later cleanup. Count it instead; the counter is what
                // makes the loss visible without a logging dependency.
                FAILED.incrementAndGet();
            }
        }
    }

    static void forget(Reaper reaper) {
        LIVE.remove(reaper);
        reaper.clear();
    }

    /** The handle, its deleter and the shared once-only flag. Never the owner. */
    static final class State {
        private final long value;
        private final Deleter deleter;
        private final java.util.concurrent.atomic.AtomicBoolean closed =
                new java.util.concurrent.atomic.AtomicBoolean();
        private volatile int lastCode;

        State(long value, Deleter deleter) {
            this.value = value;
            this.deleter = deleter;
        }

        long value() {
            if (closed.get()) {
                throw new IllegalStateException("native handle is already released");
            }
            return value;
        }

        boolean isClosed() {
            return closed.get();
        }

        int lastCode() {
            return lastCode;
        }

        /** Releases the handle if this call wins the race. True if it did. */
        boolean release() {
            if (!closed.compareAndSet(false, true)) {
                return false;
            }
            RELEASED.incrementAndGet();
            int rc = deleter.delete(value);
            lastCode = rc;
            if (rc != 0) {
                FAILED.incrementAndGet();
            }
            return true;
        }
    }

    static final class Reaper extends PhantomReference<Object> {
        final State state;

        Reaper(Object owner, State state) {
            super(owner, QUEUE);
            this.state = state;
        }
    }
}
