package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.charset.Charset;
import org.junit.jupiter.api.Test;

class CdrWriterStringTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private static byte[] payload(CdrWriter w) {
        byte[] all = w.toBytes();
        byte[] out = new byte[all.length - 4];
        System.arraycopy(all, 4, out, 0, out.length);
        return out;
    }

    @Test
    void stringIsLengthIncludingNulThenBytesThenNul() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString("abc");
            assertArrayEquals(
                    new byte[] {0x04, 0x00, 0x00, 0x00, 'a', 'b', 'c', 0x00},
                    payload(w));
        }
    }

    @Test
    void emptyStringIsLengthOneAndJustTheNul() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString("");
            assertArrayEquals(new byte[] {0x01, 0x00, 0x00, 0x00, 0x00}, payload(w));
        }
    }

    @Test
    void nullStringIsWrittenAsEmpty() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString(null);
            assertArrayEquals(new byte[] {0x01, 0x00, 0x00, 0x00, 0x00}, payload(w));
        }
    }

    @Test
    void nonAsciiTakesTheSlowPathAndCountsUtf8Bytes() {
        // The ASCII fast path must not be taken here, and the length prefix
        // counts UTF-8 BYTES, not characters.
        String s = "센서";
        byte[] utf8 = s.getBytes(UTF8);
        assertEquals(6, utf8.length, "precondition: 2 Hangul syllables are 6 UTF-8 bytes");

        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString(s);
            byte[] p = payload(w);
            assertEquals(4 + 6 + 1, p.length);
            assertEquals(7, p[0], "length prefix counts bytes plus the NUL");
            for (int i = 0; i < utf8.length; i++) {
                assertEquals(utf8[i], p[4 + i], "utf-8 byte " + i);
            }
            assertEquals(0, p[p.length - 1], "NUL terminator");
        }
    }

    @Test
    void supplementaryPlaneCharactersSurviveIntact() {
        // A surrogate pair is 4 UTF-8 bytes. This is exactly what JNI's
        // modified UTF-8 would corrupt, which is why strings cross as byte[].
        String s = "🌡";   // U+1F321
        byte[] utf8 = s.getBytes(UTF8);
        assertEquals(4, utf8.length);

        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString(s);
            byte[] p = payload(w);
            for (int i = 0; i < utf8.length; i++) {
                assertEquals(utf8[i], p[4 + i], "utf-8 byte " + i);
            }
        }
    }

    @Test
    void embeddedNulIsPreservedNotTruncated() {
        // CDR strings are length-prefixed, so an interior NUL is legal payload.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString("a\0b");
            byte[] p = payload(w);
            assertEquals(4, p[0], "3 bytes plus the terminator");
            assertEquals('a', p[4]);
            assertEquals(0, p[5]);
            assertEquals('b', p[6]);
            assertEquals(0, p[7]);
        }
    }

    @Test
    void wstringCountsUtf16UnitsAndDoesNotTerminate() {
        // serialize_wstring16 in the core writes the unit count and the units,
        // nothing else. A terminator here would be decoded as payload.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeWString("ab");
            assertArrayEquals(
                    new byte[] {0x02, 0x00, 0x00, 0x00, 'a', 0x00, 'b', 0x00},
                    payload(w));
        }
    }

    @Test
    void emptyWstringIsJustAZeroCount() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeWString("");
            assertArrayEquals(new byte[] {0x00, 0x00, 0x00, 0x00}, payload(w));
        }
    }

    @Test
    void wstringCountsSurrogatePairsAsTwoUnits() {
        // The count is UTF-16 code units, not characters, matching
        // encode_utf16().count() on the core side.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeWString("🌡");   // U+1F321 -> D83C DF21
            assertArrayEquals(
                    new byte[] {0x02, 0x00, 0x00, 0x00,
                            0x3C, (byte) 0xD8, 0x21, (byte) 0xDF},
                    payload(w));
        }
    }

    @Test
    void sequenceHeaderIsAFourByteCount() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeSeqHeader(2);
            assertArrayEquals(new byte[] {0x02, 0x00, 0x00, 0x00}, payload(w));
        }
    }

    @Test
    void rawBytesAreWrittenWithNoAlignmentAndNoLengthPrefix() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeBool(true);
            w.writeBytes(new byte[] {0x11, 0x22});
            assertArrayEquals(new byte[] {1, 0x11, 0x22}, payload(w));
        }
    }

    @Test
    void enumIsASignedFourByteDiscriminant() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeEnum(-1);
            assertArrayEquals(
                    new byte[] {(byte) 0xFF, (byte) 0xFF, (byte) 0xFF, (byte) 0xFF},
                    payload(w));
        }
    }

    @Test
    void aLongStringForcesGrowthAndStaysIntact() {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < 5000; i++) {
            sb.append('x');
        }
        String s = sb.toString();
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, false)) {
            w.writeString(s);
            byte[] p = payload(w);
            assertEquals(4 + 5000 + 1, p.length);
            assertEquals('x', p[4]);
            assertEquals('x', p[4 + 4999]);
            assertEquals(0, p[p.length - 1]);
        }
    }
}
