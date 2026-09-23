package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.util.HashMap;
import java.util.Map;
import org.junit.jupiter.api.Test;

/**
 * De-risk test for {@link DynamicValue#map} / {@link DynamicValue#insert} /
 * {@link DynamicValue#mapKey} / {@link DynamicValue#mapValue}: build a {@code
 * map<string,int32>} value entirely off {@code DynamicValue}, set it into a
 * {@code DynamicData} field, and read the key/value pairs back -- mirroring
 * {@code xml_dynamic_complex.rs}'s {@code scores} build/read. {@link
 * DynamicValue#insert} consumes TWO handles (key and value) on success; a
 * clean run (no JVM crash) is itself part of what this proves -- a wrongly
 * consumed-on-failure or double-closed handle here would SIGSEGV.
 */
class DynamicValueMapTest {

    private static final String XML = "<types>\n"
            + " <struct name=\"Rec\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"scores\" type=\"int32\" key_type=\"string\" mapMaxLength=\"8\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    @Test
    void buildsAMapValueAndReadsKeyValuePairsBackOut() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);

            try (DynamicTypeSupport support = registry.getTypeSupport("Rec")) {
                try (DynamicData data = DynamicData.create(support)) {
                    data.setU32("id", 1);

                    DynamicValue m = DynamicValue.map();
                    m.insert(DynamicValue.string("alpha"), DynamicValue.i32(10));
                    m.insert(DynamicValue.string("beta"), DynamicValue.i32(20));
                    // m (and every key/value inserted into it) is consumed by
                    // setValue below -- do not close it afterward.
                    data.setValue("scores", m);

                    try (DynamicValue scores = data.getValue("scores")) {
                        assertEquals(DynamicValueKind.MAP, scores.kind());
                        assertEquals(2, scores.length());

                        Map<String, Integer> read = new HashMap<>();
                        for (int i = 0; i < scores.length(); i++) {
                            try (DynamicValue key = scores.mapKey(i);
                                    DynamicValue value = scores.mapValue(i)) {
                                read.put(key.asString(), value.asI32());
                            }
                        }

                        Map<String, Integer> expected = new HashMap<>();
                        expected.put("alpha", 10);
                        expected.put("beta", 20);
                        assertEquals(expected, read);
                    }
                }
            }
        }
    }
}
