package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataWriterQos;
import java.util.Objects;

/**
 * Publishes {@link DynamicData} samples to a {@link DynamicTopic}. Produced by
 * {@link com.intellectus.int2dds.core.Publisher#createDynamicDataWriter}.
 * NativeCleaner-managed like {@link com.intellectus.int2dds.core.DataWriter}.
 */
public final class DynamicDataWriter implements AutoCloseable {

    private final NativeHandle handle;

    private DynamicDataWriter(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, DynamicDataWriter::deleteVoid);
    }

    /**
     * Wraps an already-created native dynamic-datawriter handle. Public, the
     * same style as {@link DynamicData#fromHandle}, so {@link
     * com.intellectus.int2dds.core.Publisher#createDynamicDataWriter} --
     * outside this package -- can hand back a handle produced by {@link
     * FfiAccess#createDataWriterDynamic}.
     */
    public static DynamicDataWriter fromHandle(long rawHandle) {
        return new DynamicDataWriter(rawHandle);
    }

    private static int deleteVoid(long h) {
        FfiAccess.dynamicWriterDestroy(h);
        return 0;
    }

    long handle() {
        return handle.value();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    /** Publishes {@code d}. Consumes {@code d}'s handle for the call, not ownership. */
    public void write(DynamicData d) {
        Objects.requireNonNull(d, "d");
        long h = handle();
        int rc = FfiAccess.dynamicWriterWrite(h, d.handle());
        // Both handles are bare longs read moments before the native call
        // that consumes them; keep this writer and d reachable across it --
        // see NativeKeepAlive's own doc for the full argument.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(d);
        ReturnCodes.check(rc);
    }

    /**
     * Reads this writer's current QoS off the native side — not the {@code
     * DataWriterQos} it was constructed with, which this class does not
     * retain. Policies with no native getter come back null; see {@link
     * QosMarshal#readWriterQos}'s own doc for the full list.
     */
    public DataWriterQos getQos() {
        long h = handle();
        long[] qosOut = new long[1];
        int rc = FfiAccess.dynamicWriterGetQos(h, qosOut);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        long qosHandle = qosOut[0];
        try {
            return QosMarshal.readWriterQos(qosHandle);
        } finally {
            FfiAccess.destroyDataWriterQos(qosHandle);
        }
    }

    /** The number of DataReaders currently matched to this writer. */
    public int publicationMatchedCount() {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicWriterPublicationMatchedCount(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (int) out[0];
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
