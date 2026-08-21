package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.status.LivelinessChangedStatus;
import com.intellectus.int2dds.status.LivelinessLostStatus;
import com.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import com.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import com.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import com.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import com.intellectus.int2dds.status.SampleLostStatus;
import com.intellectus.int2dds.status.SampleRejectedStatus;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises the 8 remaining pull-based status getters added in this branch:
 * {@code DataReader#getLivelinessChangedStatus/getRequestedDeadlineMissedStatus/
 * getRequestedIncompatibleQosStatus/getSampleLostStatus/getSampleRejectedStatus}
 * and {@code DataWriter#getLivelinessLostStatus/getOfferedDeadlineMissedStatus/
 * getOfferedIncompatibleQosStatus}. Same two-participant matching pattern as
 * {@code MatchedStatusTest}: matching does not loop back within a single
 * participant, so the writer and the reader each live on their own
 * participant.
 *
 * <p>Only {@code getLivelinessChangedStatus} fires under normal healthy
 * pub/sub -- a matched writer becoming alive is exactly what a successful
 * match produces. The other 7 statuses (missed deadlines, incompatible QoS,
 * lost/rejected samples, lost liveliness) require fault conditions this test
 * does not simulate, so for those this test only proves the getter marshals
 * a real, non-garbage struct off the native side: default/zero counts and,
 * where the type carries one, a 16-byte handle, without throwing. A wrong
 * struct size or field offset would either crash or surface non-zero noise
 * here, so this is a real (if narrowly scoped) check on the marshaling, not
 * a full event simulation.
 */
class RemainingStatusGettersTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    @Test
    void livelinessChangedStatusReportsAliveWriterAfterMatch() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic = writerParticipant.createTopic(
                    "RemainingStatusGettersTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic, writerQos);

            Topic<ConformanceRecord> readerTopic = readerParticipant.createTopic(
                    "RemainingStatusGettersTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            // Bounded wait for the match itself, then a bounded poll on the
            // status getter -- the writer's alive count is applied
            // asynchronously relative to the match becoming visible via
            // getMatchedPublications().
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

            long statusDeadline = System.nanoTime() + 5_000_000_000L;
            LivelinessChangedStatus status = r.getLivelinessChangedStatus();
            while (status.aliveCount() < 1 && System.nanoTime() < statusDeadline) {
                Thread.sleep(50);
                status = r.getLivelinessChangedStatus();
            }
            assertTrue(status.aliveCount() >= 1,
                    "aliveCount should count the matched, alive writer");
            assertTrue(status.notAliveCount() >= 0, "notAliveCount must be non-negative");
        }
    }

    @Test
    void requestedDeadlineMissedStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("RequestedDeadlineMissedTopic", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new);

            RequestedDeadlineMissedStatus status = r.getRequestedDeadlineMissedStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
            assertEquals(16, status.lastInstanceHandle().length, "handle must be 16 bytes");
        }
    }

    @Test
    void requestedIncompatibleQosStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("RequestedIncompatibleQosTopic", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new);

            RequestedIncompatibleQosStatus status = r.getRequestedIncompatibleQosStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
            assertEquals(0, status.policiesCount());
            assertTrue(status.lastPolicyId() >= 0 && status.lastPolicyId() <= 25,
                    "lastPolicyId must be a valid Int2DdsQosPolicyId value");
        }
    }

    @Test
    void sampleLostStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SampleLostTopic", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new);

            SampleLostStatus status = r.getSampleLostStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
        }
    }

    @Test
    void sampleRejectedStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("SampleRejectedTopic", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new);

            SampleRejectedStatus status = r.getSampleRejectedStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
            assertEquals(SampleRejectedStatus.REASON_NOT_REJECTED, status.lastReason());
            assertEquals(16, status.lastInstanceHandle().length, "handle must be 16 bytes");
        }
    }

    @Test
    void livelinessLostStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("LivelinessLostTopic", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            LivelinessLostStatus status = w.getLivelinessLostStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
        }
    }

    @Test
    void offeredDeadlineMissedStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("OfferedDeadlineMissedTopic", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            OfferedDeadlineMissedStatus status = w.getOfferedDeadlineMissedStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
            assertEquals(16, status.lastInstanceHandle().length, "handle must be 16 bytes");
        }
    }

    @Test
    void offeredIncompatibleQosStatusMarshalsDefaultWhenUntriggered() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("OfferedIncompatibleQosTopic", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            OfferedIncompatibleQosStatus status = w.getOfferedIncompatibleQosStatus();
            assertNotNull(status);
            assertEquals(0, status.totalCount());
            assertEquals(0, status.totalCountChange());
            assertEquals(0, status.policiesCount());
            assertTrue(status.lastPolicyId() >= 0 && status.lastPolicyId() <= 25,
                    "lastPolicyId must be a valid Int2DdsQosPolicyId value");
        }
    }
}
