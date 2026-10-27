package kr.co.intellectus.int2dds.listeners;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.time.Duration;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;
import kr.co.intellectus.int2dds.core.DataReader;
import kr.co.intellectus.int2dds.core.DataWriter;
import kr.co.intellectus.int2dds.core.DomainParticipant;
import kr.co.intellectus.int2dds.core.Publisher;
import kr.co.intellectus.int2dds.core.Subscriber;
import kr.co.intellectus.int2dds.core.Topic;
import kr.co.intellectus.int2dds.qos.DataReaderQos;
import kr.co.intellectus.int2dds.qos.DataWriterQos;
import kr.co.intellectus.int2dds.qos.Deadline;
import kr.co.intellectus.int2dds.qos.Reliability;
import kr.co.intellectus.int2dds.qos.ReliabilityKind;
import kr.co.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.StatusMask;
import kr.co.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Status callbacks that carry a struct, actually fired by the core. The matched-status tests cover
 * one struct shape each; these cover the incompatible-QoS and deadline structs, which the native
 * side hands to the listener by pointer (a JNI trampoline below JDK 22, an FFM upcall on 22+).
 */
class StatusListenerTest {

    /** {@code Int2DdsQosPolicyId::Reliability} in {@code ffi/src/status.rs}. */
    private static final int RELIABILITY_POLICY_ID = 11;

    // Mirrors DomainParticipantTest.testDomain(), which is package-private to
    // the core package and unreachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    // The core checks QoS compatibility as soon as the second endpoint exists, before that
    // endpoint's setListener can run. Each test therefore installs the listener on the endpoint
    // created first, and the other endpoint's arrival fires it.

    /** A BEST_EFFORT writer cannot serve a RELIABLE reader. */
    private static DataWriterQos bestEffortWriter() {
        DataWriterQos wq = new DataWriterQos();
        wq.setReliability(new Reliability(ReliabilityKind.BEST_EFFORT));
        return wq;
    }

    private static DataReaderQos reliableReader() {
        DataReaderQos rq = new DataReaderQos();
        rq.setReliability(new Reliability(ReliabilityKind.RELIABLE));
        return rq;
    }

    @Test
    void anIncompatibleReaderFiresOnOfferedIncompatibleQos() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("StatusListenerOfferedIncompatible", new ConformanceRecord());
            DataWriter<ConformanceRecord> writer =
                    p.createPublisher().createDataWriter(topic, bestEffortWriter());
            CountDownLatch fired = new CountDownLatch(1);
            AtomicReference<OfferedIncompatibleQosStatus> got = new AtomicReference<>();
            writer.setListener(
                    new DataWriterListenerBase() {
                        @Override
                        public void onOfferedIncompatibleQos(
                                DataWriter<?> w, OfferedIncompatibleQosStatus s) {
                            got.set(s);
                            fired.countDown();
                        }
                    },
                    StatusMask.of(StatusMask.OFFERED_INCOMPATIBLE_QOS));

            DataReader<ConformanceRecord> reader =
                    p.createSubscriber()
                            .createDataReader(topic, ConformanceRecord::new, reliableReader());

            boolean ok = fired.await(10, TimeUnit.SECONDS);
            writer.setListener(null, null);

