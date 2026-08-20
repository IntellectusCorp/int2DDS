package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class SubscriberTest {

    @Test
    void createAndCloseSubscriber() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Subscriber sub = p.createSubscriber();
            assertNotNull(sub);
            assertFalse(sub.isClosed());
            sub.close();
            assertTrue(sub.isClosed());
        } finally {
            p.close();
        }
    }
}
