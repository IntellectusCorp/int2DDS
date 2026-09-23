package com.intellectus.int2dds.core;

import static com.intellectus.int2dds.core.DomainParticipantTest.testDomain;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Deadline;
import com.intellectus.int2dds.qos.ParticipantQos;
import com.intellectus.int2dds.qos.Partition;
import com.intellectus.int2dds.qos.Property;
import com.intellectus.int2dds.qos.PublisherQos;
import com.intellectus.int2dds.qos.SubscriberQos;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.types.ConformanceRecord;
import java.time.Duration;
import org.junit.jupiter.api.Test;

/**
 * Exercises the runtime {@code setQos} plumbing added to all six entity
 * types.
 *
 * <p>{@code Deadline} is the round-trip oracle on {@link DataWriter}, {@link
 * DataReader} (and the change attempted on {@link Topic}): unlike {@code
 * LatencyBudget} and {@code TransportPriority}, which this core's own {@code
 * check_unsupported_policies} rejects outright for any non-default value
 * (dds/src/dcps/publication/qos/mod.rs, subscription/qos/mod.rs,
 * topic/qos/mod.rs -- confirmed the hard way: an earlier version of this test
 * used {@code LatencyBudget} and failed with {@code DdsUnsupportedException}),
 * {@code Deadline} is both supported and absent from every one of those
 * types' {@code check_immutable_change} lists, so a non-default value is
 * accepted at create time and a further change is accepted after the entity
 * is enabled. Publisher and Subscriber use {@code Partition} (supported,
 * outside {@code Presentation}, their one immutable policy); DomainParticipant
 * uses {@code Property}, since {@code UserData} is that type's one
 * unsupported policy (dds/src/dcps/domain/qos/mod.rs).
 *
 * <p>Publisher, Subscriber, DomainParticipant and Topic have no {@code
 * getQos()} yet, so those are treated as a can-fail round trip through the
 * absence of a thrown exception, which {@link
 * com.intellectus.int2dds.internal.ReturnCodes#check} would raise on any
 * non-OK status.
 */
class EntitySetQosTest {

    @Test
    void setQosOnDataWriterAppliesAMutableDeadlineChange() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("setqos_writer", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(t);

            Deadline target = new Deadline(Duration.ofMillis(250));
            assertNotEquals(target, w.getQos().getDeadline());

            DataWriterQos qos = w.getQos();
            qos.setDeadline(target);
            w.setQos(qos);

            assertEquals(target, w.getQos().getDeadline());
        } finally {
            p.close();
        }
    }

    @Test
    void setQosOnDataReaderAppliesAMutableDeadlineChange() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("setqos_reader", new ConformanceRecord());
            Subscriber sub = p.createSubscriber();
            DataReader<ConformanceRecord> r = sub.createDataReader(t, ConformanceRecord::new);

            Deadline target = new Deadline(Duration.ofMillis(250));
            assertNotEquals(target, r.getQos().getDeadline());

            DataReaderQos qos = r.getQos();
            qos.setDeadline(target);
            r.setQos(qos);

            assertEquals(target, r.getQos().getDeadline());
        } finally {
            p.close();
        }
    }

    @Test
    void setQosOnPublisherAppliesAMutablePartitionChange() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Publisher pub = p.createPublisher();
            PublisherQos qos = new PublisherQos();
            qos.setPartition(new Partition(new String[] {"setqos-partition"}));
            pub.setQos(qos);
        } finally {
            p.close();
        }
    }

    @Test
    void setQosOnSubscriberAppliesAMutablePartitionChange() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Subscriber sub = p.createSubscriber();
            SubscriberQos qos = new SubscriberQos();
            qos.setPartition(new Partition(new String[] {"setqos-partition"}));
            sub.setQos(qos);
        } finally {
            p.close();
        }
    }

    @Test
    void setQosOnParticipantAppliesAPropertyChange() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            ParticipantQos qos = new ParticipantQos();
            Property property = new Property();
            property.add("setqos.test.key", "value", false);
            qos.setProperty(property);
            p.setQos(qos);
        } finally {
            p.close();
        }
    }

    @Test
    void setQosOnTopicAppliesAMutableDeadlineChange() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("setqos_topic", new ConformanceRecord());
            TopicQos qos = new TopicQos();
            qos.setDeadline(new Deadline(Duration.ofMillis(250)));
            t.setQos(qos);
        } finally {
            p.close();
        }
    }

    @Test
    void setQosRejectsNullOnEveryEntityType() {
        DomainParticipant p = new DomainParticipant(testDomain());
        try {
            Topic<ConformanceRecord> t = p.createTopic("setqos_null", new ConformanceRecord());
            Publisher pub = p.createPublisher();
            Subscriber sub = p.createSubscriber();
            DataWriter<ConformanceRecord> w = pub.createDataWriter(t);
            DataReader<ConformanceRecord> r = sub.createDataReader(t, ConformanceRecord::new);

            assertThrows(NullPointerException.class, () -> p.setQos(null));
            assertThrows(NullPointerException.class, () -> t.setQos(null));
            assertThrows(NullPointerException.class, () -> pub.setQos(null));
            assertThrows(NullPointerException.class, () -> sub.setQos(null));
            assertThrows(NullPointerException.class, () -> w.setQos(null));
            assertThrows(NullPointerException.class, () -> r.setQos(null));
        } finally {
            p.close();
        }
    }
}
