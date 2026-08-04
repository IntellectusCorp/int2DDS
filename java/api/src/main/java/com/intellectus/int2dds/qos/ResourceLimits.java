package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Resource limits QoS policy. */
public class ResourceLimits {

    private int maxSamples = -1;
    private int maxInstances = -1;
    private int maxSamplesPerInstance = -1;

    public ResourceLimits() {}

    public ResourceLimits(int maxSamples, int maxInstances, int maxSamplesPerInstance) {
        this.maxSamples = maxSamples;
        this.maxInstances = maxInstances;
        this.maxSamplesPerInstance = maxSamplesPerInstance;
    }

    public int getMaxSamples() {
        return maxSamples;
    }

    public void setMaxSamples(int maxSamples) {
        this.maxSamples = maxSamples;
    }

    public int getMaxInstances() {
        return maxInstances;
    }

    public void setMaxInstances(int maxInstances) {
        this.maxInstances = maxInstances;
    }

    public int getMaxSamplesPerInstance() {
        return maxSamplesPerInstance;
    }

    public void setMaxSamplesPerInstance(int maxSamplesPerInstance) {
        this.maxSamplesPerInstance = maxSamplesPerInstance;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof ResourceLimits)) {
            return false;
        }
        ResourceLimits other = (ResourceLimits) o;
        return maxSamples == other.maxSamples
                && maxInstances == other.maxInstances
                && maxSamplesPerInstance == other.maxSamplesPerInstance;
    }

    @Override
    public int hashCode() {
        return Objects.hash(maxSamples, maxInstances, maxSamplesPerInstance);
    }

    @Override
    public String toString() {
        return "ResourceLimits{maxSamples=" + maxSamples
                + ", maxInstances=" + maxInstances
                + ", maxSamplesPerInstance=" + maxSamplesPerInstance
                + "}";
    }
}
