package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.ParticipantQos;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
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
        // A local call counter, wired through the package-private
        // deleter-seam constructor, the same pattern
        // NativeCleanerTest.anExplicitCloseReleasesExactlyOnce uses -- immune
        // to any other test's background retry activity, unlike a shared
        // NativeCleaner counter, and able to prove the deleter runs exactly
        // once rather than only that this entity stayed closed. The counting
        // deleter still delegates to the real delete, so the native
        // participant this creates is actually released, not leaked.
        AtomicInteger calls = new AtomicInteger();
        DomainParticipant p = DomainParticipant.createForTest(testDomain(), handle -> {
            calls.incrementAndGet();
            return FfiAccess.deleteParticipant(handle);
        });

        p.close();
        assertTrue(p.isClosed());
        assertEquals(1, calls.get(), "the first close must release the handle exactly once");

        // The native delete reclaims a strong reference; a second call
        // reaching the deleter would be a double free.
        p.close();
        assertTrue(p.isClosed());
        assertEquals(1, calls.get(), "closing an already-closed handle must not release again");
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
        // child to refuse it. The assertion only shows the gauge is back at
        // baseline by the time awaitReaped returns -- consistent with never
        // having been refused at all, but equally consistent with a refusal
        // that was deferred and then itself retried successfully within the
        // same window; it does not distinguish the two, and does not need to.
        assertEquals(deferredBefore, NativeCleaner.deferredCount(),
                "a childless participant should never end up stuck in the deferred list");
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

            // The stronger half: not just "didn't throw", but "actually
            // joined this run's isolated domain". The Gradle test task
            // exports DDS_DOMAIN_ID as the same random value it publishes
            // via int2dds.test.domain (see build.gradle.kts), so the core's
            // own sentinel resolution -- domain_participant_factory.rs
            // resolving -1 through DDS_DOMAIN_ID before ever constructing
            // the participant -- lands this participant on testDomain(),
            // not on the real default domain. int2dds_participant_get_domain_id
            // reads the resolved value back off the core's own participant
            // object, not the -1 this constructor was called with.
            ByteBuffer domainIdOut = ByteBuffer.allocateDirect(4).order(ByteOrder.nativeOrder());
            int rc = FfiAccess.participantGetDomainId(
                    p.handle(), FfiAccess.directBufferAddress(domainIdOut));
            assertEquals(0, rc, "get_domain_id must succeed on a live participant");
            assertEquals(testDomain(), domainIdOut.getInt(0),
                    "the sentinel must resolve to this run's isolated domain, via DDS_DOMAIN_ID");
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
        // Cast disambiguates from the (int, String) profile-create constructor.
        assertThrows(NullPointerException.class,
                () -> new DomainParticipant(testDomain(), (ParticipantQos) null));
    }
}
