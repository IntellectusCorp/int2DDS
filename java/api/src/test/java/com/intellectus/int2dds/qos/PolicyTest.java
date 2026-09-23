package com.intellectus.int2dds.qos;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.time.Duration;
import org.junit.jupiter.api.Test;

class PolicyTest {

    @Test
    void enumWireValuesAreExplicitNotOrdinals() {
        // DataRepresentationKind skips 1. Using ordinal() would send 1 for
        // XCDR2 and negotiate the wrong encoding.
        assertEquals(0, DataRepresentationKind.XCDR1.value());
        assertEquals(2, DataRepresentationKind.XCDR2.value());
        assertEquals(1, DataRepresentationKind.XCDR2.ordinal(),
                "precondition: ordinal and wire value genuinely differ here");
    }

    @Test
    void everyEnumRoundTripsThroughItsWireValue() {
        for (ReliabilityKind k : ReliabilityKind.values()) {
            assertEquals(k, ReliabilityKind.fromValue(k.value()));
        }
        for (DurabilityKind k : DurabilityKind.values()) {
            assertEquals(k, DurabilityKind.fromValue(k.value()));
        }
        for (HistoryKind k : HistoryKind.values()) {
            assertEquals(k, HistoryKind.fromValue(k.value()));
        }
        for (OwnershipKind k : OwnershipKind.values()) {
            assertEquals(k, OwnershipKind.fromValue(k.value()));
        }
        for (DestinationOrderKind k : DestinationOrderKind.values()) {
            assertEquals(k, DestinationOrderKind.fromValue(k.value()));
        }
        for (LivelinessKind k : LivelinessKind.values()) {
            assertEquals(k, LivelinessKind.fromValue(k.value()));
        }
        for (DataRepresentationKind k : DataRepresentationKind.values()) {
            assertEquals(k, DataRepresentationKind.fromValue(k.value()));
        }
    }

    @Test
    void anUnknownWireValueIsRejected() {
        assertThrows(IllegalArgumentException.class, () -> ReliabilityKind.fromValue(99));
    }

    @Test
    void defaultsMatchTheDocumentedTable() {
        assertEquals(ReliabilityKind.RELIABLE, new Reliability().getKind());
        assertEquals(DurabilityKind.VOLATILE, new Durability().getKind());
        assertEquals(HistoryKind.KEEP_LAST, new History().getKind());
        assertEquals(1, new History().getDepth());
        assertEquals(OwnershipKind.SHARED, new Ownership().getKind());
        assertEquals(-1, new ResourceLimits().getMaxSamples());
        assertEquals(DestinationOrderKind.BY_RECEPTION, new DestinationOrder().getKind());
        assertEquals(DataRepresentationKind.XCDR1, new DataRepresentation().getKind());
        assertTrue(new WriterDataLifecycle().isAutodisposeUnregisteredInstances());
        assertEquals(0, new UserData().getData().length);
        assertEquals(0, new Partition().getNames().length);
        assertTrue(new Property().getEntries().isEmpty());
    }

    @Test
    void nullDurationsFallBackToTheDocumentedNanosecondValue() {
        assertEquals(100_000_000L, new Reliability().maxBlockingTimeNs());
        assertEquals(Long.MAX_VALUE, new Lifespan().durationNs());
        assertEquals(0L, new LatencyBudget().durationNs());
        assertEquals(0L, new TimeBasedFilter().minimumSeparationNs());
        assertEquals(Long.MAX_VALUE, new Deadline().periodNs());
        assertEquals(Long.MAX_VALUE, new Liveliness().leaseDurationNs());
        assertEquals(Long.MAX_VALUE, new ReaderDataLifecycle().autopurgeNowriterNs());
        assertEquals(Long.MAX_VALUE, new ReaderDataLifecycle().autopurgeDisposedNs());
    }

    @Test
    void setDurationsConvertExactlyIncludingSubMilliseconds() {
        // C# routes this through TotalMilliseconds as a double and loses
        // sub-millisecond precision. Duration.toNanos does not.
        Reliability r = new Reliability();
        r.setMaxBlockingTime(Duration.ofNanos(1_500L));
        assertEquals(1_500L, r.maxBlockingTimeNs());

        Deadline d = new Deadline();
        d.setPeriod(Duration.ofMillis(250));
        assertEquals(250_000_000L, d.periodNs());
    }

    @Test
    void policiesCompareByValue() {
        assertEquals(new History(HistoryKind.KEEP_LAST, 10), new History(HistoryKind.KEEP_LAST, 10));
        assertEquals(new History(HistoryKind.KEEP_LAST, 10).hashCode(),
                new History(HistoryKind.KEEP_LAST, 10).hashCode());
        assertNotEquals(new History(HistoryKind.KEEP_LAST, 10),
                new History(HistoryKind.KEEP_ALL, 10));
    }

    @Test
    void arrayValuedPoliciesCompareByContent() {
        UserData a = new UserData(new byte[] {1, 2, 3});
        UserData b = new UserData(new byte[] {1, 2, 3});
        assertEquals(a, b, "byte[] must compare by content, not identity");
        assertEquals(a.hashCode(), b.hashCode());

        Partition p = new Partition(new String[] {"x", "y"});
        Partition q = new Partition(new String[] {"x", "y"});
        assertEquals(p, q);
    }

    @Test
    void toStringNamesTheTypeAndItsValues() {
        String s = new History(HistoryKind.KEEP_LAST, 5).toString();
        assertTrue(s.contains("History"), s);
        assertTrue(s.contains("KEEP_LAST"), s);
        assertTrue(s.contains("5"), s);
    }

    @Test
    void propertyEntriesAreImmutableAndMulticastTtlReplacesRatherThanAppends() {
        Property p = new Property();
        p.add("a", "1", true);
        p.setMulticastTtl(4);
        p.setMulticastTtl(8);

        int ttlCount = 0;
        String ttlValue = null;
        for (PropertyEntry e : p.getEntries()) {
            if (Property.MULTICAST_TTL_NAME.equals(e.getName())) {
                ttlCount++;
                ttlValue = e.getValue();
            }
        }
        assertEquals(1, ttlCount, "setting the TTL twice must not leave two entries");
        assertEquals("8", ttlValue);
        assertEquals(2, p.getEntries().size());
    }

    @Test
    void userDataDefensivelyCopiesSoLaterMutationDoesNotLeakIn() {
        byte[] src = {1, 2, 3};
        UserData u = new UserData(src);
        src[0] = 9;
        assertArrayEquals(new byte[] {1, 2, 3}, u.getData(),
                "the policy must not alias the caller's array");
    }
}
