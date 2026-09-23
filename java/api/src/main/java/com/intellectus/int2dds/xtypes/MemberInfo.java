package com.intellectus.int2dds.xtypes;

/**
 * Immutable snapshot of one struct member's introspection info, as returned
 * by {@link TypeObject#memberInfo}.
 */
public final class MemberInfo {

    private final int memberId;
    private final int kind;
    private final int flags;

    public MemberInfo(int memberId, int kind, int flags) {
        this.memberId = memberId;
        this.kind = kind;
        this.flags = flags;
    }

    /** The member's XTypes member id. */
    public int memberId() {
        return memberId;
    }

    /** The member's type, an {@link FieldType} constant. */
    public int kind() {
        return kind;
    }

    /** Raw member flags bitmask (key/optional/must-understand/external bits). */
    public int flags() {
        return flags;
    }
}
