package com.intellectus.int2dds.core;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertNotNull;

import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.conditions.WaitSet;
import com.intellectus.int2dds.types.ConformanceRecord;
import org.junit.jupiter.api.Test;

/**
 * Covers {@code getStatusCondition()} on the four container entity types
 * (Publisher, Subscriber, DomainParticipant, Topic) that mirror {@link
 * DataReader#getStatusCondition} and {@link DataWriter#getStatusCondition}.
 * Each case attaches the returned condition to a real {@link WaitSet} and
 * detaches it again -- a bad handle would throw or segfault on attach.
 */
class ContainerStatusConditionTest {

    @Test
    void publisherStatusConditionAttachesToWaitSet() {
        try (DomainParticipant p = new DomainParticipant(DomainParticipantTest.testDomain())) {
            Publisher pub = p.createPublisher();
            assertDoesNotThrow(() -> {
                try (WaitSet ws = new WaitSet(); StatusCondition sc = pub.getStatusCondition()) {
                    assertNotNull(sc);
                    ws.attach(sc);
                    ws.await(100L);
                    ws.detach(sc);
                }
            });
        }
    }

    @Test
    void subscriberStatusConditionAttachesToWaitSet() {
        try (DomainParticipant p = new DomainParticipant(DomainParticipantTest.testDomain())) {
            Subscriber sub = p.createSubscriber();
            assertDoesNotThrow(() -> {
                try (WaitSet ws = new WaitSet(); StatusCondition sc = sub.getStatusCondition()) {
                    assertNotNull(sc);
                    ws.attach(sc);
                    ws.await(100L);
                    ws.detach(sc);
                }
            });
        }
    }

    @Test
    void participantStatusConditionAttachesToWaitSet() {
        try (DomainParticipant p = new DomainParticipant(DomainParticipantTest.testDomain())) {
            assertDoesNotThrow(() -> {
                try (WaitSet ws = new WaitSet(); StatusCondition sc = p.getStatusCondition()) {
                    assertNotNull(sc);
                    ws.attach(sc);
                    ws.await(100L);
                    ws.detach(sc);
                }
            });
        }
    }

    @Test
    void topicStatusConditionAttachesToWaitSet() {
        try (DomainParticipant p = new DomainParticipant(DomainParticipantTest.testDomain())) {
            Topic<ConformanceRecord> topic =
                    p.createTopic("ContainerStatusConditionTopic", new ConformanceRecord());
            assertDoesNotThrow(() -> {
                try (WaitSet ws = new WaitSet(); StatusCondition sc = topic.getStatusCondition()) {
                    assertNotNull(sc);
                    ws.attach(sc);
                    ws.await(100L);
                    ws.detach(sc);
                }
            });
        }
    }
}
