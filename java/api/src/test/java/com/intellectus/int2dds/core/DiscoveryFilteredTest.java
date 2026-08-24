package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.conditions.InstanceState;
import com.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import com.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.Collections;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Covers the instance-state-filtered discovery snapshot overloads: {@code
 * takeDiscoveredPublications(long, int)} / {@code
 * takeDiscoveredSubscriptions(long, int)}. Mirrors the setup idiom in {@code
 * discovery.DiscoveryTest} (two participants, bounded-retry poll since
 * discovery is asynchronous).
 */
class DiscoveryFilteredTest {

    @Test
    void anyMaskSeesDiscoveredPublication() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(DomainParticipantTest.testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(DomainParticipantTest.testDomain())) {
            Topic<ConformanceRecord> topic =
                    writerParticipant.createTopic("DiscoFilteredPubTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            List<PublicationBuiltinTopicData> anyFound = Collections.emptyList();
            List<PublicationBuiltinTopicData> disposedFound = null;
            long deadline = System.nanoTime() + 5_000_000_000L;
            boolean hit = false;
            while (System.nanoTime() < deadline) {
                anyFound = readerParticipant.takeDiscoveredPublications(200, InstanceState.ANY);
                hit = anyFound.stream().anyMatch(d -> "DiscoFilteredPubTopic".equals(d.topicName()));
                if (hit) {
                    disposedFound = readerParticipant.takeDiscoveredPublications(500,
                            InstanceState.NOT_ALIVE_DISPOSED);
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hit, "ANY mask should discover the remote writer's publication within 5s");
            assertFalse(
                    disposedFound.stream().anyMatch(d -> "DiscoFilteredPubTopic".equals(d.topicName())),
                    "an ALIVE publication should not be returned by a NOT_ALIVE_DISPOSED-only mask");
        }
    }

    @Test
    void anyMaskSeesDiscoveredSubscription() throws InterruptedException {
        try (DomainParticipant readerParticipant = new DomainParticipant(DomainParticipantTest.testDomain());
                DomainParticipant discovererParticipant =
                        new DomainParticipant(DomainParticipantTest.testDomain())) {
            Topic<ConformanceRecord> topic =
                    readerParticipant.createTopic("DiscoFilteredSubTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            List<SubscriptionBuiltinTopicData> anyFound = Collections.emptyList();
            List<SubscriptionBuiltinTopicData> disposedFound = null;
            long deadline = System.nanoTime() + 5_000_000_000L;
            boolean hit = false;
            while (System.nanoTime() < deadline) {
                anyFound = discovererParticipant.takeDiscoveredSubscriptions(200, InstanceState.ANY);
                hit = anyFound.stream().anyMatch(d -> "DiscoFilteredSubTopic".equals(d.topicName()));
                if (hit) {
                    disposedFound = discovererParticipant.takeDiscoveredSubscriptions(500,
                            InstanceState.NOT_ALIVE_DISPOSED);
                    break;
                }
                Thread.sleep(50);
            }
            assertTrue(hit, "ANY mask should discover the remote reader's subscription within 5s");
            assertFalse(
                    disposedFound.stream().anyMatch(d -> "DiscoFilteredSubTopic".equals(d.topicName())),
                    "an ALIVE subscription should not be returned by a NOT_ALIVE_DISPOSED-only mask");
        }
    }
}
