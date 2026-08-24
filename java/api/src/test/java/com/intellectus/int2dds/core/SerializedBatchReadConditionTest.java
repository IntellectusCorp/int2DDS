package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.conditions.InstanceState;
import com.intellectus.int2dds.conditions.QueryCondition;
import com.intellectus.int2dds.conditions.ReadCondition;
import com.intellectus.int2dds.conditions.SampleState;
import com.intellectus.int2dds.conditions.ViewState;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.nio.charset.StandardCharsets;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises the ReadCondition/QueryCondition-filtered batch serialized reads
 * ({@link DataReader#takeSerializedBatch(ReadCondition, int)}, {@link
 * DataReader#readSerializedBatch(ReadCondition, int)}), the last variant of
 * the serialized-read matrix -- dynamic content-filtered batch reads,
 * distinct from the static-mask variants in {@link SerializedBatchFilteredTest}.
 */
class SerializedBatchReadConditionTest {

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
     * A QueryCondition filtering "id > 5": writes samples both above and
     * below the threshold, then a batch take through the condition must
     * return only the matching ones -- every returned element decodes to
     * id > 5, and a known non-matching id is never present.
     */
    @Test
    void queryConditionBatchTakeReturnsOnlyMatchingSamples() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            // "id" is ConformanceRecord's first CDR field -- field
            // descriptors are required for the SQL filter evaluator to
            // resolve it against sample bytes; see ContentFilteredTopicTest.
            Topic<ConformanceRecord> topic = Topic.createWithFieldDescriptors(p,
                    "SerializedBatchQueryCondition", new ConformanceRecord(),
                    new String[] {"id"}, new int[] {1}, new boolean[] {false});
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            byte[] expr = "id > %0".getBytes(StandardCharsets.UTF_8);
            byte[][] params = {"5".getBytes(StandardCharsets.UTF_8)};

            try (QueryCondition qc = r.createQueryCondition(
                    SampleState.ANY, ViewState.ANY, InstanceState.ANY, expr, params)) {

                ConformanceRecord low = new ConformanceRecord();
                low.id = 3;
                low.label = "low";
                ConformanceRecord high = new ConformanceRecord();
                high.id = 7;
                high.label = "high";
                w.write(low);
                w.write(high);

                List<SerializedSample> batch = pollUntilQueryBatchNonEmpty(r, qc);
                assertNotNull(batch, "no matching sample within 5s");
                assertTrue(batch.size() > 0, "should have found at least id=7");
                for (SerializedSample sample : batch) {
                    ConformanceRecord decoded = new ConformanceRecord();
                    decoded.deserializeCdr(CdrReader.of(sample.bytes()));
                    assertTrue(decoded.id > 5,
                            "every returned sample must satisfy id > 5, got id=" + decoded.id);
                    assertFalse(decoded.id == 3, "non-matching id=3 must never be returned");
                }

                // The non-matching sample (id=3) must still be sitting in the
                // cache, untouched by the filtered take -- a plain take
                // finds it.
                List<SerializedSample> remainder = r.takeSerializedBatch(10);
                boolean sawLow = false;
                for (SerializedSample sample : remainder) {
                    ConformanceRecord decoded = new ConformanceRecord();
                    decoded.deserializeCdr(CdrReader.of(sample.bytes()));
                    if (decoded.id == 3) {
                        sawLow = true;
                    }
                }
                assertTrue(sawLow, "id=3 should remain in the cache after the content-filtered take");
            }

            r.setListener(null, null);
        }
    }

    /**
     * A plain state-filtering ReadCondition (ANY/ANY/ANY): a batch take
     * through it retrieves every written sample, the same as the no-arg
     * {@link DataReader#takeSerializedBatch(int)}.
     */
    @Test
    void readConditionBatchTakeReturnsStateMatchingSamples() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchReadCondition", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            try (ReadCondition rc = r.createReadCondition(
                    SampleState.ANY, ViewState.ANY, InstanceState.ANY)) {

                ConformanceRecord a = new ConformanceRecord();
                a.id = 101;
                a.label = "a";
                ConformanceRecord b = new ConformanceRecord();
                b.id = 102;
                b.label = "b";
                w.write(a);
                w.write(b);

                List<SerializedSample> batch = pollUntilReadConditionBatchSize(r, rc, 2);
                assertNotNull(batch, "no batch of 2 within 5s");
                assertEquals(2, batch.size());
            }

            r.setListener(null, null);
        }
    }

    /**
     * readSerializedBatch(condition, ...) is non-removing: the sample stays
     * in the cache and is still there for a follow-up take.
     */
    @Test
    void readConditionBatchReadIsNonRemoving() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchReadConditionNonRemoving", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            try (ReadCondition rc = r.createReadCondition(
                    SampleState.ANY, ViewState.ANY, InstanceState.ANY)) {

                ConformanceRecord s = new ConformanceRecord();
                s.id = 201;
                s.label = "nonremoving";
                w.write(s);

                List<SerializedSample> read = pollUntilReadConditionReadBatchSize(r, rc, 1);
                assertNotNull(read, "no sample within 5s");
                assertEquals(1, read.size());
                assertTrue(r.hasData(), "readSerializedBatch(condition,...) must not remove the sample");

                List<SerializedSample> take = r.takeSerializedBatch(rc, 10);
                assertEquals(1, take.size(), "the sample should still be takeable after the non-removing read");
                assertFalse(r.hasData(), "the take should have removed the sample");
            }

            r.setListener(null, null);
        }
    }

    /** A fresh, unmatched reader has nothing queued: both condition-filtered methods return non-null empty lists. */
    @Test
    void conditionBatchReturnsEmptyListWhenNoData() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchReadConditionNoData", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);

            try (ReadCondition rc = reader.createReadCondition(
                    SampleState.ANY, ViewState.ANY, InstanceState.ANY)) {
                List<SerializedSample> take = reader.takeSerializedBatch(rc, 10);
                assertNotNull(take, "empty cache should still return a non-null list");
                assertEquals(0, take.size());

                List<SerializedSample> read = reader.readSerializedBatch(rc, 10);
                assertNotNull(read, "empty cache should still return a non-null list");
                assertEquals(0, read.size());
            }
        }
    }

    /** Same warm-up-and-drain idiom as {@link SerializedBatchFilteredTest#warmUpMatch}. */
    private static void warmUpMatch(DataWriter<ConformanceRecord> w, DataReader<ConformanceRecord> r)
            throws InterruptedException {
        ConformanceRecord warm = new ConformanceRecord();
        warm.id = 999999;
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

    /** Polls takeSerializedBatch(queryCondition, ...) until it removes at least one matching sample. */
    private static List<SerializedSample> pollUntilQueryBatchNonEmpty(
            DataReader<ConformanceRecord> r, QueryCondition qc) throws InterruptedException {
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            List<SerializedSample> batch = r.takeSerializedBatch(qc, 100);
            if (!batch.isEmpty()) {
                return batch;
            }
            Thread.sleep(20);
        }
        return r.takeSerializedBatch(qc, 100);
    }

    /**
     * Polls takeSerializedBatch(readCondition, ...) until it has cumulatively
     * removed {@code expectedSize} samples (writes may not all be visible in
     * the cache on the first poll), accumulating across calls in case
     * delivery is split across polls.
     */
    private static List<SerializedSample> pollUntilReadConditionBatchSize(
            DataReader<ConformanceRecord> r, ReadCondition rc, int expectedSize) throws InterruptedException {
        List<SerializedSample> acc = new java.util.ArrayList<SerializedSample>();
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline && acc.size() < expectedSize) {
            List<SerializedSample> batch = r.takeSerializedBatch(rc, 100);
            acc.addAll(batch);
            if (acc.size() < expectedSize) {
                Thread.sleep(20);
            }
        }
        return acc;
    }

    /** Polls readSerializedBatch(readCondition, ...) until it sees {@code expectedSize} samples, without removing them. */
    private static List<SerializedSample> pollUntilReadConditionReadBatchSize(
            DataReader<ConformanceRecord> r, ReadCondition rc, int expectedSize) throws InterruptedException {
        List<SerializedSample> last = null;
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            last = r.readSerializedBatch(rc, 100);
            if (last.size() >= expectedSize) {
                return last;
            }
            Thread.sleep(20);
        }
        return last;
    }
}
