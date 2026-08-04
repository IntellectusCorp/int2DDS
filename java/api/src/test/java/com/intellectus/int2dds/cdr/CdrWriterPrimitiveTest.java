package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class CdrWriterPrimitiveTest {

    /** Bytes after the 4-byte encapsulation header. */
    private static byte[] payload(CdrWriter w) {
        byte[] all = w.toBytes();
        byte[] out = new byte[all.length - 4];
        System.arraycopy(all, 4, out, 0, out.length);
        return out;
    }

    @Test
    void boolIsOneByte() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBool(true);
            w.writeBool(false);
            assertArrayEquals(new byte[] {1, 0}, payload(w));
        }
    }

    @Test
    void littleEndianIntsAreWrittenLowByteFirst() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeI32(0x01020304);
            assertArrayEquals(new byte[] {0x04, 0x03, 0x02, 0x01}, payload(w));
        }
    }

    @Test
    void bigEndianIntsAreWrittenHighByteFirst() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, false, false)) {
            w.writeI32(0x01020304);
            assertArrayEquals(new byte[] {0x01, 0x02, 0x03, 0x04}, payload(w));
        }
    }

    @Test
    void alignmentPadsRelativeToTheStreamNotTheBuffer() {
        // CDR alignment counts from the start of the encapsulated stream, i.e.
        // AFTER the 4-byte header. A bool then an i32 must pad by 3, not by 7.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBool(true);
            w.writeI32(0x01020304);
            assertArrayEquals(
                    new byte[] {1, 0, 0, 0, 0x04, 0x03, 0x02, 0x01},
                    payload(w));
        }
    }

    @Test
    void paddingBytesAreZeroEvenInAReusedBuffer() {
        // Reuse the pool slot, then check the pad bytes are zeros and not the
        // previous sample's content.
        try (CdrWriter dirty = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            dirty.writeI64(0x7F7F7F7F7F7F7F7FL);
            dirty.writeI64(0x7F7F7F7F7F7F7F7FL);
        }
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBool(true);
            w.writeI32(1);
            byte[] p = payload(w);
            assertEquals(0, p[1], "pad byte 1");
            assertEquals(0, p[2], "pad byte 2");
            assertEquals(0, p[3], "pad byte 3");
        }
    }

    @Test
    void xcdr2CapsAlignmentAtFour() {
        // XCDR2 caps max alignment at 4, so an i64 after a bool pads by 3, not 7.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            w.writeBool(true);
            w.writeI64(1L);
            assertEquals(1 + 3 + 8, payload(w).length);
        }
    }

    @Test
    void xcdr1DoesNotCapAlignment() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBool(true);
            w.writeI64(1L);
            assertEquals(1 + 7 + 8, payload(w).length);
        }
    }

    @Test
    void sixteenBitAndSixtyFourBitAlignToTheirOwnWidth() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeI8((byte) 1);
            w.writeI16((short) 0x0203);
            assertArrayEquals(new byte[] {1, 0, 0x03, 0x02}, payload(w));
        }
    }

    @Test
    void unsignedWritersTruncateToTheirWidth() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeU8(0xFF);
            w.writeU16(0xFFFF);
            byte[] p = payload(w);
            assertEquals((byte) 0xFF, p[0]);
            assertEquals((byte) 0xFF, p[2]);
            assertEquals((byte) 0xFF, p[3]);
        }
    }

    @Test
    void floatsAreWrittenAsTheirIeeeBits() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeF32(1.0f);   // 0x3F800000
            assertArrayEquals(new byte[] {0x00, 0x00, (byte) 0x80, 0x3F}, payload(w));
        }
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeF64(1.0d);   // 0x3FF0000000000000
            byte[] p = payload(w);
            assertEquals(8, p.length);
            assertEquals((byte) 0xF0, p[6]);
            assertEquals((byte) 0x3F, p[7]);
        }
    }
}
