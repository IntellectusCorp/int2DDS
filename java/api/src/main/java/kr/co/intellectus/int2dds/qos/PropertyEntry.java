package kr.co.intellectus.int2dds.qos;

import java.util.Arrays;
import java.util.Objects;

/** A single entry of the {@link Property} QoS policy, text or binary. Immutable. */
public final class PropertyEntry {

    private final String name;
    private final String value;
    private final byte[] binaryValue;
    private final boolean propagate;

    public PropertyEntry(String name, String value, boolean propagate) {
        this(name, value, null, propagate);
    }

    /** A binary entry; {@code data} is copied. */
    public PropertyEntry(String name, byte[] data, boolean propagate) {
        this(name, null, Arrays.copyOf(data, data.length), propagate);
    }

    private PropertyEntry(String name, String value, byte[] binaryValue, boolean propagate) {
        this.name = name;
        this.value = value;
        this.binaryValue = binaryValue;
        this.propagate = propagate;
    }

    public String getName() {
        return name;
    }

    /** The text value, or null for a binary entry. */
    public String getValue() {
        return value;
    }

    /** A copy of the binary value, or null for a text entry. */
    public byte[] getBinaryValue() {
        return binaryValue == null ? null : binaryValue.clone();
    }

    public boolean isBinary() {
        return binaryValue != null;
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
                && Objects.equals(value, other.value)
                && Arrays.equals(binaryValue, other.binaryValue);
    }

    @Override
    public int hashCode() {
        return Objects.hash(name, value, Arrays.hashCode(binaryValue), propagate);
    }

    @Override
    public String toString() {
        String v = binaryValue != null ? "binary[" + binaryValue.length + "]" : value;
        return "PropertyEntry{name=" + name + ", value=" + v + ", propagate=" + propagate + "}";
    }
}
