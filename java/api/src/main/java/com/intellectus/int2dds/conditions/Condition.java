package com.intellectus.int2dds.conditions;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.ReturnCodes;

/** A DDS condition: a native handle whose trigger value a WaitSet observes. */
public abstract class Condition implements AutoCloseable {
    private final NativeHandle handle;

    Condition(long rawHandle, NativeCleaner.Deleter deleter) {
        this.handle = NativeCleaner.register(this, rawHandle, deleter);
    }

    final long handle() {
        return handle.value();
    }

    /**
     * The condition's current trigger value.
     *
     * <p>Each concrete subtype must read this through its own type-specific
     * native accessor, not the generic {@code int2dds_condition_get_trigger_value}:
     * that accessor's {@code Int2DdsCondition} parameter is the wrapper type
     * {@code condition_seq_get} hands out (a fat {@code Arc<dyn Condition>}),
     * laid out differently in native memory from this condition's own handle
     * (e.g. a GuardCondition's thin {@code Arc<GuardCondition>}) — calling it
     * on the original handle reads across that mismatch and corrupts memory.
     */
    public abstract boolean triggerValue();

    public final boolean isClosed() {
        return handle.isClosed();
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
