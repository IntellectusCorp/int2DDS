package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises the identity/state getters added in this branch: {@code
 * DataReader#getGuid}, {@code DataWriter#getGuid}, {@code
 * DataReader#hasData} and {@code DomainParticipant#getCurrentTime}.
 *
 * <p>Two participants, the same pattern {@code ReliabilityLivelinessTest}
 * uses: matching does not loop back within a single participant, so the
 * writer and the reader each live on their own participant.
 */
class EntityIdentityGettersTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    @Test
    void getGuidHasDataAndCurrentTime() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("EntityIdentityTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic, writerQos);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("EntityIdentityTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            // GUIDs: 16 bytes, not all-zero, and distinct between the writer
            // and the reader.
            byte[] writerGuid = w.getGuid();
            byte[] readerGuid = r.getGuid();
            assertEquals(16, writerGuid.length);
            assertEquals(16, readerGuid.length);
            assertFalse(isAllZero(writerGuid), "writer GUID should not be all-zero");
            assertFalse(isAllZero(readerGuid), "reader GUID should not be all-zero");
            assertFalse(Arrays.equals(writerGuid, readerGuid),
                    "writer and reader GUIDs should differ");

            // getCurrentTime: positive and monotonic non-decreasing across
            // two calls.
            long t1 = writerParticipant.getCurrentTime();
            assertTrue(t1 > 0, "current time should be positive");
            long t2 = writerParticipant.getCurrentTime();
            assertTrue(t2 >= t1, "time should not go backward");

            // Fresh reader, no data written yet.
            assertFalse(r.hasData(), "fresh reader should not have data before any write");

            // Wait for the writer to see the matched reliable reader before
            // writing -- the same bounded-poll pattern ReliabilityLivelinessTest
            // uses, since RELIABLE only helps once the match exists.
            List<SubscriptionBuiltinTopicData> found = Collections.emptyList();
            long matchDeadline = System.nanoTime() + 5_000_000_000L;
            boolean matched = false;
            while (System.nanoTime() < matchDeadline) {
                found = w.getMatchedSubscriptions();
                matched = found.stream().anyMatch(d -> "EntityIdentityTopic".equals(d.topicName()));
                if (matched) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(matched, "writer should see the matched reliable reader within 5s");

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 7;
            sent.value = 2.5;
            sent.label = "identity";
            w.write(sent);

            // Bounded retry for delivery, then hasData() should flip true.
            boolean hasData = false;
            long dataDeadline = System.nanoTime() + 5_000_000_000L;
            while (System.nanoTime() < dataDeadline) {
                if (r.hasData()) {
                    hasData = true;
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hasData, "reader should have data within 5s of a reliable write");

            r.take();
            assertFalse(r.hasData(), "reader should have no data left after take()");
        }
    }

    private static boolean isAllZero(byte[] bytes) {
        for (byte b : bytes) {
            if (b != 0) {
                return false;
            }
        }
        return true;
    }
}
