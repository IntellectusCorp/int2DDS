package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import org.junit.jupiter.api.Test;

/**
 * Exercises the batch serialized reads {@link DataReader#takeSerializedBatch}
 * / {@link DataReader#readSerializedBatch} -- the multi-sample counterpart of
 * the single-sample pair in {@link SerializedIoTest}: up to N samples' raw
 * CDR bytes plus {@link SampleInfo} in one native call, returned as a {@link
 * List}.
 *
 * <p>All fixtures use {@code History(KEEP_ALL)} on both writer and reader:
 * {@code ConformanceRecord} is unkeyed (a single implicit instance), so the
 * default {@code KEEP_LAST} depth 1 would let each new write replace the
 * previous one before a multi-sample batch could observe them together.
 */
class SerializedBatchTest {

    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;

    private static DataWriterQos keepAllWriterQos() {
        DataWriterQos qos = new DataWriterQos();
        qos.setHistory(new History(HistoryKind.KEEP_ALL, 0));
        return qos;
    }

    private static DataReaderQos keepAllReaderQos() {
        DataReaderQos qos = new DataReaderQos();
        qos.setHistory(new History(HistoryKind.KEEP_ALL, 0));
        return qos;
    }

    /**
     * Strongest test: 3 distinct samples written, batch-taken in one call,
     * and every element's bytes round-trip through the type's own CDR codec
     * back to the exact record that was sent.
     */
    @Test
    void takeSerializedBatchReturnsThreeDistinctValidSamples() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchTakeThree", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            // Establish the match and drain the connection so the buffer
            // below starts from an empty cache.
            warmUpMatch(w, r);

            ConformanceRecord s1 = new ConformanceRecord();
            s1.id = 101;
            s1.value = 1.5;
            s1.label = "batch-1";
            ConformanceRecord s2 = new ConformanceRecord();
            s2.id = 102;
            s2.value = 2.5;
            s2.label = "batch-2";
            ConformanceRecord s3 = new ConformanceRecord();
            s3.id = 103;
            s3.value = 3.5;
            s3.label = "batch-3";
            w.write(s1);
            w.write(s2);
            w.write(s3);

            // Poll non-destructively until all 3 have arrived, then take them
            // as a batch in one call.
            List<SerializedSample> batch = pollUntilBatchSize(r, 3);
            assertNotNull(batch, "no batch of 3 within 5s");
            assertEquals(3, batch.size(), "takeSerializedBatch should return exactly 3 samples");

            Set<Integer> decodedIds = new HashSet<Integer>();
            for (SerializedSample sample : batch) {
                assertTrue(sample.bytes().length > 0, "each batch element should carry non-empty CDR bytes");
                assertTrue(sample.info().validData(), "each batch element should carry valid data");
                ConformanceRecord decoded = new ConformanceRecord();
                decoded.deserializeCdr(CdrReader.of(sample.bytes()));
                decodedIds.add(decoded.id);
                if (decoded.id == 101) {
                    assertEqualsRecord(s1, decoded);
                } else if (decoded.id == 102) {
                    assertEqualsRecord(s2, decoded);
                } else if (decoded.id == 103) {
                    assertEqualsRecord(s3, decoded);
                } else {
                    throw new AssertionError("unexpected decoded id: " + decoded.id);
                }
            }
            assertEquals(3, decodedIds.size(), "the 3 batch elements should decode to 3 distinct samples");

