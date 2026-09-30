package kr.co.intellectus.int2dds.qos;

import java.util.Objects;

/**
 * Lifespan reference QoS policy (DataReader only, an int2DDS extension): which
 * timestamp a reader measures the writer's {@link Lifespan} against.
 */
public class LifespanReference {

    private LifespanReferenceKind kind = LifespanReferenceKind.BY_SOURCE;

    public LifespanReference() {}

    public LifespanReference(LifespanReferenceKind kind) {
        this.kind = kind;
    }

    public LifespanReferenceKind getKind() {
        return kind;
    }

    public void setKind(LifespanReferenceKind kind) {
        this.kind = kind;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof LifespanReference)) {
            return false;
        }
        LifespanReference other = (LifespanReference) o;
        return kind == other.kind;
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind);
    }

    @Override
    public String toString() {
        return "LifespanReference{kind=" + kind + "}";
    }
}
