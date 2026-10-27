package kr.co.intellectus.int2dds.core;

import static kr.co.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.charset.StandardCharsets;
import java.util.List;
import kr.co.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import kr.co.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import kr.co.intellectus.int2dds.qos.GroupData;
import kr.co.intellectus.int2dds.qos.PublisherQos;
import kr.co.intellectus.int2dds.qos.SubscriberQos;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/** GROUP_DATA reads back through Publisher and Subscriber {@code getQos()} and reaches peers. */
class GroupDataQosTest {

    @Test
    void groupDataReadsBackAfterCreateAndSetQos() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Publisher pub = p.createPublisher(publisherQos("publisher-created"));
            Subscriber sub = p.createSubscriber(subscriberQos("subscriber-created"));

            assertEquals(groupData("publisher-created"), pub.getQos().getGroupData());
            assertEquals(groupData("subscriber-created"), sub.getQos().getGroupData());

            pub.setQos(publisherQos("publisher-changed"));
            sub.setQos(subscriberQos("subscriber-changed"));

            assertEquals(groupData("publisher-changed"), pub.getQos().getGroupData());
            assertEquals(groupData("subscriber-changed"), sub.getQos().getGroupData());
        }
    }

    @Test
    void groupDataReachesMatchedEndpoints() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            DataWriter<ConformanceRecord> writer =
                    writerParticipant
                            .createPublisher(publisherQos("publisher-group"))
                            .createDataWriter(
                                    writerParticipant.createTopic(
                                            "GroupDataMatched", new ConformanceRecord()));
            DataReader<ConformanceRecord> reader =
                    readerParticipant
                            .createSubscriber(subscriberQos("subscriber-group"))
                            .createDataReader(
                                    readerParticipant.createTopic(
                                            "GroupDataMatched", new ConformanceRecord()),
                                    ConformanceRecord::new);

            List<PublicationBuiltinTopicData> publications = reader.getMatchedPublications();
            List<SubscriptionBuiltinTopicData> subscriptions = writer.getMatchedSubscriptions();
            long deadline = System.nanoTime() + 10_000_000_000L;
            while ((publications.isEmpty() || subscriptions.isEmpty())
                    && System.nanoTime() < deadline) {
                Thread.sleep(20);
                publications = reader.getMatchedPublications();
                subscriptions = writer.getMatchedSubscriptions();
            }

            assertEquals(1, publications.size());
            assertEquals(1, subscriptions.size());
            assertArrayEquals(bytes("publisher-group"), publications.get(0).groupData());
            assertArrayEquals(bytes("subscriber-group"), subscriptions.get(0).groupData());
        }
    }

    private static PublisherQos publisherQos(String text) {
        PublisherQos qos = new PublisherQos();
        qos.setGroupData(groupData(text));
        return qos;
    }

    private static SubscriberQos subscriberQos(String text) {
        SubscriberQos qos = new SubscriberQos();
        qos.setGroupData(groupData(text));
        return qos;
    }

    private static GroupData groupData(String text) {
        return new GroupData(bytes(text));
    }

    private static byte[] bytes(String text) {
        return text.getBytes(StandardCharsets.UTF_8);
    }
}
