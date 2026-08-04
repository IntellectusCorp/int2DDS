package com.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * Subscriber QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the
 * core".
 */
public class SubscriberQos {

    private Partition partition;

    public SubscriberQos() {}

    public Partition getPartition() {
        return partition;
    }

    public void setPartition(Partition partition) {
        this.partition = partition;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof SubscriberQos)) {
            return false;
        }
        SubscriberQos other = (SubscriberQos) o;
        return Objects.equals(partition, other.partition);
    }

    @Override
    public int hashCode() {
        return Objects.hash(partition);
    }

    @Override
    public String toString() {
        return "SubscriberQos{partition=" + partition + "}";
    }
}
