package com.intellectus.int2dds.examples;

import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.xtypes.DynamicData;
import com.intellectus.int2dds.xtypes.DynamicDataReader;
import com.intellectus.int2dds.xtypes.DynamicDataWriter;
import com.intellectus.int2dds.xtypes.DynamicTopic;
import com.intellectus.int2dds.xtypes.DynamicTypeSupport;
import com.intellectus.int2dds.xtypes.XmlTypeRegistry;

/**
 * A complete dynamic (XTypes) pub/sub round trip: a struct defined at run
 * time from an XML string, with no generated {@code IDdsType} class and no
 * compiled Java type at all -- fields are read and written through {@link
 * DynamicData}'s dotted-path getters/setters instead of a class's own
 * fields, as in {@link HelloWorldPub}.
 *
 * <p>Unlike {@code HelloWorldPub}, which only publishes and can only prove
 * that the write path accepted a sample, this example runs both a writer and
 * a reader in the same process: two {@link DomainParticipant}s on the same
 * domain, each loading the same XML type independently, so that discovery
 * actually matches them and the reader can take back what the writer sent.
 * That is what makes this self-proving -- the printed "received" fields are
 * the sample as it survived serialization, transport, and deserialization,
 * not just an echo of what was set before {@code write()}.
 *
 * <p>Run with {@code -d}/{@code --domain <id>} (default 0), the same flag
 * {@link HelloWorldPub} takes.
 */
public final class DynamicHelloWorld {

    private static final String TOPIC_NAME = "TelemetryTopic";
    private static final String TYPE_NAME = "Telemetry";

    // Same shape as DynamicPubSubTest's XML: one key field plus a spread of
    // primitive kinds, enough to exercise most of DynamicData's get/set pairs.
    private static final String XML = "<types>\n"
            + " <struct name=\"" + TYPE_NAME + "\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"temperature\" type=\"float32\"/>\n"
            + "  <member name=\"active\" type=\"boolean\"/>\n"
            + "  <member name=\"label\" type=\"string\"/>\n"
            + "  <member name=\"count\" type=\"int64\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    public static void main(String[] args) throws InterruptedException {
        int domainId = parseDomain(args);

        System.out.println("=== Dynamic HelloWorld (Java, XTypes) ===");
        System.out.println("Type defined at runtime from XML, no generated Java class involved:");
        System.out.println(XML);

        // Two participants on the same domain: one plays the writer side, one
        // the reader side. Discovery does not appear to loop an endpoint back
        // to itself within a single participant (see DynamicPubSubTest), so
        // this mirrors that test's two-participant shape rather than sharing
        // one participant for both ends.
        try (DomainParticipant writerParticipant = new DomainParticipant(domainId);
                DomainParticipant readerParticipant = new DomainParticipant(domainId);
                XmlTypeRegistry writerRegistry = new XmlTypeRegistry();
                XmlTypeRegistry readerRegistry = new XmlTypeRegistry()) {
            System.out.println("Created two participants on domain " + writerParticipant.domainId());

            writerRegistry.loadString(XML);
            readerRegistry.loadString(XML);

            try (DynamicTypeSupport writerSupport = writerRegistry.getTypeSupport(TYPE_NAME);
                    DynamicTypeSupport readerSupport = readerRegistry.getTypeSupport(TYPE_NAME)) {

                try (DynamicTopic writerTopic =
                                writerParticipant.createDynamicTopic(TOPIC_NAME, writerSupport);
                        DynamicTopic readerTopic =
                                readerParticipant.createDynamicTopic(TOPIC_NAME, readerSupport)) {

                    Publisher publisher = writerParticipant.createPublisher();
                    Subscriber subscriber = readerParticipant.createSubscriber();

                    try (DynamicDataWriter writer =
                                    publisher.createDynamicDataWriter(writerTopic, writerSupport);
                            DynamicDataReader reader =
                                    subscriber.createDynamicDataReader(readerTopic, readerSupport)) {
                        System.out.println("Created dynamic writer and reader on topic " + TOPIC_NAME);

                        waitForMatch(writer);

                        try (DynamicData data = DynamicData.create(writerSupport)) {
                            data.setU32("id", 7);
                            data.setF32("temperature", 23.5f);
                            data.setBool("active", true);
                            data.setString("label", "sensor-A");
                            data.setI64("count", -100L);
                            System.out.println("Publishing: id=7, temperature=23.5, active=true, "
                                    + "label='sensor-A', count=-100");
                            writer.write(data);
                        }

                        DynamicData received = waitForSample(reader);
                        try {
                            System.out.println("Received: id=" + received.getU32("id")
                                    + ", temperature=" + received.getF32("temperature")
                                    + ", active=" + received.getBool("active")
                                    + ", label='" + received.getString("label") + "'"
                                    + ", count=" + received.getI64("count"));
                        } finally {
                            received.close();
                        }

                        System.out.println("Round trip complete.");
                    } finally {
                        publisher.close();
                        subscriber.close();
                    }
                }
            }
        }
    }

    /**
     * Polls {@link DynamicDataWriter#publicationMatchedCount()} on a bounded
     * budget (~5s) rather than sleeping as a synchronization primitive --
     * discovery is asynchronous and there is no blocking wait for it here.
     */
    private static void waitForMatch(DynamicDataWriter writer) throws InterruptedException {
        for (int i = 0; i < 100; i++) {
            if (writer.publicationMatchedCount() > 0) {
                System.out.println("Writer matched the reader after " + (i * 50) + " ms");
                return;
            }
            Thread.sleep(50);
        }
        System.out.println("Writer never matched a reader within the time budget; exiting.");
        System.exit(1);
    }

    /** Polls {@link DynamicDataReader#take()} on the same ~5s budget as {@link #waitForMatch}. */
    private static DynamicData waitForSample(DynamicDataReader reader) throws InterruptedException {
        for (int i = 0; i < 100; i++) {
            DynamicData sample = reader.take();
            if (sample != null) {
                return sample;
            }
            Thread.sleep(50);
        }
        System.out.println("No sample received within the time budget; exiting.");
        System.exit(1);
        return null; // unreachable, System.exit terminates the process
    }

    /** Parses {@code -d}/{@code --domain <id>} from the CLI, defaulting to 0. */
    private static int parseDomain(String[] args) {
        for (int i = 0; i < args.length - 1; i++) {
            if (args[i].equals("-d") || args[i].equals("--domain")) {
                return Integer.parseInt(args[i + 1]);
            }
        }
        return 0;
    }
}
