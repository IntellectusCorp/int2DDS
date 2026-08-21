package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import org.junit.jupiter.api.Test;

/**
 * Exercises named QoS profiles end to end: {@link
 * DomainParticipantFactory#loadProfiles} loads a JSON profile file into the
 * process-wide factory singleton, then {@link Publisher#createDataWriter(Topic,
 * String)} and {@link Subscriber#createDataReader(Topic, java.util.function.Supplier,
 * String)} build entities whose QoS actually comes from the named profile
 * rather than the core's default -- verified by reading it back with {@code
 * getQos()}, not merely by the absence of a thrown exception.
 *
 * <p>The library name is unique to this test ({@code JavaTestLib_QosProfile})
 * to avoid colliding with any other test's profiles in the same factory
 * singleton.
 */
class QosProfileTest {

    private static final String PROFILE_JSON = "{\n"
            + "  \"name\": \"JavaTestLib_QosProfile\",\n"
            + "  \"qos_profiles\": [{\n"
            + "    \"name\": \"ReliableProfile\",\n"
            + "    \"datawriter_qos\": {\n"
            + "      \"reliability\": { \"kind\": \"RELIABLE_RELIABILITY_QOS\" },\n"
            + "      \"history\": { \"kind\": \"KEEP_LAST_HISTORY_QOS\", \"depth\": 10 }\n"
            + "    },\n"
            + "    \"datareader_qos\": {\n"
            + "      \"reliability\": { \"kind\": \"RELIABLE_RELIABILITY_QOS\" },\n"
            + "      \"history\": { \"kind\": \"KEEP_LAST_HISTORY_QOS\", \"depth\": 10 }\n"
            + "    }\n"
            + "  }]\n"
            + "}\n";

    private static Path writeProfileFile() throws IOException {
        Path path = Files.createTempFile("qos-profile-", ".json");
        path.toFile().deleteOnExit();
        Files.write(path, PROFILE_JSON.getBytes(StandardCharsets.UTF_8));
        return path;
    }

    @Test
    void profileQosIsAppliedToWriterAndReader() throws IOException {
        Path profilePath = writeProfileFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(profilePath.toString()));

        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("qos_profile_topic", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();

            DataWriter<ConformanceRecord> w =
                    pub.createDataWriter(topic, "JavaTestLib_QosProfile::ReliableProfile");
            DataWriterQos wQos = w.getQos();
            assertEquals(ReliabilityKind.RELIABLE, wQos.getReliability().getKind());
            assertEquals(HistoryKind.KEEP_LAST, wQos.getHistory().getKind());
            assertEquals(10, wQos.getHistory().getDepth());

            DataReader<ConformanceRecord> r = sub.createDataReader(
                    topic, ConformanceRecord::new, "JavaTestLib_QosProfile::ReliableProfile");
            DataReaderQos rQos = r.getQos();
            assertEquals(ReliabilityKind.RELIABLE, rQos.getReliability().getKind());
            assertEquals(HistoryKind.KEEP_LAST, rQos.getHistory().getKind());
            assertEquals(10, rQos.getHistory().getDepth());
        }
    }

    @Test
    void unknownProfileThrows() throws IOException {
        Path profilePath = writeProfileFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(profilePath.toString()));

        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("qos_profile_missing_topic", new ConformanceRecord());
            Publisher pub = p.createPublisher();

            assertThrows(DdsException.class,
                    () -> pub.createDataWriter(topic, "JavaTestLib_QosProfile::NoSuchProfile"));
        }
    }
}
