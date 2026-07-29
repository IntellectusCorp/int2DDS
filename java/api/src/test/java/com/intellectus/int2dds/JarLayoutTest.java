package com.intellectus.int2dds;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.internal.ffi.Ffi;
import java.io.DataInputStream;
import java.io.InputStream;
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
        // 443 generated natives + directBufferAddress.
        long natives = java.util.Arrays.stream(Ffi.class.getDeclaredMethods())
                .filter(m -> java.lang.reflect.Modifier.isNative(m.getModifiers()))
                .count();
        assertEquals(444, natives, "generated native method count");
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
}
