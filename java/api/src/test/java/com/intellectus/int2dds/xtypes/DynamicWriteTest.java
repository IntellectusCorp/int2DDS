package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.exceptions.DdsException;
import org.junit.jupiter.api.Test;

/**
 * De-risk test for the dynamic write path (no networking): loads a runtime
 * type from XML, builds a {@link DynamicData} instance from it, and exercises
 * every scalar setter. Also proves a bad field path fails loudly instead of
 * silently, which the read path's tests never have to check.
 */
class DynamicWriteTest {

    // ffi/src/error.rs: INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND. No dedicated Java
    // exception type exists for the 200-204 dynamic-reflection family yet --
    // ReturnCodes.check still throws a generic DdsException carrying this code.
    private static final int RET_DYNAMIC_FIELD_NOT_FOUND = 200;

    private static final String XML = "<types>\n"
            + " <struct name=\"Telemetry\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"temperature\" type=\"float32\"/>\n"
            + "  <member name=\"active\" type=\"boolean\"/>\n"
            + "  <member name=\"label\" type=\"string\"/>\n"
            + "  <member name=\"count\" type=\"int64\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    @Test
    void buildsAndSetsEveryFieldFromAnXmlLoadedType() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(XML);
            assertEquals(1, registry.typeCount());

            try (DynamicTypeSupport support = registry.getTypeSupport("Telemetry")) {
                try (DynamicData data = DynamicData.create(support)) {
                    assertDoesNotThrow(() -> data.setU32("id", 7));
                    assertDoesNotThrow(() -> data.setF32("temperature", 23.5f));
                    assertDoesNotThrow(() -> data.setBool("active", true));
                    assertDoesNotThrow(() -> data.setString("label", "sensor-A"));
                    assertDoesNotThrow(() -> data.setI64("count", -100L));

                    DdsException e = assertThrows(DdsException.class, () -> data.setU32("nope", 1));
                    assertEquals(RET_DYNAMIC_FIELD_NOT_FOUND, e.getCode());
                }
            }
        }
    }
}
