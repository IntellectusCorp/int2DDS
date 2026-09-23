package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Destination order QoS policy. */
public class DestinationOrder {

    private DestinationOrderKind kind = DestinationOrderKind.BY_RECEPTION;

    public DestinationOrder() {}

    public DestinationOrder(DestinationOrderKind kind) {
        this.kind = kind;
    }

    public DestinationOrderKind getKind() {
        return kind;
    }

    public void setKind(DestinationOrderKind kind) {
        this.kind = kind;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof DestinationOrder)) {
            return false;
        }
        DestinationOrder other = (DestinationOrder) o;
        return kind == other.kind;
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind);
    }

    @Override
    public String toString() {
        return "DestinationOrder{kind=" + kind + "}";
    }
}
