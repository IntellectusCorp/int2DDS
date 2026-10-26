package kr.co.intellectus.int2dds.android;

import android.app.Activity;
import android.content.Context;
import android.net.wifi.WifiManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.system.ErrnoException;
import android.system.Os;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import android.widget.ScrollView;
import android.widget.TextView;
import java.net.Inet4Address;
import java.net.InetAddress;
import java.net.NetworkInterface;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import kr.co.intellectus.int2dds.conditions.Condition;
import kr.co.intellectus.int2dds.conditions.StatusCondition;
import kr.co.intellectus.int2dds.conditions.WaitSet;
import kr.co.intellectus.int2dds.core.DataReader;
import kr.co.intellectus.int2dds.core.DataWriter;
import kr.co.intellectus.int2dds.core.DomainParticipant;
import kr.co.intellectus.int2dds.core.Publisher;
import kr.co.intellectus.int2dds.core.Sample;
import kr.co.intellectus.int2dds.core.Subscriber;
import kr.co.intellectus.int2dds.core.Topic;
import kr.co.intellectus.int2dds.examples.HelloWorld;
import kr.co.intellectus.int2dds.internal.NativeLoader;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.DataWriterQos;
import kr.co.intellectus.int2dds.qos.ParticipantQos;
import kr.co.intellectus.int2dds.qos.Property;
import kr.co.intellectus.int2dds.qos.Reliability;
import kr.co.intellectus.int2dds.qos.ReliabilityKind;
import kr.co.intellectus.int2dds.status.StatusMask;

/**
 * HelloWorld publisher/subscriber on Android, interoperable with the Java, C# and Rust examples
 * (topic {@code hello_world_topic}, type {@code HelloWorld}).
 *
 * <p>All DDS calls run on a worker thread; the UI thread only renders the log. A {@link
 * WifiManager.MulticastLock} is held while a participant is alive so SPDP multicast reaches the
 * app, and {@code int2dds.initial_peers} covers networks that filter multicast anyway.
 */
public final class MainActivity extends Activity {

    private static final String TOPIC_NAME = "hello_world_topic";
    private static final String PROP_INITIAL_PEERS = "int2dds.initial_peers";
    // Cellular interfaces (rmnet_data*) are up alongside Wi-Fi; keep DDS traffic on Wi-Fi.
    private static final String NETWORK_INTERFACE = "wlan0";
    private static final long PUBLISH_INTERVAL_MS = 1000L;
    private static final long WAIT_TIMEOUT_MS = 1000L;
    private static final int MAX_LOG_CHARS = 40_000;

    private final Handler mainHandler = new Handler(Looper.getMainLooper());

    private TextView status;
    private EditText domain;
    private EditText peers;
    private CheckBox reliable;
    private Button publish;
    private Button subscribe;
    private Button stop;
    private ScrollView scroll;
    private TextView log;

