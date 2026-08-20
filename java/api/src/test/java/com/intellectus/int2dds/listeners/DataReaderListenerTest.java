package com.intellectus.int2dds.listeners;

import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.status.SubscriptionMatchedStatus;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import org.junit.jupiter.api.Test;

class DataReaderListenerTest {

    // Mirrors DomainParticipantTest.testDomain(), which is package-private to
    // the core package and unreachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void onSubscriptionMatchedFiresWhenAWriterAppears() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("ListenerMatch", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            CountDownLatch latch = new CountDownLatch(1);
            AtomicReference<SubscriptionMatchedStatus> got = new AtomicReference<>();
            reader.setListener(new DataReaderListenerBase() {
                @Override
                public void onSubscriptionMatched(DataReader<?> r, SubscriptionMatchedStatus s) {
                    got.set(s);
                    latch.countDown();
                }
            }, StatusMask.of(StatusMask.SUBSCRIPTION_MATCHED));

            // A matching writer must make the reader observe a subscription match.
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);

            boolean fired = latch.await(5, TimeUnit.SECONDS);
            reader.setListener(null, null);   // explicit teardown releases the native ctx

            assertTrue(fired, "onSubscriptionMatched did not fire within 5s -- trampoline or discovery");
            assertTrue(got.get().currentCount() >= 1, "matched writer count should be >= 1");
        }
    }

    @Test
    void onDataAvailableFiresWhenAWriterWrites() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("ListenerDataAvail", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            CountDownLatch latch = new CountDownLatch(1);
            AtomicReference<DataReader<?>> got = new AtomicReference<>();
            reader.setListener(new DataReaderListenerBase() {
                @Override
                public void onDataAvailable(DataReader<?> r) {
                    got.set(r);
                    latch.countDown();
                }
            }, StatusMask.of(StatusMask.DATA_AVAILABLE));

            // Discovery is async, so retry the write until the callback fires or
            // the bound is hit.
            ConformanceRecord sample = new ConformanceRecord();
            sample.id = 1;
            long deadline = System.nanoTime() + 5_000_000_000L;
            while (System.nanoTime() < deadline && latch.getCount() > 0) {
                w.write(sample);
                latch.await(20, TimeUnit.MILLISECONDS);
            }
            reader.setListener(null, null);   // explicit teardown releases the native ctx

            assertTrue(latch.getCount() == 0, "onDataAvailable did not fire within 5s -- trampoline or receive path");
        }
    }

    @Test
    void otherReaderCallbacksRegisterAndClearWithoutCrashing() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("ListenerOthers", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            // These statuses are hard to trigger deterministically in a unit test;
            // this only proves install/clear with all 7 trampolines wired does not
            // crash, and the mask covers every remaining callback.
            reader.setListener(new DataReaderListenerBase(), StatusMask.of(
                    StatusMask.SAMPLE_REJECTED
                            | StatusMask.LIVELINESS_CHANGED
                            | StatusMask.REQUESTED_DEADLINE_MISSED
                            | StatusMask.REQUESTED_INCOMPATIBLE_QOS
                            | StatusMask.SAMPLE_LOST));
            reader.setListener(null, null);
        }
    }
}
