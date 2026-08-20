package com.intellectus.int2dds.conditions;

/**
 * Sample-state bit values for {@link ReadCondition} masks. Matches the
 * core's {@code SampleStateKind} ({@code dds/src/dcps/subscription/sample_info.rs}).
 */
public final class SampleState {
    public static final int READ = 0x0001;
    public static final int NOT_READ = 0x0002;
    public static final int ANY = 0xFFFF;

    private SampleState() {}
}
