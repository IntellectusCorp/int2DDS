package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Data representation QoS policy. */
public class DataRepresentation {

    private DataRepresentationKind kind = DataRepresentationKind.XCDR1;

    public DataRepresentation() {}

    public DataRepresentation(DataRepresentationKind kind) {
        this.kind = kind;
    }

    public DataRepresentationKind getKind() {
        return kind;
    }

    public void setKind(DataRepresentationKind kind) {
        this.kind = kind;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof DataRepresentation)) {
            return false;
        }
        DataRepresentation other = (DataRepresentation) o;
        return kind == other.kind;
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind);
    }

    @Override
    public String toString() {
        return "DataRepresentation{kind=" + kind + "}";
    }
}
