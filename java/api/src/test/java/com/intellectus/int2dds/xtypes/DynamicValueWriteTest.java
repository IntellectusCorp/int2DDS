package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for the {@link DynamicValue} write slice: build a sequence
 * value from scalars entirely off the {@code DynamicValue} API, set it into a
 * {@link DynamicData} field, and read it back through the existing indexed
 * getters -- mirroring {@code ffi/tests/xml_dynamic_complex.rs}'s
 * {@code samples} field. A clean run (no JVM crash) is itself part of what
 * this proves: {@link DynamicValue#push} and {@link
 * DynamicData#setValue} both consume the handle they are given, and getting
 * that wrong is a double-free, not merely a wrong answer.
 */
class DynamicValueWriteTest {

    private static final String XML = "<types>\n"
            + " <struct name=\"Rec\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"vals\" type=\"int32\" sequenceMaxLength=\"-1\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    @Test
    void buildsASequenceValueAndSetsItIntoADynamicDataField() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    data.setU32("id", 1);

                    DynamicValue seq = DynamicValue.sequence();
                    seq.push(DynamicValue.i32(10));
                    seq.push(DynamicValue.i32(20));
                    seq.push(DynamicValue.i32(30));
                    // seq (and every value pushed into it) is consumed by
                    // setValue below -- do not close it, or the pushed
                    // scalars, afterward.
                    data.setValue("vals", seq);
                    assertTrue(seq.isConsumed());

                    assertEquals(3, data.getLength("vals"));
                    assertEquals(10, data.getI32("vals[0]"));
                    assertEquals(20, data.getI32("vals[1]"));
                    assertEquals(30, data.getI32("vals[2]"));

                    // A consumed value's close() is a no-op, not a
                    // double-free -- proven by not crashing the JVM.
                    seq.close();
                    assertFalse(data.isClosed());
                }
            }
        }
    }

    @Test
    void pushIntoAConsumedSequenceThrowsInsteadOfReusingADeadHandle() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    DynamicValue seq = DynamicValue.sequence();
                    seq.push(DynamicValue.i32(1));
                    data.setValue("vals", seq);

                    assertThrows(IllegalStateException.class, () -> seq.push(DynamicValue.i32(2)));
                }
            }
        }
    }
}
