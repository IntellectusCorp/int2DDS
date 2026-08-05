package com.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.exceptions.DdsPreconditionNotMetException;
import java.util.Collections;
import java.util.List;
import java.util.ArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BooleanSupplier;
import org.junit.jupiter.api.Test;

class NativeCleanerTest {

    /**
     * Drives the collector until {@code latch} falls, or fails.
     *
     * <p>{@code System.gc()} is a hint, so it is retried rather than trusted
     * once — but the loop is bounded, so a cleaner that never runs fails in two
     * seconds instead of hanging. Never replaced with a bare sleep: a sleep
     * followed by an assertion passes whether or not anything happened.
     */
    private static void awaitCleanup(CountDownLatch latch) throws InterruptedException {
        for (int i = 0; i < 10; i++) {
            System.gc();
            if (latch.await(200, TimeUnit.MILLISECONDS)) {
                return;
            }
        }
        fail("cleanup did not run within 2 seconds");
    }

    /**
     * Polls {@code condition} until true, or fails. Same bounded-retry shape
     * as {@link #awaitCleanup}, for a condition that is not a single latch.
     */
    private static void awaitCondition(BooleanSupplier condition, String failureMessage)
            throws InterruptedException {
        for (int i = 0; i < 10; i++) {
            if (condition.getAsBoolean()) {
                return;
            }
            System.gc();
            Thread.sleep(200);
        }
        fail(failureMessage);
    }

    /** Registers a resource and drops every reference to its owner. */
    private static CountDownLatch registerAndAbandon(long handle, List<Long> order) {
        CountDownLatch latch = new CountDownLatch(1);
        Object owner = new Object();
        NativeCleaner.register(owner, handle, h -> {
            order.add(h);
            latch.countDown();
            return 0;
        });
        return latch;
    }

    @Test
    void anAbandonedResourceIsReleasedByTheReaper() throws InterruptedException {
        long before = NativeCleaner.reapedCount();
        List<Long> order = Collections.synchronizedList(new ArrayList<Long>());

        awaitCleanup(registerAndAbandon(0x1111L, order));

        assertEquals(1, order.size());
        assertEquals(0x1111L, order.get(0).longValue());
        assertTrue(NativeCleaner.reapedCount() > before);
    }

    @Test
    void anExplicitCloseReleasesExactlyOnce() throws InterruptedException {
        AtomicInteger calls = new AtomicInteger();
        Object owner = new Object();
        NativeHandle h = NativeCleaner.register(owner, 0x2222L, x -> {
            calls.incrementAndGet();
            return 0;
        });

        assertEquals(0, h.close());
        assertTrue(h.isClosed());
        assertEquals(1, calls.get());

        // Closing again must not call the deleter a second time. The native
        // delete reclaims a strong reference, so a second call is a double free.
        assertEquals(0, h.close());
        assertEquals(1, calls.get());
    }

    @Test
    void aClosedHandleThenAbandonedIsNotReleasedAgain() throws InterruptedException {
        AtomicInteger calls = new AtomicInteger();
        closeThenAbandon(calls);

        // Give the reaper every chance to run for the now-unreachable owner.
        for (int i = 0; i < 10; i++) {
            System.gc();
            Thread.sleep(50);
        }
        assertEquals(1, calls.get(), "close() already released it; the reaper must not repeat");
    }

    private static void closeThenAbandon(AtomicInteger calls) {
        Object owner = new Object();
        NativeHandle h = NativeCleaner.register(owner, 0x3333L, x -> {
            calls.incrementAndGet();
            return 0;
        });
        h.close();
    }

    // --- Parent/child ordering: not GC-guaranteed, so not GC-tested. ---
    //
    // An earlier version of this file tried to prove a child's native handle
    // is always reaped before its parent's, by having the child's owner hold
    // a strong reference to the parent and abandoning both together. That
    // does not work: when both die in the same GC cycle their phantom
    // references are discovered and enqueued in that same cycle, and nothing
    // orders two references discovered together — confirmed non-deterministic
    // across Serial, Parallel and G1 GC. See NativeCleaner's class doc.
    //
    // Real ordering comes from the native layer instead: int2dds_delete_*
    // refuses (RET_PRECONDITION_NOT_MET) to delete an entity with live
    // children and leaves it valid rather than deleting it, so the tests
    // below drive that refusal and its retry directly and deterministically,
    // through explicit signalling rather than GC timing. Which one a test
    // abandons first is still meaningful — it forces which order the reaper
    // actually encounters them in — but nothing here depends on the GC
    // choosing an order on its own.

    /** A parent whose delete is refused for as long as {@code childAlive} is true. */
    private static void registerAbandonedParent(AtomicBoolean childAlive, CountDownLatch released) {
        Object owner = new Object();
        NativeCleaner.register(owner, 0xA0L, h -> {
            if (childAlive.get()) {
                return DdsException.RET_PRECONDITION_NOT_MET;
            }
            released.countDown();
            return 0;
        });
    }

