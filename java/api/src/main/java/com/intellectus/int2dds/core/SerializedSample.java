package com.intellectus.int2dds.core;

import java.util.Arrays;

/**
 * One received sample as raw CDR bytes (encapsulation header included) paired
 * with its {@link SampleInfo} -- the state-filtered counterpart of {@link
 * Sample} for callers that want the wire bytes rather than a decoded {@code
 * T}. {@code bytes} is empty ({@code length == 0}) for an invalid-data
 * (dispose/unregister) sample, i.e. when {@link #info()}'s {@link
 * SampleInfo#validData()} is {@code false}.
 */
public final class SerializedSample {

    private final byte[] bytes;
    private final SampleInfo info;

    SerializedSample(byte[] bytes, SampleInfo info) {
        this.bytes = Arrays.copyOf(bytes, bytes.length);
        this.info = info;
    }

    /** The raw CDR bytes, a defensive copy; empty when {@code info().validData()} is false. */
    public byte[] bytes() {
        return Arrays.copyOf(bytes, bytes.length);
    }

    /** This sample's metadata; valid even when {@link #bytes()} is empty. */
    public SampleInfo info() {
        return info;
    }
}
