package com.intellectus.int2dds.qos;

import java.time.Duration;
import java.util.Objects;

/**
 * Reader data lifecycle QoS policy.
 *
 * <p>Both durations may be null. The C ABI setters take a number and have no
 * way to express "unset", so a null becomes {@code Long.MAX_VALUE} for each —
 * the same fallback the C# binding uses.
 */
public class ReaderDataLifecycle {

    private Duration autopurgeNowriterSamplesDelay;
    private Duration autopurgeDisposedSamplesDelay;

    public ReaderDataLifecycle() {}

    public ReaderDataLifecycle(
            Duration autopurgeNowriterSamplesDelay, Duration autopurgeDisposedSamplesDelay) {
        this.autopurgeNowriterSamplesDelay = autopurgeNowriterSamplesDelay;
        this.autopurgeDisposedSamplesDelay = autopurgeDisposedSamplesDelay;
    }

    /** May be null, meaning the {@code Long.MAX_VALUE} fallback applies. */
    public Duration getAutopurgeNowriterSamplesDelay() {
        return autopurgeNowriterSamplesDelay;
    }

    public void setAutopurgeNowriterSamplesDelay(Duration autopurgeNowriterSamplesDelay) {
        this.autopurgeNowriterSamplesDelay = autopurgeNowriterSamplesDelay;
    }

    /** May be null, meaning the {@code Long.MAX_VALUE} fallback applies. */
    public Duration getAutopurgeDisposedSamplesDelay() {
        return autopurgeDisposedSamplesDelay;
    }

    public void setAutopurgeDisposedSamplesDelay(Duration autopurgeDisposedSamplesDelay) {
        this.autopurgeDisposedSamplesDelay = autopurgeDisposedSamplesDelay;
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long autopurgeNowriterNs() {
        return autopurgeNowriterSamplesDelay == null
                ? Long.MAX_VALUE
                : autopurgeNowriterSamplesDelay.toNanos();
    }

    /** The value handed to the C ABI, with the null fallback applied. */
    public long autopurgeDisposedNs() {
        return autopurgeDisposedSamplesDelay == null
                ? Long.MAX_VALUE
                : autopurgeDisposedSamplesDelay.toNanos();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof ReaderDataLifecycle)) {
            return false;
        }
        ReaderDataLifecycle other = (ReaderDataLifecycle) o;
        return Objects.equals(autopurgeNowriterSamplesDelay, other.autopurgeNowriterSamplesDelay)
                && Objects.equals(
                        autopurgeDisposedSamplesDelay, other.autopurgeDisposedSamplesDelay);
    }

    @Override
    public int hashCode() {
        return Objects.hash(autopurgeNowriterSamplesDelay, autopurgeDisposedSamplesDelay);
    }

    @Override
    public String toString() {
        return "ReaderDataLifecycle{autopurgeNowriterSamplesDelay="
                + autopurgeNowriterSamplesDelay
                + ", autopurgeDisposedSamplesDelay="
                + autopurgeDisposedSamplesDelay
                + "}";
    }
}
