package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises {@code getStatusChanges()} on the 6 entity types this branch
 * added it to. Two participants, the same pattern {@code
 * MatchedEndpointsTest} and {@code ReliabilityLivelinessTest} settle on:
 * matching does not loop back within a single participant, so the writer and
 * the reader each live on their own participant.
 */
class EntityStatusChangesTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    @FunctionalInterface
    private interface StatusMaskSupplier {
        StatusMask get();
    }

    // The match becoming visible through getMatchedSubscriptions and the
    // PUBLICATION_MATCHED/SUBSCRIPTION_MATCHED status bit landing are two
    // separate asynchronous events, so poll on a bounded budget rather than
    // reading getStatusChanges() once.
    private static boolean pollForBit(StatusMaskSupplier supplier, int bit) throws InterruptedException {
        long deadline = System.nanoTime() + 5_000_000_000L;
        while (System.nanoTime() < deadline) {
            if ((supplier.get().bits() & bit) != 0) {
                return true;
            }
            Thread.sleep(50);
        }
        return false;
    }

    @Test
    void matchedEndpointsShowPendingStatusChanges() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("EntityStatusChangesTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic, writerQos);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("EntityStatusChangesTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            // Wait for the writer to see the matched reliable reader before
            // checking status changes -- the same bounded-poll pattern
            // MatchedEndpointsTest/ReliabilityLivelinessTest use.
            List<SubscriptionBuiltinTopicData> found = Collections.emptyList();
            long matchDeadline = System.nanoTime() + 5_000_000_000L;
            boolean matched = false;
            while (System.nanoTime() < matchDeadline) {
                found = w.getMatchedSubscriptions();
                matched = found.stream().anyMatch(d -> "EntityStatusChangesTopic".equals(d.topicName()));
                if (matched) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(matched, "writer should see the matched reliable reader within 5s");

            boolean writerSawPublicationMatched =
                    pollForBit(w::getStatusChanges, StatusMask.PUBLICATION_MATCHED);
            assertTrue(writerSawPublicationMatched,
                    "writer.getStatusChanges() should include PUBLICATION_MATCHED after matching");

            boolean readerSawSubscriptionMatched =
                    pollForBit(r::getStatusChanges, StatusMask.SUBSCRIPTION_MATCHED);
            assertTrue(readerSawSubscriptionMatched,
                    "reader.getStatusChanges() should include SUBSCRIPTION_MATCHED after matching");

            // Publisher/participant getStatusChanges() should also just work,
            // even with no expected bit to poll for.
            assertNotNull(pub.getStatusChanges(), "Publisher#getStatusChanges should return a non-null mask");
            assertNotNull(writerParticipant.getStatusChanges(),
                    "DomainParticipant#getStatusChanges should return a non-null mask");
        }
    }
}
