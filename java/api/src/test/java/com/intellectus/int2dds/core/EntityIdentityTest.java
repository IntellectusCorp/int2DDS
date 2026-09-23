package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/** Entity-identity ops: {@code getInstanceHandle} and {@code containsEntity}. */
class EntityIdentityTest {

    @Test
    void getInstanceHandleReturnsDistinctNonZeroHandles() {
        try (DomainParticipant participant = new DomainParticipant(testDomain());
                Publisher pub = participant.createPublisher();
                Subscriber sub = participant.createSubscriber()) {
            InstanceHandle pubHandle = pub.getInstanceHandle();
            InstanceHandle subHandle = sub.getInstanceHandle();

            assertEquals(16, pubHandle.bytes().length);
            assertEquals(16, subHandle.bytes().length);
            assertNotEquals(new InstanceHandle(new byte[16]), pubHandle);
            assertNotEquals(new InstanceHandle(new byte[16]), subHandle);
            assertNotEquals(pubHandle, subHandle);
        }
    }

    @Test
    void containsEntityFindsOwnPublisherAndSubscriber() {
        try (DomainParticipant participant = new DomainParticipant(testDomain());
                Publisher pub = participant.createPublisher();
                Subscriber sub = participant.createSubscriber()) {
            assertTrue(participant.containsEntity(pub.getInstanceHandle()));
            assertTrue(participant.containsEntity(sub.getInstanceHandle()));
        }
    }

    @Test
    void containsEntityIsFalseForNilHandle() {
        try (DomainParticipant participant = new DomainParticipant(testDomain())) {
            assertFalse(participant.containsEntity(new InstanceHandle(new byte[16])));
        }
    }
}
