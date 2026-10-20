package kr.co.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * Subscriber QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the core".
 */
public class SubscriberQos {

    private Presentation presentation;
    private Partition partition;
    private GroupData groupData;

    public SubscriberQos() {}

    public Presentation getPresentation() {
        return presentation;
    }

    public void setPresentation(Presentation presentation) {
        this.presentation = presentation;
    }

    public Partition getPartition() {
        return partition;
    }

    public void setPartition(Partition partition) {
        this.partition = partition;
    }

    public GroupData getGroupData() {
        return groupData;
    }

    public void setGroupData(GroupData groupData) {
        this.groupData = groupData;
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
        return Objects.equals(presentation, other.presentation)
                && Objects.equals(partition, other.partition)
                && Objects.equals(groupData, other.groupData);
    }

    @Override
    public int hashCode() {
        return Objects.hash(presentation, partition, groupData);
    }

    @Override
    public String toString() {
        return "SubscriberQos{presentation="
                + presentation
                + ", partition="
                + partition
                + ", groupData="
                + groupData
                + "}";
    }
}
