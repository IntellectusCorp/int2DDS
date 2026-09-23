package com.intellectus.int2dds.examples;

import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;

/**
 * Publishes a {@link HelloWorld} sample once a second.
 *
 * <p>Modeled on the C# {@code HelloWorldPub} example, with one deliberate
 * difference: it does not wait for a subscriber before publishing. The C#
 * example attaches a {@code StatusCondition} to a {@code WaitSet} and blocks
 * until {@code PublicationMatched} fires; conditions are not part of this
 * branch yet. This example simply publishes on a fixed interval and prints
 * what it sent — that is honest about what a write-only branch can prove: it
 * confirms the write path accepted the sample, not that any subscriber
 * received it.
 *
 * <p>Run with {@code -d}/{@code --domain <id>} (default 0, the real DDS
 * default domain) and {@code --reliable} (default BEST_EFFORT), the same
 * flags the C# and Rust examples take.
 */
public final class HelloWorldPub {

    private static final String TOPIC_NAME = "hello_world_topic";
    private static final long PUBLISH_INTERVAL_MS = 1000L;

    public static void main(String[] args) throws InterruptedException {
        int domainId = parseDomain(args);
        boolean reliable = hasFlag(args, "--reliable");

        System.out.println("=== HelloWorld Publisher (Java) ===");
        System.out.println("QoS: " + (reliable ? "RELIABLE" : "BEST_EFFORT"));

        // A single try-with-resources on the participant: NativeEntity.close()
        // cascades to every live child (topic, publisher, writer), so an
        // exception while any of them is being built still releases whatever
        // was already created -- nothing below needs its own try block.
        try (DomainParticipant participant = new DomainParticipant(domainId)) {
            // The loop below runs until the process is killed (Ctrl-C, or the
            // timeout used to bound this example in CI); that never returns
            // from the try block normally, so this hook -- not the
            // try-with-resources -- is what actually releases the participant
            // on the way out.
            Runtime.getRuntime().addShutdownHook(new Thread(participant::close));
            System.out.println("Created participant on domain " + participant.domainId());

            Topic<HelloWorld> topic = participant.createTopic(TOPIC_NAME, new HelloWorld());
            System.out.println("Created topic: " + topic.name() + " (" + topic.typeName() + ")");

            Publisher publisher = participant.createPublisher();
            ReliabilityKind kind = reliable ? ReliabilityKind.RELIABLE : ReliabilityKind.BEST_EFFORT;
            DataWriterQos qos = new DataWriterQos();
            qos.setReliability(new Reliability(kind));
            DataWriter<HelloWorld> writer = publisher.createDataWriter(topic, qos);
            System.out.println("Created publisher and data writer");

            int index = 0;
            while (true) {
                HelloWorld sample = new HelloWorld();
                sample.index = index;
                sample.message = "Hello from Java! (" + index + ")";
                writer.write(sample);
                // Publishing succeeds whether or not a subscriber is listening --
                // this line only says the sample reached the write path, not that
                // anyone received it. See the class doc.
                System.out.println(
                        "Published: index=" + sample.index + ", message='" + sample.message + "'");
                Thread.sleep(PUBLISH_INTERVAL_MS);
                index++;
            }
        }
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

    private static boolean hasFlag(String[] args, String flag) {
        for (String arg : args) {
            if (arg.equals(flag)) {
                return true;
            }
        }
        return false;
    }
}
