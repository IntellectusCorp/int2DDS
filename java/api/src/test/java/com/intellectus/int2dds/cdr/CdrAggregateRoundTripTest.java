package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class CdrAggregateRoundTripTest {

    @Test
    void dheaderRoundTripsAndReportsItsExtent() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            int t = w.dheaderBegin();
            w.writeI32(7);
            w.writeI32(8);
            w.dheaderFinalize(t);

            CdrReader r = CdrReader.of(w.toBytes());
            CdrReader.Dheader d = r.readDheader();
            assertEquals(8, d.objectSize);
            assertEquals(7, r.readI32());
            assertEquals(8, r.readI32());
            r.readDheaderEnd(d);
            assertEquals(0, r.remaining());
        }
    }

    @Test
    void unknownTrailingMembersAreSkippedByDheaderEnd() {
        // A newer writer appended a field this reader does not know about.
        // APPENDABLE exists so that reader can still move on cleanly.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            int t = w.dheaderBegin();
            w.writeI32(1);
            w.writeI32(999);   // the "unknown" member
            w.dheaderFinalize(t);
            w.writeI32(42);    // something after the aggregate

            CdrReader r = CdrReader.of(w.toBytes());
            CdrReader.Dheader d = r.readDheader();
            assertEquals(1, r.readI32());
            r.readDheaderEnd(d);          // skip the rest without decoding it
            assertEquals(42, r.readI32());
        }
    }

    @Test
    void emheaderRoundTripsWithItsFlagsAndLength() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            int t = w.emheaderBegin(11, true);
            w.writeI64(5L);
            w.emheaderFinalize(t);

            CdrReader r = CdrReader.of(w.toBytes());
            CdrReader.Emheader e = r.readEmheader();
            assertEquals(11, e.memberId);
            assertEquals(8, e.dataLength);
            assertTrue(e.mustUnderstand);
            assertEquals(5L, r.readI64());
        }
    }

    @Test
    void sentinelIsDetectedWithoutConsumingIt() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            int t = w.emheaderBegin(1, false);
            w.writeI32(3);
            w.emheaderFinalize(t);
            w.writeSentinel();

            CdrReader r = CdrReader.of(w.toBytes());
            assertFalse(r.isSentinel(), "positioned at a real member");
            CdrReader.Emheader e = r.readEmheader();
            assertEquals(1, e.memberId);
            assertEquals(3, r.readI32());
            assertTrue(r.isSentinel(), "now at the sentinel");
            assertTrue(r.isSentinel(), "peeking twice does not consume");
        }
    }

    @Test
    void plCdrShortMemberRoundTrips() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(6);
            w.writeI32(77);
            w.memberV1Finalize(h, 6, true);
            w.endMutableStruct();

            CdrReader r = CdrReader.of(w.toBytes());
            CdrReader.ParameterHeader p = r.readParameterHeader();
            assertEquals(6, p.memberId);
            assertEquals(4, p.length);
            assertTrue(p.mustUnderstand);
            assertFalse(p.sentinel);
            assertEquals(77, r.readI32());
            assertTrue(r.readParameterHeader().sentinel);
        }
    }

    @Test
    void plCdrExtendedMemberRoundTrips() {
        int id = 0x3F00 + 5;
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(id);
            w.writeI32(88);
            w.memberV1Finalize(h, id, false);
            w.endMutableStruct();

            CdrReader r = CdrReader.of(w.toBytes());
            CdrReader.ParameterHeader p = r.readParameterHeader();
            assertEquals(id, p.memberId, "the extended form carries the full 32-bit id");
            assertEquals(4, p.length);
            assertEquals(88, r.readI32());
        }
    }

    @Test
    void aMutableStructWithSeveralMembersRoundTrips() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            int a = w.emheaderBegin(1, false);
            w.writeI32(10);
            w.emheaderFinalize(a);
            int b = w.emheaderBegin(2, true);
            w.writeString("x");
            w.emheaderFinalize(b);
            w.writeSentinel();

            CdrReader r = CdrReader.of(w.toBytes());
            assertEquals(1, r.readEmheader().memberId);
            assertEquals(10, r.readI32());
            CdrReader.Emheader second = r.readEmheader();
            assertEquals(2, second.memberId);
            assertTrue(second.mustUnderstand);
            assertEquals("x", r.readString());
            assertTrue(r.isSentinel());
        }
    }
}
