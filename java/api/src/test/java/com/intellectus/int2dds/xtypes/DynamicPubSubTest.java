package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import org.junit.jupiter.api.Test;

/**
 * End-to-end round trip through the Java xtypes dynamic write/read path:
 * loads an XML-defined type on two participants, publishes a {@link
 * DynamicData} sample from one and takes it back on the other, and verifies
 * every field survived. The Java mirror of {@code ffi/tests/xml_dynamic.rs}.
 *
 * <p>Two participants, the same pattern {@code MatchedEndpointsTest} and
 * {@code DiscoveryTest} settled on: matching does not appear to loop back
 * within a single participant.
 */
class DynamicPubSubTest {

    // Mirrors DomainParticipantTest.testDomain(), package-private to core and
    // not otherwise reachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    private static final String XML = "<types>\n"
            + " <struct name=\"Telemetry\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"temperature\" type=\"float32\"/>\n"
            + "  <member name=\"active\" type=\"boolean\"/>\n"
            + "  <member name=\"label\" type=\"string\"/>\n"
            + "  <member name=\"count\" type=\"int64\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    @Test
    void xmlDynamicPubSubRoundTrip() throws InterruptedException {
        DomainParticipant writerParticipant = new DomainParticipant(testDomain());
        DomainParticipant readerParticipant = new DomainParticipant(testDomain());

        XmlTypeRegistry writerRegistry = null;
        XmlTypeRegistry readerRegistry = null;
        DynamicTypeSupport writerSupport = null;
        DynamicTypeSupport readerSupport = null;
        DynamicTopic writerTopic = null;
        DynamicTopic readerTopic = null;
        Publisher publisher = null;
        Subscriber subscriber = null;
        DynamicDataWriter writer = null;
        DynamicDataReader reader = null;
        DynamicData data = null;
        DynamicData received = null;
        try {
            writerRegistry = new XmlTypeRegistry();
            writerRegistry.loadString(XML);
            writerSupport = writerRegistry.getTypeSupport("Telemetry");

            readerRegistry = new XmlTypeRegistry();
            readerRegistry.loadString(XML);
            readerSupport = readerRegistry.getTypeSupport("Telemetry");

            writerTopic = writerParticipant.createDynamicTopic("TelemetryTopic", writerSupport);
            publisher = writerParticipant.createPublisher();
            writer = publisher.createDynamicDataWriter(writerTopic, writerSupport);

            readerTopic = readerParticipant.createDynamicTopic("TelemetryTopic", readerSupport);
            subscriber = readerParticipant.createSubscriber();
            reader = subscriber.createDynamicDataReader(readerTopic, readerSupport);

            // Wait for the writer to match the reader. Discovery is
            // asynchronous, so this polls on a bounded budget (~10s) rather
            // than sleeping as a synchronization primitive.
            boolean matched = false;
            for (int i = 0; i < 200; i++) {
                if (writer.publicationMatchedCount() > 0) {
                    matched = true;
                    break;
                }
                Thread.sleep(50);
            }
            if (!matched) {
                fail("writer never matched the reader");
            }

            data = DynamicData.create(writerSupport);
            data.setU32("id", 7);
            data.setF32("temperature", 23.5f);
            data.setBool("active", true);
            data.setString("label", "sensor-A");
            data.setI64("count", -100L);
            for (int i = 0; i < 200 && received == null; i++) {
                // Re-write each attempt: the default reader is BestEffort, so a
                // write issued before writer/reader discovery completes is
                // dropped rather than queued. Rewriting until a sample arrives
                // establishes the match without a bare sleep as synchronization.
                writer.write(data);
                received = reader.take();
                if (received == null) {
                    Thread.sleep(50);
                }
            }
            if (received == null) {
                fail("no sample received");
            }

            assertEquals(7, received.getU32("id"));
            assertEquals(23.5f, received.getF32("temperature"), 0.0001f);
            assertTrue(received.getBool("active"));
            assertEquals("sensor-A", received.getString("label"));
            assertEquals(-100L, received.getI64("count"));
        } finally {
            // data/received have no dependents and always release cleanly.
            if (data != null) {
                data.close();
            }
            if (received != null) {
                received.close();
            }
            // Close in dependency order: writer/reader first (they now
            // unregister from their parent publisher/subscriber on close),
            // then the topics, then publisher/subscriber, then support and
            // registry, and finally the participants.
            if (writer != null) {
                writer.close();
            }
            if (reader != null) {
                reader.close();
            }
            if (writerTopic != null) {
                writerTopic.close();
            }
            if (readerTopic != null) {
                readerTopic.close();
            }
            if (publisher != null) {
                publisher.close();
            }
            if (subscriber != null) {
                subscriber.close();
            }
            if (writerSupport != null) {
                writerSupport.close();
            }
            if (readerSupport != null) {
                readerSupport.close();
            }
            if (writerRegistry != null) {
                writerRegistry.close();
            }
            if (readerRegistry != null) {
                readerRegistry.close();
            }
            writerParticipant.close();
            readerParticipant.close();
        }
    }
}
