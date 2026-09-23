package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.xtypes.DynamicData;
import com.intellectus.int2dds.xtypes.DynamicDataReader;
import com.intellectus.int2dds.xtypes.DynamicDataWriter;
import com.intellectus.int2dds.xtypes.DynamicTypeSupport;
import com.intellectus.int2dds.xtypes.XmlTypeRegistry;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Collections;
import org.junit.jupiter.api.Test;

/**
 * Exercises the declarative "participant tree from XML config" path end to
 * end: {@link DomainParticipantFactory#loadProfiles} loads a {@code
 * domain_library} + {@code domain_participant_library} pair from an XML
 * file, then {@link DomainParticipantFactory#createParticipantFromConfig}
 * builds the whole tree it declares, and its datawriter/datareader are
 * fetched by name off the returned {@link ConfiguredParticipant}.
 *
 * <p>The library, type and topic names are unique to this test ({@code CP}
 * prefix) to avoid colliding with any other test's profiles or types in the
 * same process-wide factory singleton.
 *
 * <p>The XML declares one participant ({@code App}) with both the publisher
 * and the subscriber, matching the shape a real config file would use. But
 * per {@code MatchedEndpointsTest}'s own note, matching does not loop back
 * within a single participant in this implementation, so a second
 * participant ({@code App2}, subscriber-only, in the same library) is
 * declared purely to prove the round trip actually communicates -- the
 * {@code App} tree alone already proves fetch-by-name, caching and
 * teardown.
 */
class ConfiguredParticipantTest {

    private static final String XML = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"
            + "<dds>\n"
            + "  <types>\n"
            + "    <struct name=\"CPSimpleData\" extensibility=\"final\">\n"
            + "      <member name=\"id\" type=\"int32\" key=\"true\"/>\n"
            + "      <member name=\"value\" type=\"int32\"/>\n"
            + "    </struct>\n"
            + "  </types>\n"
            + "  <domain_library name=\"CPDL\">\n"
            + "    <domain name=\"D0\" domain_id=\"" + testDomain() + "\">\n"
            + "      <register_type name=\"CPSimpleDataType\" type_ref=\"CPSimpleData\"/>\n"
            + "      <topic name=\"CPSimpleTopic\" register_type_ref=\"CPSimpleDataType\"/>\n"
            + "    </domain>\n"
            + "  </domain_library>\n"
            + "  <domain_participant_library name=\"CPPL\">\n"
            + "    <domain_participant name=\"App\" domain_ref=\"CPDL::D0\">\n"
            + "      <publisher name=\"pub\">\n"
            + "        <data_writer name=\"writer\" topic_ref=\"CPSimpleTopic\"/>\n"
            + "      </publisher>\n"
            + "      <subscriber name=\"sub\">\n"
            + "        <data_reader name=\"reader\" topic_ref=\"CPSimpleTopic\"/>\n"
            + "      </subscriber>\n"
            + "    </domain_participant>\n"
            + "    <domain_participant name=\"App2\" domain_ref=\"CPDL::D0\">\n"
            + "      <subscriber name=\"sub\">\n"
            + "        <data_reader name=\"reader\" topic_ref=\"CPSimpleTopic\"/>\n"
            + "      </subscriber>\n"
            + "    </domain_participant>\n"
            + "  </domain_participant_library>\n"
            + "</dds>\n";

    // The matching struct, loaded into a standalone registry to build a
    // writable DynamicData for the configured writer -- the same pattern
    // DynamicPubSubTest uses, with a registry independent of the one the
    // factory loaded via loadProfiles.
    private static final String TYPES_XML = "<types>\n"
            + " <struct name=\"CPSimpleData\" extensibility=\"final\">\n"
            + "  <member name=\"id\" type=\"int32\" key=\"true\"/>\n"
            + "  <member name=\"value\" type=\"int32\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    private static Path writeConfigFile() throws IOException {
        Path path = Files.createTempFile("configured-participant-", ".xml");
        path.toFile().deleteOnExit();
        Files.write(path, XML.getBytes(StandardCharsets.UTF_8));
        return path;
    }

    @Test
    void treeLoadsFetchesByNameCachesAndCommunicates() throws IOException, InterruptedException {
        Path configPath = writeConfigFile();
        DomainParticipantFactory.getInstance()
                .loadProfiles(Collections.singletonList(configPath.toString()));

        // HARD GATE: the whole tree must build from the loaded library.
        try (ConfiguredParticipant app = DomainParticipantFactory.getInstance()
                        .createParticipantFromConfig("CPPL::App");
                ConfiguredParticipant app2 = DomainParticipantFactory.getInstance()
                        .createParticipantFromConfig("CPPL::App2")) {
            assertNotNull(app);
            assertNotNull(app2);

            DynamicDataWriter writer = app.getDataWriter("pub::writer");
            DynamicDataReader reader = app.getDataReader("sub::reader");
            assertNotNull(writer);
            assertNotNull(reader);

            // Caching: repeated fetches of the same name return the same wrapper.
            assertSame(writer, app.getDataWriter("pub::writer"));
            assertSame(reader, app.getDataReader("sub::reader"));

            // Negative: a name absent from the tree throws rather than
            // returning null.
            assertThrows(DdsException.class, () -> app.getDataWriter("pub::nonexistent"));
            assertThrows(DdsException.class, () -> app.getDataReader("sub::nonexistent"));

            // End-to-end: App's own reader cannot see App's own writer (no
            // intra-participant loopback in this implementation), so the
            // round trip goes through App2's reader instead.
            DynamicDataReader remoteReader = app2.getDataReader("sub::reader");
            assertNotNull(remoteReader);

            XmlTypeRegistry registry = new XmlTypeRegistry();
            DynamicData sent = null;
            DynamicData received = null;
            try {
                registry.loadString(TYPES_XML);
                DynamicTypeSupport support = registry.getTypeSupport("CPSimpleData");

                boolean matched = false;
                for (int i = 0; i < 200; i++) {
                    if (writer.publicationMatchedCount() > 0) {
                        matched = true;
                        break;
                    }
                    Thread.sleep(50);
                }
                if (!matched) {
                    fail("configured writer never matched App2's reader");
                }

                sent = DynamicData.create(support);
                sent.setI32("id", 42);
                sent.setI32("value", 1337);
                writer.write(sent);

                for (int i = 0; i < 200 && received == null; i++) {
                    received = remoteReader.take();
                    if (received == null) {
                        Thread.sleep(50);
                    }
                }
                if (received == null) {
                    fail("no sample received on App2's reader");
                }
                assertEquals(42, received.getI32("id"));
                assertEquals(1337, received.getI32("value"));
            } finally {
                if (sent != null) {
                    sent.close();
                }
                if (received != null) {
                    received.close();
                }
                registry.close();
            }
            // close() below (try-with-resources) exercises the
            // endpoint-first-then-tree teardown for both app and app2; it
            // must not crash.
        }
    }
}
