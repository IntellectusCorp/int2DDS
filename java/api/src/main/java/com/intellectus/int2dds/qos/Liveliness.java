package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Liveliness QoS policy.
 *
 * <p>{@code leaseDuration} may be null. The C ABI setter takes a number and
 * has no way to express "unset", so a null becomes {@code Long.MAX_VALUE} —
 * the same fallback the C# binding uses.
 */
public class Liveliness {

    private LivelinessKind kind = LivelinessKind.AUTOMATIC;
    private Duration leaseDuration;

    public Liveliness() {}

    public Liveliness(LivelinessKind kind) {
        this.kind = kind;
    }

    public Liveliness(LivelinessKind kind, Duration leaseDuration) {
        this.kind = kind;
        this.leaseDuration = leaseDuration;
    }

    public LivelinessKind getKind() {
        return kind;
    }

    public void setKind(LivelinessKind kind) {
        this.kind = kind;
    }

    /** May be null, meaning the {@code Long.MAX_VALUE} fallback applies. */
    public Duration getLeaseDuration() {
        return leaseDuration;
    }

    public void setLeaseDuration(Duration leaseDuration) {
        this.leaseDuration = leaseDuration;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long leaseDurationNs() {
        return leaseDuration == null ? Long.MAX_VALUE : leaseDuration.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Liveliness)) {
            return false;
        }
        Liveliness other = (Liveliness) o;
        return kind == other.kind && Objects.equals(leaseDuration, other.leaseDuration);
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, leaseDuration);
    }

    @Override
    public String toString() {
        return "Liveliness{kind=" + kind + ", leaseDuration=" + leaseDuration + "}";
    }
}
