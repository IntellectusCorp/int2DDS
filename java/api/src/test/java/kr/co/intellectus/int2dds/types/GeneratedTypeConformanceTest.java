package kr.co.intellectus.int2dds.types;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import kr.co.intellectus.int2dds.cdr.CdrReader;
import kr.co.intellectus.int2dds.cdr.CdrWriter;
import kr.co.intellectus.int2dds.cdr.Extensibility;
import kr.co.intellectus.int2dds.core.DomainParticipant;
import kr.co.intellectus.int2dds.exceptions.DdsException;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;
import kr.co.intellectus.int2dds.xtypes.DynamicData;
import kr.co.intellectus.int2dds.xtypes.DynamicValue;
import kr.co.intellectus.int2dds.xtypes.TypeInfo;
import kr.co.intellectus.int2dds.xtypes.TypeObject;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/**
 * Hands bytes produced by the <em>generated</em> {@link CdrGolden} to the core's own deserializer
 * and asserts the field values it decodes.
 *
 * <p>Every other {@code codegen::java} test is a string snapshot against a literal the same author
 * wrote, and {@code java/scripts/check-idl-java.sh} only compiles the output. Neither can see a
 * wire-format mistake, and neither can a round trip through our own reader: writer and reader share
 * the mapping, so they agree even when both are wrong. This test cannot, because the decoder on the
 * other side is the core the interoperability workflow validates against other vendors.
 *
 * <p>No golden hex appears here on purpose. A hex snapshot of our own writer's output would
 * enshrine whatever that writer does, bug included.
 *
 * <p>The type object is built from the generated {@link CdrGolden#typeInfo()} itself -- exactly
 * what {@code createTopic} advertises, not a hand-copied list -- so that is what the core is asked
 * to decode with.
 */
class GeneratedTypeConformanceTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private TypeInfo typeInfo;
    private TypeObject typeObjectOwner;
    private long typeObject;

    private static byte[] utf8(String s) {
        return s.getBytes(UTF8);
    }

    @BeforeEach
    void buildTypeObject() {
        typeInfo = new CdrGolden().typeInfo();
        typeObjectOwner = typeInfo.toTypeObject();
        typeObject = typeObjectOwner.handle();
        assertNotEquals(0L, typeObject, "type object handle");
    }

    @AfterEach
    void releaseTypeInfo() {
        typeObject = 0L;
        if (typeObjectOwner != null) {
            typeObjectOwner.close();
            typeObjectOwner = null;
        }
        if (typeInfo != null) {
            typeInfo.close();
            typeInfo = null;
        }
    }

    // --- the sample values -------------------------------------------------

    /**
     * Values chosen so a wrong mapping shows up rather than cancelling out: every signed minimum,
     * every unsigned value past its Java type's positive range, and a multi-byte string.
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
        g.color = CdrGoldenColor.BLUE;
        g.point = point(Integer.MIN_VALUE, -Double.MAX_VALUE);
        g.inner = inner(Integer.MIN_VALUE, -1234.5d, "센서 é中");
        g.i64Seq = new long[] {Long.MIN_VALUE, -1L, Long.MAX_VALUE};
        g.f64Arr = new double[] {-Double.MAX_VALUE, 0.0d, Double.MIN_VALUE};
        // Three bytes, so whatever follows has to realign.
        g.byteSeq = new byte[] {(byte) 0xFF, 0, (byte) 0x80};
        g.strSeq = new String[] {"센서", "", "é中"};
        g.colorSeq =
                new CdrGoldenColor[] {
                    CdrGoldenColor.BLUE, CdrGoldenColor.RED, CdrGoldenColor.GREEN
                };
        g.innerSeq = new CdrGoldenInner[] {inner(1, 1.5d, "a"), inner(-2, -2.5d, "")};
        g.pointArr = new CdrGoldenPoint[] {point(3, 3.5d), point(-4, -4.5d)};
        return g;
    }

    private static CdrGoldenPoint point(int x, double y) {
        CdrGoldenPoint p = new CdrGoldenPoint();
        p.x = x;
        p.y = y;
        return p;
    }

    private static CdrGoldenInner inner(int x, double y, String label) {
        CdrGoldenInner in = new CdrGoldenInner();
        in.x = x;
        in.y = y;
        in.label = label;
        return in;
    }

    /**
     * The other end of the range: zeros, false, and two empty strings. The enum, nested structs and
     * collections keep their defaults, so every sequence is empty.
     */
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
        g.color = CdrGoldenColor.GREEN;
        g.point = point(7, 0.25d);
        g.inner = inner(-7, 1e300, "inner");
        g.i64Seq = new long[] {-1_000_000_000_000L};
        g.f64Arr = new double[] {1.5d, -2.5d, 1e300};
        g.byteSeq = new byte[] {(byte) 0xAB};
        g.strSeq = new String[] {"one"};
        g.colorSeq = new CdrGoldenColor[] {CdrGoldenColor.GREEN};
        g.innerSeq = new CdrGoldenInner[] {inner(9, 9.5d, "nine")};
        g.pointArr = new CdrGoldenPoint[] {point(1, 1.0d), point(2, 2.0d)};
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

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetI32(
                            w.address(), w.length(), typeObject, utf8("id"), outAddr),
                    what + ": the core must accept our encoding");
            assertEquals(g.id, out.getInt(0), what + ": id");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetBool(
                            w.address(), w.length(), typeObject, utf8("bool_val"), outAddr),
                    what);
            assertEquals(g.boolVal, out.get(0) != 0, what + ": bool_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetI8(
                            w.address(), w.length(), typeObject, utf8("i8_val"), outAddr),
                    what);
            assertEquals(g.i8Val, out.get(0), what + ": i8_val");

            // uint8 lives in a Java byte; the core hands back the unsigned value.
            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetU8(
                            w.address(), w.length(), typeObject, utf8("u8_val"), outAddr),
                    what);
            assertEquals(g.u8Val & 0xFF, out.get(0) & 0xFF, what + ": u8_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetI16(
                            w.address(), w.length(), typeObject, utf8("i16_val"), outAddr),
                    what);
            assertEquals(g.i16Val, out.getShort(0), what + ": i16_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetU16(
                            w.address(), w.length(), typeObject, utf8("u16_val"), outAddr),
                    what);
            assertEquals(g.u16Val & 0xFFFF, out.getShort(0) & 0xFFFF, what + ": u16_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetI32(
                            w.address(), w.length(), typeObject, utf8("i32_val"), outAddr),
                    what);
            assertEquals(g.i32Val, out.getInt(0), what + ": i32_val");

            // uint32 wraps into a Java int; compare as unsigned.
            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetU32(
                            w.address(), w.length(), typeObject, utf8("u32_val"), outAddr),
                    what);
            assertEquals(g.u32Val & 0xFFFFFFFFL, out.getInt(0) & 0xFFFFFFFFL, what + ": u32_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetI64(
                            w.address(), w.length(), typeObject, utf8("i64_val"), outAddr),
                    what);
            assertEquals(g.i64Val, out.getLong(0), what + ": i64_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetU64(
                            w.address(), w.length(), typeObject, utf8("u64_val"), outAddr),
                    what);
            assertEquals(g.u64Val, out.getLong(0), what + ": u64_val");

            assertEquals(g.unboundedStr, coreString(w, "unbounded_str"), what + ": unbounded_str");
            assertEquals(g.boundedStr, coreString(w, "bounded_str"), what + ": bounded_str");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetByte(
                            w.address(), w.length(), typeObject, utf8("byte_val"), outAddr),
                    what);
            assertEquals(g.byteVal & 0xFF, out.get(0) & 0xFF, what + ": byte_val");

            // IDL char maps to a Java byte written through writeU8.
            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetChar8(
                            w.address(), w.length(), typeObject, utf8("char_val"), outAddr),
                    what);
            assertEquals(g.charVal & 0xFF, out.get(0) & 0xFF, what + ": char_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetF32(
                            w.address(), w.length(), typeObject, utf8("f32_val"), outAddr),
                    what);
            assertEquals(g.f32Val, out.getFloat(0), 0.0f, what + ": f32_val");

            assertEquals(
                    0,
                    FfiAccess.dynamicSampleGetF64(
                            w.address(), w.length(), typeObject, utf8("f64_val"), outAddr),
                    what);
            assertEquals(g.f64Val, out.getDouble(0), 0.0d, what + ": f64_val");

            // The core hands a decoded wstring back as UTF-8 like any other
            // string, so the same getter reads both.
            assertEquals(
                    g.unboundedWstr, coreString(w, "unbounded_wstr"), what + ": unbounded_wstr");
            assertEquals(g.boundedWstr, coreString(w, "bounded_wstr"), what + ": bounded_wstr");
        }
    }

    /** Grow-and-retry around the core's string getter, as DynamicSample does. */
    private String coreString(CdrWriter w, String field) {
        int cap = 16;
        while (true) {
            byte[] buf = new byte[cap];
            ByteBuffer lenSlot = ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
            int rc =
                    FfiAccess.dynamicSampleGetString(
                            w.address(),
                            w.length(),
                            typeObject,
                            utf8(field),
                            buf,
                            cap,
                            FfiAccess.directBufferAddress(lenSlot));
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                cap = (int) lenSlot.getLong(0) + 1; // out_len excludes the NUL
                continue;
            }
            assertEquals(0, rc, field + ": the core must accept our string framing");
            return new String(buf, 0, (int) lenSlot.getLong(0), UTF8);
        }
    }

    /**
     * The enum, nested-struct and collection members. The flat getters cannot read an enum or a
     * length, so the sample is decoded whole into a {@link DynamicData}.
     */
    private void assertCoreDecodesAggregates(
            DomainParticipant p, CdrGolden g, boolean littleEndian, boolean xcdr2, String what) {
        byte[] bytes;
        try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE, littleEndian, xcdr2)) {
            g.serializeCdr(w);
            bytes = w.toBytes();
        }
        try (DynamicData d = p.dynamicDataFromSample(bytes, typeObjectOwner)) {
            assertEquals(g.color.value(), coreEnum(d, "color"), what + ": color");

            assertCorePoint(g.point, d, "point", what);
            assertCoreInner(g.inner, d, "inner", what);

            assertEquals(g.i64Seq.length, d.getLength("i64_seq"), what + ": i64_seq length");
            for (int i = 0; i < g.i64Seq.length; i++) {
                String at = "i64_seq[" + i + "]";
                assertEquals(g.i64Seq[i], d.getI64(at), what + ": " + at);
            }

            assertEquals(g.f64Arr.length, d.getLength("f64_arr"), what + ": f64_arr length");
            for (int i = 0; i < g.f64Arr.length; i++) {
                String at = "f64_arr[" + i + "]";
                assertEquals(g.f64Arr[i], d.getF64(at), 0.0d, what + ": " + at);
            }

            assertEquals(g.byteSeq.length, d.getLength("byte_seq"), what + ": byte_seq length");
            for (int i = 0; i < g.byteSeq.length; i++) {
                String at = "byte_seq[" + i + "]";
                assertEquals(g.byteSeq[i] & 0xFF, d.getU8(at), what + ": " + at);
            }

            assertEquals(g.strSeq.length, d.getLength("str_seq"), what + ": str_seq length");
            for (int i = 0; i < g.strSeq.length; i++) {
                String at = "str_seq[" + i + "]";
                assertEquals(g.strSeq[i], d.getString(at), what + ": " + at);
            }

            assertEquals(g.colorSeq.length, d.getLength("color_seq"), what + ": color_seq length");
            for (int i = 0; i < g.colorSeq.length; i++) {
                String at = "color_seq[" + i + "]";
                assertEquals(g.colorSeq[i].value(), coreEnum(d, at), what + ": " + at);
            }

            assertEquals(g.innerSeq.length, d.getLength("inner_seq"), what + ": inner_seq length");
            for (int i = 0; i < g.innerSeq.length; i++) {
                assertCoreInner(g.innerSeq[i], d, "inner_seq[" + i + "]", what);
            }

            assertEquals(g.pointArr.length, d.getLength("point_arr"), what + ": point_arr length");
            for (int i = 0; i < g.pointArr.length; i++) {
                assertCorePoint(g.pointArr[i], d, "point_arr[" + i + "]", what);
            }
        }
    }

    private static int coreEnum(DynamicData d, String path) {
        try (DynamicValue v = d.getValue(path)) {
            return v.asEnum().value();
        }
    }

    private static void assertCorePoint(
            CdrGoldenPoint want, DynamicData d, String at, String what) {
        assertEquals(want.x, d.getI32(at + ".x"), what + ": " + at + ".x");
        assertEquals(want.y, d.getF64(at + ".y"), 0.0d, what + ": " + at + ".y");
    }

    private static void assertCoreInner(
            CdrGoldenInner want, DynamicData d, String at, String what) {
        assertEquals(want.x, d.getI32(at + ".x"), what + ": " + at + ".x");
        assertEquals(want.y, d.getF64(at + ".y"), 0.0d, what + ": " + at + ".y");
        assertEquals(want.label, d.getString(at + ".label"), what + ": " + at + ".label");
    }

    // --- the tests ---------------------------------------------------------

    @Test
    void theCoreDecodesEnumsNestedStructsAndCollections() {
        DomainParticipant p =
                new DomainParticipant(
                        Integer.parseInt(System.getProperty("int2dds.test.domain", "137")));
        try {
            for (CdrGolden g : cases()) {
                for (boolean littleEndian : new boolean[] {true, false}) {
                    for (boolean xcdr2 : new boolean[] {true, false}) {
                        assertCoreDecodesAggregates(
                                p,
                                g,
                                littleEndian,
                                xcdr2,
                                "le=" + littleEndian + " xcdr2=" + xcdr2);
                    }
                }
            }
        } finally {
            p.close();
        }
    }

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
    void typeInfoDescribesEveryMemberNotJustThePrefixThroughTheLastKey() {
        // A description that stopped at the last @key (bounded_str, index 11)
        // would be advertised as a smaller type that full-type peers reject.
        assertTrue(typeInfo.hasKey(), "CdrGolden is keyed");
        assertEquals(30, typeObjectOwner.memberCount());
        assertEquals("id", typeObjectOwner.memberName(0));
        assertEquals("point_arr", typeObjectOwner.memberName(29));
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
        assertEquals(a.color, b.color, what + ": color");
        assertPointsEqual(a.point, b.point, what + ": point");
        assertInnersEqual(a.inner, b.inner, what + ": inner");
        assertArrayEquals(a.i64Seq, b.i64Seq, what + ": i64_seq");
        assertArrayEquals(a.f64Arr, b.f64Arr, 0.0d, what + ": f64_arr");
        assertArrayEquals(a.byteSeq, b.byteSeq, what + ": byte_seq");
        assertArrayEquals(a.strSeq, b.strSeq, what + ": str_seq");
        assertArrayEquals(a.colorSeq, b.colorSeq, what + ": color_seq");
        assertEquals(a.innerSeq.length, b.innerSeq.length, what + ": inner_seq length");
        for (int i = 0; i < a.innerSeq.length; i++) {
            assertInnersEqual(a.innerSeq[i], b.innerSeq[i], what + ": inner_seq[" + i + "]");
        }
        assertEquals(a.pointArr.length, b.pointArr.length, what + ": point_arr length");
        for (int i = 0; i < a.pointArr.length; i++) {
            assertPointsEqual(a.pointArr[i], b.pointArr[i], what + ": point_arr[" + i + "]");
        }
    }

    private static void assertPointsEqual(CdrGoldenPoint a, CdrGoldenPoint b, String what) {
        assertEquals(a.x, b.x, what + ".x");
        assertEquals(a.y, b.y, 0.0d, what + ".y");
    }

    private static void assertInnersEqual(CdrGoldenInner a, CdrGoldenInner b, String what) {
        assertEquals(a.x, b.x, what + ".x");
        assertEquals(a.y, b.y, 0.0d, what + ".y");
        assertEquals(a.label, b.label, what + ".label");
    }
}
