package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class CdrWriterXcdr2Test {

    private static byte[] payload(CdrWriter w) {
        byte[] all = w.toBytes();
        byte[] out = new byte[all.length - 4];
        System.arraycopy(all, 4, out, 0, out.length);
        return out;
    }

    private static int leInt(byte[] b, int at) {
        return (b[at] & 0xFF) | ((b[at + 1] & 0xFF) << 8)
                | ((b[at + 2] & 0xFF) << 16) | ((b[at + 3] & 0xFF) << 24);
    }

    @Test
    void dheaderRecordsTheSizeOfWhatFollowsExcludingItself() {
        // Getting "excluding itself" wrong by 4 is the classic DHEADER bug and
        // a round-trip test cannot see it.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            int token = w.dheaderBegin();
            w.writeI32(0x11111111);
            w.writeI32(0x22222222);
            w.dheaderFinalize(token);

            byte[] p = payload(w);
            assertEquals(4 + 8, p.length);
            assertEquals(8, leInt(p, 0), "DHEADER counts the 8 bytes after it, not 12");
        }
    }

    @Test
    void anEmptyDheaderIsZero() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            int token = w.dheaderBegin();
            w.dheaderFinalize(token);
            assertArrayEquals(new byte[] {0, 0, 0, 0}, payload(w));
        }
    }

    @Test
    void dheaderIsANoOpUnderXcdr1() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, false)) {
            int token = w.dheaderBegin();
            assertEquals(-1, token);
            w.writeI32(1);
            w.dheaderFinalize(token);
            assertEquals(4, payload(w).length, "no DHEADER bytes under XCDR1");
        }
    }

    @Test
    void emheaderPacksMustUnderstandLengthCodeAndMemberId() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            w.writeEmheader(0x0A, 4, true);
            byte[] p = payload(w);
            // must-understand bit 31, LC=4 in bits 30..28, member id in 27..0
            assertEquals(0x8000000A | (4 << 28), leInt(p, 0));
            assertEquals(4, leInt(p, 4), "NEXTINT length follows the header word");
        }
    }

    @Test
    void emheaderWithoutMustUnderstandClearsTheTopBit() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            w.writeEmheader(0x0A, 4, false);
            assertEquals((4 << 28) | 0x0A, leInt(payload(w), 0));
        }
    }

    @Test
    void emheaderBeginBackPatchesTheActualLength() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            int token = w.emheaderBegin(7, false);
            w.writeI64(1L);
            w.emheaderFinalize(token);

            byte[] p = payload(w);
            assertEquals((4 << 28) | 7, leInt(p, 0));
            assertEquals(8, leInt(p, 4), "the i64 that was actually written");
        }
    }

    @Test
    void memberIdsWiderThan28BitsAreRejected() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            assertThrows(IllegalArgumentException.class,
                    () -> w.writeEmheader(0x10000000, 4, false));
            assertThrows(IllegalArgumentException.class,
                    () -> w.emheaderBegin(0x10000000, false));
        }
    }

    @Test
    void sentinelIsTheSingleWord0x3F02() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, true)) {
            w.writeSentinel();
            assertArrayEquals(new byte[] {0x02, 0x3F, 0x00, 0x00}, payload(w));
        }
    }

    @Test
    void xcdr2OnlyOperationsRejectAnXcdr1Writer() {
        // Writing an EMHEADER into an XCDR1 stream produces bytes no peer can
        // parse, so fail loudly rather than emit them.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            assertThrows(IllegalStateException.class, () -> w.writeEmheader(1, 4, false));
            assertThrows(IllegalStateException.class, () -> w.emheaderBegin(1, false));
            assertThrows(IllegalStateException.class, w::writeSentinel);
        }
    }

    @Test
    void nestedDheadersEachCountTheirOwnExtent() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            int outer = w.dheaderBegin();
            int inner = w.dheaderBegin();
            w.writeI32(1);
            w.dheaderFinalize(inner);
            w.writeI32(2);
            w.dheaderFinalize(outer);

            byte[] p = payload(w);
            assertEquals(4, leInt(p, 4), "inner covers its own i32");
            assertEquals(12, leInt(p, 0), "outer covers the inner DHEADER, its i32, and the trailing i32");
        }
    }
}
