package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * De-risk test for the {@link DynamicValue} read/introspection slice: read
 * scalar and sequence fields of a populated {@link DynamicData} back out
 * through {@link DynamicData#getValue} and the new {@code DynamicValue}
 * extractors, mirroring {@link DynamicValueWriteTest}'s XML/registry setup.
 * Every {@code DynamicValue} obtained from {@code getValue}/{@code element}
 * is an independent clone, not consumed by anything here, so each is closed
 * explicitly -- proving that path too (a leak, not a double-free, if it were
 * skipped).
 */
class DynamicValueReadTest {

    private static final String XML = "<types>\n"
            + " <struct name=\"Rec\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"i32field\" type=\"int32\"/>\n"
            + "  <member name=\"strfield\" type=\"string\"/>\n"
            + "  <member name=\"f64field\" type=\"float64\"/>\n"
            + "  <member name=\"seqfield\" type=\"int32\" sequenceMaxLength=\"-1\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    @Test
    void getValueReadsScalarAndSequenceFieldsBackOut() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    data.setU32("id", 1);
                    data.setI32("i32field", 42);
                    data.setString("strfield", "hello");
                    data.setF64("f64field", 2.5);

                    DynamicValue seq = DynamicValue.sequence();
                    seq.push(DynamicValue.i32(10));
                    seq.push(DynamicValue.i32(20));
                    seq.push(DynamicValue.i32(30));
                    // seq (and every value pushed into it) is consumed by
                    // setValue below -- do not close it afterward.
                    data.setValue("seqfield", seq);

                    try (DynamicValue i32Val = data.getValue("i32field")) {
                        assertEquals(DynamicValueKind.INT32, i32Val.kind());
                        assertEquals(42, i32Val.asI32());
                    }

                    try (DynamicValue strVal = data.getValue("strfield")) {
                        assertEquals(DynamicValueKind.STRING, strVal.kind());
                        assertEquals("hello", strVal.asString());
                    }

                    try (DynamicValue f64Val = data.getValue("f64field")) {
                        assertEquals(DynamicValueKind.FLOAT64, f64Val.kind());
                        assertEquals(2.5, f64Val.asF64(), 0.0);
                    }

                    try (DynamicValue seqVal = data.getValue("seqfield")) {
                        assertEquals(DynamicValueKind.SEQUENCE, seqVal.kind());
                        assertEquals(3, seqVal.length());
                        try (DynamicValue elem0 = seqVal.element(0)) {
                            assertEquals(10, elem0.asI32());
                        }
                        try (DynamicValue elem2 = seqVal.element(2)) {
                            assertEquals(30, elem2.asI32());
                        }
                    }
                }
            }
        }
    }
}
