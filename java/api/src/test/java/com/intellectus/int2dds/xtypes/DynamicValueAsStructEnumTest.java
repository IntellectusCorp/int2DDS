package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for {@link DynamicValue#asStruct} and {@link
 * DynamicValue#asEnum}: read a struct value's fields and an enum value's
 * name/numeric back out of a {@code DynamicValue}, mirroring {@code
 * xml_dynamic_complex.rs}'s {@code mode} enum build/read (lines ~194-197,
 * ~280-297) and its nested-struct build (lines ~88-102). This is entirely
 * local (no participant/writer/reader) -- both readers work directly off a
 * {@link DynamicValue} cloned from a {@link DynamicData} field, the same way
 * {@link DynamicValueUnionTest} and {@link DynamicValueStructTest} exercise
 * their read paths.
 */
class DynamicValueAsStructEnumTest {

    private static final String XML = "<types>\n"
            + " <module name=\"n\">\n"
            + "  <enum name=\"Mode\">\n"
            + "   <enumerator name=\"OFF\" value=\"0\"/>\n"
            + "   <enumerator name=\"ON\" value=\"1\"/>\n"
            + "  </enum>\n"
            + "  <struct name=\"Point\">\n"
            + "   <member name=\"x\" type=\"int32\"/>\n"
            + "   <member name=\"y\" type=\"int32\"/>\n"
            + "  </struct>\n"
            + "  <struct name=\"Rec\">\n"
            + "   <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "   <member name=\"mode\" type=\"nonBasic\" nonBasicTypeName=\"n::Mode\"/>\n"
            + "   <member name=\"point\" type=\"nonBasic\" nonBasicTypeName=\"n::Point\"/>\n"
            + "  </struct>\n"
            + " </module>\n"
            + "</types>\n";

    @Test
    void readsAnEnumFieldsNameAndNumericValue() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport recSupport = registry.getTypeSupport("n::Rec");
                    DynamicTypeSupport pointSupport = registry.getTypeSupport("n::Point")) {
                try (DynamicData rec = DynamicData.create(recSupport)) {
                    rec.setU32("id", 1);

                    // "mode" is consumed by setValue -- do not close it afterward.
                    rec.setValue("mode", DynamicValue.enumValue("ON", 1));

                    // "point" is a required nested-struct member: fill it in
                    // so the record is complete, even though this test only
                    // reads "mode" back.
                    try (DynamicData pt = DynamicData.create(pointSupport)) {
                        pt.setI32("x", 0);
                        pt.setI32("y", 0);
                        rec.setValue("point", DynamicValue.struct(pt));
                    }

                    try (DynamicValue modeV = rec.getValue("mode")) {
                        assertEquals(DynamicValueKind.ENUM, modeV.kind());
                        EnumValue mode = modeV.asEnum();
                        assertEquals(1, mode.value());
                        assertEquals("ON", mode.name());
                    }
                }
            }
        }
    }

    @Test
    void readsAStructValuesFieldsAsAnOwnedDynamicData() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport recSupport = registry.getTypeSupport("n::Rec");
                    DynamicTypeSupport pointSupport = registry.getTypeSupport("n::Point")) {
                try (DynamicData rec = DynamicData.create(recSupport)) {
                    rec.setU32("id", 2);
                    rec.setValue("mode", DynamicValue.enumValue("OFF", 0));

                    try (DynamicData pt = DynamicData.create(pointSupport)) {
                        pt.setI32("x", 7);
                        pt.setI32("y", -8);
                        rec.setValue("point", DynamicValue.struct(pt));
                    }

                    try (DynamicValue pointV = rec.getValue("point")) {
                        assertEquals(DynamicValueKind.STRUCT, pointV.kind());
                        try (DynamicData s = pointV.asStruct()) {
                            assertEquals(7, s.getI32("x"));
                            assertEquals(-8, s.getI32("y"));
                        }
                        // asStruct() clones -- pointV is still usable/closeable.
                        assertEquals(DynamicValueKind.STRUCT, pointV.kind());
                    }
                }
            }
        }
    }
}
