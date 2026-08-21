package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * A standalone dynamic-type value, built up from scalars (and, via {@link
 * #push}, sequences of them) and then handed to {@link
 * DynamicData#setValue} to write a field that a plain scalar setter cannot
 * reach -- today, a sequence.
 *
 * <p><b>Ownership.</b> The native layer transfers ownership of a value's
 * handle whenever it is moved into something else: {@link #push} moves this
 * value into the sequence/array it is called on, and {@link
 * DynamicData#setValue} moves a value into a field. After either happens,
 * the moved handle is dead on the native side -- the memory it pointed to
 * has been freed -- so this class tracks a {@code consumed} flag and makes
 * every use of a consumed value's handle (further {@link #push} or {@link
 * #handle()} calls, {@link #close()}, and the NativeCleaner reaper) a
 * no-op or a loud failure instead of a double-free.
 */
public final class DynamicValue implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final AtomicBoolean consumed = new AtomicBoolean(false);
    private final NativeHandle handle;

    private DynamicValue(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, this::deleteUnlessConsumed);
    }

    /**
     * The deleter registered with the NativeCleaner. Guards the reaper path
     * the same way {@link #close()} guards the explicit path: once {@code
     * consumed} is set, the handle has already been freed as part of moving
     * it into a collection or a DynamicData field, so there is nothing left
     * here to destroy.
     */
    private int deleteUnlessConsumed(long h) {
        if (!consumed.get()) {
            FfiAccess.dynamicValueDestroy(h);
        }
        return 0;
    }

    /** Builds an {@code int32} value. */
    public static DynamicValue i32(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueI32(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code bool} value. */
    public static DynamicValue bool(boolean value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueBool(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds an {@code int8} value. */
    public static DynamicValue i8(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueI8(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code uint8} value. */
    public static DynamicValue u8(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueU8(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds an {@code int16} value. */
    public static DynamicValue i16(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueI16(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code uint16} value. */
    public static DynamicValue u16(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueU16(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code uint32} value. */
    public static DynamicValue u32(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueU32(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code byte} (octet) value. */
    public static DynamicValue byte8(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueByte(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code char8} value. */
    public static DynamicValue char8(int value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueChar8(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds an {@code int64} value. */
    public static DynamicValue i64(long value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueI64(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code uint64} value. */
    public static DynamicValue u64(long value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueU64(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code float32} value. */
    public static DynamicValue f32(float value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueF32(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code float64} value. */
    public static DynamicValue f64(double value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueF64(value, out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds a {@code string} value from UTF-8 bytes. */
    public static DynamicValue string(String value) {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueString(value.getBytes(UTF8), out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds an empty sequence value. Fill it with {@link #push}. */
    public static DynamicValue sequence() {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueSequence(out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Builds an empty array value. Fill it with {@link #push}. */
    public static DynamicValue array() {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueArray(out);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /**
     * Snapshots the current field values of {@code data} into an immutable
     * struct value. {@code data} is cloned natively, not consumed -- the
     * caller still owns {@code data} and must {@link DynamicData#close} it
     * independently, before or after this call.
     */
    public static DynamicValue struct(DynamicData data) {
        long d = data.handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueStruct(d, out);
        NativeKeepAlive.keepAlive(data);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /**
     * Appends {@code element} to this sequence/array value.
     *
     * <p>Consumes {@code element}: on success its handle has been moved into
     * this collection, so it must not be used, pushed elsewhere, or closed
     * again -- this method marks it consumed itself, making its {@link
     * #close()} a no-op. Throws (without consuming {@code element}) if this
     * value is not a sequence/array, or if either value has already been
     * consumed.
     */
    public void push(DynamicValue element) {
        if (element == null) {
            throw new NullPointerException("element");
        }
        if (element.consumed.get()) {
            throw new IllegalStateException("element has already been consumed");
        }
        long c = handle();
        long e = element.handle();
        int rc = FfiAccess.dynamicValuePush(c, e);
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(element);
        ReturnCodes.check(rc);
        element.consumed.set(true);
    }

    /**
     * The native handle. Throws if this value has already been consumed
     * (moved into a collection or a DynamicData field) -- its handle is
     * dead and must not be read again, let alone passed to another native
     * call.
     */
    long handle() {
        if (consumed.get()) {
            throw new IllegalStateException("dynamic value has already been consumed");
        }
        return handle.value();
    }

    /** True once this value's handle has been moved elsewhere and is no longer owned here. */
    public boolean isConsumed() {
        return consumed.get();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    /**
     * Marks this value consumed without a native call -- used by {@link
     * DynamicData#setValue} after a successful {@code dynamic_data_set_value},
     * which moves this value's handle into the DynamicData field.
     */
    void markConsumed() {
        consumed.set(true);
    }

    /**
     * Releases this value's handle, unless it has already been consumed
     * (moved into a collection or a DynamicData field), in which case this
     * is a no-op -- there is nothing left here to free.
     */
    @Override
    public void close() {
        if (consumed.get()) {
            return;
        }
        ReturnCodes.check(handle.close());
    }
}
