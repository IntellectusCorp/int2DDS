package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.exceptions.DdsTimeoutException;
import com.intellectus.int2dds.xtypes.DynamicData;
import com.intellectus.int2dds.xtypes.DynamicDataWriter;
import com.intellectus.int2dds.xtypes.DynamicTopic;
import com.intellectus.int2dds.xtypes.DynamicTypeSupport;
import com.intellectus.int2dds.xtypes.TypeObject;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import org.junit.jupiter.api.Test;

/**
 * Exercises the two XTypes type-discovery operations added on top of the
 * dynamic write path: {@link DomainParticipantFactory#getDynamicTypeSupport}
 * (a factory-singleton lookup for a type loaded via {@link
 * DomainParticipantFactory#loadProfiles}) and {@link
 * DomainParticipant#waitForTypeObject} (blocks for a remote type's
 * TypeObject to be discovered).
 *
 * <p>Type/library names are unique to this test ({@code XTD} prefix) to
 * avoid colliding with any other test's profiles or types in the same
 * process-wide factory singleton, the same convention {@code
 * ConfiguredParticipantTest} uses.
 */
class XTypesDiscoveryTest {

    private static final String TYPES_XML = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
            + "<dds>\n"
            + "  <types>\n"
            + "    <struct name=\"XTDSimpleData\" extensibility=\"final\">\n"
            + "      <member name=\"id\" type=\"int32\" key=\"true\"/>\n"
            + "      <member name=\"value\" type=\"int32\"/>\n"
            + "    </struct>\n"
            + "  </types>\n"
            + "</dds>\n";

    private static Path writeTypesFile() throws IOException {
        Path path = Files.createTempFile("xtypes-discovery-", ".xml");
        path.toFile().deleteOnExit();
        Files.write(path, TYPES_XML.getBytes(StandardCharsets.UTF_8));
        return path;
    }

    @Test
    void getDynamicTypeSupportBuildsUsableSupportAndThrowsOnUnknownName() throws IOException {
        Path path = writeTypesFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(path.toString()));

        DynamicTypeSupport support =
                DomainParticipantFactory.getInstance().getDynamicTypeSupport("XTDSimpleData");
        assertNotNull(support);
        try {
            // Usable: build a writable DynamicData from it and round-trip a field.
            try (DynamicData data = DynamicData.create(support)) {
                data.setI32("id", 1);
                data.setI32("value", 99);
                assertEquals(1, data.getI32("id"));
                assertEquals(99, data.getI32("value"));
            }
        } finally {
            support.close();
        }

        // Negative: an unloaded type name throws rather than returning null.
        assertThrows(DdsException.class,
                () -> DomainParticipantFactory.getInstance().getDynamicTypeSupport("XTDNoSuchType"));
    }

    @Test
    void waitForTypeObjectTimesOutOnUnknownTopic() {
        try (DomainParticipant participant = new DomainParticipant(testDomain())) {
            // A timeout must surface as DdsTimeoutException, the same type
            // findTopic's timeout throws -- not a bare DdsException.
            assertThrows(DdsTimeoutException.class,
                    () -> participant.waitForTypeObject("XTDNoSuchTopic", 200));
        }
    }

    @Test
    void waitForTypeObjectDiscoversRemoteType() throws IOException {
        Path path = writeTypesFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(path.toString()));

        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            DynamicTypeSupport support =
                    DomainParticipantFactory.getInstance().getDynamicTypeSupport("XTDSimpleData");
            DynamicTopic topic = null;
            Publisher publisher = null;
            DynamicDataWriter writer = null;
            try {
                topic = writerParticipant.createDynamicTopic("XTDWaitTopic", support);
                publisher = writerParticipant.createPublisher();
                writer = publisher.createDynamicDataWriter(topic, support);

                // Cross-participant discovery: readerParticipant has never
                // built this topic itself, so a returned TypeObject can only
                // have come from the writer side over the wire. Bounded
                // timeout -- this must not hang.
                TypeObject discovered = readerParticipant.waitForTypeObject("XTDWaitTopic", 5000);
                assertNotNull(discovered);
                assertEquals(2, discovered.memberCount());
                assertEquals(0, discovered.findMember("id"));
                discovered.close();
            } finally {
                if (writer != null) {
                    writer.close();
                }
                if (topic != null) {
                    topic.close();
                }
                if (publisher != null) {
                    publisher.close();
                }
                support.close();
            }
        }
    }
}
