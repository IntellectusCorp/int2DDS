package com.intellectus.int2dds.qos;

import java.util.Arrays;

/**
 * Partition QoS policy.
 *
 * <p>{@code names} is copied on the way in and out so that a later mutation
 * of the caller's array cannot change QoS that was already applied.
 */
public class Partition {

    private String[] names = new String[0];

    public Partition() {}

    public Partition(String[] names) {
        this.names = Arrays.copyOf(names, names.length);
    }

    /** Returns a defensive copy; mutating it has no effect on this policy. */
    public String[] getNames() {
        return Arrays.copyOf(names, names.length);
    }

    public void setNames(String[] names) {
        this.names = Arrays.copyOf(names, names.length);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Partition)) {
            return false;
        }
        Partition other = (Partition) o;
        return Arrays.equals(names, other.names);
    }

    @Override
    public int hashCode() {
        return Arrays.hashCode(names);
    }

    @Override
    public String toString() {
        return "Partition{names=" + Arrays.toString(names) + "}";
    }
}
