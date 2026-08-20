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

    /** Reads a {@code bool} field at {@code path} (dotted/indexed). */
    public boolean getBool(String path) {
        long h = handle();
        boolean[] out = new boolean[1];
        int rc = FfiAccess.dynamicDataGetBool(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads an {@code int8} field at {@code path} (dotted/indexed). */
    public byte getI8(String path) {
        long h = handle();
        byte[] out = new byte[1];
        int rc = FfiAccess.dynamicDataGetI8(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads a {@code uint8} field at {@code path} (dotted/indexed) as an unsigned 0-255 int. */
    public int getU8(String path) {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicDataGetU8(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads an {@code int16} field at {@code path} (dotted/indexed). */
    public short getI16(String path) {
        long h = handle();
        short[] out = new short[1];
        int rc = FfiAccess.dynamicDataGetI16(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads a {@code uint16} field at {@code path} (dotted/indexed) as an unsigned 0-65535 int. */
    public int getU16(String path) {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicDataGetU16(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * Reads a {@code uint32} field at {@code path} (dotted/indexed) as its raw
     * 32 bits. A value at or above 2^31 reads back negative; widen with
     * {@code & 0xFFFFFFFFL} for the unsigned magnitude.
     */
    public int getU32(String path) {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicDataGetU32(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads an {@code int64} field at {@code path} (dotted/indexed). */
    public long getI64(String path) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicDataGetI64(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * Reads a {@code uint64} field at {@code path} (dotted/indexed) as its raw
     * 64 bits. A value at or above 2^63 reads back negative.
     */
    public long getU64(String path) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicDataGetU64(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads a {@code float32} field at {@code path} (dotted/indexed). */
    public float getF32(String path) {
        long h = handle();
        float[] out = new float[1];
        int rc = FfiAccess.dynamicDataGetF32(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads a {@code char8} field at {@code path} (dotted/indexed) as its raw byte value. */
    public byte getChar8(String path) {
        long h = handle();
        byte[] out = new byte[1];
        int rc = FfiAccess.dynamicDataGetChar8(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads the element count of a sequence/array field at {@code path} (dotted/indexed). */
    public int getLength(String path) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicDataGetLen(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (int) out[0];
    }

    /**
     * Reads a nested struct field at {@code path} (dotted/indexed) as its own
     * {@link DynamicData}, NativeCleaner-managed the same way as this one --
     * the caller owns the returned handle and should {@link #close} it
     * independently of the one it was read from.
     */
    public DynamicData getMember(String path) {
        long h = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicDataGetMember(h, path.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return DynamicData.fromHandle(out[0]);
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
