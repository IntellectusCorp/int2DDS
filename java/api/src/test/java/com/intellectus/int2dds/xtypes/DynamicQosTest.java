package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import org.junit.jupiter.api.Test;

/**
 * Same round trip as {@link DynamicPubSubTest}, but the dynamic writer and
 * reader are created with an explicit RELIABLE {@link DataWriterQos} /
 * {@link DataReaderQos} instead of the core default, proving {@link
 * Publisher#createDynamicDataWriter(DynamicTopic, DynamicTypeSupport,
 * DataWriterQos)} and {@link Subscriber#createDynamicDataReader(DynamicTopic,
 * DynamicTypeSupport, DataReaderQos)} actually apply the QoS on the native
 * side rather than silently ignoring it.
 */
class DynamicQosTest {

    // Mirrors DynamicPubSubTest.testDomain().
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
    void reliableDynamicPubSubRoundTrip() throws InterruptedException {
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

            writerTopic = writerParticipant.createDynamicTopic("TelemetryQosTopic", writerSupport);
            publisher = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            writer = publisher.createDynamicDataWriter(writerTopic, writerSupport, writerQos);

            readerTopic = readerParticipant.createDynamicTopic("TelemetryQosTopic", readerSupport);
            subscriber = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            reader = subscriber.createDynamicDataReader(readerTopic, readerSupport, readerQos);

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
            writer.write(data);

            for (int i = 0; i < 200 && received == null; i++) {
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

    /**
     * {@link DynamicDataWriter#getQos} / {@link DynamicDataReader#getQos}
     * read back the RELIABLE reliability the entities were created with,
     * proving the dynamic-entity QoS story round-trips both ways (set at
     * creation, read back after). No matching/pub-sub needed for this.
     */
    @Test
    void getQosReturnsReliabilityFromCreation() {
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
        try {
            writerRegistry = new XmlTypeRegistry();
            writerRegistry.loadString(XML);
            writerSupport = writerRegistry.getTypeSupport("Telemetry");

            readerRegistry = new XmlTypeRegistry();
            readerRegistry.loadString(XML);
            readerSupport = readerRegistry.getTypeSupport("Telemetry");

            writerTopic = writerParticipant.createDynamicTopic("TelemetryQosGetTopic", writerSupport);
            publisher = writerParticipant.createPublisher();
            DataWriterQos writerQos = new DataWriterQos();
            writerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            writer = publisher.createDynamicDataWriter(writerTopic, writerSupport, writerQos);

            readerTopic = readerParticipant.createDynamicTopic("TelemetryQosGetTopic", readerSupport);
            subscriber = readerParticipant.createSubscriber();
            DataReaderQos readerQos = new DataReaderQos();
            readerQos.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            reader = subscriber.createDynamicDataReader(readerTopic, readerSupport, readerQos);

            assertEquals(ReliabilityKind.RELIABLE, writer.getQos().getReliability().getKind());
            assertEquals(ReliabilityKind.RELIABLE, reader.getQos().getReliability().getKind());
        } finally {
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
