package com.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.charset.StandardCharsets;
import java.util.function.ToIntFunction;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.ValueSource;

/**
 * Exercises {@link ReturnCodes#readMessage(ToIntFunction)} — the buffer-growth
 * logic behind {@link ReturnCodes#lastErrorMessage()} — against a stub that
 * mimics the native contract documented in {@code ffi/src/last_error.rs}:
 * writes {@code min(bufLen - 1, full)} message bytes plus a trailing NUL, and
 * always returns the message's full pre-truncation length.
 *
 * <p>The real FFI does not currently emit a message long enough to reach the
 * first-try buffer's boundary, which is exactly how the one-byte-short retry
 * buffer and the {@code >} vs {@code >=} growth check survived undetected.
 * This test drives the sizing logic directly instead of waiting for a long
 * native error.
 */
class ReturnCodesLastErrorMessageTest {

    /** A message that round-trips exactly at {@code byteLen} UTF-8 bytes,
     *  ending in a two-byte character so a one-byte truncation would corrupt
     *  the decode instead of merely trimming it. */
    private static String multiByteEndingMessage(int byteLen) {
        // 'é' (U+00E9) encodes to 2 bytes in UTF-8.
        StringBuilder sb = new StringBuilder();
        int remaining = byteLen - 2;
        for (int i = 0; i < remaining; i++) {
            sb.append('a');
        }
        sb.append('é');
        String s = sb.toString();
        assertEquals(byteLen, s.getBytes(StandardCharsets.UTF_8).length,
                "test setup: message must be exactly byteLen bytes");
        return s;
    }

    /** A plain-ASCII message of exactly {@code byteLen} bytes. */
    private static String asciiMessage(int byteLen) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < byteLen; i++) {
            sb.append('a');
        }
        return sb.toString();
    }

    /** Stub matching the {@code int2dds_last_error_message} contract: writes
     *  as much of {@code full} plus a NUL as fits, always returns the full
     *  pre-truncation byte length. */
    private static ToIntFunction<byte[]> nativeStub(byte[] full) {
        return buf -> {
            int maxBytes = buf.length - 1; // native reserves the last byte for NUL
            int end = Math.min(maxBytes, full.length);
            System.arraycopy(full, 0, buf, 0, end);
            buf[end] = 0;
            return full.length;
        };
    }

    @ParameterizedTest
    @ValueSource(ints = {1, 255, 256, 257, 1000})
    void roundTripsAsciiMessagesAtEveryBoundaryLength(int byteLen) {
        String expected = asciiMessage(byteLen);
        byte[] full = expected.getBytes(StandardCharsets.UTF_8);

        String actual = ReturnCodes.readMessage(nativeStub(full));

        assertEquals(expected, actual);
    }

    @ParameterizedTest
    @ValueSource(ints = {255, 256, 257, 1000}) // 1 byte is too short to hold a 2-byte character
    void roundTripsMessagesEndingInAMultiByteCharacter(int byteLen) {
        String expected = multiByteEndingMessage(byteLen);
        byte[] full = expected.getBytes(StandardCharsets.UTF_8);

        String actual = ReturnCodes.readMessage(nativeStub(full));

        assertEquals(expected, actual);
    }
}
