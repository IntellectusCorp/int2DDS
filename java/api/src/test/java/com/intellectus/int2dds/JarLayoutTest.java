package com.intellectus.int2dds;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.internal.ffi.Ffi;
import java.io.DataInputStream;
import java.io.File;
import java.io.InputStream;
import java.util.jar.JarFile;
import java.util.jar.Manifest;
import org.junit.jupiter.api.Test;

class JarLayoutTest {

    @Test
    void generatedFfiClassIsCompiledAtJava8Bytecode() throws Exception {
        // A single JAR must load on JDK 8. Class file major version 52 == Java 8.
        String path = Ffi.class.getName().replace('.', '/') + ".class";
        try (InputStream in = Ffi.class.getClassLoader().getResourceAsStream(path)) {
            assertNotNull(in, "Ffi.class must be on the classpath");
            DataInputStream d = new DataInputStream(in);
            assertEquals(0xCAFEBABE, d.readInt(), "class file magic");
            d.readUnsignedShort(); // minor
            assertEquals(52, d.readUnsignedShort(), "major version must be 52 (Java 8)");
        }
    }

    @Test
    void ffiDeclaresTheFullGeneratedSurface() {
        // 453 generated natives + directBufferAddress.
        long natives = java.util.Arrays.stream(Ffi.class.getDeclaredMethods())
                .filter(m -> java.lang.reflect.Modifier.isNative(m.getModifiers()))
                .count();
        assertEquals(454, natives, "generated native method count");
    }

    @Test
    void noFfiMethodExposesJavaLangString() {
        // JNI modified UTF-8 would corrupt non-ASCII topic and type names.
        for (java.lang.reflect.Method m : Ffi.class.getDeclaredMethods()) {
            assertTrue(m.getReturnType() != String.class,
                    m.getName() + " must not return String");
            for (Class<?> p : m.getParameterTypes()) {
                assertTrue(p != String.class, m.getName() + " must not take String");
            }
        }
    }

    /**
     * The other tests in this class read compiled classes straight off the
     * test classpath, which is an exploded directory, not the packaged jar
     * — no multi-release resolution happens there regardless of which JDK
     * runs the test (confirmed directly: {@code NativeKeepAlive.class}
     * reads back as major version 52, the {@code --release 8} fallback,
     * even under a JDK 17 test run). Whether the {@code java9} source set
     * actually reaches a real consumer at all depends entirely on the
     * <em>packaged</em> jar being built correctly, which nothing else here
     * checks — a {@code build.gradle.kts} edit that dropped the
     * versioned-entry wiring, or a plain compile error in {@code
     * src/main/java9}, would go undetected by every other test in this
     * module until someone actually published the artifact.
     *
     * <p>{@code int2dds.built.jar} (set in {@code build.gradle.kts}, off the
     * {@code jar} task's own lazily-configured {@code archiveFile}, with the
     * {@code test} task wired to depend on {@code jar} so this can never
     * read a stale artifact from a previous build) points at the actual
     * packaged jar. Opened with the plain, pre-9 {@code JarFile}
     * constructor — the only one available at this module's own
     * {@code --release 8} — so this test's own bytecode stays 8-compatible
     * even though what it is inspecting is not.
     *
     * <p>The versioned entry is looked up by its literal {@code
     * META-INF/versions/9/...} path rather than the unprefixed base name:
     * confirmed directly that a plain {@code JarFile} does not runtime-
     * version-resolve the unprefixed name on its own (the two-argument-or-
     * fewer constructors do not opt into that), so only the literal,
     * always-present versioned path is a reliable, JDK-version-independent
     * way to reach it — which is also the more direct assertion of what
     * this test actually wants to know.
     */
    @Test
    void packagedJarIsMultiReleaseWithTheJdk9KeepAliveOverride() throws Exception {
        String jarPath = System.getProperty("int2dds.built.jar");
        assertNotNull(jarPath, "int2dds.built.jar system property must be set by the Gradle build");
        File jarFile = new File(jarPath);
        assertTrue(jarFile.isFile(),
                "built jar not found at " + jarPath + " -- did the jar task actually run?");

        try (JarFile jar = new JarFile(jarFile)) {
            Manifest manifest = jar.getManifest();
            assertEquals("true", manifest.getMainAttributes().getValue("Multi-Release"),
                    "the manifest must declare Multi-Release: true");

            // Derived from the class itself, never written out as a literal. A
            // literal would still name a real, major-53 entry after someone moved
            // the base class to another package without moving the java9 source
            // alongside it -- so the test would pass green while the override had
            // stopped shadowing anything. That exact move happened once already.
            String basePath =
                    com.intellectus.int2dds.internal.NativeKeepAlive.class.getName()
                            .replace('.', '/') + ".class";
            String versionedPath = "META-INF/versions/9/" + basePath;

            assertNotNull(jar.getJarEntry(basePath),
                    basePath + " must be present -- the Java 8 NativeKeepAlive the override shadows");
            assertEquals(52, majorVersionOf(jar, basePath),
                    "the base NativeKeepAlive must stay compiled at Java 8 (major version 52)");

            assertNotNull(jar.getJarEntry(versionedPath),
                    versionedPath + " must be present -- the JDK 9+ NativeKeepAlive override");
            assertEquals(53, majorVersionOf(jar, versionedPath),
                    "the versioned override must be compiled at Java 9 (major version 53)");
        }
    }

    /** Reads a jar entry's class-file major version, checking the magic first. */
    private static int majorVersionOf(JarFile jar, String path) throws Exception {
        try (DataInputStream d = new DataInputStream(jar.getInputStream(jar.getJarEntry(path)))) {
            assertEquals(0xCAFEBABE, d.readInt(), "class file magic for " + path);
            d.readUnsignedShort(); // minor
            return d.readUnsignedShort();
        }
    }
}
