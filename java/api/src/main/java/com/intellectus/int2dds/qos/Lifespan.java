package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Lifespan QoS policy.
 *
 * <p>{@code duration} may be null. The C ABI setter takes a number and has no
 * way to express "unset", so a null becomes {@code Long.MAX_VALUE} — the same
 * fallback the C# binding uses.
 */
public class Lifespan {

    private Duration duration;

    public Lifespan() {}

    public Lifespan(Duration duration) {
        this.duration = duration;
    }

    /** May be null, meaning the {@code Long.MAX_VALUE} fallback applies. */
    public Duration getDuration() {
        return duration;
    }

    public void setDuration(Duration duration) {
        this.duration = duration;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long durationNs() {
        return duration == null ? Long.MAX_VALUE : duration.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Lifespan)) {
            return false;
        }
        Lifespan other = (Lifespan) o;
        return Objects.equals(duration, other.duration);
    }

    @Override
    public int hashCode() {
        return Objects.hash(duration);
    }

    @Override
    public String toString() {
        return "Lifespan{duration=" + duration + "}";
    }
}
