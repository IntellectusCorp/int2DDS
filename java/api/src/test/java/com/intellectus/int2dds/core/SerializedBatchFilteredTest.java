package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.conditions.InstanceState;
import com.intellectus.int2dds.conditions.SampleState;
import com.intellectus.int2dds.conditions.ViewState;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import com.intellectus.int2dds.xtypes.FieldType;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises the state-filtered and instance-scoped batch serialized reads
 * ({@link DataReader#takeSerializedBatch(int, int, int, int)}, {@link
 * DataReader#readSerializedBatch(int, int, int, int)}, {@link
 * DataReader#takeInstanceSerializedBatch}, {@link
 * DataReader#readInstanceSerializedBatch}) added alongside the {@code ANY}-
 * scoped batch pair in {@link SerializedBatchTest}, which this class does
 * not touch or duplicate -- it only proves the two new capabilities: masking
 * by sample/view/instance state, and scoping a batch to a single instance.
 */
class SerializedBatchFilteredTest {

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
     * The state-filter capability, both directions: a {@code NOT_READ}-only
     * batch take finds nothing once the sole sample has been marked READ by
     * a prior {@code readSerializedBatch(ANY,...)} (real negative -- the
     * sample exists but does not match the mask), while a {@code READ}-only
     * batch take retrieves that same already-READ sample.
     */
    @Test
    void stateFilteredBatchDistinguishesReadFromNotRead() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchFilteredStates", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            ConformanceRecord s1 = new ConformanceRecord();
            s1.id = 401;
            s1.label = "state-filtered";
            w.write(s1);

            // Non-destructively read with ANY state until the sample shows up,
            // which per DDS semantics also marks it READ.
            List<SerializedSample> read = pollUntilAnyBatchSize(r, 1);
            assertNotNull(read, "no sample within 5s");
            assertEquals(1, read.size());
            assertTrue(r.hasData(), "readSerializedBatch must not remove the sample");

            // Negative: the only sample is now READ, so a NOT_READ-only take
            // must find nothing -- the sample is still in the cache, it just
            // doesn't match the mask.
            List<SerializedSample> notReadBatch =
                    r.takeSerializedBatch(10, SampleState.NOT_READ, ViewState.ANY, InstanceState.ANY);
            assertNotNull(notReadBatch, "empty match should still return a non-null list");
            assertEquals(0, notReadBatch.size(),
                    "NOT_READ-only take should find nothing once the sample is READ");
            assertTrue(r.hasData(), "a non-matching filtered take must not remove the sample");

            // Positive: a READ-only take retrieves that same already-READ
            // sample -- the capability the no-arg takeSerializedBatch(int)
            // (state ANY) cannot express.
            List<SerializedSample> readOnlyBatch =
                    r.takeSerializedBatch(10, SampleState.READ, ViewState.ANY, InstanceState.ANY);
            assertEquals(1, readOnlyBatch.size(), "READ-only take should retrieve the READ sample");
            SerializedSample got = readOnlyBatch.get(0);
            assertTrue(got.bytes().length > 0, "the retrieved sample should carry non-empty CDR bytes");
            ConformanceRecord decoded = new ConformanceRecord();
            decoded.deserializeCdr(CdrReader.of(got.bytes()));
            assertEquals(401, decoded.id);
            assertFalse(r.hasData(), "the matching take should have removed the sample");

            r.setListener(null, null);
        }
    }

    /**
     * Instance-scoped batch take on a keyed topic with two distinct
     * instances (id=1, id=2): {@code takeInstanceSerializedBatch} scoped to
     * instance 1's handle returns only instance 1's sample(s) -- every
     * element decodes to id==1, id==2's sample is untouched -- and a
     * follow-up unscoped batch take still finds id==2.
     */
    @Test
    void instanceScopedBatchReturnsOnlyTargetInstance() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            // "id" is ConformanceRecord's first CDR field -- same ordering
            // rule KeyedTopicTest and ContentFilteredTopicTest rely on.
            List<TopicFieldDescriptor> idKeyField = Collections.singletonList(
                    new TopicFieldDescriptor("id", FieldType.INT32, true));
            Topic<ConformanceRecord> topic = p.createTopic(
                    "SerializedBatchFilteredInstances", new ConformanceRecord(), idKeyField);
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, keepAllWriterQos());
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, keepAllReaderQos());

            warmUpMatch(w, r);

            ConformanceRecord one = new ConformanceRecord();
            one.id = 1;
            one.label = "instance-one";
            ConformanceRecord two = new ConformanceRecord();
            two.id = 2;
            two.label = "instance-two";
            w.write(one);
            w.write(two);

            List<SerializedSample> both = pollUntilAnyBatchSize(r, 2);
            assertNotNull(both, "no batch of 2 within 5s");
            assertEquals(2, both.size());

            // Recover instance 1's handle off its own SampleInfo.
            InstanceHandle instance1 = null;
            for (SerializedSample sample : both) {
                ConformanceRecord decoded = new ConformanceRecord();
                decoded.deserializeCdr(CdrReader.of(sample.bytes()));
                if (decoded.id == 1) {
                    instance1 = new InstanceHandle(sample.info().instanceHandle());
                }
            }
            assertNotNull(instance1, "should have found instance 1's SampleInfo in the batch");

            List<SerializedSample> instanceBatch = r.takeInstanceSerializedBatch(
                    instance1, 10, SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            assertNotNull(instanceBatch, "empty match should still return a non-null list");
            assertTrue(instanceBatch.size() > 0, "instance 1's batch should not be empty");
            for (SerializedSample sample : instanceBatch) {
                ConformanceRecord decoded = new ConformanceRecord();
                decoded.deserializeCdr(CdrReader.of(sample.bytes()));
                assertEquals(1, decoded.id,
                        "takeInstanceSerializedBatch should return only instance 1's samples");
            }

            // Instance 2's sample must still be there, untouched by the
            // instance-scoped take above.
            List<SerializedSample> remainder = r.takeSerializedBatch(10);
            assertEquals(1, remainder.size(), "instance 2's sample should remain after the scoped take");
            ConformanceRecord decodedRemainder = new ConformanceRecord();
            decodedRemainder.deserializeCdr(CdrReader.of(remainder.get(0).bytes()));
            assertEquals(2, decodedRemainder.id);

            r.setListener(null, null);
        }
    }

    /** A fresh, unmatched reader has nothing queued: all four new methods return non-null empty lists. */
    @Test
    void filteredAndInstanceBatchReturnEmptyListWhenNoData() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SerializedBatchFilteredNoData", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);

            List<SerializedSample> take =
                    reader.takeSerializedBatch(10, SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            assertNotNull(take, "empty cache should still return a non-null list");
            assertEquals(0, take.size());

            List<SerializedSample> read =
                    reader.readSerializedBatch(10, SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            assertNotNull(read, "empty cache should still return a non-null list");
            assertEquals(0, read.size());

            // Nothing has ever been received, so lookupInstance's NIL handle
            // (see its own doc) stands in for "no such instance" here.
            InstanceHandle nil = new InstanceHandle(new byte[16]);
            List<SerializedSample> takeInstance = reader.takeInstanceSerializedBatch(
                    nil, 10, SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            assertNotNull(takeInstance, "empty cache should still return a non-null list");
            assertEquals(0, takeInstance.size());

            List<SerializedSample> readInstance = reader.readInstanceSerializedBatch(
                    nil, 10, SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            assertNotNull(readInstance, "empty cache should still return a non-null list");
            assertEquals(0, readInstance.size());
        }
    }

    /** Same warm-up-and-drain idiom as {@link SerializedBatchTest#warmUpMatch}. */
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

    /** Polls {@code readSerializedBatch(int)} (ANY state, non-destructive) until it sees {@code expectedSize} samples. */
    private static List<SerializedSample> pollUntilAnyBatchSize(
            DataReader<ConformanceRecord> r, int expectedSize) throws InterruptedException {
        List<SerializedSample> last = null;
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            last = r.readSerializedBatch(10, SampleState.ANY, ViewState.ANY, InstanceState.ANY);
            if (last.size() >= expectedSize) {
                return last;
            }
            Thread.sleep(20);
        }
        return last;
    }
}
