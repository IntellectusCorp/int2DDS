package kr.co.intellectus.int2dds.core;

import static kr.co.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.time.Duration;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import kr.co.intellectus.int2dds.discovery.PublicationBuiltinTopicData;
import kr.co.intellectus.int2dds.exceptions.DdsPreconditionNotMetException;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.DataWriterQos;
import kr.co.intellectus.int2dds.qos.History;
import kr.co.intellectus.int2dds.qos.HistoryKind;
import kr.co.intellectus.int2dds.qos.Presentation;
import kr.co.intellectus.int2dds.qos.PresentationAccessScopeKind;
import kr.co.intellectus.int2dds.qos.PublisherQos;
import kr.co.intellectus.int2dds.qos.Reliability;
import kr.co.intellectus.int2dds.qos.ReliabilityKind;
import kr.co.intellectus.int2dds.qos.SubscriberQos;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/** PRESENTATION QoS, coherent changes, access blocks and ordered {@code getDataReaders}. */
class PresentationTest {

    private static final Presentation GROUP_COHERENT_ORDERED =
            new Presentation(PresentationAccessScopeKind.GROUP, true, true);

    @Test
    void publisherAndSubscriberQosReadBackPresentation() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            PublisherQos publisherQos = new PublisherQos();
            publisherQos.setPresentation(GROUP_COHERENT_ORDERED);
            Publisher pub = p.createPublisher(publisherQos);

            Presentation topicCoherent =
                    new Presentation(PresentationAccessScopeKind.TOPIC, true, false);
            SubscriberQos subscriberQos = new SubscriberQos();
            subscriberQos.setPresentation(topicCoherent);
            Subscriber sub = p.createSubscriber(subscriberQos);

            assertEquals(GROUP_COHERENT_ORDERED, pub.getQos().getPresentation());
            assertEquals(topicCoherent, sub.getQos().getPresentation());
        }
    }

    @Test
    void groupCoherentSetIsListedInWriteOrder() throws InterruptedException {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            PublisherQos publisherQos = new PublisherQos();
            publisherQos.setPresentation(GROUP_COHERENT_ORDERED);
            Publisher pub = writerParticipant.createPublisher(publisherQos);

            SubscriberQos subscriberQos = new SubscriberQos();
            subscriberQos.setPresentation(GROUP_COHERENT_ORDERED);
            Subscriber sub = readerParticipant.createSubscriber(subscriberQos);

            // KEEP_ALL: ConformanceRecord has no key, so KEEP_LAST 1 would let writerA's second
            // sample replace its first and leave the coherent set incomplete.
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(
                    new Reliability(ReliabilityKind.RELIABLE, Duration.ofMillis(100)));
            writerQos.setHistory(new History(HistoryKind.KEEP_ALL, 0));
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(
                    new Reliability(ReliabilityKind.RELIABLE, Duration.ofMillis(100)));
            readerQos.setHistory(new History(HistoryKind.KEEP_ALL, 0));

            DataWriter<ConformanceRecord> writerA =
                    pub.createDataWriter(
                            writerParticipant.createTopic("PresentationA", new ConformanceRecord()),
                            writerQos);
            DataWriter<ConformanceRecord> writerB =
                    pub.createDataWriter(
                            writerParticipant.createTopic("PresentationB", new ConformanceRecord()),
                            writerQos);
            DataReader<ConformanceRecord> readerA =
                    sub.createDataReader(
                            readerParticipant.createTopic("PresentationA", new ConformanceRecord()),
                            ConformanceRecord::new,
                            readerQos);
            DataReader<ConformanceRecord> readerB =
                    sub.createDataReader(
                            readerParticipant.createTopic("PresentationB", new ConformanceRecord()),
                            ConformanceRecord::new,
                            readerQos);

            waitForMatchedSubscription(writerA);
            waitForMatchedSubscription(writerB);

            // Unbalanced end calls and GROUP access outside an access block are rejected.
            assertThrows(DdsPreconditionNotMetException.class, pub::endCoherentChanges);
            assertThrows(DdsPreconditionNotMetException.class, sub::endAccess);
            assertThrows(DdsPreconditionNotMetException.class, () -> sub.getDataReaders());

            pub.beginCoherentChanges();
            writerA.write(record(1));
            writerB.write(record(2));
            writerA.write(record(3));
            pub.endCoherentChanges();

            List<DataReader<?>> readers = Collections.emptyList();
            long deadline = System.nanoTime() + 10_000_000_000L;
            while (System.nanoTime() < deadline) {
                sub.beginAccess();
                readers = sub.getDataReaders();
                sub.endAccess();

                if (readers.size() == 3) {
                    break;
                }
                Thread.sleep(20);
            }

            assertEquals(3, readers.size(), "coherent set not delivered: " + readers);
            assertSame(readerA, readers.get(0));
            assertSame(readerB, readers.get(1));
            assertSame(readerA, readers.get(2));

            // GROUP access scope rejects a take outside an access block.
            assertThrows(DdsPreconditionNotMetException.class, readerA::take);

            // Taking one sample per listed reader, in list order, yields the write order.
            List<Integer> ids = new ArrayList<Integer>();
            sub.beginAccess();
            try {
                for (DataReader<?> reader : sub.getDataReaders()) {
                    Sample<?> sample = reader.take();
                    assertNotNull(sample);
                    ids.add(((ConformanceRecord) sample.data()).id);
                }
            } finally {
                sub.endAccess();
            }

            assertEquals(Arrays.asList(1, 2, 3), ids);

            // The matched publication carries the publisher's presentation QoS.
            List<PublicationBuiltinTopicData> publications = readerA.getMatchedPublications();
            assertEquals(1, publications.size());
            assertEquals(
                    PresentationAccessScopeKind.GROUP.value(),
                    publications.get(0).presentationAccessScope());
            assertTrue(publications.get(0).presentationCoherentAccess());
            assertTrue(publications.get(0).presentationOrderedAccess());
        }
    }

    private static ConformanceRecord record(int id) {
        ConformanceRecord sample = new ConformanceRecord();
        sample.id = id;
        return sample;
    }

    private static void waitForMatchedSubscription(DataWriter<ConformanceRecord> writer)
            throws InterruptedException {
        long deadline = System.nanoTime() + 10_000_000_000L;
        while (System.nanoTime() < deadline) {
            if (!writer.getMatchedSubscriptions().isEmpty()) {
                return;
            }
            Thread.sleep(20);
        }
        throw new AssertionError("writer not matched within 10s");
    }
}
