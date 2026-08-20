package com.intellectus.int2dds.conditions;

import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A condition tied to a {@code DataReader}'s cached samples matching a set of
 * sample/view/instance state masks.
 *
 * <p>Obtained from {@code DataReader.createReadCondition(...)}: each call
 * mints a new native box, so this wraps its own handle rather than one
 * shared with the reader.
 */
public class ReadCondition extends Condition {
    /**
     * Wraps a read-condition handle already minted by the native layer
     * (e.g. by {@code int2dds_datareader_create_readcondition}). Meant for
     * the entity {@code createReadCondition()} factory, not for wrapping an
     * arbitrary handle.
     */
    public ReadCondition(long rawHandle) {
        super(rawHandle, FfiAccess::readConditionDelete);
    }

    @Override
    public boolean triggerValue() {
        long h = handle();
        boolean[] out = new boolean[1];
        int rc = FfiAccess.readConditionGetTriggerValue(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }
}
