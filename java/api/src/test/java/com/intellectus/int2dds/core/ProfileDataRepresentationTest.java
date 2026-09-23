package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.io.IOException;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import org.junit.jupiter.api.Test;

/**
 * A writer built from a QoS profile that selects XCDR2 must actually serialize
 * XCDR2, not merely advertise it over discovery.
 *
 * <p>The check is on the bytes the core hands back, taken through {@link
 * DataReader#takeSerialized()}: nothing else in this binding can see the
 * encapsulation the writer chose. A round trip cannot -- Java's own reader
 * follows whatever encapsulation id it is given ({@code CdrReader.of}), so it
 * decodes an XCDR1 payload from an XCDR2-advertised writer without complaint,
 * exactly the peer that would not.
 *
 * <p>The library name is unique to this test: profiles land in the
 * process-wide factory singleton (see {@link ProfileCreateEntitiesTest}).
 */
class ProfileDataRepresentationTest {

    private static final String LIBRARY = "JavaTestLib_ProfileDataRepr";
    private static final String PROFILE = LIBRARY + "::Xcdr2Profile";

    // Reader and writer both request XCDR2: data representation is an
    // offered/requested policy, so a reader left at the XCDR1 default would
    // never match this writer and the test would time out instead of
    // reporting the encoding.
    private static final String PROFILE_JSON = "{\n"
            + "  \"name\": \"" + LIBRARY + "\",\n"
            + "  \"qos_profiles\": [{\n"
            + "    \"name\": \"Xcdr2Profile\",\n"
            + "    \"domain_participant_qos\": {},\n"
            + "    \"publisher_qos\": {},\n"
            + "    \"subscriber_qos\": {},\n"
            + "    \"topic_qos\": {},\n"
            + "    \"datawriter_qos\": {\n"
            + "      \"reliability\": { \"kind\": \"RELIABLE_RELIABILITY_QOS\" },\n"
            + "      \"data_representation\": { \"value\": [\"XCDR2_DATA_REPRESENTATION\"] }\n"
            + "    },\n"
            + "    \"datareader_qos\": {\n"
            + "      \"reliability\": { \"kind\": \"RELIABLE_RELIABILITY_QOS\" },\n"
            + "      \"data_representation\": { \"value\": [\"XCDR2_DATA_REPRESENTATION\"] }\n"
            + "    }\n"
            + "  }]\n"
            + "}\n";

    // Delimited XCDR2, what an APPENDABLE type gets. The writer serializes in
    // host order, so which of the pair to expect follows the host too.
    private static final int ENCAP_D_CDR2_BE = 0x0008;
    private static final int ENCAP_D_CDR2_LE = 0x0009;

    private static final int EXPECTED_ENCAP =
            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN ? ENCAP_D_CDR2_LE : ENCAP_D_CDR2_BE;

    private static Path writeProfileFile() throws IOException {
        Path path = Files.createTempFile("profile-data-representation-", ".json");
        path.toFile().deleteOnExit();
        Files.write(path, PROFILE_JSON.getBytes(StandardCharsets.UTF_8));
        return path;
    }

    private static int encapsulationId(byte[] sample) {
        // Always big-endian, whatever follows it is not.
        return ((sample[0] & 0xFF) << 8) | (sample[1] & 0xFF);
    }

    @Test
    void aWriterFromAnXcdr2ProfileSerializesXcdr2() throws IOException, InterruptedException {
        Path profilePath = writeProfileFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(profilePath.toString()));

        try (DomainParticipant p = new DomainParticipant(testDomain(), PROFILE)) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("profile_data_representation_topic", new ConformanceRecord());
            Publisher pub = p.createPublisher(PROFILE);
            Subscriber sub = p.createSubscriber(PROFILE);

            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic, PROFILE);
            DataReader<ConformanceRecord> r =
                    sub.createDataReader(topic, ConformanceRecord::new, PROFILE);

            // Precondition, not the assertion under test: the profile really
            // did reach the native writer. Without this a failure below cannot
            // be told apart from a profile that never applied.
            assertEquals(DataRepresentationKind.XCDR2, w.getDataRepresentation(),
                    "the profile's data representation did not reach the native writer");

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 4242;
            sent.value = 2.5d;
            sent.label = "xcdr2-profile";

            byte[] raw = null;
            long deadline = System.nanoTime() + 10_000_000_000L;
            while (System.nanoTime() < deadline && (raw == null || raw.length == 0)) {
                w.write(sent);
                raw = r.takeSerialized();
                if (raw == null || raw.length == 0) {
                    Thread.sleep(20);
                }
            }
            assertNotNull(raw, "no sample within 10s -- the XCDR2 profile pair did not match");
            assertTrue(raw.length >= 4, "sample is shorter than an encapsulation header");

            assertEquals(EXPECTED_ENCAP, encapsulationId(raw),
                    "writer advertises XCDR2 but serialized "
                            + Integer.toHexString(encapsulationId(raw)));
        }
    }
}
