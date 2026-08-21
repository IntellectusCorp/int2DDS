package com.intellectus.int2dds.conditions;

import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

/**
 * Blocks the calling thread until one of its attached {@link Condition}s
 * triggers. Not thread-safe: use one WaitSet per waiting thread.
 *
 * <p><b>Contract:</b> an attached {@link Condition} must be {@link #detach}ed
 * before it is closed. Closing a still-attached condition leaves a dangling
 * handle in the native WaitSet; the next {@link #await} is then undefined
 * behavior (possible crash). Per DDS, deleting an attached condition is a
 * precondition violation.
 */
public final class WaitSet implements AutoCloseable {
    private final NativeHandle handle;
    // Attached conditions, kept alive and re-checked after wait(): wait_ex's
    // returned sequence uses different pointers than what was attached, so we
    // identify triggers by re-reading triggerValue() on the originals.
    private final List<Condition> attached = new ArrayList<Condition>();

    public WaitSet() {
        long[] out = new long[1];
        ReturnCodes.check(FfiAccess.waitsetNew(out));
        this.handle = NativeCleaner.register(this, out[0], FfiAccess::waitsetDelete);
    }

    /**
     * Attaches a condition (dispatch by type). Idempotent: re-attaching an
     * already-attached instance is a no-op. Must be {@link #detach}ed before
     * {@code c} is closed; see the class Javadoc for why.
     */
    public void attach(Condition c) {
        Objects.requireNonNull(c, "condition");
        if (c.isClosed()) {
            throw new IllegalStateException("cannot attach a closed condition");
        }
        if (attached.contains(c)) {
            return;
        }
        int rc = attachNative(c);
        NativeKeepAlive.keepAlive(c);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        attached.add(c);
    }

    /** Detaches a previously attached condition. */
    public void detach(Condition c) {
        Objects.requireNonNull(c, "condition");
        if (c.isClosed()) {
            // Handle is already freed natively; nothing to detach there.
            attached.remove(c);
            return;
        }
        int rc = detachNative(c);
        NativeKeepAlive.keepAlive(c);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        attached.remove(c);
    }

    private int attachNative(Condition c) {
        if (c instanceof GuardCondition) {
            return FfiAccess.waitsetAttachGuard(handle.value(), c.handle());
        }
        if (c instanceof StatusCondition) {
            return FfiAccess.waitsetAttachStatus(handle.value(), c.handle());
        }
        if (c instanceof ReadCondition) {
            // Covers QueryCondition too: it extends ReadCondition, and the
            // native side attaches both the same way.
            return FfiAccess.waitsetAttachRead(handle.value(), c.handle());
        }
        throw new IllegalArgumentException("unsupported condition type: " + c.getClass());
    }

    private int detachNative(Condition c) {
        if (c instanceof GuardCondition) {
            return FfiAccess.waitsetDetachGuard(handle.value(), c.handle());
        }
        if (c instanceof StatusCondition) {
            return FfiAccess.waitsetDetachStatus(handle.value(), c.handle());
        }
        if (c instanceof ReadCondition) {
            return FfiAccess.waitsetDetachRead(handle.value(), c.handle());
        }
        throw new IllegalArgumentException("unsupported condition type: " + c.getClass());
    }

    /**
     * Blocks up to {@code timeoutMillis} (negative = infinite) and returns the
     * attached conditions whose trigger value is set. Empty on timeout.
     *
     * <p>Named {@code await}, not {@code wait}: {@code Object.wait(long)} is
     * {@code final}, so a same-erasure instance method named {@code wait}
     * cannot be declared here at all, regardless of return type.
     *
     * <p>Undefined behavior (possible crash) if an attached condition was
     * closed without first being {@link #detach}ed.
     */
    public List<Condition> await(long timeoutMillis) {
        long[] seqOut = new long[1];
        int rc = FfiAccess.waitsetWaitEx(handle.value(), timeoutMillis, seqOut);
        NativeKeepAlive.keepAlive(this);
        if (rc == DdsException.RET_TIMEOUT) {
            return new ArrayList<Condition>();
        }
        ReturnCodes.check(rc);
        if (seqOut[0] != 0L) {
            FfiAccess.conditionSeqDelete(seqOut[0]); // we don't read the seq
        }
        List<Condition> triggered = new ArrayList<Condition>();
        for (Condition c : attached) {
            if (c.triggerValue()) {
                triggered.add(c);
            }
        }
        return triggered;
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
