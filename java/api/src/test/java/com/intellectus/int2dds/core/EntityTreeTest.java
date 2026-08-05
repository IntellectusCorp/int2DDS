package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.awaitReaped;
import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.exceptions.DdsInconsistentPolicyException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.ResourceLimits;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.BooleanSupplier;
import org.junit.jupiter.api.Test;

class EntityTreeTest {

    @Test
    void aTopicCarriesItsNameAndTypeFromThePrototype() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t =
                    p.createTopic("tree_topic_names", new ConformanceRecord());
            assertEquals("tree_topic_names", t.name());
            assertEquals("ConformanceRecord", t.typeName());
            assertEquals(Extensibility.APPENDABLE, t.extensibility());
            assertNotEquals(0L, t.handle());
        } finally {
            p.close();
        }
    }

    @Test
    void closingAParticipantClosesItsChildrenFirst() {
        // The native layer refuses to delete a parent whose children are
        // still alive, and NativeEntity.close() reads that refusal
        // (PRECONDITION_NOT_MET) and throws rather than swallowing it (see
        // NativeEntityTest). So if the cascade below ran in the wrong order
        // -- or not at all -- p.close() would throw and fail this test on
        // its own; no failedCount()/releasedCount() bookkeeping is needed to
        // detect a wrong order, unlike what an earlier version of this test
        // assumed.
        //
        // What a plain isClosed() cannot rule out is a *double* release of
        // the participant's own handle, so that one delete is additionally
        // counted through a test-local AtomicInteger -- the same
        // createForTest seam DomainParticipantTest.closingTwiceIsHarmless
        // uses -- rather than the shared NativeCleaner.releasedCount(),
        // which a background retry thread working on some other test's
        // entities can also move (NativeCleaner's own class doc).
        AtomicInteger participantDeletes = new AtomicInteger();
        DomainParticipant p = DomainParticipant.createForTest(testDomain(), handle -> {
            participantDeletes.incrementAndGet();
            return FfiAccess.deleteParticipant(handle);
        });
        Topic<ConformanceRecord> t = p.createTopic("tree_cascade", new ConformanceRecord());
        Publisher pub = p.createPublisher();
        assertNotEquals(0L, t.handle());
        assertNotEquals(0L, pub.handle());

        p.close();

        assertTrue(t.isClosed(), "the topic was closed by the cascade");
        assertTrue(pub.isClosed(), "the publisher was closed by the cascade");
        assertTrue(p.isClosed(), "the participant itself was closed last");
        assertEquals(1, participantDeletes.get(),
                "the participant's own handle is released exactly once");
    }

    @Test
    void closingAChildThenTheParentIsFine() {
        // As above: a wrong-order or otherwise-failed delete throws here
        // rather than moving a counter, so "both calls returned" is already
        // the whole proof.
        DomainParticipant p = new DomainParticipant(testDomain());
        Publisher pub = p.createPublisher();
        pub.close();
        p.close();
        assertTrue(pub.isClosed());
        assertTrue(p.isClosed());
    }

    /**
     * Replaces the brief's {@code anAbandonedTreeIsReapedChildFirst}, which
     * tried to prove the reaper always releases a child before its parent on
     * the premise that the strong child-&gt;parent reference orders the two.
     * It does not: reachability propagates, so a parent reachable only
     * through a phantom-reachable child is itself at most
     * phantom-reachable, and a parent/child pair that die together are
     * discovered and enqueued in the very same GC cycle. Nothing orders two
     * references discovered together (see {@link NativeEntity}'s and {@link
     * NativeCleaner}'s class docs) — the strong reference's real job is only
     * to keep a parent alive while a live child still needs it, not to
     * sequence the two deleters.
     *
     * <p>What the reaper actually guarantees, and what this test proves
     * instead, is weaker but still real: whichever order the three handles
     * are tried in, a parent tried before its children is refused
     * (PRECONDITION_NOT_MET) rather than freed, stays valid, and is retried
     * with backoff until the blocking child is gone — so the whole tree
     * still ends up released, however many refusals it takes to get there.
     * This drives that guarantee through real entities and the real
     * refuse/defer/retry loop, not a fake standing in for it.
     */
    @Test
    void anAbandonedTreeIsEventuallyFullyReleased() throws InterruptedException {
        long reapedBefore = NativeCleaner.reapedCount();
        long deferredBefore = NativeCleaner.deferredCount();

        buildAndAbandonTree();
        flushNativeKeepAliveSink();

        awaitReaped(reapedBefore, 3);
        // awaitReaped only demands that reapedCount reach its target; a
        // deferred entry's own bookkeeping (DEFERRED.decrementAndGet())
        // runs a moment after the matching REAPED increment inside
        // NativeCleaner's sweep, so this polls rather than asserting
        // immediately — the same caution
        // NativeCleanerTest.aRefusedParentIsRetriedAndSucceedsOnceTheChildReleases
        // takes for an identical trailing-update race.
        awaitCondition(() -> NativeCleaner.deferredCount() == deferredBefore,
                "deferredCount() never returned to its baseline of " + deferredBefore
                        + " after the tree was abandoned; still at "
                        + NativeCleaner.deferredCount());
        assertEquals(deferredBefore, NativeCleaner.deferredCount(),
                "every refused delete in the tree must eventually leave the deferred list");
    }

    private static void buildAndAbandonTree() {
        DomainParticipant p = new DomainParticipant(testDomain());
        Topic<ConformanceRecord> t = p.createTopic("tree_abandoned", new ConformanceRecord());
        Publisher pub = p.createPublisher();
        assertNotEquals(0L, t.handle());
        assertNotEquals(0L, pub.handle());
        // all three go out of scope here with no close()
    }

    /**
     * Without this, the test above hangs until its own timeout: {@link
     * NativeKeepAlive}'s fence is one static field that holds whatever was
     * last handed to {@code keepAlive} until something else overwrites it
     * (see that class's doc on why a plain, non-volatile static write is
     * enough to work as a fence at all). {@code Topic}/{@code Publisher}'s
     * create path is the only thing that calls it, and {@code
     * buildAndAbandonTree} above ends with exactly such a call —
     * {@code createPublisher()}'s {@code keepAlive(participant)} — so once
     * that method returns, the sink is still holding a strong, static
     * reference to the abandoned participant. Confirmed directly: without
     * this flush, the topic and publisher release as expected but the
     * participant sits at zero attempts (not refused, not failed — simply
     * never enqueued) for as long as the process keeps running, because
     * nothing else ever calls {@code keepAlive} again to displace it. In a
     * real application this is harmless and transient — the next {@code
     * Topic}/{@code Publisher}/{@code DataWriter} created anywhere flushes
     * it — but a test that abandons a tree and then does nothing else never
     * supplies that later call on its own, so this does, on a throwaway
     * participant that is explicitly closed regardless of what happens to
     * the abandoned tree.
     *
     * <p>This is not a defer/retry bug: {@link NativeCleaner}'s reaper drains
     * the whole real tree on the order of 200ms once the participant is
     * actually eligible for collection, verified by instrumenting this exact
     * scenario before adding this method.
     */
    private static void flushNativeKeepAliveSink() {
        DomainParticipant decoy = new DomainParticipant(testDomain());
        try {
            decoy.createPublisher();
        } finally {
            decoy.close();
        }
    }

    /**
     * Same bounded-retry shape as {@code DomainParticipantTest.awaitReaped}
     * and {@code NativeCleanerTest.awaitCondition}: never a bare sleep,
     * since a sleep followed by an assertion passes whether or not anything
     * happened. Duplicated locally rather than reusing either of those —
     * {@code NativeCleanerTest}'s helper is private and, worse, lives in a
     * different package.
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
    void manyChildrenDoNotAccumulateDeadReferences() {
        // The parent keeps weak references to its children and sweeps them
        // periodically. Creating and abandoning far more than the sweep
        // interval must not grow without bound, and must not disturb the
        // close -- a failure in either the loop or the final close() throws
        // and fails this test on its own, the same reasoning as the other
        // explicit-close tests above.
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            for (int i = 0; i < 200; i++) {
                Publisher pub = p.createPublisher();
                pub.close();
            }
        } finally {
            p.close();
        }
        assertTrue(p.isClosed());
    }

    /**
     * The other half of the coverage gap Task 4 could not close:
     * {@code int2dds_create_participant} range-checks nothing, so no test
     * anywhere yet proved that a bad argument makes a native <em>create</em>
     * fail. {@code int2dds_create_topic} does validate its QoS —
     * {@code TopicQos::is_consistent()} (dds/src/dcps/topic/qos/mod.rs)
     * rejects {@code resource_limits.max_samples < max_samples_per_instance}
     * before any topic is created — so this is a real native-side rejection
     * of bad input, not a fabricated one.
     *
     * <p>The two mechanisms guessed as this gap's likely shape — a duplicate
     * topic name, or a topic name that does not match an existing topic's
     * type — turned out not to be reachable through the raw create path
     * {@code Topic} exposes: nothing in {@code create_topic}
     * (dds/src/dcps/domain/domain_participant.rs) checks topic names for
     * uniqueness at all, and the type-mismatch check in {@code
     * register_type}/{@code register_type_for_topic} compares {@code
     * TypeSupport::type_id()}, which every {@code RawTypeSupport} — the only
     * kind a raw-path create ever builds — reports as the same fixed {@code
     * TypeId::of::<Int2DdsData>()} regardless of type name or extensibility,
     * so two raw-path topics can never disagree on it. The QoS-consistency
     * check above is the one bad-input rejection this create path can
     * actually be driven into.
     */
    @Test
    void anInconsistentTopicQosIsRejectedByCreate() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            TopicQos badQos = new TopicQos();
            badQos.setResourceLimits(new ResourceLimits(1, -1, 10));
            assertThrows(DdsInconsistentPolicyException.class,
                    () -> p.createTopic("tree_bad_qos", new ConformanceRecord(), badQos));
        } finally {
            p.close();
        }
    }
}
