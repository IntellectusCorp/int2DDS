package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Reliability QoS policy.
 *
 * <p>{@code maxBlockingTime} may be null. The C ABI setter takes a number and
 * has no way to express "unset", so a null becomes 100 ms — the same fallback
 * the C# binding uses.
 */
public class Reliability {

    private static final long DEFAULT_MAX_BLOCKING_NS = 100_000_000L;

    private ReliabilityKind kind = ReliabilityKind.RELIABLE;
    private Duration maxBlockingTime;

    public Reliability() {}

    public Reliability(ReliabilityKind kind) {
        this.kind = kind;
    }

    public Reliability(ReliabilityKind kind, Duration maxBlockingTime) {
        this.kind = kind;
        this.maxBlockingTime = maxBlockingTime;
    }

    public ReliabilityKind getKind() {
        return kind;
    }

    public void setKind(ReliabilityKind kind) {
        this.kind = kind;
    }

    /** May be null, meaning the 100 ms fallback applies. */
    public Duration getMaxBlockingTime() {
        return maxBlockingTime;
    }

    public void setMaxBlockingTime(Duration maxBlockingTime) {
        this.maxBlockingTime = maxBlockingTime;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long maxBlockingTimeNs() {
        return maxBlockingTime == null ? DEFAULT_MAX_BLOCKING_NS : maxBlockingTime.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Reliability)) {
            return false;
        }
        Reliability other = (Reliability) o;
        return kind == other.kind && Objects.equals(maxBlockingTime, other.maxBlockingTime);
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, maxBlockingTime);
    }

    @Override
    public String toString() {
        return "Reliability{kind=" + kind + ", maxBlockingTime=" + maxBlockingTime + "}";
    }
}
