package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.status.PublicationMatchedStatus;
import com.intellectus.int2dds.status.SubscriptionMatchedStatus;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises {@code DataWriter#getPublicationMatchedStatus} and {@code
 * DataReader#getSubscriptionMatchedStatus}, added in this branch. Same
 * two-participant pattern {@code MatchedEndpointsTest} and {@code
 * ReliabilityLivelinessTest} use: matching does not loop back within a
 * single participant, so the writer and the reader each live on their own
 * participant.
 */
class MatchedStatusTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    @Test
    void matchedStatusReportsCountsAndHandleAfterMatching() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("MatchedStatusTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic, writerQos);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("MatchedStatusTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            // Bounded wait on the writer side seeing the matched reader --
            // the same poll pattern ReliabilityLivelinessTest and
            // MatchedEndpointsTest use.
            long writerDeadline = System.nanoTime() + 5_000_000_000L;
            boolean writerMatched = false;
            while (System.nanoTime() < writerDeadline) {
                writerMatched = !w.getMatchedSubscriptions().isEmpty();
                if (writerMatched) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(writerMatched, "writer should see the matched reader within 5s");

            // Bounded wait on the reader side seeing the matched writer.
            long readerDeadline = System.nanoTime() + 5_000_000_000L;
            boolean readerMatched = false;
            while (System.nanoTime() < readerDeadline) {
                readerMatched = !r.getMatchedPublications().isEmpty();
                if (readerMatched) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(readerMatched, "reader should see the matched writer within 5s");

            PublicationMatchedStatus pubStatus = w.getPublicationMatchedStatus();
            assertTrue(pubStatus.totalCount() >= 1, "totalCount should count the matched reader");
            assertTrue(pubStatus.currentCount() >= 1, "currentCount should count the matched reader");
            byte[] lastSubscriptionHandle = pubStatus.lastSubscriptionHandle();
            assertTrue(lastSubscriptionHandle.length == 16, "handle must be 16 bytes");
            assertFalse(isAllZero(lastSubscriptionHandle), "handle should not be all-zero");

            SubscriptionMatchedStatus subStatus = r.getSubscriptionMatchedStatus();
            assertTrue(subStatus.totalCount() >= 1, "totalCount should count the matched writer");
            assertTrue(subStatus.currentCount() >= 1, "currentCount should count the matched writer");
            byte[] lastPublicationHandle = subStatus.lastPublicationHandle();
            assertTrue(lastPublicationHandle.length == 16, "handle must be 16 bytes");
            assertFalse(isAllZero(lastPublicationHandle), "handle should not be all-zero");
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
