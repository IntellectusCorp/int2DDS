package com.intellectus.int2dds.core;

import com.intellectus.int2dds.types.IDdsType;

/**
 * One received sample: the deserialized {@code data}, or {@code null} for a
 * sample whose {@link SampleInfo#validData()} is false (dispose/unregister),
 * paired with its {@link SampleInfo}.
 *
 * @param <T> the DDS data type.
 */
public final class Sample<T extends IDdsType> {

    private final T data;
    private final SampleInfo info;

    Sample(T data, SampleInfo info) {
        this.data = data;
        this.info = info;
    }

    /** The deserialized sample, or {@code null} when {@code info().validData()} is false. */
    public T data() {
        return data;
    }

    /** This sample's metadata; valid even when {@link #data()} is null. */
    public SampleInfo info() {
        return info;
    }
}
