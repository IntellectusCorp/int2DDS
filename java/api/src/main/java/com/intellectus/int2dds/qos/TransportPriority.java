package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Transport priority QoS policy. */
public class TransportPriority {

    private int value = 0;

    public TransportPriority() {}

    public TransportPriority(int value) {
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
        if (!(o instanceof TransportPriority)) {
            return false;
        }
        TransportPriority other = (TransportPriority) o;
        return value == other.value;
    }

    @Override
    public int hashCode() {
        return Objects.hash(value);
    }

    @Override
    public String toString() {
        return "TransportPriority{value=" + value + "}";
    }
}
