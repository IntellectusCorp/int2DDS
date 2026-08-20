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
 */
public final class AsyncWaitSet implements AutoCloseable {
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
        exec.shutdownNow();
        try { exec.awaitTermination(2, TimeUnit.SECONDS); }
        catch (InterruptedException e) { Thread.currentThread().interrupt(); }
        waitSet.close();
    }
}
