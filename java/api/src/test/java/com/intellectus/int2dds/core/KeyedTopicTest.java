package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import com.intellectus.int2dds.xtypes.FieldType;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises the public field-descriptor topic API ({@link
 * DomainParticipant#createTopic(String, com.intellectus.int2dds.types.IDdsType, List)}):
 * declaring a field as {@link TopicFieldDescriptor#isKey()} true makes it
 * the type's instance key, so KEEP_LAST(1) keeps the latest sample of EACH
 * distinct key value rather than only the latest sample overall.
 *
 * <p>A keyed topic ("id" declared as key) and a non-keyed control topic
 * (plain {@link DomainParticipant#createTopic(String,
 * com.intellectus.int2dds.types.IDdsType)}) each receive the same two
 * samples (id=1, id=2) under KEEP_LAST(1). On the keyed topic the two ids
 * are separate instances, so both survive; on the control topic they
 * collapse onto the one (NIL) instance, so only the later write survives.
 * That instance-count difference is the whole point of a key field
 * descriptor, so this pins it rather than just checking the topic can be
 * created.
 *
 * <p>The keyed pair and the control pair use SEPARATE participants (rather
 * than sharing one writer/reader participant across both), even though both
 * sides register the same DDS type name ("ConformanceRecord"). A
 * participant's type-support registration is deduped by Rust's {@code
 * type_id()} of the {@code TypeSupport} implementation, not by its field
 * descriptors/key content -- every raw-path {@code TypeSupport} is the same
 * concrete {@code RawTypeSupport} struct regardless of configuration, so a
 * plain topic created on a participant that already registered a keyed
 * "ConformanceRecord" would silently reuse that keyed registration instead
 * of getting its own unkeyed one. Separate participants give the control
 * topic its own registration namespace, so it is genuinely unkeyed.
 *
 * <p>Two participants per side, the pattern {@code ContentFilteredTopicTest}
 * and {@code MatchedEndpointsTest} settled on: matching does not loop back
 * within a single participant.
 */
class KeyedTopicTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    private static final long MATCH_TIMEOUT_NANOS = 5_000_000_000L;
    private static final long DATA_TIMEOUT_NANOS = 3_000_000_000L;

    @Test
    void keyedTopicDeliversBothInstancesWhileNonKeyedControlCollapsesToOne()
            throws InterruptedException {
        try (DomainParticipant keyedWriterParticipant = new DomainParticipant(testDomain());
                DomainParticipant keyedReaderParticipant = new DomainParticipant(testDomain());
                DomainParticipant controlWriterParticipant = new DomainParticipant(testDomain());
                DomainParticipant controlReaderParticipant = new DomainParticipant(testDomain())) {

            // "id" is ConformanceRecord's first CDR field, so no earlier
            // field needs declaring alongside it -- same ordering rule
            // ContentFilteredTopicTest relies on.
            List<TopicFieldDescriptor> idKeyField = Collections.singletonList(
                    new TopicFieldDescriptor("id", FieldType.INT32, true));

            Topic<ConformanceRecord> keyedWriterTopic = keyedWriterParticipant.createTopic(
                    "KeyedJavaTestTopic", new ConformanceRecord(), idKeyField);
            Topic<ConformanceRecord> keyedReaderTopic = keyedReaderParticipant.createTopic(
                    "KeyedJavaTestTopic", new ConformanceRecord(), idKeyField);

            // Control: same type name, no field descriptors -- no declared
            // key, so both writes below land on the same (NIL) instance.
            Topic<ConformanceRecord> controlWriterTopic = controlWriterParticipant.createTopic(
                    "KeyedJavaTestTopicControl", new ConformanceRecord());
            Topic<ConformanceRecord> controlReaderTopic = controlReaderParticipant.createTopic(
                    "KeyedJavaTestTopicControl", new ConformanceRecord());

            Publisher keyedPub = keyedWriterParticipant.createPublisher();
            Subscriber keyedSub = keyedReaderParticipant.createSubscriber();
            Publisher controlPub = controlWriterParticipant.createPublisher();
            Subscriber controlSub = controlReaderParticipant.createSubscriber();

            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            writerQos.setHistory(new History(HistoryKind.KEEP_LAST, 1));
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            readerQos.setHistory(new History(HistoryKind.KEEP_LAST, 1));

            DataWriter<ConformanceRecord> keyedWriter =
                    keyedPub.createDataWriter(keyedWriterTopic, writerQos);
            DataReader<ConformanceRecord> keyedReader =
                    keyedSub.createDataReader(keyedReaderTopic, ConformanceRecord::new, readerQos);

            DataWriter<ConformanceRecord> controlWriter =
                    controlPub.createDataWriter(controlWriterTopic, writerQos);
            DataReader<ConformanceRecord> controlReader =
                    controlSub.createDataReader(controlReaderTopic, ConformanceRecord::new, readerQos);

            waitForMatchedSubscriptions(keyedWriter, 1);
            waitForMatchedSubscriptions(controlWriter, 1);

            ConformanceRecord one = new ConformanceRecord();
            one.id = 1;
            one.label = "one";
            ConformanceRecord two = new ConformanceRecord();
            two.id = 2;
            two.label = "two";

            // A short gap between the two writes to the control topic's single
            // NIL instance lets the first sample's reliable delivery settle
            // into the reader's cache before the second arrives, so KEEP_LAST(1)
            // eviction of the older sample is observed deterministically. The
            // keyed writer does not need this (id=1/id=2 are different
            // instances, nothing to race) but is written the same way for
            // symmetry.
            keyedWriter.write(one);
            Thread.sleep(200);
            keyedWriter.write(two);
            controlWriter.write(one);
            Thread.sleep(200);
            controlWriter.write(two);

            List<Integer> keyedIds = collectIds(keyedReader, 2000L);
            List<Integer> controlIds = collectIds(controlReader, 2000L);

            assertTrue(keyedIds.contains(1) && keyedIds.contains(2),
                    "keyed reader should receive both instances (id=1 and id=2): got " + keyedIds);
            assertEquals(2, keyedIds.size(), "keyed reader should receive exactly 2 samples"
                    + " (one per instance) under KEEP_LAST(1): got " + keyedIds);

            assertEquals(1, controlIds.size(), "non-keyed control reader should receive exactly"
                    + " 1 sample under KEEP_LAST(1): got " + controlIds);
            assertTrue(controlIds.contains(2),
                    "non-keyed control reader should keep the latest write (id=2): got " + controlIds);

            keyedReader.setListener(null, null);
            controlReader.setListener(null, null);
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

    /** Bounded take loop collecting every sample's {@code id} until the cache empties or budget runs out. */
    private static List<Integer> collectIds(DataReader<ConformanceRecord> reader, long budgetMillis)
            throws InterruptedException {
        List<Integer> ids = new ArrayList<Integer>();
        long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
        long lastProgress = System.nanoTime();
        while (System.nanoTime() < deadline) {
            Sample<ConformanceRecord> sample = reader.take();
            if (sample != null && sample.data() != null) {
                ids.add(sample.data().id);
                lastProgress = System.nanoTime();
                continue;
            }
            if (System.nanoTime() - lastProgress > budgetMillis * 1_000_000L) {
                break;
            }
            Thread.sleep(20);
        }
        return ids;
    }
}
