package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.conditions.StatusCondition;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.qos.DataReaderQos;

/**
 * Receives {@link DynamicData} samples from a {@link DynamicTopic}. Produced
 * by {@link com.intellectus.int2dds.core.Subscriber#createDynamicDataReader}.
 * NativeCleaner-managed like {@link com.intellectus.int2dds.core.DataReader}.
 */
public final class DynamicDataReader implements AutoCloseable {

    private final NativeHandle handle;

    // Keeps the creating entity reachable while this reader is: a
    // ConfiguredParticipant's deleter tears its whole tree down unconditionally.
    @SuppressWarnings("unused")
    private final Object owner;

    private DynamicDataReader(long rawHandle, Object owner) {
        this.owner = owner;
        this.handle = NativeCleaner.register(this, rawHandle, DynamicDataReader::deleteVoid);
    }

    /**
     * Wraps an already-created native dynamic-datareader handle. Public, the
     * same style as {@link DynamicData#fromHandle}, so {@link
     * com.intellectus.int2dds.core.Subscriber#createDynamicDataReader} --
     * outside this package -- can hand back a handle produced by {@link
     * FfiAccess#createDataReaderDynamic}.
     */
    public static DynamicDataReader fromHandle(long rawHandle) {
        return new DynamicDataReader(rawHandle, null);
    }

    /** {@link #fromHandle(long)}, additionally keeping {@code owner} reachable for this reader's lifetime. */
    public static DynamicDataReader fromHandle(long rawHandle, Object owner) {
        return new DynamicDataReader(rawHandle, owner);
    }

    private static int deleteVoid(long h) {
        FfiAccess.dynamicReaderDestroy(h);
        return 0;
    }

    long handle() {
        return handle.value();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    /** Takes (removes) the next sample, or null if the cache is empty. */
    public DynamicData take() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicReaderTake(h, out);
        NativeKeepAlive.keepAlive(this);
        if (rc == DdsException.RET_NO_DATA) {
            return null;
        }
        ReturnCodes.check(rc);
        return DynamicData.fromHandle(out[0]);
    }

    /**
     * A fresh {@link StatusCondition} for this dynamic reader's status
     * changes; attach it to a {@link com.intellectus.int2dds.conditions.WaitSet}
     * (e.g. enabled for {@code DATA_AVAILABLE}) to wait for data instead of
     * polling {@link #take}. Caller-owned: close it (or let its NativeCleaner
     * do so) when done.
     */
    public StatusCondition getStatusCondition() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicReaderGetStatusCondition(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new StatusCondition(out[0]);
    }

    /**
     * Reads this reader's current QoS off the native side — not the {@code
     * DataReaderQos} it was constructed with, which this class does not
     * retain. Policies with no native getter come back null; see {@link
     * QosMarshal#readReaderQos}'s own doc for the full list.
     */
    public DataReaderQos getQos() {
        long h = handle();
        long[] qosOut = new long[1];
        int rc = FfiAccess.dynamicReaderGetQos(h, qosOut);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long qosHandle = qosOut[0];
        try {
            return QosMarshal.readReaderQos(qosHandle);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    /** The number of DataWriters currently matched to this reader. */
    public int subscriptionMatchedCount() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicReaderSubscriptionMatchedCount(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (int) out[0];
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
