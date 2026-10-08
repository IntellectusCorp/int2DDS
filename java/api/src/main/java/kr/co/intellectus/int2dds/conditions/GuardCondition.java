package kr.co.intellectus.int2dds.conditions;

import kr.co.intellectus.int2dds.internal.NativeKeepAlive;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;

/** An application-triggered condition. */
public final class GuardCondition extends Condition {
    public GuardCondition() {
        super(create(), FfiAccess::guardConditionDelete);
    }

    private static long create() {
        long[] out = new long[1];
        int rc = FfiAccess.guardConditionNew(out);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Sets the trigger value; a WaitSet waiting on this wakes when true. */
    public void setTriggerValue(boolean value) {
        long h = handle();
        FfiAccess.guardConditionSetTrigger(h, value);
        NativeKeepAlive.keepAlive(this);
    }

    @Override
    public boolean triggerValue() {
        long h = handle();
        boolean[] out = new boolean[1];
        int rc = FfiAccess.guardConditionGetTriggerValue(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }
}
