package com.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertNotEquals;

import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.*;
import java.time.Duration;
import java.util.function.LongConsumer;
import org.junit.jupiter.api.Test;

class QosMarshalTest {

    /** Runs {@code body} against a fresh native writer QoS handle, always freeing it. */
    private static void withWriterQos(LongConsumer body) {
        long handle = FfiAccess.createDataWriterQos();
        assertNotEquals(0L, handle, "the FFI must hand back a QoS handle");
        try {
            body.accept(handle);
        } finally {
            FfiAccess.destroyDataWriterQos(handle);
        }
    }

    @Test
    void anEmptyQosAppliesNothingAndSucceeds() {
        // Every field null means "leave it to the core" — this must be a no-op,
        // not a call storm that overwrites core defaults with Java's idea of them.
        withWriterQos(h -> assertDoesNotThrow(
                () -> QosMarshal.applyWriterQos(h, new DataWriterQos())));
    }

    @Test
    void aFullyPopulatedWriterQosApplies() {
        DataWriterQos q = new DataWriterQos();
        q.setReliability(new Reliability(ReliabilityKind.RELIABLE, Duration.ofMillis(250)));
        q.setDurability(new Durability(DurabilityKind.TRANSIENT_LOCAL));
        q.setHistory(new History(HistoryKind.KEEP_LAST, 10));
        q.setOwnership(new Ownership(OwnershipKind.EXCLUSIVE));
        q.setOwnershipStrength(new OwnershipStrength(7));
        q.setResourceLimits(new ResourceLimits(100, 10, 10));
        q.setLifespan(new Lifespan(Duration.ofSeconds(5)));
        q.setDestinationOrder(new DestinationOrder(DestinationOrderKind.BY_SOURCE));
        q.setLatencyBudget(new LatencyBudget(Duration.ofMillis(1)));
        q.setTransportPriority(new TransportPriority(3));
        q.setUserData(new UserData(new byte[] {1, 2, 3}));
        q.setWriterDataLifecycle(new WriterDataLifecycle(false));
        q.setDataRepresentation(new DataRepresentation(DataRepresentationKind.XCDR2));
        q.setDeadline(new Deadline(Duration.ofSeconds(2)));
        q.setLiveliness(new Liveliness(LivelinessKind.MANUAL_BY_TOPIC, Duration.ofSeconds(3)));

        withWriterQos(h -> assertDoesNotThrow(() -> QosMarshal.applyWriterQos(h, q)));
    }

    @Test
    void aFullyPopulatedReaderQosApplies() {
        DataReaderQos q = new DataReaderQos();
        q.setReliability(new Reliability(ReliabilityKind.BEST_EFFORT));
        q.setDurability(new Durability(DurabilityKind.VOLATILE));
        q.setHistory(new History(HistoryKind.KEEP_ALL, 1));
        q.setTimeBasedFilter(new TimeBasedFilter(Duration.ofMillis(50)));
        q.setReaderDataLifecycle(
                new ReaderDataLifecycle(Duration.ofSeconds(1), Duration.ofSeconds(2)));

        long h = FfiAccess.createDataReaderQos();
        try {
            assertDoesNotThrow(() -> QosMarshal.applyReaderQos(h, q));
        } finally {
            FfiAccess.destroyDataReaderQos(h);
        }
    }

    @Test
    void participantPublisherSubscriberAndTopicQosApply() {
        ParticipantQos p = new ParticipantQos();
        p.setUserData(new UserData(new byte[] {9}));
        Property prop = new Property();
        prop.setMulticastTtl(4);
        p.setProperty(prop);
        long ph = FfiAccess.createParticipantQos();
        try {
            assertDoesNotThrow(() -> QosMarshal.applyParticipantQos(ph, p));
        } finally {
            FfiAccess.destroyParticipantQos(ph);
        }

        PublisherQos pub = new PublisherQos();
        pub.setPartition(new Partition(new String[] {"a", "b"}));
        long pubh = FfiAccess.createPublisherQos();
        try {
            assertDoesNotThrow(() -> QosMarshal.applyPublisherQos(pubh, pub));
        } finally {
            FfiAccess.destroyPublisherQos(pubh);
        }

        SubscriberQos sub = new SubscriberQos();
        sub.setPartition(new Partition(new String[] {"a"}));
        long subh = FfiAccess.createSubscriberQos();
        try {
            assertDoesNotThrow(() -> QosMarshal.applySubscriberQos(subh, sub));
        } finally {
            FfiAccess.destroySubscriberQos(subh);
        }

        TopicQos t = new TopicQos();
        t.setReliability(new Reliability(ReliabilityKind.RELIABLE));
        t.setHistory(new History(HistoryKind.KEEP_LAST, 3));
        long th = FfiAccess.createTopicQos();
        try {
            assertDoesNotThrow(() -> QosMarshal.applyTopicQos(th, t));
        } finally {
            FfiAccess.destroyTopicQos(th);
        }
    }

    @Test
    void nonAsciiPartitionAndPropertyNamesSurvive() {
        // These cross as UTF-8 byte[]; the point is that they do not throw and
        // do not get mangled on the way in.
        PublisherQos pub = new PublisherQos();
        pub.setPartition(new Partition(new String[] {"센서", "온도🌡"}));
        long h = FfiAccess.createPublisherQos();
        try {
            assertDoesNotThrow(() -> QosMarshal.applyPublisherQos(h, pub));
        } finally {
            FfiAccess.destroyPublisherQos(h);
        }
    }
}
