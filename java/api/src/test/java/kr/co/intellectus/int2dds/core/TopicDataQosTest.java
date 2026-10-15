package kr.co.intellectus.int2dds.core;

import static kr.co.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.charset.StandardCharsets;
import java.util.List;
import kr.co.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import kr.co.intellectus.int2dds.discovery.SubscriptionBuiltinTopicData;
import kr.co.intellectus.int2dds.qos.TopicData;
import kr.co.intellectus.int2dds.qos.TopicQos;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/** TOPIC_DATA reads back through {@code Topic#getQos()} and reaches matched endpoints. */
class TopicDataQosTest {

    @Test
    void topicDataReadsBackAfterCreateAndSetQos() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> t =
                    p.createTopic("topic_data_qos", new ConformanceRecord(), topicQos("created"));

            assertEquals(topicData("created"), t.getQos().getTopicData());

            t.setQos(topicQos("changed"));

            assertEquals(topicData("changed"), t.getQos().getTopicData());
        }
    }

    @Test
    void topicDataReachesMatchedEndpoints() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            DataWriter<ConformanceRecord> writer =
                    writerParticipant
                            .createPublisher()
                            .createDataWriter(
                                    writerParticipant.createTopic(
                                            "TopicDataMatched",
                                            new ConformanceRecord(),
                                            topicQos("writer-topic")));
            DataReader<ConformanceRecord> reader =
                    readerParticipant
                            .createSubscriber()
                            .createDataReader(
                                    readerParticipant.createTopic(
                                            "TopicDataMatched",
                                            new ConformanceRecord(),
                                            topicQos("reader-topic")),
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
            assertArrayEquals(bytes("writer-topic"), publications.get(0).topicData());
            assertArrayEquals(bytes("reader-topic"), subscriptions.get(0).topicData());
        }
    }

    private static TopicQos topicQos(String text) {
        TopicQos qos = new TopicQos();
        qos.setTopicData(topicData(text));
        return qos;
    }

    private static TopicData topicData(String text) {
        return new TopicData(bytes(text));
    }

    private static byte[] bytes(String text) {
        return text.getBytes(StandardCharsets.UTF_8);
    }
}
