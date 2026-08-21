package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import com.intellectus.int2dds.xtypes.FieldType;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises {@link DataWriter#getKeyValue} and {@link DataReader#getKeyValue}
 * on the same keyed-topic fixture {@link InstanceManagementTest} uses ("id"
 * as key on {@link ConformanceRecord}). Both return the serialized KEY bytes
 * for an instance handle -- a key-only CDR the core produces, not a full
 * sample -- so this only asserts on the raw {@code byte[]}, not a decoded
 * sample.
 */
class GetKeyValueTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    private static final long MATCH_TIMEOUT_NANOS = 5_000_000_000L;
    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;

    @Test
    void writerAndReaderGetKeyValueRoundTripAndAgree() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {

            List<TopicFieldDescriptor> idKeyField = Collections.singletonList(
                    new TopicFieldDescriptor("id", FieldType.INT32, true));

            Topic<ConformanceRecord> writerTopic = writerParticipant.createTopic(
                    "GetKeyValueJavaTestTopic", new ConformanceRecord(), idKeyField);
            Topic<ConformanceRecord> readerTopic = readerParticipant.createTopic(
                    "GetKeyValueJavaTestTopic", new ConformanceRecord(), idKeyField);

            Publisher pub = writerParticipant.createPublisher();
            Subscriber sub = readerParticipant.createSubscriber();

            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            writerQos.setHistory(new History(HistoryKind.KEEP_LAST, 1));
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            readerQos.setHistory(new History(HistoryKind.KEEP_LAST, 1));

            DataWriter<ConformanceRecord> writer = pub.createDataWriter(writerTopic, writerQos);
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            waitForMatchedSubscriptions(writer, 1);

            ConformanceRecord one = new ConformanceRecord();
            one.id = 1;
            one.label = "one";

            // Writer-side round-trip: registerInstance -> getKeyValue.
            InstanceHandle registered = writer.registerInstance(one);
            byte[] writerKey = writer.getKeyValue(registered);
            assertNotNull(writerKey);
            assertTrue(writerKey.length > 0, "writer.getKeyValue should return non-empty key bytes");

            // Reader side: write + take to get a real instance handle off a
            // received sample's SampleInfo, then getKeyValue on the reader.
            writer.write(one);
            Sample<ConformanceRecord> received = takeUntil(reader, s -> s.data() != null);
            assertNotNull(received, "reader should receive the written sample");
            InstanceHandle readerHandle = new InstanceHandle(received.info().instanceHandle());
            byte[] readerKey = reader.getKeyValue(readerHandle);
            assertNotNull(readerKey);
            assertTrue(readerKey.length > 0, "reader.getKeyValue should return non-empty key bytes");

            // Cross-side consistency: same logical instance, same key bytes.
            assertArrayEquals(writerKey, readerKey,
                    "writer-side and reader-side key bytes should agree for the same instance");

            // Negative: a nil/unknown handle is rejected (BAD_PARAMETER).
            assertThrows(DdsException.class,
                    () -> writer.getKeyValue(new InstanceHandle(new byte[16])),
                    "getKeyValue with a nil handle should throw");

            reader.setListener(null, null);
        }
    }

    /** Bounded poll for the writer to see {@code expected} matched subscriptions. */
    private static void waitForMatchedSubscriptions(DataWriter<?> writer, int expected)
            throws InterruptedException {
        long deadline = System.nanoTime() + MATCH_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            if (writer.getMatchedSubscriptions().size() >= expected) {
                return;
            }
            Thread.sleep(50);
        }
        assertTrue(writer.getMatchedSubscriptions().size() >= expected,
                "writer should see " + expected + " matched subscriptions within 5s");
    }

    /** Bounded take loop returning the first sample matching {@code predicate}, or null on timeout. */
    private static Sample<ConformanceRecord> takeUntil(
            DataReader<ConformanceRecord> reader, java.util.function.Predicate<Sample<ConformanceRecord>> predicate)
            throws InterruptedException {
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        while (System.nanoTime() < deadline) {
            Sample<ConformanceRecord> sample = reader.take();
            if (sample != null && predicate.test(sample)) {
                return sample;
            }
            Thread.sleep(20);
        }
        return null;
    }
}
