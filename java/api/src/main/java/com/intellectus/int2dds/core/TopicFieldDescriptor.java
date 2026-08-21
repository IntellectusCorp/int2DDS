package com.intellectus.int2dds.core;

import java.util.Objects;

/**
 * One field of a topic's type: its name, its {@link
 * com.intellectus.int2dds.xtypes.FieldType} kind, and whether it is part of
 * the type's instance key.
 *
 * <p>A list of these, passed to {@link
 * DomainParticipant#createTopic(String, com.intellectus.int2dds.types.IDdsType, java.util.List)},
 * gives the core the field metadata it needs to resolve instance keys (the
 * fields with {@link #isKey()} true) and to evaluate a content filter
 * expression against the topic's samples -- metadata a plain {@link
 * DomainParticipant#createTopic(String, com.intellectus.int2dds.types.IDdsType)}
 * topic does not carry.
 */
public final class TopicFieldDescriptor {

    private final String name;
    private final int fieldType;
    private final boolean isKey;

    /**
     * @param name the field's name, as it appears in the type's CDR layout
     * @param fieldType one of the {@link com.intellectus.int2dds.xtypes.FieldType}
     *     constants
     * @param isKey whether this field is part of the type's instance key
     */
    public TopicFieldDescriptor(String name, int fieldType, boolean isKey) {
        this.name = Objects.requireNonNull(name, "name");
        this.fieldType = fieldType;
        this.isKey = isKey;
    }

    /** The field's name. */
    public String name() {
        return name;
    }

    /** The field's {@link com.intellectus.int2dds.xtypes.FieldType} kind. */
    public int fieldType() {
        return fieldType;
    }

    /** Whether this field is part of the type's instance key. */
    public boolean isKey() {
        return isKey;
    }
}
