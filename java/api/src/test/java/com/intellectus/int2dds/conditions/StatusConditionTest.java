package com.intellectus.int2dds.conditions;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;

import com.intellectus.int2dds.core.DataReader;
import com.intellectus.int2dds.core.DomainParticipant;
import com.intellectus.int2dds.core.Subscriber;
import com.intellectus.int2dds.core.Topic;
import com.intellectus.int2dds.status.StatusMask;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

class StatusConditionTest {

    // Mirrors DomainParticipantTest.testDomain(), package-private to core and
    // unreachable from here.
    private static int testDomain() {
        return Integer.parseInt(System.getProperty("int2dds.test.domain", "137"));
    }

    @Test
    void enabledStatusesRoundTripsThroughSetAndGet() {
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("StatusCondMask", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            try (StatusCondition sc = reader.getStatusCondition()) {
                int mask = StatusMask.DATA_AVAILABLE | StatusMask.SUBSCRIPTION_MATCHED;
                sc.setEnabledStatuses(StatusMask.of(mask));
                assertEquals(mask, sc.enabledStatuses().bits());
            }
        }
    }

    @Test
    void attachAndDetachFromWaitSetDoesNotCrash() {
        // Exercises the type-correct statuscondition accessor path: a wrong
        // (generic condition_get_trigger_value) accessor here would segfault
        // reading a thin StatusCondition handle as a fat Arc<dyn Condition>.
        try (DomainParticipant p = new DomainParticipant(testDomain())) {
            Topic<ConformanceRecord> topic = p.createTopic("StatusCondAttach", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> reader = sub.createDataReader(topic, ConformanceRecord::new);

            try (WaitSet ws = new WaitSet(); StatusCondition sc = reader.getStatusCondition()) {
                sc.setEnabledStatuses(StatusMask.of(StatusMask.SUBSCRIPTION_MATCHED));
                assertDoesNotThrow(() -> {
                    ws.attach(sc);
                    sc.triggerValue(); // type-correct accessor; must not segfault
                    ws.detach(sc);
                });
            }
        }
    }
}
