package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.exceptions.DdsBufferTooSmallException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.exceptions.DdsNullPointerException;
import com.intellectus.int2dds.internal.NativeCleaner;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;
import org.junit.jupiter.api.Test;

/**
 * Exercises {@link NativeEntity#close()}'s child cascade directly, with fake
 * in-process handles and deleters — no real native calls, no network.
 *
 * <p>{@code DomainParticipant}, this branch's only public {@code
 * NativeEntity} subclass, is always a root and never has children, so
 * nothing in {@code DomainParticipantTest} touches this logic at all; Topic,
 * Publisher and DataWriter (Tasks 5-7) will be the first real exercise of
 * it. This file is what actually proves the cascade correct before that,
 * rather than leaving it argued only in {@link NativeEntity}'s own Javadoc.
 *
 * <p>Every test cleans up every entity it creates to an actual, successful
 * close before returning — a fake entity is registered with the real,
 * shared {@link NativeCleaner} just like any other (that registration is
 * unconditional, in {@code NativeEntity}'s constructor), so one left
 * abandoned in a permanently-failing state would be retried by the reaper
 * forever in the background, inflating {@code failedCount()}/{@code
 * deferredCount()} for every later test in the module — the same hygiene
 * {@code NativeCleanerTest} already applies to its own fake resources.
 */
class NativeEntityTest {

    /** The minimal concrete NativeEntity needed to test the base class alone. */
    private static final class FakeEntity extends NativeEntity {
        FakeEntity(NativeEntity parent, long handle, NativeCleaner.Deleter deleter) {
            super(parent, handle, deleter);
        }
    }

    private static NativeCleaner.Deleter recording(List<String> order, String name) {
        return h -> {
            order.add(name);
            return 0;
        };
    }

    @Test
    void closingAParentClosesEveryLiveChildFirst() {
        List<String> order = new ArrayList<String>();
        FakeEntity parent = new FakeEntity(null, 0x100L, recording(order, "parent"));
        new FakeEntity(parent, 0x101L, recording(order, "childA"));
        new FakeEntity(parent, 0x102L, recording(order, "childB"));

        parent.close();

        assertEquals(3, order.size());
        assertEquals("parent", order.get(2), "the parent closes only after both children");
    }

    @Test
    void childrenCloseInReverseOfInsertionOrder() {
        // Mirrors the shape a real Topic/Publisher/DataWriter tree produces:
        // create order is topic, then publisher (whose own close() cascades
        // to its writer first). Reverse-of-insertion at this level therefore
        // closes the publisher -- and transitively its writer -- before the
        // topic: the order the native layer actually requires, per the
        // "Topic delete is refused while any DataWriter uses it" finding
        // this cascade change fixes.
        List<String> order = new ArrayList<String>();
        FakeEntity parent = new FakeEntity(null, 0x200L, recording(order, "parent"));
        new FakeEntity(parent, 0x201L, recording(order, "topic"));
        new FakeEntity(parent, 0x202L, recording(order, "publisher"));

        parent.close();

        assertEquals(Arrays.asList("publisher", "topic", "parent"), order);
    }

    @Test
    void closingSucceedsRegardlessOfWhichLegalOrderTheChildrenWereCreatedIn() {
        // The test above only works because it creates the topic before the
        // publisher: reverse-of-insertion then closes the publisher (and
        // transitively its writer) before the topic, which is what the core
        // requires. Nothing enforces that creation order -- a caller is just
        // as entitled to build the publisher first and decide what topic its
        // writer will use second -- and a single reverse-of-insertion pass
        // over *that* legal order tries the topic first instead. This fake
        // deleter reproduces the real int2dds_delete_topic's refusal for
        // exactly that case (contains_topic walks publishers for a writer on
        // the topic being deleted): it refuses with PRECONDITION_NOT_MET for
        // as long as "publisher" has not yet closed, and only succeeds once
        // it has. A single-pass close() rethrows that refusal even though
        // "publisher" closes cleanly in the very same pass; a correct close()
        // retries the pass instead of giving up after one.
        AtomicBoolean publisherClosed = new AtomicBoolean(false);
        List<String> order = new ArrayList<String>();
        FakeEntity parent = new FakeEntity(null, 0x700L, recording(order, "parent"));
        new FakeEntity(parent, 0x701L, h -> {
            publisherClosed.set(true);
            order.add("publisher");
            return 0;
        });
        new FakeEntity(parent, 0x702L, h -> {
            if (!publisherClosed.get()) {
                return DdsException.RET_PRECONDITION_NOT_MET;
            }
            order.add("topic");
            return 0;
        });

        parent.close();

        assertEquals(Arrays.asList("publisher", "topic", "parent"), order);
    }

