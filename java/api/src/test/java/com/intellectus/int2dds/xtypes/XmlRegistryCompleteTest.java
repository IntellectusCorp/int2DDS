package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.exceptions.DdsException;
import java.io.IOException;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * De-risk test for the XmlTypeRegistry completion increment: {@code
 * fromFile}/{@code loadFile}/{@code getTypeObject}/{@code typeName}. Reuses
 * {@link DynamicWriteTest}'s Telemetry XML and {@link DynamicDataTest}'s
 * CdrWriter encoding approach to prove the XML-to-TypeObject-to-decode bridge
 * end to end, not just that the calls return OK.
 */
class XmlRegistryCompleteTest {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    // ffi/src/error.rs: INT2DDS_RET_DYNAMIC_FIELD_NOT_FOUND. No dedicated Java
    // exception type exists for the 200-204 dynamic-reflection family yet --
    // ReturnCodes.check still throws a generic DdsException carrying this code.
    private static final int RET_DYNAMIC_FIELD_NOT_FOUND = 200;

    // Same layout as DynamicWriteTest.XML (default extensibility = appendable,
    // per dds/src/config/xml/parser.rs's extensibility_attr).
    private static final String TELEMETRY_XML = "<types>\n"
            + " <struct name=\"Telemetry\">\n"
            + "  <member name=\"id\" type=\"uint32\" key=\"true\"/>\n"
            + "  <member name=\"temperature\" type=\"float32\"/>\n"
            + "  <member name=\"active\" type=\"boolean\"/>\n"
            + "  <member name=\"label\" type=\"string\"/>\n"
            + "  <member name=\"count\" type=\"int64\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    // A different struct so fromFile/loadFile's typeCount grows from 1 to 2.
    private static final String OTHER_XML = "<types>\n"
            + " <struct name=\"OtherRecord\">\n"
            + "  <member name=\"x\" type=\"int32\"/>\n"
            + " </struct>\n"
            + "</types>\n";

    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void getTypeObjectBridgesXmlToTheDecodePath() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(TELEMETRY_XML);

            try (TypeObject typeObject = registry.getTypeObject("Telemetry")) {
                int id = 7;
                float temperature = 23.5f;
                boolean active = true;
                String label = "sensor-A";
                long count = -100L;

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(Extensibility.APPENDABLE,
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    int token = w.dheaderBegin();
                    w.writeU32(id);
                    w.writeF32(temperature);
                    w.writeBool(active);
                    w.writeString(label);
                    w.writeI64(count);
                    w.dheaderFinalize(token);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(id, data.getU32("id"));
                    assertEquals(temperature, data.getF32("temperature"), 0.0f);
                    assertEquals(active, data.getBool("active"));
                    assertEquals(label, data.getString("label"));
                    assertEquals(count, data.getI64("count"));
                }
            }

            DdsException e = assertThrows(DdsException.class,
                    () -> registry.getTypeObject("NoSuchType"));
            assertEquals(RET_DYNAMIC_FIELD_NOT_FOUND, e.getCode());
        } finally {
            p.close();
        }
    }

    @Test
    void typeNameReadsTheLoadedNameAndRejectsOutOfRange() {
        try (XmlTypeRegistry registry = new XmlTypeRegistry()) {
            registry.loadString(TELEMETRY_XML);

            assertEquals("Telemetry", registry.typeName(0));

            DdsException e = assertThrows(DdsException.class, () -> registry.typeName(1));
            assertEquals(DdsException.RET_INVALID_ARGUMENT, e.getCode());
        }
    }

    @Test
    void fromFileAndLoadFileLoadTypesFromDisk(@TempDir Path tmp) throws IOException {
        Path telemetryFile = tmp.resolve("telemetry.xml");
        Files.write(telemetryFile, TELEMETRY_XML.getBytes(UTF8));

        try (XmlTypeRegistry registry = XmlTypeRegistry.fromFile(telemetryFile.toString())) {
            assertEquals(1, registry.typeCount());

            Path otherFile = tmp.resolve("other.xml");
            Files.write(otherFile, OTHER_XML.getBytes(UTF8));

            registry.loadFile(otherFile.toString());
            assertEquals(2, registry.typeCount());
        }
    }
}
