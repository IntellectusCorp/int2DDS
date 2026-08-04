package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Deadline QoS policy.
 *
 * <p>{@code period} may be null. The C ABI setter takes a number and has no
 * way to express "unset", so a null becomes {@code Long.MAX_VALUE} — the same
 * fallback the C# binding uses.
 */
public class Deadline {

    private Duration period;

    public Deadline() {}

    public Deadline(Duration period) {
        this.period = period;
    }

    /** May be null, meaning the {@code Long.MAX_VALUE} fallback applies. */
    public Duration getPeriod() {
        return period;
    }

    public void setPeriod(Duration period) {
        this.period = period;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long periodNs() {
        return period == null ? Long.MAX_VALUE : period.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Deadline)) {
            return false;
        }
        Deadline other = (Deadline) o;
        return Objects.equals(period, other.period);
    }

    @Override
    public int hashCode() {
        return Objects.hash(period);
    }

    @Override
    public String toString() {
        return "Deadline{period=" + period + "}";
    }
}
