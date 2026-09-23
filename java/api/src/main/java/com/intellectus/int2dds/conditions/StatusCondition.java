package com.intellectus.int2dds.conditions;

import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.status.StatusMask;

/**
 * A condition tied to an entity's own communication-status changes.
 *
 * <p>Obtained fresh from an entity's {@code getStatusCondition()} (e.g.
 * {@code DataReader}/{@code DataWriter}): each call mints a new native box,
 * so this wraps its own handle rather than one shared with the entity.
 */
public final class StatusCondition extends Condition {
    /**
     * Wraps a status-condition handle already minted by the native layer
     * (e.g. by {@code int2dds_datareader_get_statuscondition}). Meant for the
     * entity {@code getStatusCondition()} accessors, not for wrapping an
     * arbitrary handle.
     */
    public StatusCondition(long rawHandle) {
        super(rawHandle, FfiAccess::statusConditionDelete);
    }

    /** Sets which status changes this condition's trigger value reacts to. */
    public void setEnabledStatuses(StatusMask mask) {
        long h = handle();
        int rc = FfiAccess.statusConditionSetEnabledStatuses(h, mask.bits());
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** The statuses this condition's trigger value currently reacts to. */
    public StatusMask enabledStatuses() {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.statusConditionGetEnabledStatuses(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return StatusMask.of(out[0]);
    }

    @Override
    public boolean triggerValue() {
        long h = handle();
        boolean[] out = new boolean[1];
        int rc = FfiAccess.statusConditionGetTriggerValue(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }
}
