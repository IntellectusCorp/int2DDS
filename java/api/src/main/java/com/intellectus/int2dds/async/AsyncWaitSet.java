package com.intellectus.int2dds.async;

import com.intellectus.int2dds.conditions.Condition;
import com.intellectus.int2dds.conditions.WaitSet;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;

/**
 * Async wrapper over a {@link WaitSet}. Runs the blocking {@code await} on a
 * dedicated single daemon thread, so a given AsyncWaitSet's waits are
 * serialized (the WaitSet is single-consumer). Cancellation is best-effort:
 * an in-flight native wait finishes at its timeout; {@code Future.cancel}
 * does not interrupt it.
 *
 * <p>{@code attach}/{@code detach} run on the caller's thread and touch the
 * same underlying WaitSet state the executor thread reads inside an
 * in-flight {@code await} — matching WaitSet's single-consumer contract,
 * callers must not invoke them concurrently with an in-flight
 * {@link #waitAsync(long)}; attach all conditions before starting a wait.
 *
 * <p>{@link #close()} only frees the native waitset once the executor has
 * actually terminated. If a long or infinite-timeout {@code waitAsync} is
 * still running when {@code close()} is called, the native wait is not
 * interruptible, so the native waitset is left alone and its reclamation is
 * deferred to the {@code NativeCleaner} reaper, once the in-flight wait
 * finishes and this object becomes unreachable.
 */
public final class AsyncWaitSet implements AutoCloseable {
    /** How long close() waits for an in-flight wait to finish before deferring reclamation. */
    private static final long GRACE_SECONDS = 5;

    private final WaitSet waitSet = new WaitSet();
    private final ExecutorService exec = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "int2dds-async-waitset");
        t.setDaemon(true);
        return t;
    });

    public void attach(Condition c) { waitSet.attach(c); }
    public void detach(Condition c) { waitSet.detach(c); }

    WaitSet inner() { return waitSet; }

    /** Completes with the triggered conditions, or empty on timeout. */
    public CompletableFuture<List<Condition>> waitAsync(long timeoutMillis) {
        return CompletableFuture.supplyAsync(() -> waitSet.await(timeoutMillis), exec);
    }

    @Override public void close() {
        // The native wait is not interruptible, so let the in-flight task (if
        // any) finish naturally within a grace window rather than interrupting.
        exec.shutdown();
        boolean terminated = false;
        try {
            terminated = exec.awaitTermination(GRACE_SECONDS, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        if (terminated) {
            // No task can be using the native waitset now -> safe to free.
            waitSet.close();
        }
        // else: a wait is still in flight (a long or infinite timeout). Do not
        // free the native waitset here -- that would be a use-after-free. The
        // NativeCleaner reaper reclaims it once the wait finishes and this
        // AsyncWaitSet (and the in-flight task) become unreachable.
    }
}
