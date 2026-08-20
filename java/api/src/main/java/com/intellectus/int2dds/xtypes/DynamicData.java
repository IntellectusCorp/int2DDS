package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;

/**
 * A decoded sample, read field-by-field through dotted {@code path} strings
 * rather than a generated accessor class. Produced by {@link
 * com.intellectus.int2dds.core.DomainParticipant#dynamicDataFromSample}.
 * NativeCleaner-managed like {@link
 * com.intellectus.int2dds.conditions.Condition}.
 */
public final class DynamicData implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final NativeHandle handle;

    private DynamicData(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, DynamicData::deleteVoid);
    }

    /**
     * Wraps an already-created native DynamicData handle. Public, in the same
     * style as {@code ParticipantBuiltinTopicData.materialize}, so callers
     * outside this package (namely {@code DomainParticipant}) can hand back a
     * handle produced by a bridge such as {@link FfiAccess#dynamicDataFromSample}.
     */
    public static DynamicData fromHandle(long rawHandle) {
        return new DynamicData(rawHandle);
    }

    private static int deleteVoid(long h) {
        FfiAccess.dynamicDataDestroy(h);
        return 0;
    }

    long handle() {
        return handle.value();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    /** Reads an {@code int32} field at {@code path} (dotted/indexed). */
    public int getI32(String path) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicDataGetI32(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (int) out[0];
    }

    /** Reads a {@code float64} field at {@code path} (dotted/indexed). */
    public double getF64(String path) {
        long h = handle();
        double[] out = new double[1];
        int rc = FfiAccess.dynamicDataGetF64(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads a {@code string} field at {@code path} (dotted/indexed). */
    public String getString(String path) {
        long h = handle();
        byte[][] out = new byte[1][];
        int rc = FfiAccess.dynamicDataGetString(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new String(out[0], UTF8);
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
