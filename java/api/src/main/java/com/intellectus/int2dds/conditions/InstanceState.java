package com.intellectus.int2dds.conditions;

/**
 * Instance-state bit values for {@link ReadCondition} masks. Matches the
 * core's {@code InstanceStateKind} ({@code dds/src/dcps/subscription/sample_info.rs}).
 */
public final class InstanceState {
    public static final int ALIVE = 0x0001;
    public static final int NOT_ALIVE_DISPOSED = 0x0002;
    public static final int NOT_ALIVE_NO_WRITERS = 0x0004;
    public static final int ANY = 0xFFFF;

    private InstanceState() {}
}
