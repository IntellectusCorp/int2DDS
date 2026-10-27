package kr.co.intellectus.int2dds.internal;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_INT;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.InvalidPathException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.util.Locale;
import kr.co.intellectus.int2dds.exceptions.NativeLoadException;

/**
 * JDK 22+ loader: opens the plain C ABI library {@code int2dds_ffi} through FFM and verifies that
 * its product version matches this JAR's.
 *
 * <p>Nothing here goes through JNI. The library is opened with {@link SymbolLookup#libraryLookup},
 * never {@code System.load}, so the JNI library {@code int2dds_java} is neither needed nor loaded
 * on this runtime.
 *
 * <p>Search order:
 *
 * <ol>
 *   <li>{@code INT2DDS_FFI_PATH} — the library file or its directory, the same variable the C# and
 *       Python bindings read
 *   <li>a bundled copy inside this JAR, extracted to a temp directory
 *   <li>{@code java.library.path}
 *   <li>the platform loader's own search ({@code PATH}, {@code LD_LIBRARY_PATH}, ...)
 * </ol>
 */
public final class NativeLoader {

    /** Environment variable naming the library file, or the directory holding it. */
    static final String PATH_ENV = "INT2DDS_FFI_PATH";

    private static final Object LOCK = new Object();
    private static SymbolLookup lookup;
    private static String nativeVersion;

    private NativeLoader() {}

    /** Loads and verifies the native library. Idempotent and thread-safe. */
    public static void load() {
        symbolLookup();
    }

    /** The product version reported by the loaded native library. */
    public static String nativeVersion() {
        load();
        return nativeVersion;
    }

    /**
     * Symbols of the loaded {@code int2dds_ffi}, loading and verifying it on first use. The FFM
     * {@code Ffi} and {@code FfiHandwritten} bind every downcall against this lookup, reaching it
     * through {@code internal.ffi.FfmLibrary}.
     *
     * <p>Package-private on purpose: a class under {@code META-INF/versions/22} must keep the
     * public API of its base version, which has no such method ({@code jar --validate}).
     */
    static SymbolLookup symbolLookup() {
        synchronized (LOCK) {
            if (lookup == null) {
                SymbolLookup opened = doLoad();
                String version = readVersion(opened);
                verifyVersion(jarVersion(version), version);
                nativeVersion = version;
                lookup = opened;
            }
            return lookup;
        }
    }

    static void verifyVersion(String jar, String nativeVer) {
        if (!jar.equals(nativeVer)) {
            throw new NativeLoadException(
                    "int2DDS version mismatch: JAR is "
                            + jar
                            + " but the native library is "
                            + nativeVer
                            + ". Rebuild the native library with "
                            + "'cargo build --release -p int2dds-ffi'.");
        }
    }

    /** The platform-specific file name for a given {@code os.name} value. */
    static String libraryFileName(String osName) {
        String os = osName.toLowerCase(Locale.ROOT);
        if (os.contains("win")) {
            return "int2dds_ffi.dll";
        }
        if (os.contains("mac") || os.contains("darwin")) {
            return "libint2dds_ffi.dylib";
        }
        return "libint2dds_ffi.so";
    }

    /**
     * Loads the library from an explicit path. Same signature as the JNI loader's, so code compiled
     * against the base class links against this one too.
     *
     * <p>Separated from {@link #doLoad()} so the missing-file branch is testable without setting an
     * environment variable, which Java cannot do to its own process.
     */
    static void loadFrom(String path) {
        openFrom(path);
    }

    /**
     * Opens the library at an explicit path: the file itself, or a directory holding it under its
     * platform file name.
     */
    private static SymbolLookup openFrom(String path) {
        File f = new File(path);
        if (f.isDirectory()) {
            f = new File(f, libraryFileName(System.getProperty("os.name", "linux")));
        }
        if (!f.isFile()) {
            throw new NativeLoadException(PATH_ENV + " points at a missing file: " + path);
        }
        return open(f.toPath());
    }

    private static String jarVersion(String nativeVer) {
        Package p = NativeLoader.class.getPackage();
        String v = (p == null) ? null : p.getImplementationVersion();
        // When running from a Gradle source set rather than the packaged JAR there
        // is no manifest; fall back to the native version so tests still run.
        return (v == null) ? nativeVer : v;
    }

    private static SymbolLookup doLoad() {
        String explicit = System.getenv(PATH_ENV);
        if (explicit != null && !explicit.isEmpty()) {
            return openFrom(explicit);
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
                    return open(target);
                }
            } catch (IOException e) {
                throw new NativeLoadException("failed to extract " + resource, e);
            }
        }

        // SymbolLookup.libraryLookup does not consult java.library.path, so walk it here
        // to keep the search order of the JNI loader.
        for (String dir : System.getProperty("java.library.path", "").split(File.pathSeparator)) {
            if (dir.isEmpty()) {
                continue;
            }
            try {
                Path candidate = Paths.get(dir, fileName);
                if (Files.isRegularFile(candidate)) {
                    return open(candidate);
                }
            } catch (InvalidPathException ignored) {
                // A malformed entry cannot hold the library; keep searching.
            }
        }

        try {
            return SymbolLookup.libraryLookup(fileName, Arena.global());
        } catch (IllegalArgumentException e) {
            throw new NativeLoadException(
                    "could not load int2dds_ffi. "
                            + unbundledNote(platform)
                            + "Set "
                            + PATH_ENV
                            + " to the absolute path of "
                            + fileName
                            + " or of its directory, or put its directory on java.library.path.",
                    e);
        }
    }

    private static SymbolLookup open(Path library) {
        try {
            // The global arena: the library stays mapped for the life of the process, as a
            // System.load'ed library would, because downcall handles outlive any narrower scope.
            return SymbolLookup.libraryLookup(library, Arena.global());
        } catch (IllegalArgumentException e) {
            throw new NativeLoadException("failed to load " + library, e);
        }
    }

    private static String readVersion(SymbolLookup library) {
        MemorySegment symbol =
                library.find("int2dds_version")
                        .orElseThrow(
                                () ->
                                        new NativeLoadException(
                                                "the native library does not export"
                                                        + " int2dds_version, so it predates this"
                                                        + " JAR. Rebuild it with 'cargo build"
                                                        + " --release -p int2dds-ffi'."));
        MethodHandle version =
                Linker.nativeLinker()
                        .downcallHandle(symbol, FunctionDescriptor.of(JAVA_INT, ADDRESS, JAVA_INT));
        try (Arena arena = Arena.ofConfined()) {
            int capacity = 64;
            MemorySegment buf = arena.allocate(capacity);
            int len = (int) version.invokeExact(buf, capacity);
            if (len >= capacity) {
                capacity = len + 1;
                buf = arena.allocate(capacity);
                len = (int) version.invokeExact(buf, capacity);
            }
            return new String(buf.asSlice(0, len).toArray(JAVA_BYTE), StandardCharsets.UTF_8);
        } catch (Throwable t) {
            throw new NativeLoadException("failed to read the native library version", t);
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
