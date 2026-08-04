package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Durability QoS policy. */
public class Durability {

    private DurabilityKind kind = DurabilityKind.VOLATILE;

    public Durability() {}

    public Durability(DurabilityKind kind) {
        this.kind = kind;
    }

    public DurabilityKind getKind() {
        return kind;
    }

    public void setKind(DurabilityKind kind) {
        this.kind = kind;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Durability)) {
            return false;
        }
        Durability other = (Durability) o;
        return kind == other.kind;
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind);
    }

    @Override
    public String toString() {
        return "Durability{kind=" + kind + "}";
    }
}
