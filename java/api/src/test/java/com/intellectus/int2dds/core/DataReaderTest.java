package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

class DataReaderTest {

    @Test
    void takeReturnsNullWhenNoData() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("NoDataTopic", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);
            assertNull(reader.take(), "empty cache -> null");
        }
    }

    @Test
    void takeRoundTripsAWrittenSample() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("ReadRoundTrip", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);

            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 7;
            sent.value = 3.5;
            sent.label = "hello";

            Sample<ConformanceRecord> got = null;
            long deadline = System.nanoTime() + 5_000_000_000L;
            while (System.nanoTime() < deadline) {
                w.write(sent);
                got = reader.take();
                if (got != null) {
                    break;
                }
                Thread.sleep(20);
            }

            assertNotNull(got, "no sample within 5s -- discovery or receive path");
            assertTrue(got.info().validData(), "sample should carry valid data");
            assertEquals(7, got.data().id);
            assertEquals(3.5, got.data().value);
            assertEquals("hello", got.data().label);
        }
    }

    @Test
    void takeGrowsBufferForLargeSample() throws InterruptedException {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("ReadGrowBuffer", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new);

            char[] big = new char[8000];
            java.util.Arrays.fill(big, 'x');
            ConformanceRecord sent = new ConformanceRecord();
            sent.id = 42;
            sent.value = 9.75;
            sent.label = new String(big);

            Sample<ConformanceRecord> got = null;
            long deadline = System.nanoTime() + 5_000_000_000L;
            while (System.nanoTime() < deadline) {
                w.write(sent);
                got = reader.take();
                if (got != null) {
                    break;
                }
                Thread.sleep(20);
            }

            assertNotNull(got, "no sample within 5s -- discovery or receive path");
            assertTrue(got.info().validData(), "sample should carry valid data");
            assertEquals(42, got.data().id);
            assertEquals(9.75, got.data().value);
            assertEquals(sent.label, got.data().label);
        }
    }

    @Test
    void aReaderHonoursItsQos() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("reader_qos", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();

            DataReaderQos qos = new DataReaderQos();
            qos.setReliability(new Reliability(ReliabilityKind.RELIABLE));

            DataReader<ConformanceRecord> reader =
                    sub.createDataReader(topic, ConformanceRecord::new, qos);
            DataReaderQos back = reader.getQos();

            assertEquals(ReliabilityKind.RELIABLE, back.getReliability().getKind());
        }
    }
}
