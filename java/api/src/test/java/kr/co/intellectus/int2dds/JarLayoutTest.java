package kr.co.intellectus.int2dds;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.io.DataInputStream;
import java.io.File;
import java.io.InputStream;
import java.util.jar.JarFile;
import java.util.jar.Manifest;
import kr.co.intellectus.int2dds.internal.ffi.Ffi;
import kr.co.intellectus.int2dds.internal.ffi.FfiHandwritten;
import org.junit.jupiter.api.Test;

class JarLayoutTest {

    @Test
    void generatedFfiClassIsCompiledAtJava8Bytecode() throws Exception {
        // A single JAR must load on JDK 8. Class file major version 52 == Java 8.
        // On a JDK 22+ run the classpath resolves Ffi to the FFM class instead;
        // the packaged-jar test below covers the base class there.
        assumeTrue(FfiHandwritten.backend().equals("jni"), "JNI backend only");
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
        // The FFM Ffi declares no natives at all; that is the point of it.
        assumeTrue(FfiHandwritten.backend().equals("jni"), "JNI backend only");
        // 514 generated natives + directBufferAddress.
        long natives =
                java.util.Arrays.stream(Ffi.class.getDeclaredMethods())
                        .filter(m -> java.lang.reflect.Modifier.isNative(m.getModifiers()))
                        .count();
        assertEquals(515, natives, "generated native method count");
    }

    @Test
    void noFfiMethodExposesJavaLangString() {
        // JNI modified UTF-8 would corrupt non-ASCII topic and type names. Checks the
        // C ABI surface only: the FFM class also has private helpers, such as one
        // that looks a symbol up by name, which never carry DDS text.
        for (java.lang.reflect.Method m : Ffi.class.getDeclaredMethods()) {
            if (!m.getName().startsWith("int2dds_")) {
                continue;
            }
            assertTrue(m.getReturnType() != String.class, m.getName() + " must not return String");
            for (Class<?> p : m.getParameterTypes()) {
                assertTrue(p != String.class, m.getName() + " must not take String");
            }
        }
    }

    /**
     * The other tests in this class read compiled classes straight off the test classpath, which is
     * an exploded directory, not the packaged jar — no multi-release resolution happens there
     * regardless of which JDK runs the test (confirmed directly: {@code NativeKeepAlive.class}
     * reads back as major version 52, the {@code --release 8} fallback, even under a JDK 17 test
     * run). Whether the {@code java9} source set actually reaches a real consumer at all depends
     * entirely on the <em>packaged</em> jar being built correctly, which nothing else here checks —
     * a {@code build.gradle.kts} edit that dropped the versioned-entry wiring, or a plain compile
     * error in {@code src/main/java9}, would go undetected by every other test in this module until
     * someone actually published the artifact.
     *
     * <p>{@code int2dds.built.jar} (set in {@code build.gradle.kts}, off the {@code jar} task's own
     * lazily-configured {@code archiveFile}, with the {@code test} task wired to depend on {@code
     * jar} so this can never read a stale artifact from a previous build) points at the actual
     * packaged jar. Opened with the plain, pre-9 {@code JarFile} constructor — the only one
     * available at this module's own {@code --release 8} — so this test's own bytecode stays
     * 8-compatible even though what it is inspecting is not.
     *
     * <p>The versioned entry is looked up by its literal {@code META-INF/versions/9/...} path
     * rather than the unprefixed base name: confirmed directly that a plain {@code JarFile} does
     * not runtime- version-resolve the unprefixed name on its own (the two-argument-or- fewer
     * constructors do not opt into that), so only the literal, always-present versioned path is a
     * reliable, JDK-version-independent way to reach it — which is also the more direct assertion
     * of what this test actually wants to know.
     */
    @Test
    void packagedJarIsMultiReleaseWithTheJdk9KeepAliveOverride() throws Exception {
        String jarPath = System.getProperty("int2dds.built.jar");
        assertNotNull(jarPath, "int2dds.built.jar system property must be set by the Gradle build");
        File jarFile = new File(jarPath);
        assertTrue(
                jarFile.isFile(),
                "built jar not found at " + jarPath + " -- did the jar task actually run?");

        try (JarFile jar = new JarFile(jarFile)) {
            Manifest manifest = jar.getManifest();
            assertEquals(
                    "true",
                    manifest.getMainAttributes().getValue("Multi-Release"),
                    "the manifest must declare Multi-Release: true");

            // Derived from the class itself, never written out as a literal. A
            // literal would still name a real, major-53 entry after someone moved
            // the base class to another package without moving the java9 source
            // alongside it -- so the test would pass green while the override had
            // stopped shadowing anything. That exact move happened once already.
            String basePath =
                    kr.co.intellectus.int2dds.internal.NativeKeepAlive.class
                                    .getName()
                                    .replace('.', '/')
                            + ".class";
            String versionedPath = "META-INF/versions/9/" + basePath;

            assertNotNull(
                    jar.getJarEntry(basePath),
                    basePath
                            + " must be present -- the Java 8 NativeKeepAlive the override shadows");
            assertEquals(
                    52,
                    majorVersionOf(jar, basePath),
                    "the base NativeKeepAlive must stay compiled at Java 8 (major version 52)");

            assertNotNull(
                    jar.getJarEntry(versionedPath),
                    versionedPath + " must be present -- the JDK 9+ NativeKeepAlive override");
            assertEquals(
                    53,
                    majorVersionOf(jar, versionedPath),
                    "the versioned override must be compiled at Java 9 (major version 53)");
        }
    }