    @Test
    void oneFailingChildDoesNotStrandItsSiblingsOrSkipThem() {
        List<String> attempted = new ArrayList<String>();
        AtomicBoolean childBShouldFail = new AtomicBoolean(true);
        FakeEntity parent = new FakeEntity(null, 0x300L, recording(attempted, "parent"));
        new FakeEntity(parent, 0x301L, recording(attempted, "childA"));
        new FakeEntity(parent, 0x302L, h -> {
            attempted.add("childB");
            if (childBShouldFail.get()) {
                throw new IllegalStateException("boom");
            }
            return 0;
        });
        new FakeEntity(parent, 0x303L, recording(attempted, "childC"));

        assertThrows(RuntimeException.class, parent::close);
        // Every child was attempted -- childB failing did not abort the
        // loop -- and the parent's own delete was never reached, since a
        // live child (childB, still registered) would just be refused.
        // childB appears twice: childA and childC succeeding is progress,
        // so close() retries the pass over whatever is still open (just
        // childB) once more before giving up -- it cannot know in advance
        // that nothing actually changed for childB this time, only that
        // something changed for the pass as a whole. See
        // closingSucceedsRegardlessOfWhichLegalOrderTheChildrenWereCreatedIn
        // above for the case where that second attempt is exactly what
        // lets a real tree finish closing.
        assertEquals(Arrays.asList("childC", "childB", "childA", "childB"), attempted);

        // Let childB actually succeed now and retry, so nothing here is left
        // for the reaper to retry forever in the background.
        childBShouldFail.set(false);
        attempted.clear();
        parent.close();
        assertEquals(Arrays.asList("childB", "parent"), attempted,
                "only the still-registered failed child needs a retry; "
                        + "the parent follows once it's the only thing left");
    }

    @Test
    void theFirstFailureIsThrownWithLaterOnesSuppressed() {
        // Distinct non-zero, non-refusal *return codes* rather than thrown
        // exceptions: NativeCleaner.State.release() catches a thrown
        // deleter itself and reports a generic FAILED/-1 (see
        // oneFailingChildDoesNotStrandItsSiblingsOrSkipThem's deleters,
        // which throw and are fine with that, since that test only checks
        // *that* something was thrown, never *which* exception) -- so a
        // thrown exception's own identity does not survive to here and
        // cannot be used to tell which child's failure is which. A real,
        // distinct rc for each child maps through ReturnCodes to a distinct
        // exception type instead, which can.
        AtomicBoolean aShouldFail = new AtomicBoolean(true);
        AtomicBoolean bShouldFail = new AtomicBoolean(true);
        FakeEntity parent = new FakeEntity(null, 0x400L, h -> 0);
        new FakeEntity(parent, 0x401L,
                h -> aShouldFail.get() ? DdsException.RET_NULL_POINTER : 0);
        new FakeEntity(parent, 0x402L,
                h -> bShouldFail.get() ? DdsException.RET_BUFFER_TOO_SMALL : 0);

        // Reverse order means the child added second (B) is attempted, and
        // fails, first.
        RuntimeException thrown = assertThrows(RuntimeException.class, parent::close);
        assertEquals(DdsBufferTooSmallException.class, thrown.getClass(),
                "the first failure encountered is the one thrown");
        assertEquals(1, thrown.getSuppressed().length);
        assertEquals(DdsNullPointerException.class, thrown.getSuppressed()[0].getClass());

        // Let both children actually succeed now, so neither is left for the
        // reaper to retry forever in the background.
        aShouldFail.set(false);
        bShouldFail.set(false);
        parent.close();
    }

    @Test
    void aFailedChildStaysRegisteredAndASucceedingChildDoesNotRetry() {
        AtomicBoolean childShouldFail = new AtomicBoolean(true);
        List<String> parentAttempts = new ArrayList<String>();
        List<String> childAttempts = new ArrayList<String>();

        FakeEntity parent = new FakeEntity(null, 0x500L, recording(parentAttempts, "parent"));
        new FakeEntity(parent, 0x501L, h -> {
            childAttempts.add("child");
            if (childShouldFail.get()) {
                throw new IllegalStateException("still blocked");
            }
            return 0;
        });

        assertThrows(RuntimeException.class, parent::close);
        assertEquals(1, childAttempts.size());
        assertEquals(0, parentAttempts.size(), "a live child means the parent delete is never attempted");

        // Let the child succeed this time, and retry.
        childShouldFail.set(false);
        parent.close();
        assertEquals(2, childAttempts.size(), "the failed child is still registered, so the retry reaches it");
        assertEquals(1, parentAttempts.size(), "now that no child remains, the parent's own delete runs");

        // A further close() is a no-op: NativeHandle.close() is idempotent,
        // and no children remain registered to re-attempt.
        parent.close();
        assertEquals(2, childAttempts.size());
        assertEquals(1, parentAttempts.size());
    }
}
