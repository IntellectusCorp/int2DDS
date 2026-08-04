package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Latency budget QoS policy.
 *
 * <p>{@code duration} may be null. The C ABI setter takes a number and has no
 * way to express "unset", so a null becomes {@code 0} — the same fallback the
 * C# binding uses.
 */
public class LatencyBudget {

    private Duration duration;

    public LatencyBudget() {}

    public LatencyBudget(Duration duration) {
        this.duration = duration;
    }

    /** May be null, meaning the {@code 0} fallback applies. */
    public Duration getDuration() {
        return duration;
    }

    public void setDuration(Duration duration) {
        this.duration = duration;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long durationNs() {
        return duration == null ? 0L : duration.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof LatencyBudget)) {
            return false;
        }
        LatencyBudget other = (LatencyBudget) o;
        return Objects.equals(duration, other.duration);
    }

    @Override
    public int hashCode() {
        return Objects.hash(duration);
    }

    @Override
    public String toString() {
        return "LatencyBudget{duration=" + duration + "}";
    }
}
