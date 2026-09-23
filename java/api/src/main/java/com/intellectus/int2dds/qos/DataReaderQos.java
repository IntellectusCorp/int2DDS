package com.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * DataReader QoS.
 *
 * <p>Every field is nullable and defaults to null, meaning "leave it to the
 * core".
 */
public class DataReaderQos {

    private Reliability reliability;
    private Durability durability;
    private History history;
    private Ownership ownership;
    private ResourceLimits resourceLimits;
    private DestinationOrder destinationOrder;
    private TimeBasedFilter timeBasedFilter;
    private LatencyBudget latencyBudget;
    private UserData userData;
    private ReaderDataLifecycle readerDataLifecycle;
    private DataRepresentation dataRepresentation;
    private Deadline deadline;
    private Liveliness liveliness;

    public DataReaderQos() {}

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

    public ResourceLimits getResourceLimits() {
        return resourceLimits;
    }

    public void setResourceLimits(ResourceLimits resourceLimits) {
        this.resourceLimits = resourceLimits;
    }

    public DestinationOrder getDestinationOrder() {
        return destinationOrder;
    }

    public void setDestinationOrder(DestinationOrder destinationOrder) {
        this.destinationOrder = destinationOrder;
    }

    public TimeBasedFilter getTimeBasedFilter() {
        return timeBasedFilter;
    }

    public void setTimeBasedFilter(TimeBasedFilter timeBasedFilter) {
        this.timeBasedFilter = timeBasedFilter;
    }

    public LatencyBudget getLatencyBudget() {
        return latencyBudget;
    }

    public void setLatencyBudget(LatencyBudget latencyBudget) {
        this.latencyBudget = latencyBudget;
    }

    public UserData getUserData() {
        return userData;
    }

    public void setUserData(UserData userData) {
        this.userData = userData;
    }

    public ReaderDataLifecycle getReaderDataLifecycle() {
        return readerDataLifecycle;
    }

    public void setReaderDataLifecycle(ReaderDataLifecycle readerDataLifecycle) {
        this.readerDataLifecycle = readerDataLifecycle;
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

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof DataReaderQos)) {
            return false;
        }
        DataReaderQos other = (DataReaderQos) o;
        return Objects.equals(reliability, other.reliability)
                && Objects.equals(durability, other.durability)
                && Objects.equals(history, other.history)
                && Objects.equals(ownership, other.ownership)
                && Objects.equals(resourceLimits, other.resourceLimits)
                && Objects.equals(destinationOrder, other.destinationOrder)
                && Objects.equals(timeBasedFilter, other.timeBasedFilter)
                && Objects.equals(latencyBudget, other.latencyBudget)
                && Objects.equals(userData, other.userData)
                && Objects.equals(readerDataLifecycle, other.readerDataLifecycle)
                && Objects.equals(dataRepresentation, other.dataRepresentation)
                && Objects.equals(deadline, other.deadline)
                && Objects.equals(liveliness, other.liveliness);
    }

    @Override
    public int hashCode() {
        return Objects.hash(
                reliability,
                durability,
                history,
                ownership,
                resourceLimits,
                destinationOrder,
                timeBasedFilter,
                latencyBudget,
                userData,
                readerDataLifecycle,
                dataRepresentation,
                deadline,
                liveliness);
    }

    @Override
    public String toString() {
        return "DataReaderQos{reliability="
                + reliability
                + ", durability="
                + durability
                + ", history="
                + history
                + ", ownership="
                + ownership
                + ", resourceLimits="
                + resourceLimits
                + ", destinationOrder="
                + destinationOrder
                + ", timeBasedFilter="
                + timeBasedFilter
                + ", latencyBudget="
                + latencyBudget
                + ", userData="
                + userData
                + ", readerDataLifecycle="
                + readerDataLifecycle
                + ", dataRepresentation="
                + dataRepresentation
                + ", deadline="
                + deadline
                + ", liveliness="
                + liveliness
                + "}";
    }
}
