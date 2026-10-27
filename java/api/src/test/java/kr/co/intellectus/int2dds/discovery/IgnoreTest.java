package kr.co.intellectus.int2dds.discovery;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.function.BooleanSupplier;
import kr.co.intellectus.int2dds.core.DataReader;
import kr.co.intellectus.int2dds.core.DataWriter;
import kr.co.intellectus.int2dds.core.DomainParticipant;
import kr.co.intellectus.int2dds.core.Topic;
import kr.co.intellectus.int2dds.exceptions.DdsInvalidArgumentException;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.DataWriterQos;
import kr.co.intellectus.int2dds.qos.ParticipantQos;
import kr.co.intellectus.int2dds.qos.UserData;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * {@code ignoreParticipant}, {@code ignorePublication} and {@code ignoreSubscription}: each rejects
 * a local handle, unmatches the remote entity, keeps it unmatched after it re-announces itself
 * through {@code setQos}, and no data flows to or from it.
 */
class IgnoreTest {

    private static final long MATCH_TIMEOUT_NANOS = 5_000_000_000L;
    private static final long NO_DATA_WINDOW_MILLIS = 1_000L;

    // Mirrors DiscoveryTest.testDomain(), package-private to core and not
    // otherwise reachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void ignoreParticipantUnmatchesAndDropsItsData() throws InterruptedException {
        try (DomainParticipant localParticipant = new DomainParticipant(testDomain());
                DomainParticipant remoteParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> localTopic =
                    localParticipant.createTopic("IgnoreParticipantJava", new ConformanceRecord());
            Topic<ConformanceRecord> remoteTopic =
                    remoteParticipant.createTopic("IgnoreParticipantJava", new ConformanceRecord());
            DataReader<ConformanceRecord> reader =
                    localParticipant
                            .createSubscriber()
                            .createDataReader(localTopic, ConformanceRecord::new);
            DataWriter<ConformanceRecord> remoteWriter =
                    remoteParticipant.createPublisher().createDataWriter(remoteTopic);

            assertThrows(
                    DdsInvalidArgumentException.class,
                    () -> localParticipant.ignoreParticipant(localParticipant.getInstanceHandle()));

            waitUntil(
                    () -> reader.getSubscriptionMatchedStatus().currentCount() == 1,
                    "reader did not match");

            localParticipant.ignoreParticipant(remoteParticipant.getInstanceHandle());
            waitUntil(
                    () -> reader.getSubscriptionMatchedStatus().currentCount() == 0,
                    "reader still matched after ignore");

            ParticipantQos participantQos = remoteParticipant.getQos();
            participantQos.setUserData(new UserData(new byte[] {1}));
            remoteParticipant.setQos(participantQos);
            remoteWriter.write(new ConformanceRecord());

            Thread.sleep(NO_DATA_WINDOW_MILLIS);
            assertNull(reader.take());
            assertEquals(0, reader.getSubscriptionMatchedStatus().currentCount());
        }
    }

    @Test
    void ignorePublicationUnmatchesAndDropsItsData() throws InterruptedException {
        try (DomainParticipant localParticipant = new DomainParticipant(testDomain());
                DomainParticipant remoteParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> localTopic =
                    localParticipant.createTopic("IgnorePublicationJava", new ConformanceRecord());
            Topic<ConformanceRecord> localOnlyTopic =
                    localParticipant.createTopic(
                            "IgnorePublicationLocalJava", new ConformanceRecord());
            Topic<ConformanceRecord> remoteTopic =
                    remoteParticipant.createTopic("IgnorePublicationJava", new ConformanceRecord());
            DataReader<ConformanceRecord> reader =
                    localParticipant
                            .createSubscriber()
                            .createDataReader(localTopic, ConformanceRecord::new);
            DataWriter<ConformanceRecord> localWriter =
                    localParticipant.createPublisher().createDataWriter(localOnlyTopic);
            DataWriter<ConformanceRecord> remoteWriter =
                    remoteParticipant.createPublisher().createDataWriter(remoteTopic);

            assertThrows(
                    DdsInvalidArgumentException.class,
                    () -> localParticipant.ignorePublication(localWriter.getInstanceHandle()));

            waitUntil(
                    () -> reader.getSubscriptionMatchedStatus().currentCount() == 1,
                    "reader did not match");

            localParticipant.ignorePublication(remoteWriter.getInstanceHandle());
            waitUntil(
                    () -> reader.getSubscriptionMatchedStatus().currentCount() == 0,
                    "reader still matched after ignore");

            DataWriterQos writerQos = remoteWriter.getQos();
            writerQos.setUserData(new UserData(new byte[] {1}));
            remoteWriter.setQos(writerQos);
            remoteWriter.write(new ConformanceRecord());

            Thread.sleep(NO_DATA_WINDOW_MILLIS);
            assertNull(reader.take());
            assertEquals(0, reader.getSubscriptionMatchedStatus().currentCount());
        }
    }

    @Test
    void ignoreSubscriptionUnmatchesAndSendsItNoData() throws InterruptedException {
        try (DomainParticipant localParticipant = new DomainParticipant(testDomain());
                DomainParticipant remoteParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> localTopic =
                    localParticipant.createTopic("IgnoreSubscriptionJava", new ConformanceRecord());
            Topic<ConformanceRecord> localOnlyTopic =
                    localParticipant.createTopic(
                            "IgnoreSubscriptionLocalJava", new ConformanceRecord());
            Topic<ConformanceRecord> remoteTopic =
                    remoteParticipant.createTopic(
                            "IgnoreSubscriptionJava", new ConformanceRecord());
            DataWriter<ConformanceRecord> writer =
                    localParticipant.createPublisher().createDataWriter(localTopic);
            DataReader<ConformanceRecord> localReader =
                    localParticipant
                            .createSubscriber()
                            .createDataReader(localOnlyTopic, ConformanceRecord::new);
            DataReader<ConformanceRecord> remoteReader =
                    remoteParticipant
                            .createSubscriber()
                            .createDataReader(remoteTopic, ConformanceRecord::new);

            assertThrows(
                    DdsInvalidArgumentException.class,
                    () -> localParticipant.ignoreSubscription(localReader.getInstanceHandle()));

            waitUntil(
                    () -> writer.getPublicationMatchedStatus().currentCount() == 1,
                    "writer did not match");

            localParticipant.ignoreSubscription(remoteReader.getInstanceHandle());
            waitUntil(
                    () -> writer.getPublicationMatchedStatus().currentCount() == 0,
                    "writer still matched after ignore");

            DataReaderQos readerQos = remoteReader.getQos();
            readerQos.setUserData(new UserData(new byte[] {1}));
            remoteReader.setQos(readerQos);
            writer.write(new ConformanceRecord());

            Thread.sleep(NO_DATA_WINDOW_MILLIS);
            assertNull(remoteReader.take());
            assertEquals(0, writer.getPublicationMatchedStatus().currentCount());
        }
    }

    private static void waitUntil(BooleanSupplier condition, String failureMessage)
            throws InterruptedException {
        long deadline = System.nanoTime() + MATCH_TIMEOUT_NANOS;
        while (!condition.getAsBoolean()) {
            assertTrue(System.nanoTime() < deadline, failureMessage);
            Thread.sleep(10);
        }
    }
}
