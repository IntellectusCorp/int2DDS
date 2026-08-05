package com.intellectus.int2dds.core;

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
        // What isClosed() alone cannot rule out is a *double* release of any
        // of the three handles, so each delete is additionally counted
        // through its own test-local AtomicInteger -- the createForTest seam
        // DomainParticipantTest.closingTwiceIsHarmless already uses for
        // DomainParticipant, mirrored here for Topic and Publisher -- rather
        // than the shared NativeCleaner.releasedCount(), which a background
        // retry thread working on some other test's entities can also move
        // (NativeCleaner's own class doc).
        AtomicInteger participantDeletes = new AtomicInteger();
        AtomicInteger topicDeletes = new AtomicInteger();
        AtomicInteger publisherDeletes = new AtomicInteger();
        DomainParticipant p = DomainParticipant.createForTest(testDomain(), handle -> {
            participantDeletes.incrementAndGet();
            return FfiAccess.deleteParticipant(handle);
        });
        Topic<ConformanceRecord> t = Topic.createForTest(
                p, "tree_cascade", new ConformanceRecord(), handle -> {
                    topicDeletes.incrementAndGet();
                    return FfiAccess.deleteTopic(handle);
                });
        Publisher pub = Publisher.createForTest(p, handle -> {
            publisherDeletes.incrementAndGet();
            return FfiAccess.deletePublisher(handle);
        });
        assertNotEquals(0L, t.handle());
        assertNotEquals(0L, pub.handle());

        p.close();

        assertTrue(t.isClosed(), "the topic was closed by the cascade");
        assertTrue(pub.isClosed(), "the publisher was closed by the cascade");
        assertTrue(p.isClosed(), "the participant itself was closed last");
        assertEquals(1, topicDeletes.get(), "the topic's own handle is released exactly once");
        assertEquals(1, publisherDeletes.get(),
                "the publisher's own handle is released exactly once");
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
     *
     * <p>An earlier version of this test needed a decoy create afterward to
     * unstick a false negative: {@code NativeKeepAlive}'s fence used to keep
     * whatever it last fenced strongly reachable indefinitely, and
     * {@code buildAndAbandonTree}'s last constructor call left the abandoned
     * participant as that last-fenced object, so it was never actually
     * eligible for collection at all. Fixed at the source in {@code
     * NativeKeepAlive} itself (store, then immediately clear, the sink) —
     * see that class's doc — so this test needs no workaround of its own
     * anymore.
     *
     * <p>A later version of this test also relied on {@code
     * DomainParticipantTest.awaitReaped}, which compares against the
     * process-global {@code reapedCount()}. That counter is shared with
     * every other test in the module: three reaps from anywhere else in the
     * suite would satisfy a {@code baseline + 3} target without this
     * specific tree draining at all. Sequential JUnit execution makes that
     * unlikely in practice, but "unlikely" is weak footing for the one test
     * that proves the corrected design actually works, and an exact
     * alternative is available now that {@link DomainParticipant#createForTest},
     * {@link Topic#createForTest} and {@link Publisher#createForTest} all
     * exist: each of this tree's three handles is built with its own
     * counting deleter instead, so the oracle below is about these three
     * handles specifically, not the shared global gauge.
     */
    @Test
    void anAbandonedTreeIsEventuallyFullyReleased() throws InterruptedException {
        long deferredBefore = NativeCleaner.deferredCount();

        AtomicInteger participantDeletes = new AtomicInteger();
        AtomicInteger topicDeletes = new AtomicInteger();
        AtomicInteger publisherDeletes = new AtomicInteger();
        buildAndAbandonTree(participantDeletes, topicDeletes, publisherDeletes);

        awaitCondition(
                () -> participantDeletes.get() == 1 && topicDeletes.get() == 1
                        && publisherDeletes.get() == 1,
                "not all three handles in the abandoned tree released within the timeout: "
                        + "participant=" + participantDeletes.get()
                        + " topic=" + topicDeletes.get()
                        + " publisher=" + publisherDeletes.get());
        // Each counter above increments from inside its own handle's
        // State.release() (see countOnSuccess), which is called by
        // attempt()/sweepDeferred() *before* either of those does its own
        // DEFERRED/REAPED bookkeeping (NativeCleaner.sweepDeferred: it.remove()
        // and DEFERRED.decrementAndGet() run first, then REAPED.incrementAndGet()
        // only if the outcome was RELEASED). So deferredCount() can still
        // show a stale, not-yet-decremented entry for a moment after the
        // three counters above already confirm success -- poll rather than
        // assert immediately.
        awaitCondition(() -> NativeCleaner.deferredCount() == deferredBefore,
                "deferredCount() never returned to its baseline of " + deferredBefore
                        + " after the tree was abandoned; still at "
                        + NativeCleaner.deferredCount());
    }

    private static void buildAndAbandonTree(AtomicInteger participantDeletes,
            AtomicInteger topicDeletes, AtomicInteger publisherDeletes) {
        DomainParticipant p = DomainParticipant.createForTest(
                testDomain(), countOnSuccess(participantDeletes, FfiAccess::deleteParticipant));
        Topic<ConformanceRecord> t = Topic.createForTest(p, "tree_abandoned",
                new ConformanceRecord(), countOnSuccess(topicDeletes, FfiAccess::deleteTopic));
        Publisher pub = Publisher.createForTest(
                p, countOnSuccess(publisherDeletes, FfiAccess::deletePublisher));
        assertNotEquals(0L, t.handle());
        assertNotEquals(0L, pub.handle());
        // all three go out of scope here with no close()
    }

    /**
     * Wraps {@code real} so {@code counter} increments only when a delete
     * attempt actually succeeds ({@code rc == 0}), never on a refusal or any
     * other failed attempt. This distinction is load-bearing here, unlike in
     * {@code closingAParticipantClosesItsChildrenFirst}'s inline counters:
     * that test's explicit cascade guarantees every delete's first attempt
     * already succeeds, so counting attempts and counting successes agree.
     * Nothing here controls the order the reaper tries this tree's three
     * handles in, so a handle can legitimately be refused
     * (PRECONDITION_NOT_MET) one or more times before it finally succeeds —
     * {@code NativeCleaner.State.release()} invokes {@code real} on every
     * attempt, not only the last one — and counting every attempt would make
     * {@code == 1} the wrong assertion for exactly the scenario this test
     * exists to exercise. A handle can only ever reach {@code CLOSED} once,
     * so counting only successes is both correct and still exactly the
     * "released exactly once" property being tested; the same
     * attempts-vs-successes distinction {@code
     * NativeCleanerTest.registerAbandonedParent} draws for the same reason.
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
        // The parent keeps a weak reference to each child, but a child's own
        // close() does not deregister it from the parent -- only the
        // parent's own close() cascade does that (removeChildLocked), or a
        // sweep that finds the referent already collected (every
        // SWEEP_INTERVAL additions, and only for entries the GC has actually
        // cleared by then). Nothing here forces a GC mid-loop, so most of
        // these 200 weak references are plausibly still sitting in the
        // registry, un-swept, when the loop ends -- this does not observe
        // that registry's length directly (private, no test-only accessor),
        // so what it actually proves is narrower than the name suggests:
        // creating and individually closing far more children than the
        // sweep interval does not corrupt the sweep or the registry, and
        // closing the parent afterward -- which reattempts every
        // still-listed child, even ones already closed individually -- is
        // harmless, since NativeHandle.close() is idempotent. A failure in
        // either the loop or the final close() throws and fails this test
        // on its own, the same reasoning as the other explicit-close tests
        // above.
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
     * {@code TopicQos::is_consistent()} (dds/src/dcps/topic/qos/mod.rs) calls
     * {@code ResourceLimitsQosPolicy::is_consistent()}
     * (dds/src/dcps/infrastructure/qos_policy.rs), which rejects {@code
     * max_samples != UNLIMITED && (max_samples_per_instance == UNLIMITED ||
     * max_samples < max_samples_per_instance)} — two disjuncts, not one: a
     * bounded {@code max_samples} needs a {@code max_samples_per_instance}
     * that is both itself bounded <em>and</em> no greater. The values below
     * (1, unlimited instances, 10) trip the second disjunct specifically
     * ({@code 1 < 10}), not the first — before any topic is created, so this
     * is a real native-side rejection of bad input, not a fabricated one.
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
