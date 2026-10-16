package kr.co.intellectus.int2dds.internal.ffi;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import kr.co.intellectus.int2dds.internal.NativeLoader;
import org.junit.jupiter.api.Test;

/**
 * Pins which backend a run exercises. Without this, a classpath or packaging mistake that put the
 * JNI classes on a JDK 22+ runtime would still pass every other test, against the wrong backend.
 */
class BackendSelectionTest {

    @Test
    void theRuntimeSelectsTheExpectedBackend() {
        // Set by api/build.gradle.kts from the test JDK: "ffm" on 22+, "jni" below.
        String expected = System.getProperty("int2dds.expected.backend");
        assertNotNull(expected, "int2dds.expected.backend must be set by the Gradle build");
        assertEquals(expected, FfiHandwritten.backend());
    }

    @Test
    void theNativeVersionComesFromTheSelectedLibrary() {
        // FFM reads int2dds_version from int2dds_ffi; JNI asks int2dds_java. Either
        // way it must be the workspace version the JAR was built for.
        assertEquals(System.getProperty("int2dds.version"), NativeLoader.nativeVersion());
        assertEquals(
                NativeLoader.nativeVersion(),
                new String(FfiHandwritten.nativeVersion(), java.nio.charset.StandardCharsets.UTF_8));
    }

    @Test
    void anAddressRoundTripsThroughADirectByteBuffer() {
        ByteBuffer original = ByteBuffer.allocateDirect(16).order(ByteOrder.nativeOrder());
        original.putLong(0, 0x0123456789ABCDEFL);
        long addr = FfiAccess.directBufferAddress(original);

        ByteBuffer view = FfiAccess.addressToDirectByteBuffer(addr, 16);
        assertNotNull(view);
        assertEquals(16, view.capacity());
        assertEquals(0x0123456789ABCDEFL, view.order(ByteOrder.nativeOrder()).getLong(0));
        kr.co.intellectus.int2dds.internal.NativeKeepAlive.keepAlive(original);
    }

    @Test
    void aNullAddressIsRefused() {
        assertNull(FfiAccess.addressToDirectByteBuffer(0L, 16));
    }
}