            r.setListener(null, null);
        }
    }

    /**
     * {@code readSerializedBatch} must not remove; a following {@code
     * takeSerializedBatch} must still return the same samples. Proven two
     * ways: {@link DataReader#hasData()} stays true after the read and
     * flips false only after the take, and the take's size still matches --
     * confirming the native batch take/read match sample state {@code ANY}
     * (not NOT_READ-only like the no-arg single-sample {@link
     * DataReader#readSerialized()}), so an already-READ sample left by the
     * read remains eligible for the take.
     */
    @Test
    void readSerializedBatchDoesNotRemoveButTakeSerializedBatchDoes() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchReadVsTake", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            ConformanceRecord s1 = new ConformanceRecord();
            s1.id = 201;
            s1.label = "rvt-1";
            ConformanceRecord s2 = new ConformanceRecord();
            s2.id = 202;
            s2.label = "rvt-2";
            w.write(s1);
            w.write(s2);

            List<SerializedSample> read = pollUntilBatchSize(r, 2);
            assertNotNull(read, "no batch of 2 within 5s");
            assertEquals(2, read.size(), "readSerializedBatch should see both samples");
            assertTrue(r.hasData(), "readSerializedBatch must not remove samples from the cache");

            List<SerializedSample> taken = r.takeSerializedBatch(10);
            assertEquals(2, taken.size(),
                    "takeSerializedBatch matches ANY sample state, so it should still retrieve "
                            + "the samples readSerializedBatch already marked READ");
            assertFalse(r.hasData(), "takeSerializedBatch should remove the samples from the cache");

            r.setListener(null, null);
        }
    }

    /** {@code maxSamples} caps how many are returned in one call; a follow-up call drains the rest. */
    @Test
    void takeSerializedBatchRespectsMaxSamplesCap() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchMaxCap", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            for (int i = 0; i < 3; i++) {
                ConformanceRecord s = new ConformanceRecord();
                s.id = 300 + i;
                s.label = "cap-" + i;
                w.write(s);
            }

            List<SerializedSample> peek = pollUntilBatchSize(r, 3);
            assertNotNull(peek, "no batch of 3 within 5s");
            assertEquals(3, peek.size());

            List<SerializedSample> capped = r.takeSerializedBatch(2);
            assertEquals(2, capped.size(), "takeSerializedBatch(2) should respect the cap");

            List<SerializedSample> remainder = r.takeSerializedBatch(10);
            assertEquals(1, remainder.size(), "the remaining sample should still be takeable");

            r.setListener(null, null);
        }
    }

    /** A fresh, unmatched reader has nothing queued: takeSerializedBatch returns an empty, non-null list. */
    @Test
    void takeSerializedBatchReturnsEmptyListWhenNoData() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchNoData", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);
            List<SerializedSample> batch = reader.takeSerializedBatch(10);
            assertNotNull(batch, "empty cache should still return a non-null list");
            assertEquals(0, batch.size());

            List<SerializedSample> readBatch = reader.readSerializedBatch(10);
            assertNotNull(readBatch, "empty cache should still return a non-null list");
            assertEquals(0, readBatch.size());
        }
    }

    /** {@code maxSamples <= 0} is rejected before any native call. */
    @Test
    void takeAndReadSerializedBatchRejectNonPositiveMaxSamples() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchBadArg", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);
            assertThrows(IllegalArgumentException.class, () -> reader.takeSerializedBatch(0));
            assertThrows(IllegalArgumentException.class, () -> reader.takeSerializedBatch(-1));
            assertThrows(IllegalArgumentException.class, () -> reader.readSerializedBatch(0));
        }
    }

    private static void assertEqualsRecord(ConformanceRecord expected, ConformanceRecord actual) {
        assertEquals(expected.id, actual.id);
        assertEquals(expected.value, actual.value);
        assertEquals(expected.label, actual.label);
    }

    /**
     * Establishes the match between {@code w} and {@code r}, then drains the
     * cache back to empty. Repeatedly writing a throwaway "warm" sample until
     * a {@code take()} succeeds is necessary because a write racing ahead of
     * discovery is silently dropped (VOLATILE durability) -- but under {@code
     * KEEP_ALL} that repetition can also land more than one warm sample once
     * the match completes, since each write to the sole (unkeyed) instance is
     * queued rather than replacing the last; a short drain loop below sweeps
     * up any such leftovers so the cache is empty before the real test data
     * is written.
     */
    private static void warmUpMatch(DataWriter<ConformanceRecord> w, DataReader<ConformanceRecord> r)
            throws InterruptedException {
        ConformanceRecord warm = new ConformanceRecord();
        warm.id = 1;
        warm.label = "warm";
        Sample<ConformanceRecord> matched = null;
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            w.write(warm);
            matched = r.take();
            if (matched != null) {
                break;
            }
            Thread.sleep(20);
        }
        assertNotNull(matched, "writer/reader failed to match within 5s");

        // Drain until a full pass finds nothing, then wait a short grace
        // period and confirm -- a late-arriving warm sample from the loop
        // above can otherwise show up after this method already returned.
        long drainDeadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < drainDeadline) {
            boolean tookAny = false;
            while (r.hasData()) {
                r.take();
                tookAny = true;
            }
            if (!tookAny) {
                Thread.sleep(50);
                if (!r.hasData()) {
                    return;
                }
            } else {
                Thread.sleep(10);
            }
        }
    }

    /** Polls {@code readSerializedBatch} (non-destructive) until it sees {@code expectedSize} samples. */
    private static List<SerializedSample> pollUntilBatchSize(
            DataReader<ConformanceRecord> r, int expectedSize) throws InterruptedException {
        List<SerializedSample> last = null;
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            last = r.readSerializedBatch(10);
            if (last.size() >= expectedSize) {
                return last;
            }
            Thread.sleep(20);
        }
        return last;
    }
}
