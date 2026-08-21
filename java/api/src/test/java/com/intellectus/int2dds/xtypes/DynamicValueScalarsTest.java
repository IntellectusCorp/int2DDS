package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for the 12 new scalar {@link DynamicValue} factories added
 * alongside {@code i32}/{@code string}/{@code sequence}: build sequences of
 * two different widths ({@code float64} and {@code boolean}) entirely off
 * the {@code DynamicValue} API, set each into a {@link DynamicData} field,
 * and read it back through the existing indexed getters -- mirroring {@link
 * DynamicValueWriteTest}. A clean run (no JVM crash) is itself part of what
 * this proves: {@link DynamicValue#push} and {@link DynamicData#setValue}
 * both consume the handle they are given, and getting that wrong is a
 * double-free, not merely a wrong answer.
 */
class DynamicValueScalarsTest {

    private static final String XML = "<types>\n"
            + " <struct name=\"Rec\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"vals\" type=\"float64\" sequenceMaxLength=\"-1\"/>\n"
            + "  <member name=\"flags\" type=\"boolean\" sequenceMaxLength=\"-1\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    @Test
    void buildsAFloat64SequenceValueAndSetsItIntoADynamicDataField() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    data.setU32("id", 1);

                    DynamicValue seq = DynamicValue.sequence();
                    seq.push(DynamicValue.f64(1.5));
                    seq.push(DynamicValue.f64(2.5));
                    seq.push(DynamicValue.f64(3.5));
                    // seq (and every value pushed into it) is consumed by
                    // setValue below -- do not close it, or the pushed
                    // scalars, afterward.
                    data.setValue("vals", seq);
                    assertTrue(seq.isConsumed());

                    assertEquals(3, data.getLength("vals"));
                    assertEquals(1.5, data.getF64("vals[0]"));
                    assertEquals(2.5, data.getF64("vals[1]"));
                    assertEquals(3.5, data.getF64("vals[2]"));
                }
            }
        }
    }

    @Test
    void buildsABooleanSequenceValueAndSetsItIntoADynamicDataField() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    data.setU32("id", 1);

                    DynamicValue seq = DynamicValue.sequence();
                    seq.push(DynamicValue.bool(true));
                    seq.push(DynamicValue.bool(false));
                    seq.push(DynamicValue.bool(true));
                    data.setValue("flags", seq);
                    assertTrue(seq.isConsumed());

                    assertEquals(3, data.getLength("flags"));
                    assertEquals(true, data.getBool("flags[0]"));
                    assertEquals(false, data.getBool("flags[1]"));
                    assertEquals(true, data.getBool("flags[2]"));
                }
            }
        }
    }
}
