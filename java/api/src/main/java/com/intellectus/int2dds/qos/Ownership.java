package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Ownership QoS policy. */
public class Ownership {

    private OwnershipKind kind = OwnershipKind.SHARED;

    public Ownership() {}

    public Ownership(OwnershipKind kind) {
        this.kind = kind;
    }

    public OwnershipKind getKind() {
        return kind;
    }

    public void setKind(OwnershipKind kind) {
        this.kind = kind;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Ownership)) {
            return false;
        }
        Ownership other = (Ownership) o;
        return kind == other.kind;
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind);
    }

    @Override
    public String toString() {
        return "Ownership{kind=" + kind + "}";
    }
}
