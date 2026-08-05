package com.intellectus.int2dds.internal;

import com.intellectus.int2dds.exceptions.DdsException;
import java.lang.ref.PhantomReference;
import java.lang.ref.ReferenceQueue;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Iterator;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicInteger;
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
 *
 * <p><b>This class does not, and cannot, order the release of a parent and
 * child relative to each other.</b> A strong reference from a child's owner to
 * its parent (or its parent's {@link NativeHandle}) is still worth holding —
 * it keeps the parent phantom-unreachable, and therefore un-reaped, for as
 * long as the child is in use — but it does not make the JVM release the
 * parent after the child. When both become unreachable in the same GC cycle,
 * both phantom references are discovered and enqueued in that same cycle, and
 * nothing specifies an order between two references discovered together.
 * Ordering is instead the native layer's job: {@code int2dds_delete_*}
 * refuses to delete an entity with live children ({@link
 * DdsException#RET_PRECONDITION_NOT_MET}) and leaves the handle valid and the
 * entity alive rather than deleting it, so a refused attempt is safe to retry
 * once the child is actually gone. {@link #drain()} below is what retries it.
 */
public final class NativeCleaner {

    /** Releases one native handle. Returns the C ABI status code. */
    public interface Deleter {
        int delete(long handle);
    }

    /**
     * The result of one release attempt.
     *
     * <p>{@code REFUSED} is distinct from {@code FAILED}: it means the native
     * layer's own "not while this still has live children" precondition check
     * rejected the delete and left the handle untouched and still valid — the
     * entity was never freed, so retrying later is safe and expected to
     * eventually succeed. {@code FAILED} covers every other non-zero code, and
     * a thrown deleter; those are not retried.
     */
    enum ReleaseOutcome {
        RELEASED,
        REFUSED,
        FAILED,
        ALREADY_HANDLED
    }

    /**
     * The native layer's numbering for "precondition not met" — see {@code
     * ffi/src/error.rs}'s {@code INT2DDS_RET_PRECONDITION_NOT_MET}. Read off
     * {@link DdsException} rather than repeated as a literal, since that class
     * is this codebase's single transcription of the mapping.
     */
    private static final int RET_PRECONDITION_NOT_MET = DdsException.RET_PRECONDITION_NOT_MET;

    /**
     * How often {@link #drain()} rechecks a non-empty deferred list even
     * without a new phantom-reference event. An explicit {@link
     * NativeHandle#close()} that releases a blocking child produces no queue
     * event of its own, so without this a deferred parent could sit refused
     * forever even after nothing was left to block it.
     */
    private static final long DEFERRED_RETRY_INTERVAL_MS = 100L;

    private static final ReferenceQueue<Object> QUEUE = new ReferenceQueue<Object>();

    /** Keeps the phantom references themselves alive until they are enqueued. */
    private static final Set<Reaper> LIVE =
            Collections.newSetFromMap(new ConcurrentHashMap<Reaper, Boolean>());

    private static final AtomicLong RELEASED = new AtomicLong();
    private static final AtomicLong REAPED = new AtomicLong();
    private static final AtomicLong FAILED = new AtomicLong();
    private static final AtomicLong DEFERRED = new AtomicLong();

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

    /** Deletes that actually happened, by either path. Refusals do not count. */
    public static long releasedCount() {
        return RELEASED.get();
    }

    /** Deletes performed by the reaper rather than by an explicit close. */
    public static long reapedCount() {
        return REAPED.get();
    }

    /** Deletes that returned a non-zero, non-refusal code, or threw. */
    public static long failedCount() {
        return FAILED.get();
    }

    /**
     * Handles currently refused because a live child is blocking their
     * release. The reaper retries them automatically as other handles
     * release; a handle can legitimately stay counted here indefinitely if
     * its blocking child is never released.
     */
    public static long deferredCount() {
        return DEFERRED.get();
    }

    /**
     * The reaper loop. Blocks for the next phantom reference when there is
     * nothing deferred; otherwise wakes at least every {@link
     * #DEFERRED_RETRY_INTERVAL_MS} to retry the deferred list even absent a
     * new queue event.
     *
     * <p>{@code deferred} is local to this method and touched only by this
     * thread — the daemon thread started above is the only caller — so it
     * needs no synchronization of its own; {@link #DEFERRED} is the only part
     * of this state other threads can observe, via {@link #deferredCount()}.
     */
    private static void drain() {
        List<Reaper> deferred = new ArrayList<Reaper>();
        for (;;) {
            Reaper reaper;
            try {
                reaper = deferred.isEmpty()
                        ? (Reaper) QUEUE.remove()
                        : (Reaper) QUEUE.remove(DEFERRED_RETRY_INTERVAL_MS);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
            try {
                if (reaper != null) {
                    LIVE.remove(reaper);
                    if (attempt(reaper, deferred)) {
                        sweepDeferred(deferred);
                    }
                } else if (!deferred.isEmpty()) {
                    // Woke on the retry timeout with no new queue event — an
                    // explicit close() elsewhere may have unblocked a deferred
                    // parent without producing one.
                    sweepDeferred(deferred);
                }
            } catch (Throwable t) {
                // Defensive: State.release() already contains the deleter's own
                // exceptions, but a bug here escaping would still kill this
                // thread and silently stop every later cleanup.
                FAILED.incrementAndGet();
            }
        }
    }

    /**
     * One release attempt for a freshly dequeued reaper.
     *
     * @return true if it released (so the caller should sweep the deferred
     *     list — this success may have been the last thing blocking another
     *     entry in it)
     */
    private static boolean attempt(Reaper reaper, List<Reaper> deferred) {
        ReleaseOutcome outcome = reaper.state.release();
        if (outcome == ReleaseOutcome.RELEASED) {
            REAPED.incrementAndGet();
            return true;
        }
        if (outcome == ReleaseOutcome.REFUSED) {
            deferred.add(reaper);
            DEFERRED.incrementAndGet();
        }
        // FAILED and ALREADY_HANDLED: nothing more to do with this reaper.
        return false;
    }

    /**
     * Retries every deferred reaper, and keeps retrying the whole list while a
     * pass releases at least one of them — a release can be exactly what
     * unblocks another entry (a grandparent whose only child was the parent
     * just freed here). Terminates because each reaper can move off this list
     * at most once: {@link NativeCleaner.State#release()} only ever
     * transitions a handle to {@code CLOSED} a single time, so a pass either
     * frees at least one entry for good or frees none, and a list that
     * shrinks by at least one every productive pass reaches empty (or
     * all-still-refused) in finitely many passes.
     */
    private static void sweepDeferred(List<Reaper> deferred) {
        boolean progress = true;
        while (progress && !deferred.isEmpty()) {
            progress = false;
            Iterator<Reaper> it = deferred.iterator();
            while (it.hasNext()) {
                Reaper r = it.next();
                ReleaseOutcome outcome = r.state.release();
                if (outcome == ReleaseOutcome.REFUSED) {
                    continue; // still blocked; leave it for a later pass
                }
                it.remove();
                DEFERRED.decrementAndGet();
                if (outcome == ReleaseOutcome.RELEASED) {
                    REAPED.incrementAndGet();
                    progress = true;
                }
                // FAILED / ALREADY_HANDLED: drop it; not retried further.
            }
        }
    }

    static void forget(Reaper reaper) {
        LIVE.remove(reaper);
        reaper.clear();
    }

    /** The handle, its deleter and the shared once-only state. Never the owner. */
    static final class State {
        private static final int OPEN = 0;
        private static final int RELEASING = 1;
        private static final int CLOSED = 2;

        /** {@link #lastCode()} when the deleter threw instead of returning a code. */
        static final int NO_CODE = -1;

        private final long value;
        private final Deleter deleter;
        private final AtomicInteger status = new AtomicInteger(OPEN);
        private volatile int lastCode;

        State(long value, Deleter deleter) {
            this.value = value;
            this.deleter = deleter;
        }

        long value() {
            if (status.get() == CLOSED) {
                throw new IllegalStateException("native handle is already released");
            }
            return value;
        }

        boolean isClosed() {
            return status.get() == CLOSED;
        }

        /** The C ABI code from the most recent attempt, or {@link #NO_CODE}. */
        int lastCode() {
            return lastCode;
        }

        /**
         * Attempts the release. Claims the attempt with a CAS from {@code
         * OPEN} to {@code RELEASING} first, so of every caller that might
         * race here — an explicit {@code close()}, the reaper, and a retry of
         * this same handle off the deferred list — at most one ever reaches
         * the deleter for a given attempt.
         *
         * <p>{@code RELEASING} is held exactly as long as the deleter call
         * takes: success moves it to the terminal {@code CLOSED}, and a
         * refusal or any other failure moves it back to {@code OPEN} so the
         * handle stays usable and a later attempt can still win the CAS. A
         * failed delete must never be mistaken for a closed one — the native
         * side did not free anything, so marking this closed anyway would
         * strand a live entity with no way to ever delete it.
         */
        ReleaseOutcome release() {
            if (!status.compareAndSet(OPEN, RELEASING)) {
                return ReleaseOutcome.ALREADY_HANDLED;
            }
            int rc;
            try {
                rc = deleter.delete(value);
            } catch (Throwable t) {
                lastCode = NO_CODE;
                status.set(OPEN);
                FAILED.incrementAndGet();
                return ReleaseOutcome.FAILED;
            }
            lastCode = rc;
            if (rc == 0) {
                status.set(CLOSED);
                RELEASED.incrementAndGet();
                return ReleaseOutcome.RELEASED;
            }
            status.set(OPEN);
            if (rc == RET_PRECONDITION_NOT_MET) {
                return ReleaseOutcome.REFUSED;
            }
            FAILED.incrementAndGet();
            return ReleaseOutcome.FAILED;
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
