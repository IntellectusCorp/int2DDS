package com.intellectus.int2dds.qos;

import java.util.Objects;

/** History QoS policy. */
public class History {

    private HistoryKind kind = HistoryKind.KEEP_LAST;
    private int depth = 1;

    public History() {}

    public History(HistoryKind kind, int depth) {
        this.kind = kind;
        this.depth = depth;
    }

    public HistoryKind getKind() {
        return kind;
    }

    public void setKind(HistoryKind kind) {
        this.kind = kind;
    }

    public int getDepth() {
        return depth;
    }

    public void setDepth(int depth) {
        this.depth = depth;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof History)) {
            return false;
        }
        History other = (History) o;
        return depth == other.depth && kind == other.kind;
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, depth);
    }

    @Override
    public String toString() {
        return "History{kind=" + kind + ", depth=" + depth + "}";
    }
}
