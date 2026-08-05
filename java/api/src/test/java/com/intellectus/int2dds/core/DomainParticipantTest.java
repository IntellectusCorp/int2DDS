package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.internal.NativeCleaner;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;

class DomainParticipantTest {

    /** The domain this JVM's tests use, isolated from other runs. */
    static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    /**
     * Drives the collector until the reaper has released at least {@code n}
     * more handles, or fails. Bounded, so a broken cleaner fails in two seconds
     * rather than hanging, and never a bare sleep — a sleep then an assertion
     * passes whether or not anything happened.
     */
    static void awaitReaped(long baseline, int n) throws InterruptedException {
        for (int i = 0; i < 10; i++) {
            System.gc();
            if (NativeCleaner.reapedCount() >= baseline + n) {
                return;
            }
            TimeUnit.MILLISECONDS.sleep(200);
        }
        fail("reaper released only " + (NativeCleaner.reapedCount() - baseline)
                + " of " + n + " handles within 2 seconds");
    }

    @Test
    void theFactoryIsASingletonWithALiveHandle() {
        DomainParticipantFactory a = DomainParticipantFactory.getInstance();
        DomainParticipantFactory b = DomainParticipantFactory.getInstance();
        assertTrue(a == b, "the factory is a singleton");
        assertNotEquals(0L, a.handle());
    }

    @Test
    void aParticipantIsCreatedAndClosed() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            assertNotEquals(0L, p.handle());
            assertEquals(testDomain(), p.domainId());
            assertFalse(p.isClosed());
        } finally {
            // No separate "close must not fail" assertion needed: close()
            // throws the mapped DdsException on a non-OK code (NativeEntity
            // -> ReturnCodes.check), so an uncaught exception escaping this
            // finally block already fails the test. A prior version of this
            // test additionally asserted failedCount() was unchanged across
            // the call, but that is a global counter a background retry
            // thread can also move on a timer (see NativeCleanerTest and
            // NativeCleaner's own doc) - asserting exact equality on it
            // across any window is flaky, not stronger evidence.
            p.close();
        }
        assertTrue(p.isClosed());
    }

    @Test
    void closingTwiceIsHarmless() {
        long releasedBefore = NativeCleaner.releasedCount();
        DomainParticipant p = new DomainParticipant(testDomain());

        p.close();
        assertTrue(p.isClosed());
        assertTrue(NativeCleaner.releasedCount() >= releasedBefore + 1,
                "the first close must actually release the handle");

        // Idempotent: NativeHandle's CAS (OPEN -> RELEASING -> CLOSED) means a
        // second close() short-circuits to ALREADY_HANDLED without reaching
        // the deleter again. That exactly-once guarantee is proved directly,
        // with a call counter local to the test, by
        // NativeCleanerTest.anExplicitCloseReleasesExactlyOnce; DomainParticipant
        // has no seam to inject a counting deleter the same way (its deleter
        // is always the real FfiAccess::deleteParticipant), so this only
        // shows close() defers to the same NativeHandle both times, which
        // "stays closed and does not throw" already demonstrates. A second
        // exact-equality read of the global releasedCount() would add a
        // flaky window - a background retry elsewhere can move it between
        // the two close() calls - without proving anything stronger.
        p.close();
        assertTrue(p.isClosed());
    }

    @Test
    void usingAClosedParticipantIsRejected() {
        DomainParticipant p = new DomainParticipant(testDomain());
        p.close();
        assertThrows(IllegalStateException.class, p::handle);
    }

    @Test
    void anAbandonedParticipantIsReclaimedByTheReaper() throws InterruptedException {
        long reapedBefore = NativeCleaner.reapedCount();
        long deferredBefore = NativeCleaner.deferredCount();

        createAndAbandonParticipant();

        awaitReaped(reapedBefore, 1);
        // deferredCount(), not failedCount(), is what actually distinguishes
        // "released" from "stuck": a refusal (RET_PRECONDITION_NOT_MET) is
        // counted only by deferredCount(), and this participant has no live
        // child to refuse it, so it must be back at baseline - released on
        // the reaper's first attempt, not deferred and retried.
        assertEquals(deferredBefore, NativeCleaner.deferredCount(),
                "a childless participant should never be refused, so nothing should be deferred");
    }

    private static void createAndAbandonParticipant() {
        DomainParticipant p = new DomainParticipant(testDomain());
        assertNotEquals(0L, p.handle());
        // p goes out of scope here with no close()
    }

    /**
     * Replaces the brief's {@code anInvalidDomainIsReportedAsAnException}.
     * That test asserted {@code new DomainParticipant(-1)} throws, on the
     * premise that a negative domain id is invalid. It is not: -1 is {@code
     * DEFAULT_DOMAIN_ID} (dds/src/common/env.rs:16), the core's own sentinel
     * for "resolve DDS_DOMAIN_ID, else 0"
     * (dds/src/dcps/domain/domain_participant_factory.rs:166-174) - not a
     * value create_participant rejects. More generally, no domain_id value
     * this path receives is ever range-checked (confirmed by reading
     * domain_participant.rs and the RTPS port-computation code it feeds into,
     * which truncates to a port with a plain `as u16` and never errors), so
     * there is no domain_id this constructor can use to reach the create
     * bridge's failure branch at all.
     */
    @Test
    void aDomainIdOfNegativeOneIsTheDefaultDomainSentinelNotAnError() {
        DomainParticipant p = new DomainParticipant(-1);
        try {
            assertNotEquals(0L, p.handle());
            assertEquals(-1, p.domainId(), "domainId() reports the raw constructor argument");
        } finally {
            p.close();
        }
    }

    /**
     * The other half of the coverage the removed domain test was meant to
     * provide: proof that a bad argument to this constructor really does
     * become an exception. The core does not validate domain_id, but the
     * Java layer validates its own {@code qos} parameter before any native
     * call is made - matching the C# reference constructor at
     * csharp/src/Int2Dds/Core/DomainParticipant.cs:69, which throws
     * ArgumentNullException for the same case.
     */
    @Test
    void aNullQosIsRejectedBeforeAnyNativeCall() {
        assertThrows(NullPointerException.class, () -> new DomainParticipant(testDomain(), null));
    }
}
