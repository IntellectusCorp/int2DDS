package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Time-based filter QoS policy.
 *
 * <p>{@code minimumSeparation} may be null. The C ABI setter takes a number
 * and has no way to express "unset", so a null becomes {@code 0} — the same
 * fallback the C# binding uses.
 */
public class TimeBasedFilter {

    private Duration minimumSeparation;

    public TimeBasedFilter() {}

    public TimeBasedFilter(Duration minimumSeparation) {
        this.minimumSeparation = minimumSeparation;
    }

    /** May be null, meaning the {@code 0} fallback applies. */
    public Duration getMinimumSeparation() {
        return minimumSeparation;
    }

    public void setMinimumSeparation(Duration minimumSeparation) {
        this.minimumSeparation = minimumSeparation;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long minimumSeparationNs() {
        return minimumSeparation == null ? 0L : minimumSeparation.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof TimeBasedFilter)) {
            return false;
        }
        TimeBasedFilter other = (TimeBasedFilter) o;
        return Objects.equals(minimumSeparation, other.minimumSeparation);
    }

    @Override
    public int hashCode() {
        return Objects.hash(minimumSeparation);
    }

    @Override
    public String toString() {
        return "TimeBasedFilter{minimumSeparation=" + minimumSeparation + "}";
    }
}
