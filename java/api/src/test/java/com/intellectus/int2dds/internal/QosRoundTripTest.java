package com.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.*;
import java.time.Duration;
import org.junit.jupiter.api.Test;

class QosRoundTripTest {

    @Test
    void writerPoliciesSurviveTheNativeRoundTrip() {
        DataWriterQos sent = new DataWriterQos();
        sent.setReliability(new Reliability(ReliabilityKind.RELIABLE, Duration.ofMillis(250)));
        sent.setDurability(new Durability(DurabilityKind.TRANSIENT_LOCAL));
        sent.setHistory(new History(HistoryKind.KEEP_LAST, 10));
        sent.setOwnership(new Ownership(OwnershipKind.EXCLUSIVE));
        sent.setOwnershipStrength(new OwnershipStrength(7));
        sent.setResourceLimits(new ResourceLimits(100, 10, 5));
        sent.setTransportPriority(new TransportPriority(3));
        sent.setDestinationOrder(new DestinationOrder(DestinationOrderKind.BY_SOURCE));
        sent.setLifespan(new Lifespan(Duration.ofSeconds(30)));
        sent.setLatencyBudget(new LatencyBudget(Duration.ofMillis(15)));
        sent.setDeadline(new Deadline(Duration.ofSeconds(5)));
        sent.setLiveliness(
                new Liveliness(LivelinessKind.MANUAL_BY_PARTICIPANT, Duration.ofSeconds(9)));
        // The field's default is true; sending false is the only value that
        // actually proves the *mut bool byte was read rather than assumed.
        sent.setWriterDataLifecycle(new WriterDataLifecycle(false));
        sent.setDataFrag(4096);

        long h = FfiAccess.createDataWriterQos();
        DataWriterQos back;
        try {
            QosMarshal.applyWriterQos(h, sent);
            back = QosMarshal.readWriterQos(h);
        } finally {
            FfiAccess.destroyDataWriterQos(h);
        }

        assertNotNull(back);
        assertEquals(ReliabilityKind.RELIABLE, back.getReliability().getKind());
        assertEquals(Duration.ofMillis(250), back.getReliability().getMaxBlockingTime());
        assertEquals(DurabilityKind.TRANSIENT_LOCAL, back.getDurability().getKind());
        assertEquals(HistoryKind.KEEP_LAST, back.getHistory().getKind());
        assertEquals(10, back.getHistory().getDepth());
        assertEquals(OwnershipKind.EXCLUSIVE, back.getOwnership().getKind());
        assertEquals(7, back.getOwnershipStrength().getValue());
        assertEquals(100, back.getResourceLimits().getMaxSamples());
        assertEquals(10, back.getResourceLimits().getMaxInstances());
        assertEquals(5, back.getResourceLimits().getMaxSamplesPerInstance());
        assertEquals(3, back.getTransportPriority().getValue());
        assertEquals(DestinationOrderKind.BY_SOURCE, back.getDestinationOrder().getKind());
        assertEquals(Duration.ofSeconds(30), back.getLifespan().getDuration());
        assertEquals(Duration.ofMillis(15), back.getLatencyBudget().getDuration());
        assertEquals(Duration.ofSeconds(5), back.getDeadline().getPeriod());
        assertEquals(LivelinessKind.MANUAL_BY_PARTICIPANT, back.getLiveliness().getKind());
        assertEquals(Duration.ofSeconds(9), back.getLiveliness().getLeaseDuration());
        assertEquals(
                false, back.getWriterDataLifecycle().isAutodisposeUnregisteredInstances());
        assertEquals(4096, back.getDataFrag());
    }

    @Test
    void readerPoliciesSurviveTheNativeRoundTrip() {
        DataReaderQos sent = new DataReaderQos();
        sent.setReliability(new Reliability(ReliabilityKind.BEST_EFFORT));
        sent.setHistory(new History(HistoryKind.KEEP_ALL, 1));
        sent.setDurability(new Durability(DurabilityKind.VOLATILE));
        sent.setTimeBasedFilter(new TimeBasedFilter(Duration.ofMillis(50)));
        sent.setLatencyBudget(new LatencyBudget(Duration.ofMillis(20)));
        // Distinct, asymmetric values so a transposition between the two
        // delays is visible rather than silently passing.
        sent.setReaderDataLifecycle(
                new ReaderDataLifecycle(Duration.ofSeconds(12), Duration.ofSeconds(34)));
        sent.setDeadline(new Deadline(Duration.ofSeconds(7)));
        sent.setLiveliness(new Liveliness(LivelinessKind.MANUAL_BY_TOPIC, Duration.ofSeconds(11)));

        long h = FfiAccess.createDataReaderQos();
        DataReaderQos back;
        try {
            QosMarshal.applyReaderQos(h, sent);
            back = QosMarshal.readReaderQos(h);
        } finally {
            FfiAccess.destroyDataReaderQos(h);
        }

        assertEquals(ReliabilityKind.BEST_EFFORT, back.getReliability().getKind());
        assertEquals(HistoryKind.KEEP_ALL, back.getHistory().getKind());
        assertEquals(DurabilityKind.VOLATILE, back.getDurability().getKind());
        assertEquals(Duration.ofMillis(50), back.getTimeBasedFilter().getMinimumSeparation());
        assertEquals(Duration.ofMillis(20), back.getLatencyBudget().getDuration());
        assertEquals(
                Duration.ofSeconds(12),
                back.getReaderDataLifecycle().getAutopurgeNowriterSamplesDelay());
        assertEquals(
                Duration.ofSeconds(34),
                back.getReaderDataLifecycle().getAutopurgeDisposedSamplesDelay());
        assertEquals(Duration.ofSeconds(7), back.getDeadline().getPeriod());
        assertEquals(LivelinessKind.MANUAL_BY_TOPIC, back.getLiveliness().getKind());
        assertEquals(Duration.ofSeconds(11), back.getLiveliness().getLeaseDuration());
    }

    @Test
    void readingAFreshHandleGivesTheCoreDefaultsNotNulls() {
        // A getter that exists always reports something, so read-back on an
        // untouched handle shows what the core's defaults actually are.
        long h = FfiAccess.createDataWriterQos();
        DataWriterQos back;
        try {
            back = QosMarshal.readWriterQos(h);
        } finally {
            FfiAccess.destroyDataWriterQos(h);
        }
        assertNotNull(back.getReliability(), "reliability has a getter, so it reads back");
        assertNotNull(back.getHistory());
        assertNotNull(back.getDurability());
    }

    @Test
    void enumValuesRoundTripByWireValueNotOrdinal() {
        // XCDR2 is wire value 2 and ordinal 1. If either direction used
        // ordinal(), this comes back as XCDR1.
        DataWriterQos sent = new DataWriterQos();
        sent.setDataRepresentation(new DataRepresentation(DataRepresentationKind.XCDR2));

        long h = FfiAccess.createDataWriterQos();
        DataWriterQos back;
        try {
            QosMarshal.applyWriterQos(h, sent);
            back = QosMarshal.readWriterQos(h);
        } finally {
            FfiAccess.destroyDataWriterQos(h);
        }
        assertEquals(DataRepresentationKind.XCDR2, back.getDataRepresentation().getKind());
    }
}
