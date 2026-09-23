package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises the type-agnostic serialized I/O pair {@link
 * DataWriter#writeSerialized} / {@link DataReader#takeSerialized} / {@link
 * DataReader#readSerialized} -- the primitives a DDS gateway/bridge needs to
 * forward samples without knowing the compiled type.
 */
class SerializedIoTest {

    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;

    /**
     * Strongest test: proves both directions at once. {@code r1.takeSerialized()}
     * off a normally-written sample must emit correct CDR, and {@code
     * w2.writeSerialized} must accept those exact bytes and have them decode
     * back through the ordinary typed path to an equal sample.
     */
    @Test
    void writeSerializedAndTakeSerializedRoundTripThroughTypedDecode() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedIoRoundTrip", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();

            DataWriter<ConformanceRecord> w1 = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r1 = sub.createDataReader(topic, ConformanceRecord::new);

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 11;
            sent.value = 2.25;
            sent.label = "round-trip";

            byte[] cdr = null;
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w1.write(sent);
                cdr = r1.takeSerialized();
                if (cdr != null && cdr.length > 0) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(cdr, "no serialized sample within 5s -- discovery or receive path");
            assertTrue(cdr.length > 0, "takeSerialized should return non-empty CDR bytes");

            // Second writer/reader pair on the same topic: writeSerialized the
            // bytes takeSerialized just produced, then decode with the
            // ordinary typed path.
            DataWriter<ConformanceRecord> w2 = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r2 = sub.createDataReader(topic, ConformanceRecord::new);

            Sample<ConformanceRecord> got = null;
            deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w2.writeSerialized(cdr, p.getCurrentTime());
                got = r2.take();
                if (got != null) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(got, "no sample within 5s after writeSerialized");
            assertTrue(got.info().validData(), "sample should carry valid data");
            assertEqualsRecord(sent, got.data());

            r1.setListener(null, null);
            r2.setListener(null, null);
        }
    }

    /** {@code readSerialized} must not remove; {@code takeSerialized} must. */
    @Test
    void readSerializedDoesNotRemoveButTakeSerializedDoes() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedIoReadVsTake", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            // Establish the match and drain the connection with the ordinary
            // typed path, so the buffer below starts from an empty cache.
            ConformanceRecord warm = new ConformanceRecord();
            warm.id = 1;
            warm.label = "warm";
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            Sample<ConformanceRecord> matched = null;
            while (System.nanoTime() < deadline) {
                w.write(warm);
                matched = r.take();
                if (matched != null) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(matched, "writer/reader failed to match within 5s");

            // readSerialized: proves non-removal via hasData(), a cache-level
            // (any sample state) readiness check -- rather than a second
            // readSerialized/takeSerialized call, since this reader's
            // take/read-next-sample primitives (like the pre-existing typed
            // take()/read()) only ever match a NOT_READ sample; once read
            // marks the sample READ, neither a further read nor a take can
            // observe that same sample again through these convenience
            // calls, standard OMG read_next_sample()/take_next_sample()
            // scoping, not something introduced by this method.
            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 2;
            sent.value = 5.5;
            sent.label = "non-removing";
            w.write(sent);
            pollUntil(r::hasData);
            assertTrue(r.hasData(), "sample should have arrived");

            byte[] a = r.readSerialized();
            assertNotNull(a, "no serialized sample");
            assertTrue(a.length > 0, "readSerialized should return non-empty CDR bytes");
            assertTrue(r.hasData(), "readSerialized must not remove the sample from the cache");

            // takeSerialized: a fresh sample on the same reader (default
            // History KEEP_LAST depth 1 replaces the still-cached, already-read
            // instance above), taken without an intervening read, proves
            // removal: hasData() drops to false and a further takeSerialized
            // finds nothing.
            ConformanceRecord sent2 = new ConformanceRecord();
            sent2.id = 3;
            sent2.value = 6.5;
            sent2.label = "removing";
            byte[] c = null;
            deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w.write(sent2);
                c = r.takeSerialized();
                if (c != null) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(c, "takeSerialized should return the just-written sample");
            assertTrue(c.length > 0, "takeSerialized should return non-empty CDR bytes");
            assertFalse(r.hasData(), "takeSerialized should remove the sample from the cache");

            r.setListener(null, null);
        }
    }

    /** A fresh, unmatched reader has nothing queued: takeSerialized returns null. */
    @Test
    void takeSerializedReturnsNullWhenNoData() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedIoNoData", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);
            assertNull(reader.takeSerialized(), "empty cache -> null");
        }
    }

    private static void assertEqualsRecord(ConformanceRecord expected, ConformanceRecord actual) {
        assertEquals(expected.id, actual.id);
        assertEquals(expected.value, actual.value);
        assertEquals(expected.label, actual.label);
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
