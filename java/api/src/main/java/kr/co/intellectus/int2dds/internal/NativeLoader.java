package kr.co.intellectus.int2dds.internal;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.Charset;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.Locale;
import kr.co.intellectus.int2dds.exceptions.NativeLoadException;
import kr.co.intellectus.int2dds.internal.ffi.FfiHandwritten;

/**
 * Locates and loads {@code int2dds_java}, then verifies that the native library's product version
 * matches this JAR's.
 *
 * <p>Search order:
 *
 * <ol>
 *   <li>{@code INT2DDS_JAVA_LIB} — an absolute path, for local development
 *   <li>a bundled copy inside this JAR, extracted to a temp directory
 *   <li>{@code java.library.path}
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
            byte[] version;
            try {
                version = FfiHandwritten.nativeVersion();
            } catch (UnsatisfiedLinkError e) {
                // A library built from an older tree can carry the same version number but
                // lack the entry point this JAR calls; say so instead of a bare link error.
                throw new NativeLoadException(
                        "int2dds_java does not match this JAR: it lacks the version entry point,"
                                + " so it was built from an older source tree. Rebuild it with"
                                + " 'cargo build --release -p int2dds-java'.",
                        e);
            }
            nativeVersion = new String(version, UTF8);
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
                    "int2DDS version mismatch: JAR is "
                            + jar
                            + " but the native library is "
                            + nativeVer
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
     * <p>Separated from {@link #doLoad()} so the missing-file branch is testable without setting an
     * environment variable, which Java cannot do to its own process.
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
        String platform = platformDir();
        if (platform != null) {
            String resource = "/native/" + platform + "/" + fileName;
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
        }

        try {
            System.loadLibrary("int2dds_java");
        } catch (UnsatisfiedLinkError e) {
            throw new NativeLoadException(
                    "could not load int2dds_java. "
                            + unbundledNote(platform)
                            + "Set INT2DDS_JAVA_LIB to the absolute path of "
                            + fileName
                            + ", or put its directory on java.library.path.",
                    e);
        }
    }

    /** {@code <os>-<arch>} of the bundled copy, or null when the JAR bundles none for this CPU. */
    private static String platformDir() {
        String os = System.getProperty("os.name", "linux").toLowerCase(Locale.ROOT);
        String o =
                os.contains("win")
                        ? "windows"
                        : (os.contains("mac") || os.contains("darwin")) ? "macos" : "linux";
        String a = archDir(System.getProperty("os.arch", ""));
        return (a == null) ? null : o + "-" + a;
    }

    private static String archDir(String osArch) {
        String arch = osArch.toLowerCase(Locale.ROOT);
        if (arch.equals("amd64") || arch.equals("x86_64")) {
            return "x86_64";
        }
        if (arch.equals("aarch64") || arch.equals("arm64")) {
            return "aarch64";
        }
        return null;
    }

    private static String unbundledNote(String platform) {
        return (platform != null)
                ? ""
                : "This JAR bundles no native library for os.arch="
                        + System.getProperty("os.arch")
                        + ". ";
    }
}
