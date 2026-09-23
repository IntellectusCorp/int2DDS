package com.intellectus.int2dds.listeners;

import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.status.PublicationMatchedStatus;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;

class DataWriterListenerTest {

    // Mirrors DomainParticipantTest.testDomain(), which is package-private to
    // the core package and unreachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void onPublicationMatchedFiresWhenAReaderAppears() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("WriterListenerMatch", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> writer = pub.createDataWriter(topic);

            CountDownLatch latch = new CountDownLatch(1);
            AtomicReference<PublicationMatchedStatus> got = new AtomicReference<>();
            writer.setListener(new DataWriterListenerBase() {
                @Override
                public void onPublicationMatched(DataWriter<?> w, PublicationMatchedStatus s) {
                    got.set(s);
                    latch.countDown();
                }
            }, StatusMask.of(StatusMask.PUBLICATION_MATCHED));

            // A matching reader must make the writer observe a publication match.
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            boolean fired = latch.await(5, TimeUnit.SECONDS);
            writer.setListener(null, null);   // explicit teardown releases the native ctx

            assertTrue(fired, "onPublicationMatched did not fire within 5s -- trampoline or discovery");
            assertTrue(got.get().currentCount() >= 1, "matched reader count should be >= 1");
        }
    }

    @Test
    void otherWriterCallbacksRegisterAndClearWithoutCrashing() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("WriterListenerOthers", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> writer = pub.createDataWriter(topic);

            // These statuses are hard to trigger deterministically in a unit test;
            // this only proves install/clear with all 4 trampolines wired does not
            // crash, and the mask covers every remaining callback.
            writer.setListener(new DataWriterListenerBase(), StatusMask.of(
                    StatusMask.OFFERED_DEADLINE_MISSED
                            | StatusMask.OFFERED_INCOMPATIBLE_QOS
                            | StatusMask.LIVELINESS_LOST));
            writer.setListener(null, null);
        }
    }
}
