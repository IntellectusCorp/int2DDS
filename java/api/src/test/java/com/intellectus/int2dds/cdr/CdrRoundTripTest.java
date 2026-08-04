package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class CdrRoundTripTest {

    private static CdrReader reread(CdrWriter w) {
        return CdrReader.of(w.toBytes());
    }

    @Test
    void primitivesRoundTripInEveryCombination() {
        for (boolean le : new boolean[] {true, false}) {
            for (boolean x2 : new boolean[] {true, false}) {
                try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, le, x2)) {
                    w.writeBool(true);
                    w.writeI8((byte) -8);
                    w.writeU8(200);
                    w.writeI16((short) -1600);
                    w.writeU16(60000);
                    w.writeI32(-320000);
                    w.writeI64(-64000000000L);
                    w.writeF32(2.5f);
                    w.writeF64(-3.25d);

                    CdrReader r = reread(w);
                    String at = "le=" + le + " xcdr2=" + x2;
                    assertTrue(r.readBool(), at);
                    assertEquals((byte) -8, r.readI8(), at);
                    assertEquals(200, r.readU8(), at);
                    assertEquals((short) -1600, r.readI16(), at);
                    assertEquals(60000, r.readU16(), at);
                    assertEquals(-320000, r.readI32(), at);
                    assertEquals(-64000000000L, r.readI64(), at);
                    assertEquals(2.5f, r.readF32(), 0.0f, at);
                    assertEquals(-3.25d, r.readF64(), 0.0d, at);
                    assertEquals(0, r.remaining(), at);
                }
            }
        }
    }

    @Test
    void stringsRoundTripIncludingNonAsciiAndSupplementary() {
        String[] cases = {"", "abc", "센서/온도", "🌡", "a\0b", "mixed 한글 and ASCII"};
        for (String s : cases) {
            try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
                w.writeString(s);
                assertEquals(s, reread(w).readString(), "case: " + s);
            }
        }
    }

    @Test
    void wstringsRoundTrip() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeWString("ab한");
            assertEquals("ab한", reread(w).readWString());
        }
    }

    @Test
    void sequencesAndRawBytesRoundTrip() {
        byte[] blob = {1, 2, 3, 4, 5};
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeSeqHeader(blob.length);
            w.writeBytes(blob);
            CdrReader r = reread(w);
            assertEquals(blob.length, r.readSeqHeader());
            assertArrayEquals(blob, r.readBytes(blob.length));
        }
    }

    @Test
    void enumsRoundTripIncludingNegatives() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeEnum(-1);
            w.writeEnum(0);
            w.writeEnum(7);
            CdrReader r = reread(w);
            assertEquals(-1, r.readEnum());
            assertEquals(0, r.readEnum());
            assertEquals(7, r.readEnum());
        }
    }

    @Test
    void aMixedRecordRoundTripsWithItsAlignmentIntact() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBool(true);
            w.writeI32(42);
            w.writeString("hello");
            w.writeF64(3.5d);
            w.writeI16((short) 9);

            CdrReader r = reread(w);
            assertTrue(r.readBool());
            assertEquals(42, r.readI32());
            assertEquals("hello", r.readString());
            assertEquals(3.5d, r.readF64(), 0.0d);
            assertEquals((short) 9, r.readI16());
            assertEquals(0, r.remaining());
        }
    }

    @Test
    void readBytesPastTheEndUnderflows() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBytes(new byte[] {1, 2});
            CdrReader r = reread(w);
            assertThrows(CdrUnderflowException.class, () -> r.readBytes(3));
        }
    }

    @Test
    void aTruncatedStringLengthUnderflowsRatherThanReadingGarbage() {
        // Length prefix claims 100 bytes, buffer holds 2.
        CdrReader r = CdrReader.of(new byte[] {
                0x00, 0x01, 0x00, 0x00,
                0x64, 0x00, 0x00, 0x00,
                'a', 'b'});
        assertThrows(CdrUnderflowException.class, r::readString);
    }
}
