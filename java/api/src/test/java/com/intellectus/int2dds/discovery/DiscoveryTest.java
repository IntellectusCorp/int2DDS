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

class DiscoveryTest {

    // Mirrors DomainParticipantTest.testDomain(), package-private to core and
    // not otherwise reachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    /**
     * End-to-end proof that {@code takeDiscoveredPublications} actually sees a
     * real writer through SPDP/SEDP, not just an empty list. Discovery is
     * asynchronous, so this polls on a bounded budget rather than asserting
     * immediately after the writer is created.
     *
     * <p>Uses two participants in the same domain: a first attempt reading
     * back on the writer's own participant never observed its own publication
     * within the bounded wait (this core's builtin publication reader does
     * not appear to loop local announcements back to the announcing
     * participant itself), so the writer lives on one participant and the
     * discovering side is a second, separate participant.
     */
    @Test
    void takeDiscoveredPublicationsSeesAWriter() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    writerParticipant.createTopic("DiscoTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            List<PublicationBuiltinTopicData> found = Collections.emptyList();
            long deadline = System.nanoTime() + 5_000_000_000L;
            boolean hit = false;
            while (System.nanoTime() < deadline) {
                found = readerParticipant.takeDiscoveredPublications(200);
                hit = found.stream().anyMatch(d -> "DiscoTopic".equals(d.topicName()));
                if (hit) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hit, "should discover the remote writer's publication within 5s");
        }
    }

    /**
     * Mirrors {@link #takeDiscoveredPublicationsSeesAWriter()} for the
     * subscription side: a reader lives on one participant, and a second,
     * separate participant in the same domain polls
     * {@code takeDiscoveredSubscriptions} for its {@code
     * SubscriptionBuiltinTopicData}.
     */
    @Test
    void takeDiscoveredSubscriptionsSeesAReader() throws InterruptedException {
        try (DomainParticipant readerParticipant = new DomainParticipant(testDomain());
                DomainParticipant discovererParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    readerParticipant.createTopic("DiscoSubTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            List<SubscriptionBuiltinTopicData> found = Collections.emptyList();
            long deadline = System.nanoTime() + 5_000_000_000L;
            boolean hit = false;
            while (System.nanoTime() < deadline) {
                found = discovererParticipant.takeDiscoveredSubscriptions(200);
                hit = found.stream().anyMatch(d -> "DiscoSubTopic".equals(d.topicName()));
                if (hit) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hit, "should discover the remote reader's subscription within 5s");
        }
    }

    /**
     * Proof that {@code getDiscoveredParticipants} works with two live
     * participants -- i.e. capacity &gt;= 2 on the underlying {@code
     * int2dds_participant_get_discovered_participants} call. Before the JNI
     * codegen fix, the generated shim allocated a fixed 16-byte stack buffer
     * regardless of the requested capacity, so a call that actually needed to
     * copy back more than one 16-byte handle would have overflowed that stack
     * buffer. Two participants in the same domain is the minimal case that
     * exercises capacity &gt;= 2 without crashing.
     */
    @Test
    void getDiscoveredParticipantsSeesAnotherParticipant() throws InterruptedException {
        try (DomainParticipant a = new DomainParticipant(testDomain());
                DomainParticipant b = new DomainParticipant(testDomain())) {
            List<ParticipantBuiltinTopicData> found = Collections.emptyList();
            long deadline = System.nanoTime() + 5_000_000_000L;
            while (System.nanoTime() < deadline) {
                found = a.getDiscoveredParticipants();
                if (!found.isEmpty()) {
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(!found.isEmpty(), "should discover at least one remote participant within 5s");
        }
    }
}
