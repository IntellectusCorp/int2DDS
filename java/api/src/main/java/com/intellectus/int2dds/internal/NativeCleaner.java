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
 * restores the caller's handle rather than freeing it on every failure path,
 * not only a refusal, so any failed attempt — refused because a child is
 * still alive, or failed for some other transient reason — is safe to retry.
 * {@link #drain()} below is what retries it, for as long as it takes.
 */
public final class NativeCleaner {

    /** Releases one native handle. Returns the C ABI status code. */
    public interface Deleter {
        int delete(long handle);
    }

    /**
     * The result of one release attempt.
     *
     * <p>{@code REFUSED} is the native layer's own "not while this still has
     * live children" precondition check; {@code FAILED} covers every other
     * non-zero code, and a thrown deleter. Both leave the handle untouched
     * and still valid — the native restore-on-failure contract this class
     * relies on does not distinguish between them — so both are retried the
     * same way. They are kept as separate outcomes because {@link
     * NativeHandle#close()} reports them differently: a refusal throws,
     * naming specifically what happened, while another failure returns the
     * raw code as it always has.
     */
    enum ReleaseOutcome {
        RELEASED,
        REFUSED,
        FAILED,
        ALREADY_HANDLED
    }

    /**
     * An outcome and the C ABI code that produced it, returned together.
     *
     * <p>{@code status} reverts to {@code OPEN} on anything but success, so a
     * fresh attempt — from another thread, or a later retry of this same
     * handle — can start immediately after this one returns. If the code
     * were kept in a shared field instead of returned here, a caller reading
     * it after the fact could read a different attempt's code than the one
     * whose outcome it just received.
     */
    static final class Result {
        static final int NO_CODE = -1;

        final ReleaseOutcome outcome;
        final int code;

        Result(ReleaseOutcome outcome, int code) {
            this.outcome = outcome;
            this.code = code;
        }
    }

    /**
     * The native layer's numbering for "precondition not met" — see {@code
     * ffi/src/error.rs}'s {@code INT2DDS_RET_PRECONDITION_NOT_MET}. Read off
     * {@link DdsException} rather than repeated as a literal, since that class
     * is this codebase's single transcription of the mapping.
     */
    private static final int RET_PRECONDITION_NOT_MET = DdsException.RET_PRECONDITION_NOT_MET;

    /**
     * How often {@link #drain()} wakes to check the deferred list for due
     * entries, even without a new phantom-reference event — an explicit
     * {@link NativeHandle#close()} that releases a blocking child produces no
     * queue event of its own. This is a cheap Java-side check only: whether a
     * given entry is actually retried (the expensive part — a real native
     * delete attempt, typically a lock and a list walk on the core side, not
     * a no-op) is separately gated by that entry's own backoff below.
     */
    private static final long WAKEUP_INTERVAL_MS = 100L;

    /** A deferred entry's backoff starts at one wakeup interval ... */
    private static final long INITIAL_BACKOFF_NANOS = WAKEUP_INTERVAL_MS * 1_000_000L;

    /** ... and doubles on every still-blocked retry, capped at a few seconds.
     *  A handle can be deferred for a structural reason that will never
     *  resolve (a builtin entity, for instance) as easily as a live child
     *  that eventually goes away, and this class cannot tell which — so it
     *  never stops retrying, only slows down. */
    private static final long MAX_BACKOFF_NANOS = 4_000_000_000L;

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

    /** Deletes that actually happened, by either path. Refusals and other
     *  failures do not count, however many times they were attempted. */
    public static long releasedCount() {
        return RELEASED.get();
    }

    /** Deletes performed by the reaper rather than by an explicit close. */
    public static long reapedCount() {
        return REAPED.get();
    }

    /** Failed release attempts — refused or otherwise — counted per attempt,
     *  not per handle: a handle retried five times before succeeding, or
     *  before being given up on by an explicit close, adds five here. */
    public static long failedCount() {
        return FAILED.get();
    }

    /**
     * Handles currently blocked — refused because a live child is in the
     * way, or failed for some other reason the native restore-on-failure
     * contract says is safe to retry. The reaper retries them automatically,
     * with backoff, as other handles release. A handle can legitimately stay
     * counted here for the life of the JVM: nothing distinguishes a live
     * child that will eventually go away from a delete that can structurally
     * never succeed (a builtin entity, for instance), so this class never
     * stops retrying either kind.
     */
    public static long deferredCount() {
        return DEFERRED.get();
    }

    /**
     * The reaper loop. Blocks for the next phantom reference when there is
     * nothing deferred; otherwise wakes at least every {@link
     * #WAKEUP_INTERVAL_MS} to check the deferred list, though an individual
     * entry is only actually retried once its own backoff has elapsed.
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
                        : (Reaper) QUEUE.remove(WAKEUP_INTERVAL_MS);
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
                return;
            }
            try {
                if (reaper != null) {
                    LIVE.remove(reaper);
                    // A fresh release always ignores backoff: it is a real
                    // event, not a speculative poll, and cannot itself be the
                    // thing responsible for excess retry cost.
                    if (attempt(reaper, deferred)) {
                        sweepDeferred(deferred, false);
                    }
                } else if (!deferred.isEmpty()) {
                    // Woke on the wakeup interval with no new queue event —
                    // check the deferred list, but only actually retry entries
                    // whose own backoff has elapsed.
                    sweepDeferred(deferred, true);
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
        Result result = reaper.state.release();
        if (result.outcome == ReleaseOutcome.RELEASED) {
            REAPED.incrementAndGet();
            return true;
        }
        if (result.outcome == ReleaseOutcome.REFUSED || result.outcome == ReleaseOutcome.FAILED) {
            reaper.backoffNanos = INITIAL_BACKOFF_NANOS;
            reaper.retryNotBeforeNanos = System.nanoTime() + reaper.backoffNanos;
            deferred.add(reaper);
            DEFERRED.incrementAndGet();
        }
        // ALREADY_HANDLED: nothing more to do with this reaper.
        return false;
    }

    /**
     * Retries deferred reapers, and keeps retrying the whole list while a
     * pass releases at least one of them — a release can be exactly what
     * unblocks another entry (a grandparent whose only child was the parent
     * just freed here). Terminates because each reaper can leave this list in
     * the released state at most once: {@link NativeCleaner.State#release()}
     * only ever transitions a handle to {@code CLOSED} a single time, so a
     * pass either frees at least one entry for good or frees none, and a list
     * that shrinks by at least one every productive pass reaches empty (or
     * all-still-blocked) in finitely many passes.
     *
     * @param respectBackoff false to retry every entry regardless of its
     *     schedule (a fresh success just happened; that is a real signal
     *     worth checking everything against, not excess polling); true to
     *     skip entries not yet due (a bare wakeup, not itself a sign anything
     *     changed)
     */
    private static void sweepDeferred(List<Reaper> deferred, boolean respectBackoff) {
        boolean progress = true;
        while (progress && !deferred.isEmpty()) {
            progress = false;
            long now = System.nanoTime();
            Iterator<Reaper> it = deferred.iterator();
            while (it.hasNext()) {
                Reaper r = it.next();
                if (respectBackoff && now - r.retryNotBeforeNanos < 0) {
                    continue; // not due yet; leave it for a later wakeup
                }
                Result result = r.state.release();
                if (result.outcome == ReleaseOutcome.REFUSED || result.outcome == ReleaseOutcome.FAILED) {
                    r.backoffNanos = Math.min(r.backoffNanos * 2, MAX_BACKOFF_NANOS);
                    r.retryNotBeforeNanos = System.nanoTime() + r.backoffNanos;
                    continue; // still blocked; leave it in the list
                }
                // RELEASED or ALREADY_HANDLED: this entry leaves the list
                // either way. Only RELEASED actually deleted anything; the
                // gauge must not drop except when a handle actually left in
                // the released state or was genuinely handled elsewhere.
                it.remove();
                DEFERRED.decrementAndGet();
                if (result.outcome == ReleaseOutcome.RELEASED) {
                    REAPED.incrementAndGet();
                    progress = true;
                }
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

        private final long value;
        private final Deleter deleter;
        private final AtomicInteger status = new AtomicInteger(OPEN);

        State(long value, Deleter deleter) {
            this.value = value;
            this.deleter = deleter;
        }

        /**
         * The raw handle. Throws once released.
         *
         * <p>{@code CLOSED} is set only after the deleter call returns
         * successfully — not before it is made — so this can still observe a
         * pointer as valid a moment before the underlying native object is
         * actually freed by a concurrent, in-flight release. That window is
         * inherent to handing back a raw {@code long} rather than a checked-
         * out resource; it is not closed here. A caller with a genuine
         * concurrent-use requirement needs synchronization of its own above
         * this layer.
         */
        long value() {
            if (status.get() == CLOSED) {
                throw new IllegalStateException("native handle is already released");
            }
            return value;
        }

        boolean isClosed() {
            return status.get() == CLOSED;
        }

        /**
         * Attempts the release. Claims the attempt with a CAS from {@code
         * OPEN} to {@code RELEASING}, so of every caller that might race here
         * — an explicit {@code close()}, the reaper, and a retry of this same
         * handle off the deferred list — at most one ever reaches the
         * deleter at a time.
         *
         * <p>If the CAS loses because another attempt is already {@code
         * RELEASING}, this waits for it rather than reporting {@code
         * ALREADY_HANDLED} on the strength of a status that might revert to
         * {@code OPEN} a moment later — that would let a caller believe a
         * refusal or failure elsewhere was a success. The wait always
         * terminates: the in-flight attempt's critical section is a single
         * deleter call, which this class's own structure guarantees leaves
         * {@code RELEASING} for {@code CLOSED} or back to {@code OPEN} before
         * its {@code release()} returns. Once it does, this either reports
         * that attempt's real outcome ({@code CLOSED} reached — this call did
         * nothing, {@code ALREADY_HANDLED}) or takes the attempt itself (back
         * to {@code OPEN} — loop and race the CAS again).
         *
         * <p>{@code RELEASING} reverts to {@code OPEN} on a refusal or any
         * other failure, so the handle stays usable and a later attempt can
         * still win the CAS. A failed delete must never be mistaken for a
         * closed one — the native side did not free anything, so marking
         * this closed anyway would strand a live entity with no way to ever
         * delete it.
         */
        Result release() {
            for (;;) {
                if (status.compareAndSet(OPEN, RELEASING)) {
                    break;
                }
                if (status.get() == CLOSED) {
                    return new Result(ReleaseOutcome.ALREADY_HANDLED, 0);
                }
                // RELEASING: another attempt is in flight. Yield rather than
                // busy-spin tightly; the wait is expected to be brief.
                Thread.yield();
            }
            int rc;
            try {
                rc = deleter.delete(value);
            } catch (Throwable t) {
                status.set(OPEN);
                FAILED.incrementAndGet();
                return new Result(ReleaseOutcome.FAILED, Result.NO_CODE);
            }
            if (rc == 0) {
                status.set(CLOSED);
                RELEASED.incrementAndGet();
                return new Result(ReleaseOutcome.RELEASED, 0);
            }
            status.set(OPEN);
            if (rc == RET_PRECONDITION_NOT_MET) {
                return new Result(ReleaseOutcome.REFUSED, rc);
            }
            FAILED.incrementAndGet();
            return new Result(ReleaseOutcome.FAILED, rc);
        }
    }

    static final class Reaper extends PhantomReference<Object> {
        final State state;

        /** Backoff state; meaningful only while this reaper sits in the
         *  reaper thread's deferred list, which is the only thing that reads
         *  or writes these two fields, so they need no synchronization. */
        long backoffNanos;
        long retryNotBeforeNanos;

        Reaper(Object owner, State state) {
            super(owner, QUEUE);
            this.state = state;
        }
    }
}
