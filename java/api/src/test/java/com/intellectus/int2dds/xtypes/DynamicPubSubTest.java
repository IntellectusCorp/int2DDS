package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import java.util.ArrayList;
import java.util.List;
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
 *
 * <p><b>Why this test never closes the topics, publisher, subscriber or
 * participants:</b> confirmed against the native layer directly (a scratch
 * {@code ffi/tests} case, run and discarded -- not part of this change),
 * {@code int2dds_dynamic_writer_destroy}/{@code int2dds_dynamic_reader_destroy}
 * (ffi/src/dynamic.rs) free only the FFI-level handle -- unlike the typed
 * {@code int2dds_delete_datawriter}/{@code int2dds_delete_datareader}, they
 * never call the core's {@code Publisher::delete_datawriter}/{@code
 * Subscriber::delete_datareader}, so the DCPS-level writer/reader stays
 * registered forever. A {@link DynamicTopic#close} (or a {@code Publisher}/
 * {@code Subscriber} close that cascades to one) on a topic/publisher/
 * subscriber that ever had a dynamic writer/reader attached is refused with
 * {@code RET_PRECONDITION_NOT_MET} for the lifetime of the process, even
 * after the writer/reader's own {@code close()} has already returned
 * successfully -- reproduced directly against the C ABI, independent of this
 * binding or the JVM. That is a native-layer gap outside this task's
 * Java-only scope (no `ffi/` changes) to fix.
 *
 * <p>Rather than let that refusal surface as a spurious test failure -- or
 * silently leave these handles for {@link
 * com.intellectus.int2dds.internal.NativeCleaner}'s reaper to retry forever,
 * permanently inflating {@code NativeCleaner.deferredCount()} for every test
 * that runs afterward in this same JVM -- this test deliberately keeps a
 * strong reference to every entity it cannot actually release ({@link
 * #LEAKED}), for the rest of the test JVM's life. A {@code
 * PhantomReference}-based reaper only ever sees an object once it becomes
 * unreachable, so an object that stays reachable never gets enqueued in the
 * first place, and {@code deferredCount()} is never touched by it. The
 * round-trip assertions above -- the actual point of this test -- are
 * unmodified and unrelaxed; only this known, reported, out-of-scope teardown
 * limitation is worked around.
 */
class DynamicPubSubTest {

    // Deliberately never cleared: see this class's doc for why. Retains
    // exactly the entities this test cannot cleanly close, for the life of
    // the test JVM.
    private static final List<Object> LEAKED = new ArrayList<Object>();

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
            // writer/reader always release cleanly at the Java-visible level
            // too (int2dds_dynamic_{writer,reader}_destroy always reports
            // success) -- it is what that native call leaves unregistered
            // underneath that blocks the topic/publisher/subscriber below.
            // See this class's doc.
            if (writer != null) {
                writer.close();
            }
            if (reader != null) {
                reader.close();
            }
            // support/registry are independent of the topic/writer and
            // always release cleanly.
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
            // writerTopic/readerTopic/publisher/subscriber/*Participant are
            // deliberately never closed -- see this class's doc. Retaining
            // the topics and publisher/subscriber is enough: NativeEntity
            // holds its parent strongly, so each participant stays reachable
            // through them too.
            LEAKED.add(writerTopic);
            LEAKED.add(readerTopic);
            LEAKED.add(publisher);
            LEAKED.add(subscriber);
        }
    }
}
