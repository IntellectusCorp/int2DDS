package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.exceptions.DdsException;
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
 * Exercises the whole QoS-profile creation family end to end: {@link
 * DomainParticipantFactory#loadProfiles} loads a JSON profile file into the
 * process-wide factory singleton, then a participant, topic, publisher,
 * subscriber, datawriter and datareader are all built from the same named
 * profile -- {@code "JavaTestLib_ProfileEntities::TreeProfile"} -- and the
 * resulting tree is proven to actually communicate, not merely to have been
 * constructed without an exception.
 *
 * <p>The library name is unique to this test to avoid colliding with any
 * other test's profiles in the same factory singleton (see {@link
 * QosProfileTest} for the sibling test that covers the writer/reader-only
 * profile paths this one builds on).
 */
class ProfileCreateEntitiesTest {

    private static final String LIBRARY = "JavaTestLib_ProfileEntities";
    private static final String PROFILE = LIBRARY + "::TreeProfile";

    private static final String PROFILE_JSON = "{\n"
            + "  \"name\": \"" + LIBRARY + "\",\n"
            + "  \"qos_profiles\": [{\n"
            + "    \"name\": \"TreeProfile\",\n"
            + "    \"domain_participant_qos\": {},\n"
            + "    \"publisher_qos\": {},\n"
            + "    \"subscriber_qos\": {},\n"
            + "    \"topic_qos\": { \"reliability\": { \"kind\": \"RELIABLE_RELIABILITY_QOS\" } },\n"
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

    // domain_participant_qos/publisher_qos/subscriber_qos are present here as
    // empty objects only because the core's profile resolver requires the
    // field to exist at all to find the profile
    // (get_participant_qos_from_profile/get_publisher_qos_from_profile/
    // get_subscriber_qos_from_profile each fail with "QoS profile not found"
    // when their own section is absent, even though every policy inside it
    // is left at its default) -- confirmed by reading QosProvider's
    // impl_get_qos! macro (dds/src/config/json/provider.rs), which returns
    // None whenever `profile.$field.as_ref()` is None.

    private static Path writeProfileFile() throws IOException {
        Path path = Files.createTempFile("profile-create-entities-", ".json");
        path.toFile().deleteOnExit();
        Files.write(path, PROFILE_JSON.getBytes(StandardCharsets.UTF_8));
        return path;
    }

    /** Takes one sample if available, polling until {@code deadlineNanos}. */
    private static Sample<ConformanceRecord> takeWithin(
            DataReader<ConformanceRecord> reader, DataWriter<ConformanceRecord> writer,
            ConformanceRecord sample, long deadlineNanos) throws InterruptedException {
        Sample<ConformanceRecord> got = null;
        while (System.nanoTime() < deadlineNanos && got == null) {
            writer.write(sample);
            got = reader.take();
            if (got == null) {
                Thread.sleep(20);
            }
        }
        return got;
    }

    @Test
    void wholeTreeIsCreatedFromAProfileAndCommunicates() throws IOException, InterruptedException {
        Path profilePath = writeProfileFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(profilePath.toString()));

        try (DomainParticipant p = new DomainParticipant(testDomain(), PROFILE)) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("profile_create_entities_topic", new ConformanceRecord(), PROFILE);
            Publisher pub = p.createPublisher(PROFILE);
            Subscriber sub = p.createSubscriber(PROFILE);

            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, PROFILE);
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new, PROFILE);

            // Writer QoS is readable off the native side; participant/pub/sub/
            // topic QoS have no getters in this core, so the profile's effect
            // on them is proven below by the end-to-end communication instead.
            DataWriterQos wQos = w.getQos();
            assertEquals(ReliabilityKind.RELIABLE, wQos.getReliability().getKind());
            assertEquals(HistoryKind.KEEP_LAST, wQos.getHistory().getKind());
            assertEquals(10, wQos.getHistory().getDepth());

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 9001;
            sent.value = 1.5d;
            sent.label = "profile-tree";

            Sample<ConformanceRecord> got =
                    takeWithin(r, w, sent, System.nanoTime() + 10_000_000_000L);
            assertNotNull(got, "no sample within 10s -- the profile-created tree did not communicate");
            assertTrue(got.info().validData(), "sample should carry valid data");
            assertEquals(sent.id, got.data().id);
            assertEquals(sent.value, got.data().value);
            assertEquals(sent.label, got.data().label);
        }
    }

    @Test
    void unknownProfileRejectsParticipantCreation() throws IOException {
        Path profilePath = writeProfileFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(profilePath.toString()));

        assertThrows(DdsException.class,
                () -> new DomainParticipant(testDomain(), LIBRARY + "::NoSuchProfile"));
    }

    @Test
    void unknownProfileRejectsPublisherCreation() throws IOException {
        Path profilePath = writeProfileFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(profilePath.toString()));

        try (DomainParticipant p = new DomainParticipant(testDomain(), PROFILE)) {
            assertThrows(DdsException.class,
                    () -> p.createPublisher(LIBRARY + "::NoSuchProfile"));
        }
    }
}