    private WifiManager.MulticastLock multicastLock;
    private volatile boolean running;
    private Thread worker;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);

        status = findViewById(R.id.status);
        domain = findViewById(R.id.domain);
        peers = findViewById(R.id.peers);
        reliable = findViewById(R.id.reliable);
        publish = findViewById(R.id.publish);
        subscribe = findViewById(R.id.subscribe);
        stop = findViewById(R.id.stop);
        scroll = findViewById(R.id.scroll);
        log = findViewById(R.id.log);

        publish.setOnClickListener(v -> start(true));
        subscribe.setOnClickListener(v -> start(false));
        stop.setOnClickListener(v -> stop());
        findViewById(R.id.clear).setOnClickListener(v -> log.setText(""));

        WifiManager wifi =
                (WifiManager) getApplicationContext().getSystemService(Context.WIFI_SERVICE);
        multicastLock = wifi.createMulticastLock("int2dds");
        multicastLock.setReferenceCounted(false);

        status.setText("Loading native library...");
        new Thread(
                        () -> {
                            String text;
                            try {
                                text =
                                        "int2dds "
                                                + NativeLoader.nativeVersion()
                                                + "  device IP: "
                                                + String.join(", ", localIpv4Addresses());
                            } catch (Throwable t) {
                                text = "Native load failed: " + t;
                            }
                            String finalText = text;
                            mainHandler.post(() -> status.setText(finalText));
                        },
                        "int2dds-load")
                .start();
    }

    @Override
    protected void onDestroy() {
        stop();
        super.onDestroy();
    }

    private void start(boolean asPublisher) {
        if (running) {
            return;
        }
        int domainId;
        try {
            domainId = Integer.parseInt(domain.getText().toString().trim());
        } catch (NumberFormatException e) {
            appendLog("Invalid domain id");
            return;
        }
        String initialPeers = peers.getText().toString().trim();
        ReliabilityKind kind =
                reliable.isChecked() ? ReliabilityKind.RELIABLE : ReliabilityKind.BEST_EFFORT;

        try {
            Os.setenv("INT2DDS_NETWORK_INTERFACE", NETWORK_INTERFACE, true);
        } catch (ErrnoException e) {
            appendLog("setenv failed: " + e);
        }

        running = true;
        setBusy(true);
        multicastLock.acquire();
        appendLog(
                "=== "
                        + (asPublisher ? "Publisher" : "Subscriber")
                        + " domain="
                        + domainId
                        + " qos="
                        + kind
                        + (initialPeers.isEmpty() ? "" : " peers=" + initialPeers)
                        + " ===");

        worker =
                new Thread(
                        () -> {
                            try {
                                if (asPublisher) {
                                    runPublisher(domainId, initialPeers, kind);
                                } else {
                                    runSubscriber(domainId, initialPeers, kind);
                                }
                            } catch (Throwable t) {
                                appendLog("Error: " + t);
                                for (Throwable c = t.getCause(); c != null; c = c.getCause()) {
                                    appendLog("  caused by: " + c);
                                }
                            } finally {
                                running = false;
                                mainHandler.post(
                                        () -> {
                                            multicastLock.release();
                                            setBusy(false);
                                            appendLog("Stopped");
                                        });
                            }
                        },
                        "int2dds-worker");
        worker.start();
    }

    private void stop() {
        running = false;
        Thread t = worker;
        if (t != null) {
            t.interrupt();
        }
    }

    private void setBusy(boolean busy) {
        publish.setEnabled(!busy);
        subscribe.setEnabled(!busy);
        stop.setEnabled(busy);
    }

    private static ParticipantQos participantQos(String initialPeers) {
        ParticipantQos qos = new ParticipantQos();
        if (!initialPeers.isEmpty()) {
            Property property = new Property();
            property.add(PROP_INITIAL_PEERS, initialPeers, false);
            qos.setProperty(property);
        }
        return qos;
    }

    private void runPublisher(int domainId, String initialPeers, ReliabilityKind kind)
            throws InterruptedException {
        try (DomainParticipant participant =
                new DomainParticipant(domainId, participantQos(initialPeers))) {
            Topic<HelloWorld> topic = participant.createTopic(TOPIC_NAME, new HelloWorld());
            Publisher publisher = participant.createPublisher();
            DataWriterQos qos = new DataWriterQos();
            qos.setReliability(new Reliability(kind));
            DataWriter<HelloWorld> writer = publisher.createDataWriter(topic, qos);
            appendLog("Created participant, topic " + topic.name() + " and writer");

            int index = 0;
            int matched = -1;
            while (running) {
                int now = writer.getPublicationMatchedStatus().currentCount();
                if (now != matched) {
                    matched = now;
                    appendLog("Matched subscribers: " + now);
                }
                HelloWorld sample = new HelloWorld();
                sample.index = index;
                sample.message = "Hello from Android! (" + index + ")";
                writer.write(sample);
                appendLog("Published: index=" + index);
                index++;
                Thread.sleep(PUBLISH_INTERVAL_MS);
            }
        }
    }

    private void runSubscriber(int domainId, String initialPeers, ReliabilityKind kind) {
        try (DomainParticipant participant =
                new DomainParticipant(domainId, participantQos(initialPeers))) {
            Topic<HelloWorld> topic = participant.createTopic(TOPIC_NAME, new HelloWorld());
            Subscriber subscriber = participant.createSubscriber();
            DataReaderQos qos = new DataReaderQos();
            qos.setReliability(new Reliability(kind));
            DataReader<HelloWorld> reader = subscriber.createDataReader(topic, HelloWorld::new, qos);
            appendLog("Created participant, topic " + topic.name() + " and reader");

            try (WaitSet waitSet = new WaitSet();
                    StatusCondition statusCondition = reader.getStatusCondition()) {
                statusCondition.setEnabledStatuses(StatusMask.of(StatusMask.DATA_AVAILABLE));
                waitSet.attach(statusCondition);
                try {
                    int matched = -1;
                    while (running) {
                        int now = reader.getSubscriptionMatchedStatus().currentCount();
                        if (now != matched) {
                            matched = now;
                            appendLog("Matched publishers: " + now);
                        }
                        List<Condition> triggered = waitSet.await(WAIT_TIMEOUT_MS);
                        if (triggered.isEmpty()) {
                            continue;
                        }
                        Sample<HelloWorld> sample;
                        while ((sample = reader.take()) != null) {
                            HelloWorld data = sample.data();
                            if (data == null) {
                                continue;
                            }
                            appendLog(
                                    "Received: index="
                                            + data.index
                                            + ", message='"
                                            + data.message
                                            + "'");
                        }
                    }
                } finally {
                    waitSet.detach(statusCondition);
                }
            }
        }
    }

    private void appendLog(String line) {
        mainHandler.post(
                () -> {
                    if (log.length() > MAX_LOG_CHARS) {
                        log.setText("");
                    }
                    log.append(line + "\n");
                    scroll.post(() -> scroll.fullScroll(ScrollView.FOCUS_DOWN));
                });
    }

    private static List<String> localIpv4Addresses() {
        List<String> out = new ArrayList<>();
        try {
            for (NetworkInterface nic : Collections.list(NetworkInterface.getNetworkInterfaces())) {
                if (nic.isLoopback() || !nic.isUp()) {
                    continue;
                }
                for (InetAddress addr : Collections.list(nic.getInetAddresses())) {
                    if (addr instanceof Inet4Address) {
                        out.add(nic.getName() + "=" + addr.getHostAddress());
                    }
                }
            }
        } catch (Exception e) {
            out.add("unknown (" + e.getMessage() + ")");
        }
        return out;
    }
}