            assertTrue(ok, "onOfferedIncompatibleQos did not fire within 10s");
            assertTrue(got.get().totalCount() >= 1, "total_count " + got.get().totalCount());
            assertEquals(RELIABILITY_POLICY_ID, got.get().lastPolicyId());
            reader.close();
        }
    }

    @Test
    void anIncompatibleWriterFiresOnRequestedIncompatibleQos() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("StatusListenerRequestedIncompatible", new ConformanceRecord());
            DataReader<ConformanceRecord> reader =
                    p.createSubscriber()
                            .createDataReader(topic, ConformanceRecord::new, reliableReader());
            CountDownLatch fired = new CountDownLatch(1);
            AtomicReference<RequestedIncompatibleQosStatus> got = new AtomicReference<>();
            reader.setListener(
                    new DataReaderListenerBase() {
                        @Override
                        public void onRequestedIncompatibleQos(
                                DataReader<?> r, RequestedIncompatibleQosStatus s) {
                            got.set(s);
                            fired.countDown();
                        }
                    },
                    StatusMask.of(StatusMask.REQUESTED_INCOMPATIBLE_QOS));

            DataWriter<ConformanceRecord> writer =
                    p.createPublisher().createDataWriter(topic, bestEffortWriter());

            boolean ok = fired.await(10, TimeUnit.SECONDS);
            reader.setListener(null, null);

            assertTrue(
                    ok,
                    "onRequestedIncompatibleQos did not fire within 10s; getter total_count "
                            + reader.getRequestedIncompatibleQosStatus().totalCount());
            assertTrue(got.get().totalCount() >= 1, "total_count " + got.get().totalCount());
            assertEquals(RELIABILITY_POLICY_ID, got.get().lastPolicyId());
            writer.close();
        }
    }

    @Test
    void aSilentWriterFiresBothDeadlineMissedCallbacks() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("StatusListenerDeadline", new ConformanceRecord());
            Deadline deadline = new Deadline(Duration.ofMillis(100));

            DataWriterQos wq = new DataWriterQos();
            wq.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            wq.setDeadline(deadline);
            DataReaderQos rq = new DataReaderQos();
            rq.setReliability(new Reliability(ReliabilityKind.RELIABLE));
            rq.setDeadline(deadline);

            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> writer = pub.createDataWriter(topic, wq);
            CountDownLatch offered = new CountDownLatch(1);
            AtomicReference<OfferedDeadlineMissedStatus> offeredStatus = new AtomicReference<>();
            writer.setListener(
                    new DataWriterListenerBase() {
                        @Override
                        public void onOfferedDeadlineMissed(
                                DataWriter<?> w, OfferedDeadlineMissedStatus s) {
                            offeredStatus.set(s);
                            offered.countDown();
                        }
                    },
                    StatusMask.of(StatusMask.OFFERED_DEADLINE_MISSED));

            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new, rq);
            CountDownLatch requested = new CountDownLatch(1);
            AtomicReference<RequestedDeadlineMissedStatus> requestedStatus =
                    new AtomicReference<>();
            reader.setListener(
                    new DataReaderListenerBase() {
                        @Override
                        public void onRequestedDeadlineMissed(
                                DataReader<?> r, RequestedDeadlineMissedStatus s) {
                            requestedStatus.set(s);
                            requested.countDown();
                        }
                    },
                    StatusMask.of(StatusMask.REQUESTED_DEADLINE_MISSED));

            long until = System.nanoTime() + TimeUnit.SECONDS.toNanos(10);
            while (writer.getPublicationMatchedStatus().currentCount() == 0
                    || reader.getSubscriptionMatchedStatus().currentCount() == 0) {
                assertTrue(System.nanoTime() < until, "writer and reader did not match");
                Thread.sleep(10);
            }

            // One sample starts the instance's deadline; then the writer goes silent.
            writer.write(new ConformanceRecord());

            boolean offeredFired = offered.await(10, TimeUnit.SECONDS);
            boolean requestedFired = requested.await(10, TimeUnit.SECONDS);
            writer.setListener(null, null);
            reader.setListener(null, null);

            assertTrue(offeredFired, "onOfferedDeadlineMissed did not fire within 10s");
            assertTrue(requestedFired, "onRequestedDeadlineMissed did not fire within 10s");
            assertTrue(offeredStatus.get().totalCount() >= 1, "total_count " + offeredStatus.get().totalCount());
            assertEquals(16, offeredStatus.get().lastInstanceHandle().length);
            assertTrue(requestedStatus.get().totalCount() >= 1, "total_count " + requestedStatus.get().totalCount());
            assertEquals(16, requestedStatus.get().lastInstanceHandle().length);
        }
    }
}
