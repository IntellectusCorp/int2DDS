package com.intellectus.int2dds.discovery;

import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
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
}
