package com.intellectus.int2dds.conditions;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DataWriter;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Publisher;
import com.intellectus.int2dds.core.Sample;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

class ReadConditionTest {

    // Mirrors DomainParticipantTest.testDomain(), package-private to core and
    // unreachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void readConditionTriggersWhenAMatchingSampleArrives() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("ReadCondTrigger", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(topic);
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            try (WaitSet ws = new WaitSet();
                    ReadCondition rc = reader.createReadCondition(
                            SampleState.ANY, ViewState.ANY, InstanceState.ANY)) {
                ws.attach(rc);

                ConformanceRecord sent = new ConformanceRecord();
                sent.id = 11;
                sent.value = 1.0;
                sent.label = "rc";

                List<Condition> hit = new ArrayList<Condition>();
                long deadline = System.nanoTime() + 5_000_000_000L;
                while (System.nanoTime() < deadline && hit.isEmpty()) {
                    w.write(sent);
                    hit = ws.await(200);
                }

                assertTrue(!hit.isEmpty(), "read condition should have triggered within 5s");
                assertTrue(hit.contains(rc));
                assertTrue(rc.triggerValue()); // type-correct accessor; must not segfault

                Sample<ConformanceRecord> got = reader.take();
                assertNotNull(got, "triggered read condition implies a sample is queued");
                assertTrue(got.info().validData());

                ws.detach(rc);
            }
        }
    }

    @Test
    void queryConditionCreateAttachSetParamsDetachDoesNotCrash() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("QueryCondSmoke", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            byte[] expr = "id >= %0".getBytes(StandardCharsets.UTF_8);
            byte[][] params = {"0".getBytes(StandardCharsets.UTF_8)};

            try (WaitSet ws = new WaitSet();
                    QueryCondition qc = reader.createQueryCondition(
                            SampleState.ANY, ViewState.ANY, InstanceState.ANY, expr, params)) {
                assertDoesNotThrow(() -> {
                    qc.setQueryParameters(new byte[][] {"1".getBytes(StandardCharsets.UTF_8)});
                    ws.attach(qc);
                    qc.triggerValue(); // inherited type-correct accessor; must not segfault
                    ws.detach(qc);
                });
            }
        }
    }
}
