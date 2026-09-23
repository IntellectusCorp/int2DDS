package com.intellectus.int2dds.internal;

import com.intellectus.int2dds.internal.ffi.FfiHandwritten;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.Charset;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;

/**
 * Locates and loads {@code int2dds_java}, then verifies that the native
 * library's product version matches this JAR's.
 *
 * <p>Search order:
 * <ol>
 *   <li>{@code INT2DDS_JAVA_LIB} — an absolute path, for local development</li>
 *   <li>a bundled copy inside this JAR, extracted to a temp directory</li>
 *   <li>{@code java.library.path}</li>
 * </ol>
 */
public final class NativeLoader {

    private static final Charset UTF8 = Charset.forName("UTF-8");
    private static final Object LOCK = new Object();
    private static boolean loaded;
    private static String nativeVersion;

    private NativeLoader() {}

    /** Loads and verifies the native library. Idempotent and thread-safe. */
    public static void load() {
        synchronized (LOCK) {
            if (loaded) {
                return;
            }
            doLoad();
            nativeVersion = new String(FfiHandwritten.nativeVersion(), UTF8);
            verifyVersion(jarVersion(), nativeVersion);
            loaded = true;
        }
    }

    /** The product version reported by the loaded native library. */
    public static String nativeVersion() {
        load();
        return nativeVersion;
    }

    static void verifyVersion(String jar, String nativeVer) {
        if (!jar.equals(nativeVer)) {
            throw new NativeLoadException(
                    "int2DDS version mismatch: JAR is " + jar
                            + " but the native library is " + nativeVer
                            + ". Rebuild the native library with "
                            + "'cargo build --release -p int2dds-java'.");
        }
    }

    /** The platform-specific file name for a given {@code os.name} value. */
    static String libraryFileName(String osName) {
        String os = osName.toLowerCase(Locale.ROOT);
        if (os.contains("win")) {
            return "int2dds_java.dll";
        }
        if (os.contains("mac") || os.contains("darwin")) {
            return "libint2dds_java.dylib";
        }
        return "libint2dds_java.so";
    }

    /**
     * Loads the library from an explicit path.
     *
     * <p>Separated from {@link #doLoad()} so the missing-file branch is testable
     * without setting an environment variable, which Java cannot do to its own
     * process.
     */
    static void loadFrom(String path) {
        File f = new File(path);
        if (!f.isFile()) {
            throw new NativeLoadException("INT2DDS_JAVA_LIB points at a missing file: " + path);
        }
        System.load(f.getAbsolutePath());
    }

    private static String jarVersion() {
        Package p = NativeLoader.class.getPackage();
        String v = (p == null) ? null : p.getImplementationVersion();
        // When running from a Gradle source set rather than the packaged JAR there
        // is no manifest; fall back to the native version so tests still run.
        return (v == null) ? nativeVersion : v;
    }

    private static void doLoad() {
        String explicit = System.getenv("INT2DDS_JAVA_LIB");
        if (explicit != null && !explicit.isEmpty()) {
            loadFrom(explicit);
            return;
        }

        String fileName = libraryFileName(System.getProperty("os.name", "linux"));
        String resource = "/native/" + platformDir() + "/" + fileName;
        try (InputStream in = NativeLoader.class.getResourceAsStream(resource)) {
            if (in != null) {
                Path dir = Files.createTempDirectory("int2dds-native");
                dir.toFile().deleteOnExit();
                Path target = dir.resolve(fileName);
                Files.copy(in, target, StandardCopyOption.REPLACE_EXISTING);
                target.toFile().deleteOnExit();
                System.load(target.toAbsolutePath().toString());
                return;
            }
        } catch (IOException e) {
            throw new NativeLoadException("failed to extract " + resource, e);
        }

        try {
            System.loadLibrary("int2dds_java");
        } catch (UnsatisfiedLinkError e) {
            throw new NativeLoadException(
                    "could not load int2dds_java. Set INT2DDS_JAVA_LIB to the "
                            + "absolute path of " + fileName
                            + ", or put its directory on java.library.path.", e);
        }
    }

    private static String platformDir() {
        String os = System.getProperty("os.name", "linux").toLowerCase(Locale.ROOT);
        String arch = System.getProperty("os.arch", "amd64").toLowerCase(Locale.ROOT);
        String o = os.contains("win") ? "windows"
                : (os.contains("mac") || os.contains("darwin")) ? "macos" : "linux";
        String a = (arch.contains("aarch64") || arch.contains("arm64")) ? "aarch64" : "x86_64";
        return o + "-" + a;
    }
}
