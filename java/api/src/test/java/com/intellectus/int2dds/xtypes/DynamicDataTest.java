package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.intellectus.int2dds.cdr.CdrWriter;
import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.nio.ByteOrder;
import org.junit.jupiter.api.Test;

/**
 * End-to-end proof of the DynamicData read path: build a runtime type with
 * {@link TypeInfo}, encode a sample with the same {@link CdrWriter} the write
 * path uses, decode it back with {@link DomainParticipant#dynamicDataFromSample},
 * and read the fields back through the {@link DynamicData} object API.
 */
class DynamicDataTest {

    /** The domain this JVM's tests use, isolated from other runs. */
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void readsFieldsWrittenThroughTheSameCdrEncoding() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try (TypeInfo typeInfo = new TypeInfo("ConformanceRecord", Extensibility.APPENDABLE)) {
            typeInfo.addField("id", FieldType.INT32, 0);
            typeInfo.addField("value", FieldType.FLOAT64, 0);
            typeInfo.addField("label", FieldType.STRING, 0);

            try (TypeObject typeObject = typeInfo.toTypeObject()) {
                ConformanceRecord sent = new ConformanceRecord();
                sent.id = 7;
                sent.value = 3.5d;
                sent.label = "hello";

                byte[] serialized;
                try (CdrWriter w = CdrWriter.acquire(sent.extensibility(),
                        ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN, false)) {
                    sent.serializeCdr(w);
                    serialized = w.toBytes();
                }

                try (DynamicData data = p.dynamicDataFromSample(serialized, typeObject)) {
                    assertEquals(7, data.getI32("id"));
                    assertEquals(3.5d, data.getF64("value"), 0.0d);
                    assertEquals("hello", data.getString("label"));
                }
            }
        } finally {
            p.close();
        }
    }
}
