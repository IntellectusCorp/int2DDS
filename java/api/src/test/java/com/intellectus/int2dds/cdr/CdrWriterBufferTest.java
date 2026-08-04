package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class CdrWriterBufferTest {

    @Test
    void encapsulationHeaderIsFourBytesBigEndianRegardlessOfDataEndianness() {
        // The 2-byte encapsulation id is always big-endian on the wire, even
        // when the payload that follows is little-endian.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, false)) {
            assertEquals(4, w.length());
            assertArrayEquals(new byte[] {0x00, 0x01, 0x00, 0x00}, w.toBytes());
        }
    }

    @Test
    void encapsulationIdSelectsOnExtensibilityAndVersion() {
        assertArrayEquals(new byte[] {0x00, 0x00, 0x00, 0x00},
                bytesOf(Extensibility.APPENDABLE, false, false));   // CDR_BE
        assertArrayEquals(new byte[] {0x00, 0x03, 0x00, 0x00},
                bytesOf(Extensibility.MUTABLE, true, false));       // PL_CDR_LE
        assertArrayEquals(new byte[] {0x00, 0x07, 0x00, 0x00},
                bytesOf(Extensibility.FINAL, true, true));          // CDR2_LE
        assertArrayEquals(new byte[] {0x00, 0x09, 0x00, 0x00},
                bytesOf(Extensibility.APPENDABLE, true, true));     // D_CDR2_LE
        assertArrayEquals(new byte[] {0x00, 0x0B, 0x00, 0x00},
                bytesOf(Extensibility.MUTABLE, true, true));        // PL_CDR2_LE
    }

    @Test
    void rawWriterHasNoHeader() {
        try (CdrWriter w = CdrWriter.acquireRaw(true, false)) {
            assertEquals(0, w.length());
            assertArrayEquals(new byte[0], w.toBytes());
        }
    }

    @Test
    void addressIsANonZeroNativeAddress() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            assertNotEquals(0L, w.address(), "the backing buffer must be direct");
        }
    }

    @Test
    void twoWritersAreAliveAtOnceWithDistinctBuffers() {
        // The FFI write takes payload and key together, so a single per-thread
        // buffer is not enough.
        try (CdrWriter payload = CdrWriter.acquire(Extensibility.FINAL, true, true);
             CdrWriter key = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            assertNotEquals(payload.address(), key.address());
        }
    }

    @Test
    void buffersAreReusedAcrossAcquireAndClose() {
        long first;
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            first = w.address();
        }
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            assertEquals(first, w.address(), "a closed writer's buffer returns to the pool");
        }
    }

    @Test
    void reacquiredWriterStartsAtAFreshHeader() {
        // Checks that reacquiring a pooled buffer yields a writer whose visible
        // content is exactly its own header, with no carry-over into
        // [0, length()). This does NOT exercise zeroFill: toBytes() is bounded
        // to [0, pos), so bytes the dirty writer left past its own pos (here,
        // offsets 4-7) are outside what this test can observe. The real
        // zero-fill-of-reserved-spans guarantee becomes observable once
        // alignment padding exists, inside [0, pos) where toBytes() can see
        // it — covered by paddingBytesAreZeroEvenInAReusedBuffer in the next
        // task.
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            w.writeBytes(new byte[] {(byte) 0xAA, (byte) 0xBB, (byte) 0xCC, (byte) 0xDD});
        }
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            byte[] fresh = w.toBytes();
            assertEquals(4, fresh.length, "only the header");
            assertArrayEquals(new byte[] {0x00, 0x07, 0x00, 0x00}, fresh);
        }
    }

    @Test
    void growthPreservesContentAndUpdatesTheAddress() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            long before = w.address();
            byte[] big = new byte[4096];
            for (int i = 0; i < big.length; i++) {
                big[i] = (byte) i;
            }
            w.writeBytes(big);
            byte[] out = w.toBytes();
            assertEquals(4 + big.length, out.length);
            for (int i = 0; i < big.length; i++) {
                assertEquals((byte) i, out[4 + i], "byte " + i + " survived the grow");
            }
            assertNotEquals(before, w.address(),
                    "growth allocates a new direct buffer; a stale cached address is a bug");
        }
    }

    @Test
    void growthPastTheCapIsRejected() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            assertThrows(CdrOverflowException.class,
                    () -> w.writeBytes(new byte[CdrWriter.MAX_CAPACITY + 1]));
        }
    }

    @Test
    void bufferViewIsReadOnlyAndBounded() {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.FINAL, true, true)) {
            java.nio.ByteBuffer view = w.buffer();
            assertTrue(view.isReadOnly());
            assertEquals(0, view.position());
            assertEquals(w.length(), view.limit());
        }
    }

    private static byte[] bytesOf(Extensibility ext, boolean le, boolean xcdr2) {
        try (CdrWriter w = CdrWriter.acquire(ext, le, xcdr2)) {
            return w.toBytes();
        }
    }
}
