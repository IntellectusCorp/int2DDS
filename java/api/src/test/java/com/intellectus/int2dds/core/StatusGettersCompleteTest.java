package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.status.InconsistentTopicStatus;
import com.intellectus.int2dds.status.OfferedIncompatibleTypeStatus;
import com.intellectus.int2dds.status.RequestedIncompatibleTypeStatus;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises the last 3 status getters completing the pull-based status
 * surface: {@code Topic#getInconsistentTopicStatus},
 * {@code DataReader#getRequestedIncompatibleTypeStatus} and
 * {@code DataWriter#getOfferedIncompatibleTypeStatus}. Same two-participant
 * matching pattern as {@code MatchedStatusTest} and {@code
 * RemainingStatusGettersTest}: matching does not loop back within a single
 * participant, so the writer and the reader each live on their own
 * participant.
 *
 * <p>None of these 3 statuses fire under normal healthy pub/sub -- an
 * inconsistent topic or an incompatible type requires a type or topic
 * mismatch this test does not simulate. So this test only proves each
 * getter marshals a real, non-garbage 8-byte {@code
 * {i32 total_count; i32 total_count_change;}} struct off the native side --
 * zero counts, no crash. A wrong struct size or field offset would either
 * crash or surface non-zero noise here, so this is a real (if narrowly
 * scoped) check on the marshaling, not a full event simulation.
 */
class StatusGettersCompleteTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    @Test
    void inconsistentTopicRequestedIncompatibleTypeAndOfferedIncompatibleTypeMarshalDefaults()
            throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic = writerParticipant.createTopic(
                    "StatusGettersCompleteTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic, writerQos);

            Topic<ConformanceRecord> readerTopic = readerParticipant.createTopic(
                    "StatusGettersCompleteTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            // Bounded wait for the match to settle before reading statuses --
            // same pattern as MatchedStatusTest / RemainingStatusGettersTest.
            long matchDeadline = System.nanoTime() + 5_000_000_000L;
            boolean matched = false;
            while (System.nanoTime() < matchDeadline) {
                matched = !r.getMatchedPublications().isEmpty();
                if (matched) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(matched, "reader should see the matched writer within 5s");

            // Topic status getter lives on the Topic object -- reuse the
            // topic created for the writer.
            InconsistentTopicStatus topicStatus = writerTopic.getInconsistentTopicStatus();
            assertNotNull(topicStatus);
            assertEquals(0, topicStatus.totalCount());
            assertEquals(0, topicStatus.totalCountChange());

            RequestedIncompatibleTypeStatus readerStatus = r.getRequestedIncompatibleTypeStatus();
            assertNotNull(readerStatus);
            assertEquals(0, readerStatus.totalCount());
            assertEquals(0, readerStatus.totalCountChange());

            OfferedIncompatibleTypeStatus writerStatus = w.getOfferedIncompatibleTypeStatus();
            assertNotNull(writerStatus);
            assertEquals(0, writerStatus.totalCount());
            assertEquals(0, writerStatus.totalCountChange());
        }
    }
}
