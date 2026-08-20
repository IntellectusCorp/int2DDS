package com.intellectus.int2dds.async;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Sample;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;

/**
 * End-to-end proof that {@link AsyncDataReader} actually delivers data
 * across the async wait/take path, not just against a mocked reader.
 *
 * <p>Two participants in the same domain -- the pattern {@code
 * MatchedEndpointsTest}/{@code DiscoveryTest} settled on, since matching
 * does not appear to loop back within a single participant.
 */
class AsyncDataReaderTest {

    // Mirrors DomainParticipantTest.testDomain(), package-private to core
    // and not otherwise reachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void waitForDataThenTakeSeesTheWrittenSample() throws Exception {
        try (DomainParticipant writerParticipant = new DomainParticipant(testDomain());
                DomainParticipant readerParticipant = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> writerTopic =
                    writerParticipant.createTopic("AsyncDataReaderTopic", new ConformanceRecord());
            Publisher pub = writerParticipant.createPublisher();
            DataWriter<ConformanceRecord> writer = pub.createDataWriter(writerTopic);

            Topic<ConformanceRecord> readerTopic =
                    readerParticipant.createTopic("AsyncDataReaderTopic", new ConformanceRecord());
            Subscriber sub = readerParticipant.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(readerTopic, ConformanceRecord::new);

            try (AsyncDataReader<ConformanceRecord> asyncReader = new AsyncDataReader<>(reader)) {
                ConformanceRecord sent = new ConformanceRecord();
                sent.id = 7;
                sent.value = 3.5d;
                sent.label = "async";

                // Discovery is asynchronous: keep writing and re-arming the
                // wait until either it reports data available, or a bounded
                // budget runs out.
                boolean gotData = false;
                long deadline = System.nanoTime() + 10_000_000_000L;
                while (System.nanoTime() < deadline && !gotData) {
                    writer.write(sent);
                    gotData = asyncReader.waitForDataAsync(2000).get(5, TimeUnit.SECONDS);
                }
                assertTrue(gotData, "waitForDataAsync should observe DATA_AVAILABLE within budget");

                Sample<ConformanceRecord> sample = asyncReader.takeAsync().get(2, TimeUnit.SECONDS);
                assertNotNull(sample, "takeAsync should return the written sample");
                assertNotNull(sample.data());
                assertEquals(7, sample.data().id);
            }
        }
    }

    @Test
    void takeAsyncOnAnEmptyReaderCompletesWithNull() throws Exception {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("AsyncDataReaderEmptyTopic", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            try (AsyncDataReader<ConformanceRecord> asyncReader = new AsyncDataReader<>(reader)) {
                Sample<ConformanceRecord> sample = asyncReader.takeAsync().get();
                assertNull(sample, "empty reader's takeAsync should complete with null");
            }
        }
    }
}
