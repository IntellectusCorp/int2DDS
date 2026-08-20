package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertNull;

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
}
