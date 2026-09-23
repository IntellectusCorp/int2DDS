package com.intellectus.int2dds.async;

import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.Sample;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.IDdsType;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;

/**
 * Async wrapper over a {@link DataReader}'s blocking {@code take}/{@code
 * read}, plus a {@code waitForDataAsync} built on {@link AsyncWaitSet}.
 *
 * <p>Does not own {@code reader}: the caller created it and must close it
 * separately. This class owns two things instead: a single daemon thread
 * that {@code takeAsync}/{@code readAsync} run on, and an internal {@link
 * AsyncWaitSet} + {@link StatusCondition} pair used only by {@code
 * waitForDataAsync}.
 *
 * <p>The single thread is required, not incidental: a {@code DataReader} is
 * single-consumer (see its own class doc), so {@code takeAsync} and {@code
 * readAsync} must never run concurrently against it. Submitting both to one
 * single-thread executor serializes them for free. Callers must not
 * otherwise call {@code reader.take()}/{@code reader.read()} directly while
 * futures from this class may still be in flight.
 *
 * <p>{@link #close()} mirrors {@link AsyncWaitSet#close()}'s UAF-safe
 * discipline: {@code exec.shutdown()}, then a bounded wait for the
 * in-flight task (if any) to finish. Only once the executor has actually
 * terminated is it safe to release the internal AsyncWaitSet and
 * StatusCondition — freeing them while a take/read/wait is still running
 * on the executor thread would be a use-after-free. If the grace window
 * elapses first (a long-blocking take/read/wait), those resources are left
 * alone and their reclamation deferred to the {@code NativeCleaner} reaper,
 * once the in-flight task finishes and this object becomes unreachable.
 *
 * @param <T> the DDS data type this reader receives.
 */
public final class AsyncDataReader<T extends IDdsType> implements AutoCloseable {
    /** How long close() waits for an in-flight task to finish before deferring reclamation. */
    private static final long GRACE_SECONDS = 5;

    private final DataReader<T> reader;
    private final ExecutorService exec = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "int2dds-async-datareader");
        t.setDaemon(true);
        return t;
    });

    // Set up once, lazily, on first waitForDataAsync -- not re-attached per
    // call. Guarded by this monitor since a caller could reasonably call
    // waitForDataAsync from a different thread than takeAsync/readAsync.
    private AsyncWaitSet waitSet;
    private StatusCondition statusCondition;

    public AsyncDataReader(DataReader<T> reader) {
        this.reader = reader;
    }

    /** The wrapped reader. Not owned by this class -- the caller closes it. */
    public DataReader<T> reader() {
        return reader;
    }

    /** Takes (removes) the next sample asynchronously; completes with null if the cache is empty. */
    public CompletableFuture<Sample<T>> takeAsync() {
        return CompletableFuture.supplyAsync(reader::take, exec);
    }

    /** Reads (without removing) the next sample asynchronously; completes with null if the cache is empty. */
    public CompletableFuture<Sample<T>> readAsync() {
        return CompletableFuture.supplyAsync(reader::read, exec);
    }

    /**
     * Completes with {@code true} once this reader's DATA_AVAILABLE status
     * triggers within {@code timeoutMillis}, or {@code false} on timeout.
     */
    public synchronized CompletableFuture<Boolean> waitForDataAsync(long timeoutMillis) {
        ensureWaitSet();
        return waitSet.waitAsync(timeoutMillis).thenApply(hits -> !hits.isEmpty());
    }

    private void ensureWaitSet() {
        if (waitSet == null) {
            statusCondition = reader.getStatusCondition();
            statusCondition.setEnabledStatuses(StatusMask.of(StatusMask.DATA_AVAILABLE));
            waitSet = new AsyncWaitSet();
            waitSet.attach(statusCondition);
        }
    }

    @Override
    public synchronized void close() {
        // The pending take/read/wait is not interruptible, so let it finish
        // naturally within a grace window rather than forcing it.
        exec.shutdown();
        boolean terminated = false;
        try {
            terminated = exec.awaitTermination(GRACE_SECONDS, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        if (terminated) {
            // No task can be touching the wait set or condition now -> safe to free.
            if (waitSet != null) {
                waitSet.close();
            }
            if (statusCondition != null) {
                statusCondition.close();
            }
        }
        // else: a task is still in flight. Do not free the wait set or
        // condition here -- that would be a use-after-free. The
        // NativeCleaner reaper reclaims them once the task finishes and
        // this AsyncDataReader becomes unreachable.
    }
}
