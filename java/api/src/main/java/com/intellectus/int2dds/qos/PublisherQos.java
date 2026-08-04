package com.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * Publisher QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the
 * core".
 */
public class PublisherQos {

    private Partition partition;

    public PublisherQos() {}

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
        if (!(o instanceof PublisherQos)) {
            return false;
        }
        PublisherQos other = (PublisherQos) o;
        return Objects.equals(partition, other.partition);
    }

    @Override
    public int hashCode() {
        return Objects.hash(partition);
    }

    @Override
    public String toString() {
        return "PublisherQos{partition=" + partition + "}";
    }
}
