package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BooleanSupplier;
import org.junit.jupiter.api.Test;

class DataWriterTest {

    private static ConformanceRecord sample(int id) {
        ConformanceRecord r = new ConformanceRecord();
        r.id = id;
        r.value = id * 1.5d;
        r.label = "sample-" + id;
        return r;
    }

    @Test
    void aWriterIsCreatedAndPublishesWithoutError() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("writer_basic", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(t);

            assertNotEquals(0L, w.handle());
            assertTrue(t == w.topic(), "the writer keeps its topic reachable");

            for (int i = 0; i < 10; i++) {
                w.write(sample(i));
            }
            // The brief's original oracle here was `assertEquals(failedBefore,
            // NativeCleaner.failedCount())` -- dead: failedCount() is the
            // *reaper's* delete-attempt counter (NativeCleaner's own
            // Javadoc), and nothing in this test ever abandons a handle for
            // the reaper to touch, so that assertion could not have failed
            // regardless of what write() actually did. A write() that threw
            // would already have failed this test via the uncaught
            // exception; the real postcondition worth stating explicitly is
            // that the writer is still open after ten successful writes.
            assertFalse(w.isClosed());
        } finally {
            p.close();
        }
    }

    @Test
    void aWriterHonoursItsQos() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("writer_qos", new ConformanceRecord());
            Publisher pub = p.createPublisher();

            DataWriterQos qos = new DataWriterQos();
            qos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            qos.setHistory(new History(HistoryKind.KEEP_LAST, 5));

            DataWriter<ConformanceRecord> w = pub.createDataWriter(t, qos);
            DataWriterQos back = w.getQos();

            assertEquals(ReliabilityKind.RELIABLE, back.getReliability().getKind());
            assertEquals(HistoryKind.KEEP_LAST, back.getHistory().getKind());
            assertEquals(5, back.getHistory().getDepth());
        } finally {
            p.close();
        }
    }

    @Test
    void writingAfterCloseIsRejected() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("writer_closed", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(t);
            w.close();
            assertThrows(IllegalStateException.class, () -> w.write(sample(1)));
        } finally {
            p.close();
        }
    }

    @Test
    void repeatedWritesDoNotStarveTheWriterPool() {
        // write() borrows a pooled CdrWriter and must return it. If it leaked
        // one per call, the pool would empty and every later write would
        // allocate a fresh direct buffer — invisible except as a slowdown, so
        // this asserts the borrow is balanced by observing that a long run
        // completes without error rather than by timing it.
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("writer_pool", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(t);
            for (int i = 0; i < 5000; i++) {
                w.write(sample(i));
            }
        } finally {
            p.close();
        }
    }

    /**
     * Replaces the brief's version, which captured {@code failedBefore =
     * NativeCleaner.failedCount()} and asserted it unchanged at the end.
     * That counter only moves for a delete attempt that throws or returns a
     * non-{@code PRECONDITION_NOT_MET} code (NativeCleaner's own Javadoc);
     * under the current NativeCleaner a refusal — exactly what an
     * out-of-order reap of this four-handle tree produces routinely — moves
     * {@code deferredCount()} instead, so the brief's assertion could not
     * have failed here either. This is the same dead-oracle defect Task 5
     * found and fixed for the three-handle participant/topic/publisher tree
     * (see {@code EntityTreeTest.anAbandonedTreeIsEventuallyFullyReleased}),
     * now extended by one more handle for the writer this task adds.
     *
     * <p>Uses the {@code createForTest} seam on all four entity types —
     * {@code DataWriter} gains its own here, mirroring {@code
     * DomainParticipant}/{@code Topic}/{@code Publisher}'s existing ones —
     * with a per-handle deleter that counts only successful releases ({@code
     * rc == 0}), not attempts. Counting attempts would be wrong here:
     * nothing controls which order the reaper tries this tree's four handles
     * in, so a handle can legitimately be refused (PRECONDITION_NOT_MET) one
     * or more times before it finally succeeds — {@code
     * NativeCleaner.State.release()} invokes the deleter on every attempt,
     * not only the last.
     */
    @Test
    void anAbandonedWriterTreeIsReapedCleanly() throws InterruptedException {
        long deferredBefore = NativeCleaner.deferredCount();

        AtomicInteger participantDeletes = new AtomicInteger();
        AtomicInteger topicDeletes = new AtomicInteger();
        AtomicInteger publisherDeletes = new AtomicInteger();
        AtomicInteger writerDeletes = new AtomicInteger();
        buildAndAbandonWriter(participantDeletes, topicDeletes, publisherDeletes, writerDeletes);

        awaitCondition(
                () -> participantDeletes.get() == 1 && topicDeletes.get() == 1
                        && publisherDeletes.get() == 1 && writerDeletes.get() == 1,
                "not all four handles in the abandoned tree released within the timeout: "
                        + "participant=" + participantDeletes.get()
                        + " topic=" + topicDeletes.get()
                        + " publisher=" + publisherDeletes.get()
                        + " writer=" + writerDeletes.get());
        // The per-handle counters above increment inside State.release(),
        // called before NativeCleaner's own DEFERRED/REAPED bookkeeping (see
        // EntityTreeTest's identical note) — so deferredCount() can still
        // read stale for a moment after all four counters already confirm
        // success. Poll rather than assert immediately.
        awaitCondition(() -> NativeCleaner.deferredCount() == deferredBefore,
                "deferredCount() never returned to its baseline of " + deferredBefore
                        + " after the tree was abandoned; still at "
                        + NativeCleaner.deferredCount());
    }

    private static void buildAndAbandonWriter(AtomicInteger participantDeletes,
            AtomicInteger topicDeletes, AtomicInteger publisherDeletes,
            AtomicInteger writerDeletes) {
        DomainParticipant p = DomainParticipant.createForTest(
                testDomain(), countOnSuccess(participantDeletes, FfiAccess::deleteParticipant));
        Topic<ConformanceRecord> t = Topic.createForTest(p, "writer_abandoned",
                new ConformanceRecord(), countOnSuccess(topicDeletes, FfiAccess::deleteTopic));
        Publisher pub = Publisher.createForTest(
                p, countOnSuccess(publisherDeletes, FfiAccess::deletePublisher));
        DataWriter<ConformanceRecord> w = DataWriter.createForTest(
                pub, t, countOnSuccess(writerDeletes, FfiAccess::deleteDataWriter));
        assertNotEquals(0L, w.handle());
        w.write(sample(1));
        // all four go out of scope here with no close()
    }

    /**
     * Wraps {@code real} so {@code counter} increments only when a delete
     * attempt actually succeeds ({@code rc == 0}) — the same helper {@code
     * EntityTreeTest} uses for the same reason; duplicated locally rather
     * than shared since neither class exposes the other's private helpers.
     */
    private static NativeCleaner.Deleter countOnSuccess(
            AtomicInteger counter, NativeCleaner.Deleter real) {
        return handle -> {
            int rc = real.delete(handle);
            if (rc == 0) {
                counter.incrementAndGet();
            }
            return rc;
        };
    }

    /**
     * Same bounded-retry shape as {@code DomainParticipantTest.awaitReaped}
     * and {@code EntityTreeTest.awaitCondition}: never a bare sleep, since a
     * sleep followed by an assertion passes whether or not anything
     * happened. Duplicated locally rather than reusing either of those, the
     * same reasoning {@code EntityTreeTest}'s own copy already states.
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

    @Test
    void instanceHandleComparesByContent() {
        byte[] key = new byte[16];
        key[0] = 7;
        InstanceHandle a = new InstanceHandle(key);
        InstanceHandle b = new InstanceHandle(key.clone());
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
        assertArrayEquals(key, a.bytes());

        // Defensive copy: mutating the caller's array must not change the handle.
        key[0] = 9;
        assertEquals((byte) 7, a.bytes()[0]);
    }
}
