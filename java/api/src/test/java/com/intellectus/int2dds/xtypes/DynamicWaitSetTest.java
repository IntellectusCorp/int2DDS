package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.conditions.Condition;
import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.conditions.WaitSet;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.status.StatusMask;
import java.util.List;
import org.junit.jupiter.api.Test;

/**
 * Proves a {@link WaitSet} can wait on a dynamic reader's {@link
 * StatusCondition} instead of polling {@link DynamicDataReader#take}: same
 * two-participant XML Telemetry round trip as {@link DynamicPubSubTest}, but
 * the reader side blocks on {@code ws.await} until DATA_AVAILABLE triggers.
 */
class DynamicWaitSetTest {

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
    void waitSetWakesOnDynamicReaderStatusCondition() throws InterruptedException {
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
        StatusCondition sc = null;
        WaitSet ws = null;
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

            sc = reader.getStatusCondition();
            sc.setEnabledStatuses(StatusMask.of(StatusMask.DATA_AVAILABLE));
            ws = new WaitSet();
            ws.attach(sc);

            data = DynamicData.create(writerSupport);
            data.setU32("id", 7);
            data.setF32("temperature", 23.5f);
            data.setBool("active", true);
            data.setString("label", "sensor-A");
            data.setI64("count", -100L);
            // Bounded retry: a single await(5000) is the intended path, but
            // guard against a lost wakeup racing sample delivery the same
            // way the polling test bounds its own wait, without using a
            // bare sleep as the synchronization mechanism. Re-write each
            // attempt: the default reader is BestEffort, so a write issued
            // before writer/reader discovery completes is dropped rather than
            // queued, and a single pre-match write would never wake the WaitSet.
            List<Condition> triggered = java.util.Collections.emptyList();
            for (int i = 0; i < 5 && triggered.isEmpty(); i++) {
                writer.write(data);
                triggered = ws.await(2000L);
            }
            assertFalse(triggered.isEmpty(), "WaitSet never woke on the reader's StatusCondition");
            assertTrue(triggered.contains(sc));

            received = reader.take();
            if (received == null) {
                fail("StatusCondition triggered but take() returned no sample");
            }

            assertEquals(7, received.getU32("id"));
            assertEquals(23.5f, received.getF32("temperature"), 0.0001f);
            assertTrue(received.getBool("active"));
            assertEquals("sensor-A", received.getString("label"));
            assertEquals(-100L, received.getI64("count"));
        } finally {
            if (data != null) {
                data.close();
            }
            if (received != null) {
                received.close();
            }
            // Detach before close, per WaitSet's contract.
            if (ws != null && sc != null) {
                ws.detach(sc);
            }
            if (sc != null) {
                sc.close();
            }
            if (ws != null) {
                ws.close();
            }
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
