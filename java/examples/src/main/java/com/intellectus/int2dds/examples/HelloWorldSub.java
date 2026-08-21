package com.intellectus.int2dds.examples;

import com.intellectus.int2dds.conditions.Condition;
import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.conditions.WaitSet;
import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Sample;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.status.StatusMask;
import java.util.List;

/**
 * Subscribes to the {@link HelloWorld} samples {@link HelloWorldPub} writes.
 *
 * <p>The read-path half of the HelloWorld pair: {@link HelloWorldPub}'s class
 * doc noted that conditions were not yet available on this branch and it
 * therefore could not wait for a subscriber. This example demonstrates the
 * idiomatic DDS wait-for-data pattern that was missing then — a {@link
 * WaitSet} with the reader's {@link StatusCondition} enabled for {@link
 * StatusMask#DATA_AVAILABLE}, {@link WaitSet#await} blocking the thread until
 * data arrives, and {@link DataReader#take} draining the cache once woken.
 * That means this example blocks between samples rather than busy-polling.
 *
 * <p>Uses the same topic name ({@code hello_world_topic}) and type ({@code
 * HelloWorld}) as {@link HelloWorldPub}, so it interoperates on the wire with
 * the Java, C# and Rust publishers alike.
 *
 * <p>Run with {@code -d}/{@code --domain <id>} (default 0) and {@code
 * --reliable} (default BEST_EFFORT), the same flags {@link HelloWorldPub}
 * takes — pass the same reliability on both sides for the two to match.
 */
public final class HelloWorldSub {

    private static final String TOPIC_NAME = "hello_world_topic";
    private static final long WAIT_TIMEOUT_MS = 1000L;

    public static void main(String[] args) {
        int domainId = parseDomain(args);
        boolean reliable = hasFlag(args, "--reliable");

        System.out.println("=== HelloWorld Subscriber (Java) ===");
        System.out.println("QoS: " + (reliable ? "RELIABLE" : "BEST_EFFORT"));

        // Same shape as HelloWorldPub: a single try-with-resources on the
        // participant, whose close() cascades to every live child (topic,
        // subscriber, reader) -- nothing below needs its own try block for
        // that. The WaitSet and StatusCondition are not participant
        // children (WaitSet is a standalone AutoCloseable), so they get
        // their own nested try-with-resources further down.
        try (DomainParticipant participant = new DomainParticipant(domainId)) {
            // The loop below runs until the process is killed; this hook,
            // not the try-with-resources, is what releases the participant
            // on the way out. See HelloWorldPub for the same note.
            Runtime.getRuntime().addShutdownHook(new Thread(participant::close));
            System.out.println("Created participant on domain " + participant.domainId());

            Topic<HelloWorld> topic = participant.createTopic(TOPIC_NAME, new HelloWorld());
            System.out.println("Created topic: " + topic.name() + " (" + topic.typeName() + ")");

            Subscriber subscriber = participant.createSubscriber();
            ReliabilityKind kind = reliable ? ReliabilityKind.RELIABLE : ReliabilityKind.BEST_EFFORT;
            DataReaderQos qos = new DataReaderQos();
            qos.setReliability(new Reliability(kind));
            DataReader<HelloWorld> reader = subscriber.createDataReader(topic, HelloWorld::new, qos);
            System.out.println("Created subscriber and data reader");

            try (WaitSet waitSet = new WaitSet();
                    StatusCondition statusCondition = reader.getStatusCondition()) {
                statusCondition.setEnabledStatuses(StatusMask.of(StatusMask.DATA_AVAILABLE));
                waitSet.attach(statusCondition);
                System.out.println("Waiting for data...");
                try {
                    while (true) {
                        List<Condition> triggered = waitSet.await(WAIT_TIMEOUT_MS);
                        if (triggered.isEmpty()) {
                            // Timed out with no DATA_AVAILABLE: nothing arrived
                            // this cycle. Honest heartbeat, not an error.
                            System.out.println("waiting...");
                            continue;
                        }
                        Sample<HelloWorld> sample;
                        while ((sample = reader.take()) != null) {
                            HelloWorld data = sample.data();
                            if (data == null) {
                                // info().validData() is false: a dispose or
                                // unregister, not a real sample.
                                continue;
                            }
                            System.out.println(
                                    "Received: index=" + data.index + ", message='" + data.message + "'");
                        }
                    }
                } finally {
                    // Honor the WaitSet contract: detach before the
                    // try-with-resources above closes the condition and the
                    // WaitSet. Reached only on an exceptional exit -- the
                    // normal exit is the shutdown hook killing the process,
                    // same as HelloWorldPub.
                    waitSet.detach(statusCondition);
                }
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
