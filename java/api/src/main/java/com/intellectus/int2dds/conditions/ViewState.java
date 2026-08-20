package com.intellectus.int2dds.conditions;

/**
 * View-state bit values for {@link ReadCondition} masks. Matches the
 * core's {@code ViewStateKind} ({@code dds/src/dcps/subscription/sample_info.rs}).
 */
public final class ViewState {
    public static final int NEW = 0x0001;
    public static final int NOT_NEW = 0x0002;
    public static final int ANY = 0xFFFF;

    private ViewState() {}
}
