package com.intellectus.int2dds.xtypes;

/**
 * An enum literal read from a {@link DynamicValue} via {@link
 * DynamicValue#asEnum}: its numeric value and literal name.
 */
public final class EnumValue {

    private final int value;
    private final String name;

    public EnumValue(int value, String name) {
        this.value = value;
        this.name = name;
    }

    /** The enum literal's numeric value. */
    public int value() {
        return value;
    }

    /** The enum literal's name. */
    public String name() {
        return name;
    }
}