    /** A child whose delete always succeeds and clears {@code childAlive}. */
    private static void registerAbandonedChild(AtomicBoolean childAlive, CountDownLatch released) {
        Object owner = new Object();
        NativeCleaner.register(owner, 0xC1L, h -> {
            childAlive.set(false);
            released.countDown();
            return 0;
        });
    }

    @Test
    void aParentWithNoLiveChildReleasesOnTheFirstAttempt() throws InterruptedException {
        // The trivial order: the child is already gone before the parent's
        // owner is even dropped, so the parent is never refused at all.
        AtomicBoolean childAlive = new AtomicBoolean(true);
        CountDownLatch childReleased = new CountDownLatch(1);
        registerAbandonedChild(childAlive, childReleased);
        awaitCleanup(childReleased);

        long deferredBefore = NativeCleaner.deferredCount();
        CountDownLatch parentReleased = new CountDownLatch(1);
        registerAbandonedParent(childAlive, parentReleased);
        awaitCleanup(parentReleased);

        assertEquals(deferredBefore, NativeCleaner.deferredCount(),
                "the child was already gone, so the parent was never refused");
    }

    @Test
    void aRefusedParentIsRetriedAndSucceedsOnceTheChildReleases() throws InterruptedException {
        // The order that actually exercises the retry machinery: the
        // parent's owner is dropped, and the reaper is confirmed to have
        // tried it and been refused, before the child's owner is dropped at
        // all.
        AtomicBoolean childAlive = new AtomicBoolean(true);
        CountDownLatch parentReleased = new CountDownLatch(1);
        long deferredBefore = NativeCleaner.deferredCount();
        long releasedBefore = NativeCleaner.releasedCount();

        registerAbandonedParent(childAlive, parentReleased);
        awaitCondition(() -> NativeCleaner.deferredCount() > deferredBefore,
                "the parent was never refused, so this test forced nothing");
        assertEquals(1, parentReleased.getCount(), "must not release while the child is alive");

        CountDownLatch childReleased = new CountDownLatch(1);
        registerAbandonedChild(childAlive, childReleased);
        awaitCleanup(childReleased);
        awaitCleanup(parentReleased);

        // parentReleased counts down from inside the fake deleter, which runs
        // inside State.release() — but sweepDeferred's own bookkeeping
        // (it.remove(), DEFERRED.decrementAndGet()) happens after release()
        // returns to it, a little later than the latch. Polling rather than
        // asserting immediately avoids a race against that trailing update;
        // it is not a race in NativeCleaner's own correctness, only in how
        // soon this thread can observe the caller-side list bookkeeping.
        awaitCondition(() -> NativeCleaner.deferredCount() == deferredBefore,
                "parent never left the deferred list");
        assertEquals(deferredBefore, NativeCleaner.deferredCount(), "no longer stuck");
        assertEquals(releasedBefore + 2, NativeCleaner.releasedCount(),
                "parent and child each released exactly once; refusals do not count as releases");
    }

    @Test
    void explicitCloseOnARefusedHandleThrowsAndLeavesItOpen() {
        AtomicBoolean refuse = new AtomicBoolean(true);
        NativeHandle h = NativeCleaner.register(new Object(), 0x8888L,
                x -> refuse.get() ? DdsException.RET_PRECONDITION_NOT_MET : 0);

        assertThrows(DdsPreconditionNotMetException.class, h::close);
        assertFalse(h.isClosed(), "refused: the entity is still alive, per the native contract");
        assertEquals(0x8888L, h.value(), "still usable after a refused close");

        // Let it actually close so this handle's owner — unreachable once this
        // method returns — is not left for the reaper to trip over later.
        refuse.set(false);
        assertEquals(0, h.close());
        assertTrue(h.isClosed());
    }

    @Test
    void aDeleterReturningNonZeroCountsAsAFailure() {
        long before = NativeCleaner.failedCount();
        Object owner = new Object();
        NativeHandle h = NativeCleaner.register(owner, 0x4444L, x -> 11);

        assertEquals(11, h.close(), "close returns the C ABI code");
        assertEquals(before + 1, NativeCleaner.failedCount());
    }

    @Test
    void aDeleterThatThrowsDoesNotKillTheReaperThread() throws InterruptedException {
        // If an exception escapes the reaper loop the thread dies and every
        // later cleanup silently stops. Register a throwing resource, then a
        // well-behaved one, and require the second still runs.
        abandonThrowing();

        List<Long> order = Collections.synchronizedList(new ArrayList<Long>());
        awaitCleanup(registerAndAbandon(0x5555L, order));
        assertEquals(1, order.size(), "the reaper survived the throwing deleter");
    }

    private static void abandonThrowing() {
        Object owner = new Object();
        NativeCleaner.register(owner, 0x6666L, x -> {
            throw new IllegalStateException("deliberate");
        });
    }

    @Test
    void valueIsUnavailableOnceClosed() {
        Object owner = new Object();
        NativeHandle h = NativeCleaner.register(owner, 0x7777L, x -> 0);
        assertEquals(0x7777L, h.value());
        h.close();
        assertThrows(IllegalStateException.class, h::value);
    }
}