    /**
     * The JDK 22+ backend is a set of classes the Multi-Release JAR substitutes, all of which must
     * ship together: a jar missing any one of them would mix FFM and JNI classes on a JDK 22+
     * runtime. Each base class must stay at Java 8 and each override at Java 22.
     */
    @Test
    void packagedJarCarriesTheJdk22FfmBackend() throws Exception {
        String jarPath = System.getProperty("int2dds.built.jar");
        assertNotNull(jarPath, "int2dds.built.jar system property must be set by the Gradle build");

        try (JarFile jar = new JarFile(new File(jarPath))) {
            Class<?>[] overridden = {
                Ffi.class,
                FfiHandwritten.class,
                kr.co.intellectus.int2dds.internal.NativeLoader.class
            };
            for (Class<?> c : overridden) {
                String basePath = c.getName().replace('.', '/') + ".class";
                String versionedPath = "META-INF/versions/22/" + basePath;
                assertNotNull(jar.getJarEntry(basePath), basePath + " must be present");
                assertEquals(52, majorVersionOf(jar, basePath), basePath + " must stay at Java 8");
                assertNotNull(
                        jar.getJarEntry(versionedPath),
                        versionedPath + " must be present -- the JDK 22+ FFM override");
                assertEquals(
                        66,
                        majorVersionOf(jar, versionedPath),
                        versionedPath + " must be compiled at Java 22 (major version 66)");
            }

            // Code compiled against a base class links against its override at run
            // time, so every non-private member of the base must exist in the
            // override with the same descriptor, or the call fails with
            // NoSuchMethodError on JDK 22+ only.
            for (Class<?> c : overridden) {
                String basePath = c.getName().replace('.', '/') + ".class";
                java.util.Set<String> base = nonPrivateMethods(jar, basePath);
                java.util.Set<String> override =
                        nonPrivateMethods(jar, "META-INF/versions/22/" + basePath);
                base.removeAll(override);
                assertTrue(
                        base.isEmpty(),
                        c.getSimpleName() + " override is missing base members: " + base);
            }

            // `jar --validate` (and tools that apply the same Multi-Release rule) rejects a
            // versioned class whose public API differs from its base in either direction --
            // an added public method, or a modifier such as `native` that only one side has.
            for (Class<?> c : overridden) {
                String basePath = c.getName().replace('.', '/') + ".class";
                assertEquals(
                        publicApi(jar, basePath),
                        publicApi(jar, "META-INF/versions/22/" + basePath),
                        c.getSimpleName() + " override must keep the base public API exactly");
            }

            // FFM-only helpers with no base counterpart: the listener upcalls and the
            // hand-over of the library NativeLoader opened. Package-private, as a versioned
            // directory may not add public classes either.
            for (String helper : new String[] {"FfmListeners", "FfmLibrary"}) {
                String path =
                        "META-INF/versions/22/"
                                + Ffi.class.getPackage().getName().replace('.', '/')
                                + "/"
                                + helper
                                + ".class";
                assertNotNull(jar.getJarEntry(path), path + " must be present");
            }
        }
    }

