package com.intellectus.int2dds.types;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.TopicFieldDescriptor;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.xtypes.FieldType;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.util.List;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/**
 * Hands bytes produced by the <em>generated</em> {@link CdrGolden} to the
 * core's own deserializer and asserts the field values it decodes.
 *
 * <p>Every other {@code codegen::java} test is a string snapshot against a
 * literal the same author wrote, and {@code scripts/check-idl-java.sh} only
 * compiles the output. Neither can see a wire-format mistake, and neither can
 * a round trip through our own reader: writer and reader share the mapping, so
 * they agree even when both are wrong. This test cannot, because the decoder
 * on the other side is the core the interoperability workflow validates
 * against other vendors.
 *
 * <p>No golden hex appears here on purpose. A hex snapshot of our own writer's
 * output would enshrine whatever that writer does, bug included.
 *
 * <p>The type object is built mostly from {@link CdrGolden#ddsFields()}, so
 * the generator's own declared {@link FieldType} codes -- not a hand-copied
 * list -- are what the core is asked to decode with. Only the trailing fields
 * that the keyed-topic path cannot describe (octet, char, float, double and the
 * two wstrings) are added explicitly.
 */
class GeneratedTypeConformanceTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    // INT2DDS_MEMBER_KEY, from ffi/src/type_info.rs.
    private static final int MEMBER_KEY = 1;

    private long typeInfo;
    private long typeObject;

    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }

    @BeforeEach
    void buildTypeObject() {
        typeInfo = FfiAccess.typeInfoCreate(utf8("CdrGolden"), Extensibility.APPENDABLE.value());
        assertNotEquals(0L, typeInfo, "type info handle");

        for (TopicFieldDescriptor d : CdrGolden.ddsFields()) {
            assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8(d.name()), d.fieldType(),
                    d.isKey() ? MEMBER_KEY : 0), d.name());
        }
        // Declaration order must match serializeCdr; these sit after the last
        // @key field, so ddsFields() does not describe them.
        assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("byte_val"), FieldType.BYTE, 0));
        assertEquals(0, FfiAccess.typeInfoAddField(typeInfo, utf8("char_val"), FieldType.CHAR8, 0));
        assertEquals(0,
                FfiAccess.typeInfoAddField(typeInfo, utf8("f32_val"), FieldType.FLOAT32, 0));
        assertEquals(0,
                FfiAccess.typeInfoAddField(typeInfo, utf8("f64_val"), FieldType.FLOAT64, 0));
        // The two collections carry no assertion of their own: nothing reads a
        // sequence element back through this path. They are here so the core has
        // to walk past them to reach the two scalar wstrings after them, which
        // is what makes those assertions cover the per-element framing too.
        assertEquals(0, FfiAccess.typeInfoAddSequenceField(
                typeInfo, utf8("wstr_seq"), FieldType.WSTRING, 0, 0));
        assertEquals(0, FfiAccess.typeInfoAddArrayField(
                typeInfo, utf8("wstr_arr"), FieldType.WSTRING, 2, 0));
        // wstring needs the dedicated adder rather than a FieldType code; 0
        // means unbounded.
        assertEquals(0,
                FfiAccess.typeInfoAddWstringField(typeInfo, utf8("unbounded_wstr"), 0, 0));
        assertEquals(0,
                FfiAccess.typeInfoAddWstringField(typeInfo, utf8("bounded_wstr"), 32, 0));

        typeObject = FfiAccess.typeInfoToTypeObject(typeInfo);
        assertNotEquals(0L, typeObject, "type object handle");
    }

    @AfterEach
    void releaseTypeInfo() {
        if (typeObject != 0L) {
            FfiAccess.typeObjectDestroy(typeObject);
            typeObject = 0L;
        }
        if (typeInfo != 0L) {
            FfiAccess.typeInfoDestroy(typeInfo);
            typeInfo = 0L;
        }
    }

    // --- the sample values -------------------------------------------------

    /**
     * Values chosen so a wrong mapping shows up rather than cancelling out:
     * every signed minimum, every unsigned value past its Java type's positive
     * range, and a multi-byte string.
     */
    private static CdrGolden extremes() {
        CdrGolden g = new CdrGolden();
        g.id = Integer.MIN_VALUE;
        g.boolVal = true;
        g.i8Val = Byte.MIN_VALUE;
        g.u8Val = (byte) 0xFF;
        g.i16Val = Short.MIN_VALUE;
        g.u16Val = (short) 0xFFFF;
        g.i32Val = Integer.MIN_VALUE;
        g.u32Val = 0xDEADBEEF;
        g.i64Val = Long.MIN_VALUE;
        g.u64Val = 0xFFFFFFFFFFFFFFFFL;
        g.unboundedStr = "센서/온도 é中";
        g.boundedStr = "bounded";
        g.byteVal = (byte) 0xFF;
        g.charVal = (byte) 0x7F;
        g.f32Val = -Float.MAX_VALUE;
        g.f64Val = -1234.5d;
        // The clef is a surrogate pair: two UTF-16 units for one code point, so
        // a length written in code points instead of units desynchronizes here.
        g.wstrSeq = new String[] {"센서 𝄞", "", "é中"};
        g.wstrArr = new String[] {"𝄞", "arr"};
        g.unboundedWstr = "센서 é中 𝄞";
        g.boundedWstr = "wide";
        return g;
    }

    /** The other end of the range: zeros, false, and two empty strings. */
    private static CdrGolden zeros() {
        CdrGolden g = new CdrGolden();
        g.id = 0;
        g.boolVal = false;
        g.i8Val = 0;
        g.u8Val = 0;
        g.i16Val = 0;
        g.u16Val = 0;
        g.i32Val = 0;
        g.u32Val = 0;
        g.i64Val = 0L;
        g.u64Val = 0L;
        g.unboundedStr = "";
        g.boundedStr = "";
        g.byteVal = 0;
        g.charVal = 0;
        g.f32Val = 0.0f;
        g.f64Val = 0.0d;
        g.wstrSeq = new String[0];
        g.wstrArr = new String[] {"", ""};
        g.unboundedWstr = "";
        g.boundedWstr = "";
        return g;
    }

    /** A third set with the ordinary positive values a mapping bug can hide in. */
    private static CdrGolden ordinary() {
        CdrGolden g = new CdrGolden();
        g.id = 42;
        g.boolVal = true;
        g.i8Val = -8;
        g.u8Val = (byte) 200;
        g.i16Val = -1234;
        g.u16Val = (short) 50000;
        g.i32Val = -7;
        g.u32Val = (int) 3_000_000_000L; // above Integer.MAX_VALUE, wraps negative
        g.i64Val = -1_000_000_000_000L;
        g.u64Val = -1L; // 2^64 - 1
        g.unboundedStr = "hello";
        g.boundedStr = "x";
        g.byteVal = (byte) 0xAB;
        g.charVal = (byte) 'Z';
        g.f32Val = 1.5f;
        g.f64Val = 1e300;
        g.wstrSeq = new String[] {"one"};
        g.wstrArr = new String[] {"a", "bb"};
        g.unboundedWstr = "wide hello";
        g.boundedWstr = "w";
        return g;
    }

    private static CdrGolden[] cases() {
        return new CdrGolden[] {extremes(), zeros(), ordinary()};
    }

    // --- the oracle --------------------------------------------------------

    private void assertCoreDecodes(CdrGolden g, boolean littleEndian, boolean xcdr2, String what) {
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, littleEndian, xcdr2)) {
            g.serializeCdr(w);
            ByteBuffer out = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            long outAddr = FfiAccess.directBufferAddress(out);

            assertEquals(0, FfiAccess.dynamicSampleGetI32(
                    w.address(), w.length(), typeObject, utf8("id"), outAddr),
                    what + ": the core must accept our encoding");
            assertEquals(g.id, out.getInt(0), what + ": id");

            assertEquals(0, FfiAccess.dynamicSampleGetBool(
                    w.address(), w.length(), typeObject, utf8("bool_val"), outAddr), what);
            assertEquals(g.boolVal, out.get(0) != 0, what + ": bool_val");

            assertEquals(0, FfiAccess.dynamicSampleGetI8(
                    w.address(), w.length(), typeObject, utf8("i8_val"), outAddr), what);
            assertEquals(g.i8Val, out.get(0), what + ": i8_val");

            // uint8 lives in a Java byte; the core hands back the unsigned value.
            assertEquals(0, FfiAccess.dynamicSampleGetU8(
                    w.address(), w.length(), typeObject, utf8("u8_val"), outAddr), what);
            assertEquals(g.u8Val & 0xFF, out.get(0) & 0xFF, what + ": u8_val");

            assertEquals(0, FfiAccess.dynamicSampleGetI16(
                    w.address(), w.length(), typeObject, utf8("i16_val"), outAddr), what);
            assertEquals(g.i16Val, out.getShort(0), what + ": i16_val");

            assertEquals(0, FfiAccess.dynamicSampleGetU16(
                    w.address(), w.length(), typeObject, utf8("u16_val"), outAddr), what);
            assertEquals(g.u16Val & 0xFFFF, out.getShort(0) & 0xFFFF, what + ": u16_val");

            assertEquals(0, FfiAccess.dynamicSampleGetI32(
                    w.address(), w.length(), typeObject, utf8("i32_val"), outAddr), what);
            assertEquals(g.i32Val, out.getInt(0), what + ": i32_val");

            // uint32 wraps into a Java int; compare as unsigned.
            assertEquals(0, FfiAccess.dynamicSampleGetU32(
                    w.address(), w.length(), typeObject, utf8("u32_val"), outAddr), what);
            assertEquals(g.u32Val & 0xFFFFFFFFL, out.getInt(0) & 0xFFFFFFFFL,
                    what + ": u32_val");

            assertEquals(0, FfiAccess.dynamicSampleGetI64(
                    w.address(), w.length(), typeObject, utf8("i64_val"), outAddr), what);
            assertEquals(g.i64Val, out.getLong(0), what + ": i64_val");

            assertEquals(0, FfiAccess.dynamicSampleGetU64(
                    w.address(), w.length(), typeObject, utf8("u64_val"), outAddr), what);
            assertEquals(g.u64Val, out.getLong(0), what + ": u64_val");

            assertEquals(g.unboundedStr, coreString(w, "unbounded_str"),
                    what + ": unbounded_str");
            assertEquals(g.boundedStr, coreString(w, "bounded_str"), what + ": bounded_str");

            assertEquals(0, FfiAccess.dynamicSampleGetByte(
                    w.address(), w.length(), typeObject, utf8("byte_val"), outAddr), what);
            assertEquals(g.byteVal & 0xFF, out.get(0) & 0xFF, what + ": byte_val");

            // IDL char maps to a Java byte written through writeU8.
            assertEquals(0, FfiAccess.dynamicSampleGetChar8(
                    w.address(), w.length(), typeObject, utf8("char_val"), outAddr), what);
            assertEquals(g.charVal & 0xFF, out.get(0) & 0xFF, what + ": char_val");

            assertEquals(0, FfiAccess.dynamicSampleGetF32(
                    w.address(), w.length(), typeObject, utf8("f32_val"), outAddr), what);
            assertEquals(g.f32Val, out.getFloat(0), 0.0f, what + ": f32_val");

            assertEquals(0, FfiAccess.dynamicSampleGetF64(
                    w.address(), w.length(), typeObject, utf8("f64_val"), outAddr), what);
            assertEquals(g.f64Val, out.getDouble(0), 0.0d, what + ": f64_val");

            // The core hands a decoded wstring back as UTF-8 like any other
            // string, so the same getter reads both.
            assertEquals(g.unboundedWstr, coreString(w, "unbounded_wstr"),
                    what + ": unbounded_wstr");
            assertEquals(g.boundedWstr, coreString(w, "bounded_wstr"), what + ": bounded_wstr");
        }
    }

    /** Grow-and-retry around the core's string getter, as DynamicSample does. */
    private String coreString(CdrWriter w, String field) {
        int cap = 16;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer lenSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc = FfiAccess.dynamicSampleGetString(w.address(), w.length(), typeObject,
                    utf8(field), buf, cap, FfiAccess.directBufferAddress(lenSlot));
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                cap = (int) lenSlot.getLong(0) + 1; // out_len excludes the NUL
                continue;
            }
            assertEquals(0, rc, field + ": the core must accept our string framing");
            return new String(buf, 0, (int) lenSlot.getLong(0), UTF8);
        }
    }

    // --- the tests ---------------------------------------------------------

    @Test
    void theCoreDecodesEveryFieldUnderXcdr2() {
        // XCDR2 wraps the APPENDABLE struct in a DHEADER, so this also checks
        // that our DHEADER extent covers exactly the fields we wrote.
        for (CdrGolden g : cases()) {
            assertCoreDecodes(g, true, true, "xcdr2");
        }
    }

    @Test
    void theCoreDecodesEveryFieldUnderXcdr1() {
        // No DHEADER here -- a different framing, and the one a plain
        // createDataWriter actually resolves to.
        for (CdrGolden g : cases()) {
            assertCoreDecodes(g, true, false, "xcdr1");
        }
    }

    @Test
    void theCoreDecodesEveryFieldFromABigEndianEncoding() {
        // The core picks its decoder from our encapsulation header, so this
        // checks that header too, not just the payload after it.
        for (CdrGolden g : cases()) {
            assertCoreDecodes(g, false, true, "xcdr2-be");
            assertCoreDecodes(g, false, false, "xcdr1-be");
        }
    }

    @Test
    void theGeneratedTypeRoundTripsThroughItsOwnReader() {
        // Weaker evidence than the oracle above -- a shared writer/reader
        // mistake survives it -- but it is what catches a reader-only
        // regression, and it costs nothing.
        for (boolean xcdr2 : new boolean[] {true, false}) {
            for (CdrGolden sent : cases()) {
                byte[] bytes;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, xcdr2)) {
                    sent.serializeCdr(w);
                    bytes = w.toBytes();
                }
                CdrGolden back = new CdrGolden();
                back.deserializeCdr(CdrReader.of(bytes));
                assertFieldsEqual(sent, back, "xcdr2=" + xcdr2);
            }
        }
    }

    @Test
    void aBoundedStringIsCheckedInBytesNotUtf16Units() {
        // boundedStr is string<64>. A Hangul syllable is one UTF-16 unit and
        // three UTF-8 bytes, so length() accepts 22 of them and writeString
        // then puts 66 bytes on the wire. The core rejects that on
        // deserialize -- @try_construct defaults to Discard -- and drops the
        // sample with nothing logged on the writing side.
        CdrGolden over = ordinary();
        over.boundedStr = repeat('센', 22);
        assertEquals(22, over.boundedStr.length(), "within the bound in UTF-16 units");
        assertEquals(66, CdrWriter.utf8Length(over.boundedStr), "over it in bytes");
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, false)) {
            assertThrows(IllegalStateException.class, () -> over.serializeCdr(w));
        }

        // 63 bytes still fits, so the check is not merely rejecting non-ASCII.
        CdrGolden under = ordinary();
        under.boundedStr = repeat('센', 21);
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, true, false)) {
            under.serializeCdr(w);
        }
    }

    private static String repeat(char c, int n) {
        StringBuilder sb = new StringBuilder(n);
        for (int i = 0; i < n; i++) {
            sb.append(c);
        }
        return sb.toString();
    }

    @Test
    void ddsFieldsDescribesThePrefixThroughTheLastKeyField() {
        // The core's flat parser walks these in declaration order, so the
        // prefix must be unbroken and the last entry must be the last @key.
        List<TopicFieldDescriptor> fields = CdrGolden.ddsFields();
        assertEquals(12, fields.size());
        assertEquals("id", fields.get(0).name());
        assertTrue(fields.get(0).isKey(), "id is a key");
        assertEquals("bounded_str", fields.get(11).name());
        assertTrue(fields.get(11).isKey(), "bounded_str is a key");
    }

    private static void assertFieldsEqual(CdrGolden a, CdrGolden b, String what) {
        assertEquals(a.id, b.id, what + ": id");
        assertEquals(a.boolVal, b.boolVal, what + ": bool_val");
        assertEquals(a.i8Val, b.i8Val, what + ": i8_val");
        assertEquals(a.u8Val, b.u8Val, what + ": u8_val");
        assertEquals(a.i16Val, b.i16Val, what + ": i16_val");
        assertEquals(a.u16Val, b.u16Val, what + ": u16_val");
        assertEquals(a.i32Val, b.i32Val, what + ": i32_val");
        assertEquals(a.u32Val, b.u32Val, what + ": u32_val");
        assertEquals(a.i64Val, b.i64Val, what + ": i64_val");
        assertEquals(a.u64Val, b.u64Val, what + ": u64_val");
        assertEquals(a.unboundedStr, b.unboundedStr, what + ": unbounded_str");
        assertEquals(a.boundedStr, b.boundedStr, what + ": bounded_str");
        assertEquals(a.byteVal, b.byteVal, what + ": byte_val");
        assertEquals(a.charVal, b.charVal, what + ": char_val");
        assertEquals(a.f32Val, b.f32Val, 0.0f, what + ": f32_val");
        assertEquals(a.f64Val, b.f64Val, 0.0d, what + ": f64_val");
        assertArrayEquals(a.wstrSeq, b.wstrSeq, what + ": wstr_seq");
        assertArrayEquals(a.wstrArr, b.wstrArr, what + ": wstr_arr");
        assertEquals(a.unboundedWstr, b.unboundedWstr, what + ": unbounded_wstr");
        assertEquals(a.boundedWstr, b.boundedWstr, what + ": bounded_wstr");
    }
}
