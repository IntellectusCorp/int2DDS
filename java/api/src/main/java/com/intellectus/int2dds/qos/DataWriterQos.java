package com.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * DataWriter QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the
 * core". {@code dataFrag} is an {@code Integer} rather than an {@code int} so
 * that null is distinguishable from zero; it is an int2DDS extension (DATA_FRAG
 * max fragment size), not a standard DDS policy.
 */
public class DataWriterQos {

    private Reliability reliability;
    private Durability durability;
    private History history;
    private Ownership ownership;
    private OwnershipStrength ownershipStrength;
    private ResourceLimits resourceLimits;
    private Lifespan lifespan;
    private DestinationOrder destinationOrder;
    private LatencyBudget latencyBudget;
    private TransportPriority transportPriority;
    private UserData userData;
    private WriterDataLifecycle writerDataLifecycle;
    private DataRepresentation dataRepresentation;
    private Deadline deadline;
    private Liveliness liveliness;
    private Integer dataFrag;

    public DataWriterQos() {}

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

    public Ownership getOwnership() {
        return ownership;
    }

    public void setOwnership(Ownership ownership) {
        this.ownership = ownership;
    }

    public OwnershipStrength getOwnershipStrength() {
        return ownershipStrength;
    }

    public void setOwnershipStrength(OwnershipStrength ownershipStrength) {
        this.ownershipStrength = ownershipStrength;
    }

    public ResourceLimits getResourceLimits() {
        return resourceLimits;
    }

    public void setResourceLimits(ResourceLimits resourceLimits) {
        this.resourceLimits = resourceLimits;
    }

    public Lifespan getLifespan() {
        return lifespan;
    }

    public void setLifespan(Lifespan lifespan) {
        this.lifespan = lifespan;
    }

    public DestinationOrder getDestinationOrder() {
        return destinationOrder;
    }

    public void setDestinationOrder(DestinationOrder destinationOrder) {
        this.destinationOrder = destinationOrder;
    }

    public LatencyBudget getLatencyBudget() {
        return latencyBudget;
    }

    public void setLatencyBudget(LatencyBudget latencyBudget) {
        this.latencyBudget = latencyBudget;
    }

    public TransportPriority getTransportPriority() {
        return transportPriority;
    }

    public void setTransportPriority(TransportPriority transportPriority) {
        this.transportPriority = transportPriority;
    }

    public UserData getUserData() {
        return userData;
    }

    public void setUserData(UserData userData) {
        this.userData = userData;
    }

    public WriterDataLifecycle getWriterDataLifecycle() {
        return writerDataLifecycle;
    }

    public void setWriterDataLifecycle(WriterDataLifecycle writerDataLifecycle) {
        this.writerDataLifecycle = writerDataLifecycle;
    }

    public DataRepresentation getDataRepresentation() {
        return dataRepresentation;
    }

    public void setDataRepresentation(DataRepresentation dataRepresentation) {
        this.dataRepresentation = dataRepresentation;
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

    /** May be null, meaning DATA_FRAG max fragment size is left to the core. */
    public Integer getDataFrag() {
        return dataFrag;
    }

    public void setDataFrag(Integer dataFrag) {
        this.dataFrag = dataFrag;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof DataWriterQos)) {
            return false;
        }
        DataWriterQos other = (DataWriterQos) o;
        return Objects.equals(reliability, other.reliability)
                && Objects.equals(durability, other.durability)
                && Objects.equals(history, other.history)
                && Objects.equals(ownership, other.ownership)
                && Objects.equals(ownershipStrength, other.ownershipStrength)
                && Objects.equals(resourceLimits, other.resourceLimits)
                && Objects.equals(lifespan, other.lifespan)
                && Objects.equals(destinationOrder, other.destinationOrder)
                && Objects.equals(latencyBudget, other.latencyBudget)
                && Objects.equals(transportPriority, other.transportPriority)
                && Objects.equals(userData, other.userData)
                && Objects.equals(writerDataLifecycle, other.writerDataLifecycle)
                && Objects.equals(dataRepresentation, other.dataRepresentation)
                && Objects.equals(deadline, other.deadline)
                && Objects.equals(liveliness, other.liveliness)
                && Objects.equals(dataFrag, other.dataFrag);
    }

    @Override
    public int hashCode() {
        return Objects.hash(
                reliability,
                durability,
                history,
                ownership,
                ownershipStrength,
                resourceLimits,
                lifespan,
                destinationOrder,
                latencyBudget,
                transportPriority,
                userData,
                writerDataLifecycle,
                dataRepresentation,
                deadline,
                liveliness,
                dataFrag);
    }

    @Override
    public String toString() {
        return "DataWriterQos{reliability="
                + reliability
                + ", durability="
                + durability
                + ", history="
                + history
                + ", ownership="
                + ownership
                + ", ownershipStrength="
                + ownershipStrength
                + ", resourceLimits="
                + resourceLimits
                + ", lifespan="
                + lifespan
                + ", destinationOrder="
                + destinationOrder
                + ", latencyBudget="
                + latencyBudget
                + ", transportPriority="
                + transportPriority
                + ", userData="
                + userData
                + ", writerDataLifecycle="
                + writerDataLifecycle
                + ", dataRepresentation="
                + dataRepresentation
                + ", deadline="
                + deadline
                + ", liveliness="
                + liveliness
                + ", dataFrag="
                + dataFrag
                + "}";
    }
}
