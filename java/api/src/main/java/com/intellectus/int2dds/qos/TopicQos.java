package com.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * Topic QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the
 * core".
 */
public class TopicQos {

    private Reliability reliability;
    private Durability durability;
    private History history;
    private Deadline deadline;
    private Liveliness liveliness;
    private DestinationOrder destinationOrder;
    private ResourceLimits resourceLimits;
    private TransportPriority transportPriority;
    private Lifespan lifespan;
    private Ownership ownership;
    private DataRepresentation dataRepresentation;

    public TopicQos() {}

    public Reliability getReliability() {
        return reliability;
    }

    public void setReliability(Reliability reliability) {
        this.reliability = reliability;
    }

    public Durability getDurability() {
        return durability;
    }

    public void setDurability(Durability durability) {
        this.durability = durability;
    }

    public History getHistory() {
        return history;
    }

    public void setHistory(History history) {
        this.history = history;
    }

    public Deadline getDeadline() {
        return deadline;
    }

    public void setDeadline(Deadline deadline) {
        this.deadline = deadline;
    }

    public Liveliness getLiveliness() {
        return liveliness;
    }

    public void setLiveliness(Liveliness liveliness) {
        this.liveliness = liveliness;
    }

    public DestinationOrder getDestinationOrder() {
        return destinationOrder;
    }

    public void setDestinationOrder(DestinationOrder destinationOrder) {
        this.destinationOrder = destinationOrder;
    }

    public ResourceLimits getResourceLimits() {
        return resourceLimits;
    }

    public void setResourceLimits(ResourceLimits resourceLimits) {
        this.resourceLimits = resourceLimits;
    }

    public TransportPriority getTransportPriority() {
        return transportPriority;
    }

    public void setTransportPriority(TransportPriority transportPriority) {
        this.transportPriority = transportPriority;
    }

    public Lifespan getLifespan() {
        return lifespan;
    }

    public void setLifespan(Lifespan lifespan) {
        this.lifespan = lifespan;
    }

    public Ownership getOwnership() {
        return ownership;
    }

    public void setOwnership(Ownership ownership) {
        this.ownership = ownership;
    }

    public DataRepresentation getDataRepresentation() {
        return dataRepresentation;
    }

    public void setDataRepresentation(DataRepresentation dataRepresentation) {
        this.dataRepresentation = dataRepresentation;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof TopicQos)) {
            return false;
        }
        TopicQos other = (TopicQos) o;
        return Objects.equals(reliability, other.reliability)
                && Objects.equals(durability, other.durability)
                && Objects.equals(history, other.history)
                && Objects.equals(deadline, other.deadline)
                && Objects.equals(liveliness, other.liveliness)
                && Objects.equals(destinationOrder, other.destinationOrder)
                && Objects.equals(resourceLimits, other.resourceLimits)
                && Objects.equals(transportPriority, other.transportPriority)
                && Objects.equals(lifespan, other.lifespan)
                && Objects.equals(ownership, other.ownership)
                && Objects.equals(dataRepresentation, other.dataRepresentation);
    }

    @Override
    public int hashCode() {
        return Objects.hash(
                reliability,
                durability,
                history,
                deadline,
                liveliness,
                destinationOrder,
                resourceLimits,
                transportPriority,
                lifespan,
                ownership,
                dataRepresentation);
    }

    @Override
    public String toString() {
        return "TopicQos{reliability="
                + reliability
                + ", durability="
                + durability
                + ", history="
                + history
                + ", deadline="
                + deadline
                + ", liveliness="
                + liveliness
                + ", destinationOrder="
                + destinationOrder
                + ", resourceLimits="
                + resourceLimits
                + ", transportPriority="
                + transportPriority
                + ", lifespan="
                + lifespan
                + ", ownership="
                + ownership
                + ", dataRepresentation="
                + dataRepresentation
                + "}";
    }
}
