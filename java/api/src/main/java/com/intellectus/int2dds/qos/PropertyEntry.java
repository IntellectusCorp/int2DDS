package com.intellectus.int2dds.qos;

import java.util.Objects;

/** A single entry of the {@link Property} QoS policy. Immutable. */
public final class PropertyEntry {

    private final String name;
    private final String value;
    private final boolean propagate;

    public PropertyEntry(String name, String value, boolean propagate) {
        this.name = name;
        this.value = value;
        this.propagate = propagate;
    }

    public String getName() {
        return name;
    }

    public String getValue() {
        return value;
    }

    public boolean isPropagate() {
        return propagate;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof PropertyEntry)) {
            return false;
        }
        PropertyEntry other = (PropertyEntry) o;
        return propagate == other.propagate
                && Objects.equals(name, other.name)
                && Objects.equals(value, other.value);
    }

    @Override
    public int hashCode() {
        return Objects.hash(name, value, propagate);
    }

    @Override
    public String toString() {
        return "PropertyEntry{name=" + name + ", value=" + value + ", propagate=" + propagate + "}";
    }
}
