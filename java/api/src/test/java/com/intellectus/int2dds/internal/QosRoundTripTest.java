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
    }

    @Test
    void readerPoliciesSurviveTheNativeRoundTrip() {
        DataReaderQos sent = new DataReaderQos();
        sent.setReliability(new Reliability(ReliabilityKind.BEST_EFFORT));
        sent.setHistory(new History(HistoryKind.KEEP_ALL, 1));
        sent.setDurability(new Durability(DurabilityKind.VOLATILE));

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
