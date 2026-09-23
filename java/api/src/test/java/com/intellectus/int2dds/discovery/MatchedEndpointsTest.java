package com.intellectus.int2dds.discovery;

import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * End-to-end proof that {@code DataWriter#getMatchedSubscriptions} and
 * {@code DataReader#getMatchedPublications} actually see a real remote
 * endpoint through SPDP/SEDP, not just an empty list.
 *
 * <p>Uses two participants in the same domain, the same pattern {@link
 * DiscoveryTest} settled on: matching does not appear to loop back within a
 * single participant, so the writer and the reader each live on their own
 * participant. Discovery is asynchronous, so both assertions poll on a
 * bounded budget.
 */
class MatchedEndpointsTest {

    // Mirrors DiscoveryTest.testDomain(), package-private to core and not
    // otherwise reachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    /**
     * Proof that {@code getMatchedSubscriptions} works with a live matching
     * reader -- i.e. capacity &gt;= 1 on the underlying {@code
     * int2dds_datawriter_get_matched_subscriptions} call, exercising the same
     * JNI codegen fix {@code getDiscoveredParticipants} exercises for the
     * participant-discovery path.
     */
    @Test
    void writerSeesMatchedSubscription() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("MatchedEndpointsTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(writerTopic);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("MatchedEndpointsTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            sub.createDataReader(readerTopic, ConformanceRecord::new);

            List<SubscriptionBuiltinTopicData> found = Collections.emptyList();
            long deadline = System.nanoTime() + 5_000_000_000L;
            boolean hit = false;
            while (System.nanoTime() < deadline) {
                found = w.getMatchedSubscriptions();
                hit = found.stream().anyMatch(d -> "MatchedEndpointsTopic".equals(d.topicName()));
                if (hit) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hit, "writer should see the matched remote subscription within 5s");
        }
    }

    /**
     * Symmetric mirror of {@link #writerSeesMatchedSubscription()} for the
     * reader side: proof that {@code getMatchedPublications} works with a
     * live matching writer, exercising capacity &gt;= 1 on {@code
     * int2dds_datareader_get_matched_publications}.
     */
    @Test
    void readerSeesMatchedPublication() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("MatchedEndpointsTopic2", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            pub.createDataWriter(writerTopic);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("MatchedEndpointsTopic2", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(readerTopic, ConformanceRecord::new);

            List<PublicationBuiltinTopicData> found = Collections.emptyList();
            long deadline = System.nanoTime() + 5_000_000_000L;
            boolean hit = false;
            while (System.nanoTime() < deadline) {
                found = r.getMatchedPublications();
                hit = found.stream().anyMatch(d -> "MatchedEndpointsTopic2".equals(d.topicName()));
                if (hit) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hit, "reader should see the matched remote publication within 5s");
        }
    }
}
