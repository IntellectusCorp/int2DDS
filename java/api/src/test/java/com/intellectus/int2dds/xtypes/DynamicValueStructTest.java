package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for {@link DynamicValue#struct} and the sequence-of-struct
 * write path: build two nested {@code Point} struct values off {@code
 * DynamicData}, push both into a sequence value built entirely off {@code
 * DynamicValue}, set that into an outer {@code Rec}'s {@code points} field,
 * and read the nested members back -- mirroring {@code
 * xml_dynamic_complex.rs}'s {@code vec2_value}/{@code points} build. A clean
 * run (no JVM crash) is itself part of what this proves: {@link
 * DynamicValue#struct} clones the source {@link DynamicData} natively rather
 * than consuming it, so the caller closing that DynamicData afterward must
 * not double-free the struct value's own handle.
 */
class DynamicValueStructTest {

    private static final String XML = "<types>\n"
            + " <module name=\"n\">\n"
            + "  <struct name=\"Point\">\n"
            + "   <member name=\"x\" type=\"int32\"/>\n"
            + "   <member name=\"y\" type=\"int32\"/>\n"
            + "  </struct>\n"
            + "  <struct name=\"Rec\">\n"
            + "   <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "   <member name=\"points\" type=\"nonBasic\" nonBasicTypeName=\"n::Point\""
            + " sequenceMaxLength=\"-1\"/>\n"
            + "  </struct>\n"
            + " </module>\n"
            + "</types>\n";

    private static DynamicValue point(DynamicTypeSupport pointSupport, int x, int y) {
        // pt is owned by us and closed here -- struct() clones it natively,
        // it does not consume it, so this DynamicData is ours to release
        // independently of the DynamicValue it produced.
        try (DynamicData pt = DynamicData.create(pointSupport)) {
            pt.setI32("x", x);
            pt.setI32("y", y);
            return DynamicValue.struct(pt);
        }
    }

    @Test
    void buildsASequenceOfStructValuesAndSetsItIntoADynamicDataField() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport recSupport = registry.getTypeSupport("n::Rec");
                    DynamicTypeSupport pointSupport = registry.getTypeSupport("n::Point")) {
                try (DynamicData rec = DynamicData.create(recSupport)) {
                    rec.setU32("id", 1);

                    DynamicValue pt0 = point(pointSupport, 1, 2);
                    DynamicValue pt1 = point(pointSupport, 3, 4);

                    DynamicValue seq = DynamicValue.sequence();
                    seq.push(pt0);
                    seq.push(pt1);
                    // seq (and the two struct values pushed into it) is
                    // consumed by setValue below -- do not close them
                    // afterward.
                    rec.setValue("points", seq);

                    assertEquals(2, rec.getLength("points"));
                    try (DynamicData p0 = rec.getMember("points[0]")) {
                        assertEquals(1, p0.getI32("x"));
                        assertEquals(2, p0.getI32("y"));
                    }
                    try (DynamicData p1 = rec.getMember("points[1]")) {
                        assertEquals(3, p1.getI32("x"));
                        assertEquals(4, p1.getI32("y"));
                    }
                }
            }
        }
    }
}
