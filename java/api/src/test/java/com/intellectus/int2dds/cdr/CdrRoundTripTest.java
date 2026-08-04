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
        for (boolean le : new boolean[] {true, false}) {
            for (boolean x2 : new boolean[] {true, false}) {
                for (String s : cases) {
                    try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, le, x2)) {
                        w.writeString(s);
                        String at = "case: " + s + " (le=" + le + " xcdr2=" + x2 + ")";
                        assertEquals(s, reread(w).readString(), at);
                    }
                }
            }
        }
    }

    @Test
    void wstringsRoundTrip() {
        for (boolean le : new boolean[] {true, false}) {
            for (boolean x2 : new boolean[] {true, false}) {
                try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, le, x2)) {
                    w.writeWString("ab한");
                    String at = "le=" + le + " xcdr2=" + x2;
                    assertEquals("ab한", reread(w).readWString(), at);
                }
            }
        }
    }

    @Test
    void sequencesAndRawBytesRoundTrip() {
        byte[] blob = {1, 2, 3, 4, 5};
        for (boolean le : new boolean[] {true, false}) {
            for (boolean x2 : new boolean[] {true, false}) {
                try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, le, x2)) {
                    w.writeSeqHeader(blob.length);
                    w.writeBytes(blob);
                    CdrReader r = reread(w);
                    String at = "le=" + le + " xcdr2=" + x2;
                    assertEquals(blob.length, r.readSeqHeader(), at);
                    assertArrayEquals(blob, r.readBytes(blob.length), at);
                }
            }
        }
    }

    @Test
    void enumsRoundTripIncludingNegatives() {
        for (boolean le : new boolean[] {true, false}) {
            for (boolean x2 : new boolean[] {true, false}) {
                try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, le, x2)) {
                    w.writeEnum(-1);
                    w.writeEnum(0);
                    w.writeEnum(7);
                    CdrReader r = reread(w);
                    String at = "le=" + le + " xcdr2=" + x2;
                    assertEquals(-1, r.readEnum(), at);
                    assertEquals(0, r.readEnum(), at);
                    assertEquals(7, r.readEnum(), at);
                }
            }
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

    @Test
    void aWstringWithLargeUnitCountDoesNotAllocateBeforeBoundsCheckingIt() {
        // A wstring claiming 100,000,000 units (200 MB) against a tiny buffer.
        // The bounds check must fire before the allocation.
        // We assert on the message text, not just the exception type, because both the
        // pre-allocation guard and the per-unit require(2) inside readU16() throw
        // CdrUnderflowException. Only the guard mentions the unit count.
        byte[] hostile = new byte[16];
        // Encapsulation header (LE XCDR1)
        hostile[0] = 0x00; hostile[1] = 0x01;
        hostile[2] = 0x00; hostile[3] = 0x00;
        // Length: 100,000,000 in LE
        hostile[4] = (byte) 0x00; hostile[5] = (byte) 0xE1; hostile[6] = (byte) 0xF5; hostile[7] = (byte) 0x05;
        CdrReader r = CdrReader.of(hostile);
        CdrUnderflowException e = assertThrows(CdrUnderflowException.class, r::readWString);
        assertTrue(e.getMessage().contains("100000000"),
                "Exception must be from pre-allocation guard (mentions unit count): " + e.getMessage());
    }

    @Test
    void aStringWithMaxIntLengthPrefixUnderflowsRatherThanAllocatingOrHanging() {
        // Length prefix is 0x7FFFFFFF (Integer.MAX_VALUE) against a short buffer.
        // This would wrap in 32-bit arithmetic. The fixed require() in long arithmetic
        // must catch it.
        byte[] hostile = new byte[16];
        // Encapsulation header (LE XCDR1)
        hostile[0] = 0x00; hostile[1] = 0x01;
        hostile[2] = 0x00; hostile[3] = 0x00;
        // Length: 0x7FFFFFFF in LE
        hostile[4] = (byte) 0xFF; hostile[5] = (byte) 0xFF; hostile[6] = (byte) 0xFF; hostile[7] = (byte) 0x7F;
        CdrReader r = CdrReader.of(hostile);
        assertThrows(CdrUnderflowException.class, r::readString);
    }

    @Test
    void readBytesWithNegativeLengthThrowsCdrUnderflowExceptionNotNegativeArraySizeException() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBytes(new byte[] {1, 2});
            CdrReader r = reread(w);
            assertThrows(CdrUnderflowException.class, () -> r.readBytes(-1));
        }
    }
}
