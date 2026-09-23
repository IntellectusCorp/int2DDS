package com.intellectus.int2dds.xtypes;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Exercises {@link DynamicSample} against a REAL serialized sample: {@link
 * ConformanceRecord} (id: int32, value: float64, label: string, all flat)
 * written through an ordinary typed {@link DataWriter} and pulled back off
 * the wire as raw CDR bytes with {@link DataReader#takeSerialized}, decoded
 * against a {@link TypeObject} built independently through {@link TypeInfo}
 * -- the same field order/types {@code CdrConformanceTest} uses, kept in
 * step with {@link ConformanceRecord#serializeCdr}.
 */
class DynamicSamplePeekTest {

    private static final long DATA_TIMEOUT_NANOS = 5_000_000_000L;

    // Mirrors DomainParticipantTest.testDomain(), package-private to core and
    // so not reachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    private static TypeObject conformanceRecordTypeObject() {
        try (TypeInfo ti = new TypeInfo("ConformanceRecord", Extensibility.APPENDABLE)) {
            ti.addField("id", FieldType.INT32, 0);
            ti.addField("value", FieldType.FLOAT64, 0);
            ti.addStringField("label", 0, 0);
            return ti.toTypeObject();
        }
    }

    @Test
    void peekedFieldsMatchWhatWasWrittenOnARealSerializedSample() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain());
                TypeObject type = conformanceRecordTypeObject()) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("DynamicSamplePeek", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            DataReader<ConformanceRecord> r = sub.createDataReader(topic, ConformanceRecord::new);

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 4711;
            sent.value = -98.75d;
            sent.label = "peek-me";

            byte[] cdr = null;
            long deadline = System.nanoTime() + DATA_TIMEOUT_NANOS;
            while (System.nanoTime() < deadline) {
                w.write(sent);
                cdr = r.takeSerialized();
                if (cdr != null && cdr.length > 0) {
                    break;
                }
                Thread.sleep(20);
            }
            assertNotNull(cdr, "no serialized sample within 5s -- discovery or receive path");
            assertTrue(cdr.length > 0, "takeSerialized should return non-empty CDR bytes");
            byte[] sample = cdr;

            assertEquals(sent.id, DynamicSample.getI32(sample, type, "id"));
            assertEquals(sent.value, DynamicSample.getF64(sample, type, "value"), 0.0d);
            assertEquals(sent.label, DynamicSample.getString(sample, type, "label"));

            // Real can-fail: an unknown field name throws rather than
            // silently returning a garbage decode.
            assertThrows(DdsException.class,
                    () -> DynamicSample.getI32(sample, type, "no_such_field"));

            r.setListener(null, null);
        }
    }

    @Test
    void nullArgumentsAreRejected() {
        try (TypeObject type = conformanceRecordTypeObject()) {
            byte[] cdr = new byte[] {0, 0, 0, 0};
            assertThrows(NullPointerException.class, () -> DynamicSample.getI32(null, type, "id"));
            assertThrows(NullPointerException.class, () -> DynamicSample.getI32(cdr, null, "id"));
            assertThrows(NullPointerException.class, () -> DynamicSample.getI32(cdr, type, null));
        }
    }
}