    /**
     * {@code name + descriptor} of every non-private method in a jar's class entry, read straight
     * from the class file: reflection cannot see a versioned entry the running JDK did not select.
     */
    private static java.util.Set<String> nonPrivateMethods(JarFile jar, String path)
            throws Exception {
        return methods(jar, path, false);
    }

    /** {@code access-flags name + descriptor} of every public or protected method. */
    private static java.util.Set<String> publicApi(JarFile jar, String path) throws Exception {
        return methods(jar, path, true);
    }

    private static java.util.Set<String> methods(JarFile jar, String path, boolean publicApi)
            throws Exception {
        try (DataInputStream d = new DataInputStream(jar.getInputStream(jar.getJarEntry(path)))) {
            d.readInt(); // magic
            d.readInt(); // minor, major
            int count = d.readUnsignedShort();
            String[] utf8 = new String[count];
            for (int i = 1; i < count; i++) {
                int tag = d.readUnsignedByte();
                switch (tag) {
                    case 1: // Utf8
                        utf8[i] = d.readUTF();
                        break;
                    case 3: // Integer
                    case 4: // Float
                    case 9: // Fieldref
                    case 10: // Methodref
                    case 11: // InterfaceMethodref
                    case 12: // NameAndType
                    case 17: // Dynamic
                    case 18: // InvokeDynamic
                        d.skipBytes(4);
                        break;
                    case 5: // Long
                    case 6: // Double
                        d.skipBytes(8);
                        i++; // takes two constant-pool slots
                        break;
                    case 7: // Class
                    case 8: // String
                    case 16: // MethodType
                    case 19: // Module
                    case 20: // Package
                        d.skipBytes(2);
                        break;
                    case 15: // MethodHandle
                        d.skipBytes(3);
                        break;
                    default:
                        throw new IllegalStateException("constant pool tag " + tag + " in " + path);
                }
            }
            d.skipBytes(6); // access_flags, this_class, super_class
            d.skipBytes(2 * d.readUnsignedShort()); // interfaces
            for (int f = d.readUnsignedShort(); f > 0; f--) {
                d.skipBytes(6);
                skipAttributes(d);
            }
            java.util.Set<String> out = new java.util.TreeSet<>();
            for (int m = d.readUnsignedShort(); m > 0; m--) {
                int access = d.readUnsignedShort();
                String name = utf8[d.readUnsignedShort()];
                String descriptor = utf8[d.readUnsignedShort()];
                skipAttributes(d);
                boolean isPrivate = (access & 0x0002) != 0;
                boolean isSynthetic = (access & 0x1000) != 0;
                boolean isPublicApi = (access & (0x0001 | 0x0004)) != 0; // public, protected
                if (publicApi) {
                    if (isPublicApi && !isSynthetic) {
                        out.add(Integer.toHexString(access) + " " + name + descriptor);
                    }
                } else if (!isPrivate && !isSynthetic && !name.equals("<clinit>")) {
                    out.add(name + descriptor);
                }
            }
            return out;
        }
    }

    private static void skipAttributes(DataInputStream d) throws Exception {
        for (int a = d.readUnsignedShort(); a > 0; a--) {
            d.skipBytes(2);
            d.skipBytes(d.readInt());
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
