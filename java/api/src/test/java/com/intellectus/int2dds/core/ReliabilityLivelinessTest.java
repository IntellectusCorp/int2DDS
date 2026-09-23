package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.exceptions.DdsUnsupportedException;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Exercises the reliability/liveliness operations added in this branch:
 * {@code DataWriter#waitForAcknowledgments}, {@code
 * Publisher#waitForAcknowledgments}, {@code DataWriter#assertLiveliness},
 * {@code DomainParticipant#assertLiveliness} and {@code
 * DataReader#waitForHistoricalData}.
 *
 * <p>Two participants, the same pattern {@code MatchedEndpointsTest} settled
 * on: matching does not loop back within a single participant, so the writer
 * and the reader each live on their own participant.
 */
class ReliabilityLivelinessTest {

    private static int testDomain() {
        return DomainParticipantTest.testDomain();
    }

    @Test
    void writerSeesAcknowledgmentFromAReliableReader() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("ReliabilityAckTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic, writerQos);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("ReliabilityAckTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(readerTopic, ConformanceRecord::new, readerQos);

            // Wait for the writer to see the matched reliable reader before
            // writing -- the same bounded-poll pattern MatchedEndpointsTest
            // uses, since RELIABLE only helps once the match exists.
            List<SubscriptionBuiltinTopicData> found = Collections.emptyList();
            long matchDeadline = System.nanoTime() + 5_000_000_000L;
            boolean matched = false;
            while (System.nanoTime() < matchDeadline) {
                found = w.getMatchedSubscriptions();
                matched = found.stream().anyMatch(d -> "ReliabilityAckTopic".equals(d.topicName()));
                if (matched) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(matched, "writer should see the matched reliable reader within 5s");

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 99;
            sent.value = 1.5;
            sent.label = "ack";
            w.write(sent);

            assertTrue(w.waitForAcknowledgments(2000L),
                    "the matched reliable reader should ack within 2s");

            // Manual liveliness assertions must return without throwing.
            w.assertLiveliness();
            writerParticipant.assertLiveliness();

            // Publisher-level ack wait covers the same writer.
            assertTrue(pub.waitForAcknowledgments(2000L),
                    "publisher-level wait should also observe the ack");

            // waitForHistoricalData: the underlying core operation
            // (DataReader::wait_for_historical_data,
            // dds/src/dcps/subscription/data_reader.rs:3745) is a stub that
            // unconditionally returns DdsError::Unsupported, never checking
            // durability -- confirmed by this call throwing here, not by
            // reading the Rust source alone. There is no meaningful
            // durable-QoS/late-joiner scenario this branch could fake around
            // that, so this only pins the actually observed behavior: the
            // Java wrapper correctly surfaces RET_UNSUPPORTED as a
            // DdsUnsupportedException (RET_TIMEOUT is its only
            // special-cased code, per its own Javadoc). See the task report.
            DdsUnsupportedException ex = assertThrows(DdsUnsupportedException.class,
                    () -> r.waitForHistoricalData(500L));
            assertEquals(DdsException.RET_UNSUPPORTED, ex.getCode());
        }
    }
}
