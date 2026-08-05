package com.intellectus.int2dds.cdr;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;

import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.nio.Buffer;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/**
 * Hands bytes produced by {@link CdrWriter} to the core's own deserializer and
 * asserts the field values it decodes.
 *
 * <p>This is the byte-exact oracle the CDR layer did not have. A round trip
 * against our own reader cannot fail when the writer and reader share a
 * mistake; this can, because the decoder on the other side is the one the
 * repository's interoperability workflow validates against other vendors.
 */
class CdrConformanceTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    // Field type codes, from csharp/src/Int2Dds/Types/FieldType.cs. Kept local
    // rather than promoted to public API — the IDL backend branch owns that.
    private static final int FIELD_INT32 = 5;
    private static final int FIELD_FLOAT64 = 12;
    private static final int FIELD_STRING = 13;

    private long typeInfo;
    private long typeObject;

    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }

    @BeforeEach
    void buildTypeObject() {
        typeInfo = FfiAccess.typeInfoCreate(utf8("ConformanceRecord"),
                Extensibility.APPENDABLE.value());
        assertNotEquals(0L, typeInfo, "type info handle");

        // Field order and types must match ConformanceRecord.serializeCdr.
        assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("id"), FIELD_INT32, 0));
        assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("value"), FIELD_FLOAT64, 0));
        assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("label"), FIELD_STRING, 0));

        typeObject = FfiAccess.typeInfoToTypeObject(typeInfo);
        assertNotEquals(0L, typeObject, "type object handle");
    }

    @AfterEach
    void releaseTypeInfo() {
        if (typeInfo != 0L) {
            FfiAccess.typeInfoDestroy(typeInfo);
            typeInfo = 0L;
        }
    }

    @Test
    void theCoreDecodesTheIntegerFieldWeWrote() {
        ConformanceRecord r = new ConformanceRecord();
        r.id = 42;
        r.value = 2.5d;
        r.label = "x";

        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            r.serializeCdr(w);

            ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            long outAddr = FfiAccess.directBufferAddress(out);
            int rc = FfiAccess.dynamicSampleGetI32(
                    w.address(), w.length(), typeObject, utf8("id"), outAddr);
            assertEquals(0, rc, "the core must accept our encoding");
            assertEquals(42, out.getInt(0));
        }
    }

    @Test
    void theCoreDecodesTheDoubleFieldWeWrote() {
        // The double sits after an i32 inside a DHEADER. If our alignment or
        // our DHEADER extent were wrong, this is the field that moves.
        ConformanceRecord r = new ConformanceRecord();
        r.id = 7;
        r.value = -1234.5d;
        r.label = "x";

        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            r.serializeCdr(w);

            ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            long outAddr = FfiAccess.directBufferAddress(out);
            int rc = FfiAccess.dynamicSampleGetF64(
                    w.address(), w.length(), typeObject, utf8("value"), outAddr);
            assertEquals(0, rc, "the core must accept our encoding");
            assertEquals(-1234.5d, out.getDouble(0), 0.0d);
        }
    }

    @Test
    void bothFieldsDecodeFromOneEncodingAcrossASpreadOfValues() {
        int[] ids = {0, 1, -1, Integer.MAX_VALUE, Integer.MIN_VALUE};
        double[] values = {0.0d, 1.0d, -0.5d, 1e300, -1e-300};

        for (int i = 0; i < ids.length; i++) {
            ConformanceRecord r = new ConformanceRecord();
            r.id = ids[i];
            r.value = values[i];
            r.label = "case" + i;

            try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
                r.serializeCdr(w);

                ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
                long outAddr = FfiAccess.directBufferAddress(out);

                assertEquals(0, FfiAccess.dynamicSampleGetI32(
                        w.address(), w.length(), typeObject, utf8("id"), outAddr), "case " + i);
                assertEquals(ids[i], out.getInt(0), "id, case " + i);

                assertEquals(0, FfiAccess.dynamicSampleGetF64(
                        w.address(), w.length(), typeObject, utf8("value"), outAddr), "case " + i);
                assertEquals(values[i], out.getDouble(0), 0.0d, "value, case " + i);
            }
        }
    }

    @Test
    void aBigEndianEncodingIsAlsoAccepted() {
        // The core picks its decoder from our encapsulation header, so this
        // checks that header too — not just the payload bytes after it.
        ConformanceRecord r = new ConformanceRecord();
        r.id = 0x01020304;
        r.value = 2.5d;
        r.label = "x";

        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, false, true)) {
            r.serializeCdr(w);

            ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            long outAddr = FfiAccess.directBufferAddress(out);
            assertEquals(0, FfiAccess.dynamicSampleGetI32(
                    w.address(), w.length(), typeObject, utf8("id"), outAddr));
            assertEquals(0x01020304, out.getInt(0));
        }
    }

    @Test
    void aTruncatedSampleIsRejectedRatherThanMisread() {
        ConformanceRecord r = new ConformanceRecord();
        r.id = 42;
        r.value = 2.5d;
        r.label = "x";

        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, true)) {
            r.serializeCdr(w);

            ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            long outAddr = FfiAccess.directBufferAddress(out);
            int rc = FfiAccess.dynamicSampleGetI32(
                    w.address(), w.length() - 4, typeObject, utf8("id"), outAddr);
            assertNotEquals(0, rc, "a short buffer must fail rather than decode");
        }
    }
}
