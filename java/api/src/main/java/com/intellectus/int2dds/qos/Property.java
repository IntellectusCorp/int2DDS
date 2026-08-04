package com.intellectus.int2dds.qos;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * Property QoS policy (DomainParticipant only).
 *
 * <p>The well-known name {@code int2dds.transport.UDPv4.multicast_ttl}
 * configures the IPv4 multicast TTL for the participant; {@link
 * #setMulticastTtl(int)} is a convenience for that entry that replaces rather
 * than appends.
 */
public class Property {

    public static final String MULTICAST_TTL_NAME = "int2dds.transport.UDPv4.multicast_ttl";

    private final List<PropertyEntry> entries = new ArrayList<>();

    public Property() {}

    /** The live list of entries; mutate it directly or via {@link #add}. */
    public List<PropertyEntry> getEntries() {
        return entries;
    }

    public void add(String name, String value, boolean propagate) {
        entries.add(new PropertyEntry(name, value, propagate));
    }

    /** Removes any existing multicast TTL entry, then appends the new value. */
    public void setMulticastTtl(int ttl) {
        entries.removeIf(e -> MULTICAST_TTL_NAME.equals(e.getName()));
        entries.add(new PropertyEntry(MULTICAST_TTL_NAME, Integer.toString(ttl), false));
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Property)) {
            return false;
        }
        Property other = (Property) o;
        return Objects.equals(entries, other.entries);
    }

    @Override
    public int hashCode() {
        return Objects.hash(entries);
    }

    @Override
    public String toString() {
        return "Property{entries=" + entries + "}";
    }
}
