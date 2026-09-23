package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DomainParticipant;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

/**
 * Proves the nested-deps fix (c47dd479) end to end from the XML entry point:
 * a {@link TypeObject} sourced from {@link XmlTypeRegistry#getTypeObject} --
 * not {@link TypeInfo} -- must still resolve its {@code deps} closure so a
 * struct-typed nested field decodes. Mirrors {@link
 * DynamicDataTest#getMemberReadsANestedStructField}, but the TypeObject comes
 * from XML instead of the TypeInfo builder.
 */
class XmlNestedTypeObjectTest {

    // Point must be FINAL (inline, no nested dheader) and NestedRecord must be
    // APPENDABLE (the parser's default, dds/src/config/xml/parser.rs
    // extensibility_attr) so the CDR layout matches the hand-written encoding
    // below. Nested struct member: type="nonBasic" nonBasicTypeName="Point".
    private static final String XML = "<types>\n"
            + " <struct name=\"Point\" extensibility=\"final\">\n"
            + "  <member name=\"x\" type=\"int32\"/>\n"
            + "  <member name=\"y\" type=\"int32\"/>\n"
            + " </struct>\n"
            + " <struct name=\"NestedRecord\">\n"
            + "  <member name=\"point\" type=\"nonBasic\" nonBasicTypeName=\"Point\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void getMemberReadsANestedStructFieldFromAnXmlTypeObject() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (TypeObject typeObject = registry.getTypeObject("NestedRecord")) {
                int x = 11;
                int y = 22;

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    int token = w.dheaderBegin();
                    w.writeI32(x); // point.x -- Point is FINAL, so inline (no nested dheader)
                    w.writeI32(y); // point.y
                    w.dheaderFinalize(token);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    try (DynamicData point = data.getMember("point")) {
                        assertEquals(x, point.getI32("x"));
                        assertEquals(y, point.getI32("y"));
                    }
                }
            }
        } finally {
            p.close();
        }
    }
}
