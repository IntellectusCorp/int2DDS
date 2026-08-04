package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;

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
    void dheaderIsANoOpUnderXcdr1() {
        // Version-agnostic caller code: same readDheader/readDheaderEnd
        // sequence as the XCDR2 test above, but against an XCDR1 writer.
        // CdrWriter.dheaderBegin() returns -1 and dheaderFinalize() is a
        // no-op under XCDR1; the reader must mirror that so nothing is
        // consumed and the payload survives untouched.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, false)) {
            int t = w.dheaderBegin();
            w.writeI32(7);
            w.writeI32(8);
            w.dheaderFinalize(t);

            CdrReader r = CdrReader.of(w.toBytes());
            assertFalse(r.isXcdr2());
            CdrReader.Dheader d = r.readDheader();
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

    // ---- Overflow and protocol correctness tests ----------------------------

    @Test
    void dheaderWithNegativeObjectSizeThrows() {
        // Hand-build a DHEADER with negative objectSize.
        ByteBuffer buf = ByteBuffer.allocate(16);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x07);           // encapsulation ID low byte (CDR2 LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        // Now data in little-endian
        buf.order(ByteOrder.LITTLE_ENDIAN);
        buf.putInt(-1);                 // negative objectSize
        buf.putInt(0);                  // padding/data
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        CdrReader.Dheader d = r.readDheader();
        assertThrows(CdrUnderflowException.class, () -> r.readDheaderEnd(d),
                "Negative objectSize should throw, not move position backward");
    }

    @Test
    void dheaderWithLargeObjectSizeThrows() {
        // Hand-build a DHEADER with objectSize near Integer.MAX_VALUE.
        ByteBuffer buf = ByteBuffer.allocate(16);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x07);           // encapsulation ID low byte (CDR2 LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        buf.order(ByteOrder.LITTLE_ENDIAN);
        buf.putInt(Integer.MAX_VALUE);  // huge objectSize
        buf.putInt(0);
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        CdrReader.Dheader d = r.readDheader();
        assertThrows(CdrUnderflowException.class, () -> r.readDheaderEnd(d),
                "Large objectSize causing overflow should throw");
    }

    @Test
    void emheaderWithLargeNextintThrows() {
        // Hand-build an EMHEADER with LC 4 and a NEXTINT that would overflow.
        ByteBuffer buf = ByteBuffer.allocate(16);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x07);           // encapsulation ID low byte (CDR2 LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        buf.order(ByteOrder.LITTLE_ENDIAN);
        // EMHEADER: memberId=1, LC=4, mustUnderstand=0
        buf.putInt(0x40000001);         // LC 4 in bits [30:28]
        buf.putInt(0x7FFFFFFF);         // NEXTINT: huge, exceeds buffer
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        assertThrows(CdrUnderflowException.class, () -> r.readEmheader(),
                "NEXTINT overflow should throw, not wrap and pass");
    }

    @Test
    void emheaderWithLc6PeeksNextint() {
        // Hand-build EMHEADER with LC 6 and a small element count.
        // LC 6: length = 4 + 4 * nextInt, NEXTINT is peeked (not consumed).
        ByteBuffer buf = ByteBuffer.allocate(32);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x07);           // encapsulation ID low byte (CDR2 LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        buf.order(ByteOrder.LITTLE_ENDIAN);
        // EMHEADER: memberId=1, LC=6, mustUnderstand=0
        // LC=6 is bits [30:28] = 110 = 6
        buf.putInt(0x60000001);         // LC 6
        buf.putInt(2);                  // nextInt: 2 elements (4 + 4*2 = 12 bytes data)
        buf.putInt(10);                 // element 1
        buf.putInt(20);                 // element 2
        buf.putInt(0);                  // padding
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        int posBeforeRead = r.position();
        CdrReader.Emheader e = r.readEmheader();
        int posAfterRead = r.position();

        assertEquals(1, e.memberId);
        assertEquals(4 + 4 * 2, e.dataLength, "Length should be 4 + 4*2");
        assertEquals(posBeforeRead + 4, posAfterRead,
                "Position should only advance past header, not past the peeked NEXTINT");
    }

    @Test
    void emheaderWithLc7PeeksNextint() {
        // Hand-build EMHEADER with LC 7 and a small element count.
        // LC 7: length = 4 + 8 * nextInt, NEXTINT is peeked.
        ByteBuffer buf = ByteBuffer.allocate(36);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x07);           // encapsulation ID low byte (CDR2 LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        buf.order(ByteOrder.LITTLE_ENDIAN);
        // EMHEADER: memberId=2, LC=7, mustUnderstand=0
        // LC=7 is bits [30:28] = 111 = 7
        buf.putInt(0x70000002);         // LC 7
        buf.putInt(2);                  // nextInt: 2 elements (4 + 8*2 = 20 bytes data)
        buf.putLong(100L);              // element 1
        buf.putLong(200L);              // element 2
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        int posBeforeRead = r.position();
        CdrReader.Emheader e = r.readEmheader();
        int posAfterRead = r.position();

        assertEquals(2, e.memberId);
        assertEquals(4 + 8 * 2, e.dataLength, "Length should be 4 + 8*2");
        assertEquals(posBeforeRead + 4, posAfterRead,
                "Position should only advance past header, not past the peeked NEXTINT");
    }

    @Test
    void emheaderWithLc5PeeksNextint() {
        // Hand-build EMHEADER with LC 5.
        // LC 5: length is the NEXTINT value itself, peeked.
        ByteBuffer buf = ByteBuffer.allocate(20);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x07);           // encapsulation ID low byte (CDR2 LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        buf.order(ByteOrder.LITTLE_ENDIAN);
        // EMHEADER: memberId=5, LC=5, mustUnderstand=0
        // LC=5 is bits [30:28] = 101 = 5
        buf.putInt(0x50000005);         // LC 5
        buf.putInt(8);                  // nextInt: data length is 8 bytes
        buf.putLong(42L);               // the actual data
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        int posBeforeRead = r.position();
        CdrReader.Emheader e = r.readEmheader();
        int posAfterRead = r.position();

        assertEquals(5, e.memberId);
        assertEquals(8, e.dataLength, "Length should be the NEXTINT value");
        assertEquals(posBeforeRead + 4, posAfterRead,
                "Position should only advance past header, not past the peeked NEXTINT");
    }

    @Test
    void plCdrExtendedFormWithNegativeLengthThrows() {
        // Hand-build extended PL_CDR header with negative length.
        ByteBuffer buf = ByteBuffer.allocate(20);
        buf.put((byte) 0x00);           // encapsulation ID high byte
        buf.put((byte) 0x03);           // encapsulation ID low byte (PL_CDR LE)
        buf.put((byte) 0x00);           // options
        buf.put((byte) 0x00);           // options
        buf.order(ByteOrder.LITTLE_ENDIAN);
        // PID=0x3F01 (extended form), length placeholder
        buf.putShort((short) 0x3F01);   // PID: extended form
        buf.putShort((short) 0);        // length: not used for extended form
        buf.putInt(42);                 // fullId
        buf.putInt(-1);                 // fullLen: negative!
        buf.flip();

        CdrReader r = CdrReader.of(buf);
        assertThrows(CdrUnderflowException.class, () -> r.readParameterHeader(),
                "Negative length in extended form should throw");
    }
}
