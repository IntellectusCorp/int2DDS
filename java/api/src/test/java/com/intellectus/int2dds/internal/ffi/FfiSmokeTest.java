package com.intellectus.int2dds.internal.ffi;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.UnsupportedEncodingException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

class FfiSmokeTest {

    @Test
    void callsAScalarFfiFunctionThroughJni() {
        // ExtensibilityKind::default() is Appendable == 1 in
        // dds/src/xtypes/type_object.rs. This proves the full path:
        // Java -> Ffi -> JNI forwarder -> int2dds-ffi -> DDS core.
        assertEquals(1, FfiAccess.defaultExtensibility());
    }

    @Test
    void dataRepresentationDefaultIsReadable() {
        // Any value is acceptable; the point is that the call returns without
        // an UnsatisfiedLinkError, proving symbol resolution works.
        int rep = FfiAccess.defaultDataRepresentation();
        assertTrue(rep >= 0, "expected a non-negative data representation, got " + rep);
    }

    @Test
    void writesIntoAByteArrayOutParameter() {
        // The last-error slot is a per-thread global that any earlier
        // negative-path test on this thread may have left set; clear it first
        // so this asserts the genuine no-error behavior, not test ordering.
        FfiAccess.clearLastError();
        // With no error recorded, the FFI writes "" and returns length 0.
        byte[] buf = new byte[256];
        int len = FfiAccess.lastErrorMessage(buf);
        assertEquals(0, len, "expected no pending error after clear");
    }

    @Test
    void resolvesTheAddressOfADirectByteBuffer() {
        ByteBuffer direct = ByteBuffer.allocateDirect(64);
        long addr = FfiAccess.directBufferAddress(direct);
        assertNotEquals(0L, addr, "a direct buffer must have a native address");
    }

    @Test
    void reportsZeroForANonDirectByteBuffer() {
        ByteBuffer heap = ByteBuffer.allocate(64);
        assertEquals(0L, FfiAccess.directBufferAddress(heap));
    }

    @Test
    void textWrittenByTheFfiReachesTheCallersByteArray()
            throws UnsupportedEncodingException {
        // The strongest check in this file. A `*mut c_char` parameter looks
        // identical to the in direction, so a forwarder that only copies Java ->
        // native compiles, runs, returns success, and hands back an untouched
        // array. Asserting on a length of zero would not notice. This does a
        // real round trip instead: build a dynamic value natively, render it
        // back into a Java byte[], and read the characters.
        ByteBuffer handleOut = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
        assertEquals(0, FfiAccess.dynamicValueI32(42, FfiAccess.directBufferAddress(handleOut)));
        long value = handleOut.getLong(0);
        assertNotEquals(0L, value, "the FFI must have written a handle");

        try {
            byte[] text = new byte[64];
            ByteBuffer lenOut = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = FfiAccess.dynamicValueToString(
                    value, text, text.length, FfiAccess.directBufferAddress(lenOut));
            assertEquals(0, rc, "to_string must succeed");

            int len = (int) lenOut.getLong(0);
            assertEquals(2, len, "\"42\" is two bytes");
            assertEquals("42", new String(text, 0, len, "UTF-8"));
        } finally {
            FfiAccess.dynamicValueDestroy(value);
        }
    }
}
