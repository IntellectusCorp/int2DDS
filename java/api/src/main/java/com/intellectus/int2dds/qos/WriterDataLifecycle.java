package com.intellectus.int2dds.qos;

import java.util.Objects;

/** Writer data lifecycle QoS policy. */
public class WriterDataLifecycle {

    private boolean autodisposeUnregisteredInstances = true;

    public WriterDataLifecycle() {}

    public WriterDataLifecycle(boolean autodisposeUnregisteredInstances) {
        this.autodisposeUnregisteredInstances = autodisposeUnregisteredInstances;
    }

    public boolean isAutodisposeUnregisteredInstances() {
        return autodisposeUnregisteredInstances;
    }

    public void setAutodisposeUnregisteredInstances(boolean autodisposeUnregisteredInstances) {
        this.autodisposeUnregisteredInstances = autodisposeUnregisteredInstances;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof WriterDataLifecycle)) {
            return false;
        }
        WriterDataLifecycle other = (WriterDataLifecycle) o;
        return autodisposeUnregisteredInstances == other.autodisposeUnregisteredInstances;
    }

    @Override
    public int hashCode() {
        return Objects.hash(autodisposeUnregisteredInstances);
    }

    @Override
    public String toString() {
        return "WriterDataLifecycle{autodisposeUnregisteredInstances="
                + autodisposeUnregisteredInstances + "}";
    }
}
