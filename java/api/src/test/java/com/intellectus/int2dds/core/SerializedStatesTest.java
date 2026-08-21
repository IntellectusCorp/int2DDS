package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.conditions.InstanceState;
import com.intellectus.int2dds.conditions.SampleState;
import com.intellectus.int2dds.conditions.ViewState;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises the state-filtered serialized reads {@link
 * DataReader#takeSerialized(int, int, int)} / {@link
 * DataReader#readSerialized(int, int, int)} -- the raw-CDR-plus-{@link
 * SampleInfo} counterpart of the no-arg (NOT_READ-only) serialized pair in
 * {@link SerializedIoTest}, capable of retrieving samples in states the
 * no-arg/typed take()/read() cannot reach.
 */
class SerializedStatesTest {

    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;

    /** Basic take with ANY/ANY/ANY masks: bytes, validData and instanceHandle all populated. */
    @Test
    void takeSerializedWithAnyMasksReturnsBytesAndInfo() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedStatesBasic", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 21;
            sent.value = 3.5;
            sent.label = "states-basic";

            SerializedSample s = null;
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w.write(sent);
                s = r.takeSerialized(SampleState.ANY, ViewState.ANY, InstanceState.ANY);
                if (s != null) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(s, "no serialized sample within 5s -- discovery or receive path");
            assertTrue(s.bytes().length > 0, "takeSerialized(states) should return non-empty CDR bytes");
            assertTrue(s.info().validData(), "sample should carry valid data");
            assertEquals(16, s.info().instanceHandle().length, "instanceHandle should be a 16-byte handle");

            r.setListener(null, null);
        }
    }

    /**
     * The unique capability: {@code readSerialized} marks a sample READ
     * without removing it; a state-filtered {@code takeSerialized} with
     * {@code SampleState.READ} then retrieves that same already-READ sample
     * -- something the no-arg (NOT_READ-only) {@code takeSerialized()} cannot
     * do, since it only ever matches a NOT_READ sample.
     */
    @Test
    void takeSerializedWithReadMaskRetrievesAlreadyReadSample() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedStatesReadThenTake", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            // Establish the match with a throwaway sample, drained via the
            // ordinary no-arg takeSerialized() so the buffer below starts
            // from an empty cache.
            ConformanceRecord warm = new ConformanceRecord();
            warm.id = 1;
            warm.label = "warm";
            byte[] warmed = null;
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w.write(warm);
                warmed = r.takeSerialized();
                if (warmed != null) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(warmed, "writer/reader failed to match within 5s");

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 22;
            sent.value = 4.5;
            sent.label = "read-then-take";
            w.write(sent);
            pollUntil(r::hasData);
            assertTrue(r.hasData(), "sample should have arrived");

            SerializedSample firstRead = r.readSerialized(SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            assertNotNull(firstRead, "readSerialized(states) should see the just-arrived sample");
            assertTrue(firstRead.bytes().length > 0);
            assertTrue(r.hasData(), "readSerialized(states) must not remove the sample from the cache");

            SerializedSample again = r.takeSerialized(SampleState.READ, ViewState.ANY, InstanceState.ANY);
            assertNotNull(again, "READ-filtered takeSerialized should retrieve the already-READ sample");
            assertArrayEquals(firstRead.bytes(), again.bytes(),
                    "the READ-filtered take should return the exact bytes the earlier read saw");
            assertFalse(r.hasData(), "the READ-filtered take should have removed the sample");

            r.setListener(null, null);
        }
    }

    /** A mask matching no sample in the cache (only a NOT_READ one present) returns null. */
    @Test
    void takeSerializedReturnsNullWhenMaskMatchesNothing() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedStatesNoMatch", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            // Establish the match, same warm-up as above.
            ConformanceRecord warm = new ConformanceRecord();
            warm.id = 1;
            warm.label = "warm";
            byte[] warmed = null;
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w.write(warm);
                warmed = r.takeSerialized();
                if (warmed != null) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(warmed, "writer/reader failed to match within 5s");

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 23;
            sent.value = 5.5;
            sent.label = "not-read-only";
            w.write(sent);
            pollUntil(r::hasData);
            assertTrue(r.hasData(), "sample should have arrived, still NOT_READ");

            SerializedSample none = r.takeSerialized(SampleState.READ, ViewState.ANY, InstanceState.ANY);
            assertNull(none, "no READ sample is present yet -- mask matches nothing");
            assertTrue(r.hasData(), "the unmatched NOT_READ sample should remain in the cache");

            r.setListener(null, null);
        }
    }

    /** Bounded poll for {@code condition} to become true, without re-writing anything. */
    private static void pollUntil(java.util.function.BooleanSupplier condition)
            throws InterruptedException {
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            if (condition.getAsBoolean()) {
                return;
            }
            Thread.sleep(20);
        }
    }
}
