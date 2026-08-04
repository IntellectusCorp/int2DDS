package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import org.junit.jupiter.api.Test;

class CdrReaderTest {

    @Test
    void littleEndianHeaderSelectsLittleEndianReads() {
        CdrReader r = CdrReader.of(new byte[] {0x00, 0x01, 0x00, 0x00, 0x04, 0x03, 0x02, 0x01});
        assertFalse(r.isXcdr2());
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void bigEndianHeaderSelectsBigEndianReads() {
        CdrReader r = CdrReader.of(new byte[] {0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04});
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void cdr2HeadersAreDetectedAsXcdr2() {
        for (int id : new int[] {0x0007, 0x0009, 0x000B}) {
            CdrReader r = CdrReader.of(new byte[] {0x00, (byte) id, 0x00, 0x00});
            assertTrue(r.isXcdr2(), "encap 0x" + Integer.toHexString(id));
        }
    }

    @Test
    void plCdrIsXcdr1NotXcdr2() {
        // PL_CDR is the XCDR1 mutable encoding. Treating it as XCDR2 would cap
        // alignment at 4 and misread every 8-byte field that follows.
        CdrReader r = CdrReader.of(new byte[] {0x00, 0x03, 0x00, 0x00});
        assertFalse(r.isXcdr2());
    }

    @Test
    void anUnknownEncapsulationIdIsRejected() {
        assertThrows(CdrInvalidEncapsulationException.class,
                () -> CdrReader.of(new byte[] {0x00, 0x7F, 0x00, 0x00}));
    }

    @Test
    void aBufferTooShortForTheHeaderIsRejected() {
        assertThrows(CdrUnderflowException.class,
                () -> CdrReader.of(new byte[] {0x00, 0x01, 0x00}));
    }

    @Test
    void alignmentSkipsPaddingMeasuredFromTheStream() {
        // bool at stream offset 0, then 3 pad bytes, then the i32.
        CdrReader r = CdrReader.of(new byte[] {
                0x00, 0x01, 0x00, 0x00,
                0x01, 0x00, 0x00, 0x00,
                0x04, 0x03, 0x02, 0x01});
        assertTrue(r.readBool());
        assertEquals(0x01020304, r.readI32());
        assertEquals(0, r.remaining());
    }

    @Test
    void readingPastTheEndUnderflows() {
        CdrReader r = CdrReader.of(new byte[] {0x00, 0x01, 0x00, 0x00, 0x01});
        assertEquals(1, r.readU8());
        assertThrows(CdrUnderflowException.class, r::readI32);
    }

    @Test
    void alignmentPastTheEndUnderflows() {
        CdrReader r = CdrReader.of(new byte[] {0x00, 0x01, 0x00, 0x00, 0x01, 0x00});
        assertEquals(1, r.readU8());
        assertThrows(CdrUnderflowException.class, r::readI32);
    }

    @Test
    void unsignedReadersWidenRatherThanTruncate() {
        CdrReader r = CdrReader.of(new byte[] {
                0x00, 0x01, 0x00, 0x00,
                (byte) 0xFF, 0x00,
                (byte) 0xFF, (byte) 0xFF});
        assertEquals(255, r.readU8());
        r.skip(1);
        assertEquals(65535, r.readU16());
    }

    @Test
    void aDirectBufferIsReadWithoutCopying() {
        // The receive path hands the reader a direct buffer wrapping a native
        // sample loan, so this is the shape that actually matters at runtime.
        ByteBuffer direct = ByteBuffer.allocateDirect(8);
        direct.put(new byte[] {0x00, 0x01, 0x00, 0x00, 0x04, 0x03, 0x02, 0x01});
        ((java.nio.Buffer) direct).position(0);
        CdrReader r = CdrReader.of(direct);
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void rawReaderStartsAtOffsetZeroWithNoHeader() {
        ByteBuffer b = ByteBuffer.wrap(new byte[] {0x04, 0x03, 0x02, 0x01});
        CdrReader r = CdrReader.ofRaw(b, true, false);
        assertEquals(0, r.position());
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void floatsRoundTripTheirBitPattern() {
        CdrReader r = CdrReader.of(new byte[] {
                0x00, 0x01, 0x00, 0x00,
                0x00, 0x00, (byte) 0x80, 0x3F});
        assertEquals(1.0f, r.readF32(), 0.0f);
    }

    @Test
    void heapBufferAtNonZeroPositionReadsFromTheWindowStart() {
        // Create a 20-byte array with a valid little-endian header and i32 at offset 10.
        byte[] buffer = new byte[20];
        buffer[10] = 0x00;
        buffer[11] = 0x01;
        buffer[12] = 0x00;
        buffer[13] = 0x00;
        buffer[14] = 0x04;
        buffer[15] = 0x03;
        buffer[16] = 0x02;
        buffer[17] = 0x01;

        ByteBuffer b = ByteBuffer.wrap(buffer);
        ((java.nio.Buffer) b).position(10);
        ((java.nio.Buffer) b).limit(18);

        CdrReader r = CdrReader.of(b);
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void directBufferAtNonZeroPositionReadsFromTheWindowStart() {
        // Direct buffer starting at position 10 within a larger allocated region.
        ByteBuffer large = ByteBuffer.allocateDirect(20);
        large.put(new byte[10]); // Write 10 padding bytes
        large.put(new byte[] {0x00, 0x01, 0x00, 0x00, 0x04, 0x03, 0x02, 0x01});
        ((java.nio.Buffer) large).position(10);
        ((java.nio.Buffer) large).limit(18);

        CdrReader r = CdrReader.of(large);
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void rawReaderAtNonZeroPositionReadsFromTheWindowStart() {
        // ofRaw with no header, positioned at offset 10 in a larger buffer.
        byte[] buffer = new byte[20];
        buffer[10] = 0x04;
        buffer[11] = 0x03;
        buffer[12] = 0x02;
        buffer[13] = 0x01;

        ByteBuffer b = ByteBuffer.wrap(buffer);
        ((java.nio.Buffer) b).position(10);
        ((java.nio.Buffer) b).limit(14);

        CdrReader r = CdrReader.ofRaw(b, true, false);
        assertEquals(0x01020304, r.readI32());
    }

    @Test
    void readerDoesNotDisturbCallerBuffer() {
        ByteBuffer b = ByteBuffer.wrap(new byte[] {0x00, 0x01, 0x00, 0x00, 0x04, 0x03, 0x02, 0x01});
        int originalPos = b.position();

        CdrReader r = CdrReader.of(b);
        r.readI32();

        assertEquals(originalPos, b.position(), "Reader should not modify caller's buffer position");
    }
}
