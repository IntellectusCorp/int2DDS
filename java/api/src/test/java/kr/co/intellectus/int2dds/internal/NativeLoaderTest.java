package kr.co.intellectus.int2dds.internal;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import kr.co.intellectus.int2dds.exceptions.NativeLoadException;
import kr.co.intellectus.int2dds.internal.ffi.FfiHandwritten;
import org.junit.jupiter.api.Test;

class NativeLoaderTest {

    @Test
    void loadIsIdempotent() {
        assertDoesNotThrow(NativeLoader::load);
        assertDoesNotThrow(NativeLoader::load);
    }

    @Test
    void nativeVersionMatchesTheJarVersion() {
        NativeLoader.load();
        String nativeVersion = NativeLoader.nativeVersion();
        assertTrue(
                nativeVersion.matches("\\d+\\.\\d+\\.\\d+.*"),
                "expected a semver-ish version, got: " + nativeVersion);
        // Set by api/build.gradle.kts from Cargo.toml [workspace.package].
        assertEquals(System.getProperty("int2dds.version"), nativeVersion);
    }

    @Test
    void mismatchedVersionsAreRejected() {
        NativeLoadException e =
                assertThrows(
                        NativeLoadException.class,
                        () -> NativeLoader.verifyVersion("0.0.1", "9.9.9"));
        assertTrue(e.getMessage().contains("0.0.1"), e.getMessage());
        assertTrue(e.getMessage().contains("9.9.9"), e.getMessage());
    }

    @Test
    void matchingVersionsPass() {
        assertDoesNotThrow(() -> NativeLoader.verifyVersion("0.0.1", "0.0.1"));
    }

    @Test
    void libraryFileNameIsPlatformCorrect() {
        // JDK 8-21 load the JNI library int2dds_java; JDK 22+ the plain C ABI
        // library int2dds_ffi, through FFM.
        String lib = FfiHandwritten.backend().equals("ffm") ? "int2dds_ffi" : "int2dds_java";
        assertEquals("lib" + lib + ".so", NativeLoader.libraryFileName("linux"));
        assertEquals(lib + ".dll", NativeLoader.libraryFileName("windows"));
        assertEquals("lib" + lib + ".dylib", NativeLoader.libraryFileName("mac"));
        // os.name is "Mac OS X" on macOS and "Windows 11" on Windows; the match
        // has to be case-insensitive and substring-based, not exact.
        assertEquals("lib" + lib + ".dylib", NativeLoader.libraryFileName("Mac OS X"));
        assertEquals(lib + ".dll", NativeLoader.libraryFileName("Windows 11"));
    }

    @Test
    void aMissingExplicitPathIsReportedClearly() {
        NativeLoadException e =
                assertThrows(
                        NativeLoadException.class,
                        () -> NativeLoader.loadFrom("/nonexistent/libint2dds_java.so"));
        assertTrue(e.getMessage().contains("/nonexistent/libint2dds_java.so"), e.getMessage());
    }
}
