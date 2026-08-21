package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises the ContentFilteredTopic (CFT) path added in this branch: a
 * reader created on a CFT should only receive samples matching the filter
 * expression, while a plain reader on the same topic receives everything.
 *
 * <p>The related topic must be created with explicit field descriptors
 * ({@link Topic#createWithFieldDescriptors}) for the core's SQL filter
 * evaluator to resolve {@code id} against sample bytes at all -- see that
 * seam's own doc. A plain {@link DomainParticipant#createTopic} topic (no
 * field descriptors) makes every filter evaluation error, which the core
 * treats as "passes", silently turning the CFT into a no-op passthrough;
 * this test's whole point is pinning that filtering actually drops samples,
 * so it deliberately does not use the plain path for the related topic.
 *
 * <p>Two participants, the pattern {@code ReliabilityLivelinessTest} and
 * {@code MatchedEndpointsTest} settled on: matching does not loop back
 * within a single participant.
 */
class ContentFilteredTopicTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    private static final long MATCH_TIMEOUT_NANOS = 5_000_000_000L;
    private static final long DATA_TIMEOUT_NANOS = 3_000_000_000L;

    @Test
    void cftReaderReceivesOnlyMatchingSamplesWhilePlainReaderReceivesAll() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {

            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("CftJavaTestTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            // KEEP_ALL: ConformanceRecord has no declared key, so both samples
            // below land on the same (NIL) instance -- KEEP_LAST's default
            // depth of 1 would let the second write evict the first before
            // either reader ever saw it, which would look exactly like
            // filtering but isn't.
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            writerQos.setHistory(new History(HistoryKind.KEEP_ALL, 0));
            DataWriter<ConformanceRecord> writer = pub.createDataWriter(writerTopic, writerQos);

            // Field descriptors ("id" only -- it's the first CDR field, so no
            // earlier field needs declaring) are what let the core's filter
            // evaluator actually resolve "id" against sample bytes.
            Topic<ConformanceRecord> readerTopic = Topic.createWithFieldDescriptors(
                    readerParticipant, "CftJavaTestTopic", new ConformanceRecord(),
                    new String[] {"id"}, new int[] {1}, new boolean[] {false});
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            readerQos.setHistory(new History(HistoryKind.KEEP_ALL, 0));

            DataReader<ConformanceRecord> plainReader =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            ContentFilteredTopic<ConformanceRecord> cft = readerParticipant.createContentFilteredTopic(
                    "CftJavaTestTopicFiltered", readerTopic, "id > %0", "5");
            DataReader<ConformanceRecord> cftReader =
                    sub.createDataReader(cft, ConformanceRecord::new, readerQos);

            waitForMatchedSubscriptions(writer, 2);

            ConformanceRecord low = new ConformanceRecord();
            low.id = 3;
            low.label = "low";
            ConformanceRecord high = new ConformanceRecord();
            high.id = 7;
            high.label = "high";
            writer.write(low);
            writer.write(high);

            List<Integer> cftIds = collectIds(cftReader, 2000L);
            List<Integer> plainIds = collectIds(plainReader, 2000L);

            assertTrue(cftIds.contains(7), "CFT reader should receive id=7 (matches id > 5): got " + cftIds);
            assertFalse(cftIds.contains(3),
                    "CFT reader should NOT receive id=3 (filtered out by id > 5): got " + cftIds);

            assertTrue(plainIds.contains(7), "plain reader should receive id=7: got " + plainIds);
            assertTrue(plainIds.contains(3), "plain reader should receive id=3 (unfiltered): got " + plainIds);

            cft.setEnabled(false);

            cftReader.setListener(null, null);
            plainReader.setListener(null, null);
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
