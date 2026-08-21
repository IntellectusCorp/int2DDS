package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import com.intellectus.int2dds.qos.DataRepresentation;
import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * {@link DataWriter#getDataRepresentation()} reports a valid wire encoding and
 * agrees with the same value read back through {@link DataWriter#getQos()} —
 * two independent native paths that must not disagree.
 */
class WriterDataRepresentationTest {

    @Test
    void reportsAValidKindConsistentWithGetQos() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("datarepr", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(t);

            DataRepresentationKind kind = w.getDataRepresentation();
            assertNotNull(kind);

            // Cross-check against the QoS read-back: when the policy is
            // populated the two paths must report the same wire encoding.
            DataRepresentation fromQos = w.getQos().getDataRepresentation();
            if (fromQos != null) {
                assertEquals(fromQos.getKind(), kind);
            }
        } finally {
            p.close();
        }
    }
}
