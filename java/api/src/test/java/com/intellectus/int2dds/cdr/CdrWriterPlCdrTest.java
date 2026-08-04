package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class CdrWriterPlCdrTest {

    private static byte[] payload(CdrWriter w) {
        byte[] all = w.toBytes();
        byte[] out = new byte[all.length - 4];
        System.arraycopy(all, 4, out, 0, out.length);
        return out;
    }

    private static int leShort(byte[] b, int at) {
        return (b[at] & 0xFF) | ((b[at + 1] & 0xFF) << 8);
    }

    private static int leInt(byte[] b, int at) {
        return (b[at] & 0xFF) | ((b[at + 1] & 0xFF) << 8)
                | ((b[at + 2] & 0xFF) << 16) | ((b[at + 3] & 0xFF) << 24);
    }

    @Test
    void shortFormCarriesTheIdAndContentLength() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(5);
            w.writeI32(0x01020304);
            w.memberV1Finalize(h, 5, false);

            byte[] p = payload(w);
            assertEquals(5, leShort(p, 0), "PID holds the member id");
            assertEquals(4, leShort(p, 2), "content length");
            assertEquals(0x01020304, leInt(p, 4));
            assertEquals(8, p.length);
        }
    }

    @Test
    void mustUnderstandSetsBit0x4000() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(5);
            w.writeI32(1);
            w.memberV1Finalize(h, 5, true);
            assertEquals(0x4000 | 5, leShort(payload(w), 0));
        }
    }

    @Test
    void aLargeMemberIdUsesTheLongFormFromTheStart() {
        int id = 0x3F00 + 1;
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(id);
            w.writeI32(0x0A0B0C0D);
            w.memberV1Finalize(h, id, false);

            byte[] p = payload(w);
            assertEquals(0x3F01, leShort(p, 0), "extended PID marker");
            assertEquals(8, leShort(p, 2), "long-form header length");
            assertEquals(id, leInt(p, 4));
            assertEquals(4, leInt(p, 8), "content length");
            assertEquals(0x0A0B0C0D, leInt(p, 12));
        }
    }

    @Test
    void contentTooLongForTheShortFormIsShiftedAndRewritten() {
        // Reserved 4 bytes for a small id, then wrote more than 0xFFFF bytes:
        // the header has to grow to 12 and the content move forward by 8.
        byte[] big = new byte[0x10000];
        for (int i = 0; i < big.length; i++) {
            big[i] = (byte) (i & 0x7F);
        }
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(5);
            w.writeBytes(big);
            w.memberV1Finalize(h, 5, false);

            byte[] p = payload(w);
            assertEquals(0x3F01, leShort(p, 0), "promoted to the extended PID");
            assertEquals(8, leShort(p, 2));
            assertEquals(5, leInt(p, 4), "the original member id");
            assertEquals(big.length, leInt(p, 8));
            assertEquals(12 + big.length, p.length);
            for (int i = 0; i < big.length; i++) {
                assertEquals(big[i], p[12 + i], "content byte " + i + " survived the shift");
            }
        }
    }

    @Test
    void twoMembersFollowEachOtherWithFourByteAlignment() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int a = w.memberV1Begin(1);
            w.writeI8((byte) 0x77);
            w.memberV1Finalize(a, 1, false);
            int b = w.memberV1Begin(2);
            w.writeI8((byte) 0x88);
            w.memberV1Finalize(b, 2, false);

            byte[] p = payload(w);
            assertEquals(1, leShort(p, 0));
            assertEquals(1, leShort(p, 2), "one content byte");
            assertEquals(0x77, p[4]);
            // memberV1Begin aligns to 4, so the second header starts at 8.
            assertEquals(2, leShort(p, 8));
            assertEquals((byte) 0x88, p[12]);
        }
    }

    @Test
    void endMutableStructWritesThePlCdrSentinel() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            w.endMutableStruct();
            assertArrayEquals(new byte[] {0x02, 0x3F, 0x00, 0x00}, payload(w));
        }
    }

    @Test
    void shortFormHeaderIsCorrectFromAReusedPoolSlot() {
        // Acquiring a dirty pooled buffer still yields a correct PID and
        // length for a short-form member: the finalize writes cover all four
        // reserved header bytes unconditionally, regardless of what a prior
        // sample left behind in the slot.
        try (CdrWriter dirty = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            dirty.writeBytes(new byte[] {(byte) 0xEE, (byte) 0xEE, (byte) 0xEE, (byte) 0xEE,
                    (byte) 0xEE, (byte) 0xEE, (byte) 0xEE, (byte) 0xEE});
        }
        try (CdrWriter w = CdrWriter.acquire(Extensibility.MUTABLE, true, false)) {
            int h = w.memberV1Begin(5);
            w.writeI8((byte) 1);
            w.memberV1Finalize(h, 5, false);
            byte[] p = payload(w);
            assertEquals(5, leShort(p, 0));
            assertEquals(1, leShort(p, 2));
        }
    }
}
