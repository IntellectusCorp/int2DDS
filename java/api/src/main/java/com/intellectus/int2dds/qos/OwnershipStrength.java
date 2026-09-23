package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Ownership strength QoS policy. */
public class OwnershipStrength {

    private int value = 0;

    public OwnershipStrength() {}

    public OwnershipStrength(int value) {
        this.value = value;
    }

    public int getValue() {
        return value;
    }

    public void setValue(int value) {
        this.value = value;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof OwnershipStrength)) {
            return false;
        }
        OwnershipStrength other = (OwnershipStrength) o;
        return value == other.value;
    }

    @Override
    public int hashCode() {
        return Objects.hash(value);
    }

    @Override
    public String toString() {
        return "OwnershipStrength{value=" + value + "}";
    }
}
