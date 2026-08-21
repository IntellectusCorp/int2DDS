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
 * #push} or {@link #insert}, sequences/arrays or maps of them) and then
 * handed to {@link DynamicData#setValue} to write a field that a plain
 * scalar setter cannot reach -- today, a sequence, array or map.
 *
 * <p><b>Ownership.</b> The native layer transfers ownership of a value's
 * handle whenever it is moved into something else: {@link #push} moves this
 * value into the sequence/array it is called on, {@link #insert} moves a key
 * and a value into the map it is called on, and {@link DynamicData#setValue}
 * moves a value into a field. After any of these happen, the moved handle is
 * dead on the native side -- the memory it pointed to has been freed -- so
 * this class tracks a {@code consumed} flag and makes every use of a
 * consumed value's handle (further {@link #push}, {@link #insert} or {@link
 * #handle()} calls, {@link #close()}, and the NativeCleaner reaper) a no-op
 * or a loud failure instead of a double-free.
 */
public final class DynamicValue implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final AtomicBoolean consumed = new AtomicBoolean(false);
    private final NativeHandle handle;

    /**
     * Package-visible so {@link DynamicData#getValue} can wrap a handle
     * produced by {@code int2dds_dynamic_data_get_value} -- that call clones
     * a new, independently-owned value rather than transferring ownership of
     * an existing one, so the result is a fresh {@code DynamicValue} exactly
     * like one of the static factories below returns.
     */
    DynamicValue(long rawHandle) {
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

    /** Builds an empty map value. Fill it with {@link #insert}. */
    public static DynamicValue map() {
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueMap(out);
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
     * Builds a union value from {@code discriminator} and {@code value}, the
     * selected branch.
     *
     * <p>Consumes BOTH {@code discriminator} and {@code value}: the native
     * call frees their handles unconditionally -- right after its null
     * checks and before any fallible step -- so, unlike {@link #push} and
     * {@link #insert}, there is no failure path that leaves either argument
     * still owned by the caller. Both are therefore marked consumed the same
     * way regardless of the returned code, mirroring {@link
     * DynamicData#setValue}: if the native call somehow still failed, the
     * handles are already dead, and marking them consumed before the {@link
     * ReturnCodes#check} that raises the exception avoids a double-free from
     * a subsequent {@link #close()}. Throws (without consuming either) only
     * if a null is passed or an argument has already been consumed --
     * conditions Java rejects before making the native call.
     */
    public static DynamicValue union(DynamicValue discriminator, DynamicValue value) {
        if (discriminator == null) {
            throw new NullPointerException("discriminator");
        }
        if (value == null) {
            throw new NullPointerException("value");
        }
        if (discriminator.consumed.get()) {
            throw new IllegalStateException("discriminator has already been consumed");
        }
        if (value.consumed.get()) {
            throw new IllegalStateException("value has already been consumed");
        }
        long d = discriminator.handle();
        long v = value.handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueUnion(d, v, out);
        NativeKeepAlive.keepAlive(discriminator);
        NativeKeepAlive.keepAlive(value);
        discriminator.consumed.set(true);
        value.consumed.set(true);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /**
     * Clones this union value's discriminator into a new,
     * independently-owned {@link DynamicValue}. The caller owns the
     * returned value and must {@link #close} it -- this value is untouched
     * and remains usable afterward.
     */
    public DynamicValue unionDiscriminator() {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueUnionDiscriminator(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /**
     * Clones this union value's selected branch value into a new,
     * independently-owned {@link DynamicValue}. The caller owns the
     * returned value and must {@link #close} it -- this value is untouched
     * and remains usable afterward.
     */
    public DynamicValue unionValue() {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueUnionValue(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** The kind of this value, one of the {@link DynamicValueKind} constants. */
    public int kind() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueKind(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** The element count of this sequence/array/map value. */
    public int length() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueLen(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * Clones the element at {@code index} of this sequence/array value into a
     * new, independently-owned {@link DynamicValue}. The caller owns the
     * returned value and must {@link #close} it -- this value is untouched
     * and remains usable afterward.
     */
    public DynamicValue element(int index) {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueElement(v, index, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /**
     * Clones the key at {@code index} of this map value into a new,
     * independently-owned {@link DynamicValue}. The caller owns the returned
     * value and must {@link #close} it -- this value is untouched and
     * remains usable afterward.
     */
    public DynamicValue mapKey(int index) {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueMapKey(v, index, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /**
     * Clones the value at {@code index} of this map value into a new,
     * independently-owned {@link DynamicValue}. The caller owns the returned
     * value and must {@link #close} it -- this value is untouched and
     * remains usable afterward.
     */
    public DynamicValue mapValue(int index) {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueMapValue(v, index, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new DynamicValue(out[0]);
    }

    /** Reads this value as a {@code bool}. Throws if it is not a bool. */
    public boolean asBool() {
        long v = handle();
        boolean[] out = new boolean[1];
        int rc = FfiAccess.dynamicValueAsBool(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as an {@code int8}. Throws if it is not an int8. */
    public byte asI8() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsI8(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (byte) out[0];
    }

    /** Reads this value as a {@code uint8}, its unsigned 0-255 value. Throws if it is not a uint8. */
    public int asU8() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsU8(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as an {@code int16}. Throws if it is not an int16. */
    public short asI16() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsI16(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return (short) out[0];
    }

    /** Reads this value as a {@code uint16}, its unsigned 0-65535 value. Throws if it is not a uint16. */
    public int asU16() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsU16(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as an {@code int32}. Throws if it is not an int32. */
    public int asI32() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsI32(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * Reads this value as a {@code uint32}, its raw 32 bits. A value at or
     * above 2^31 reads back negative; widen with {@code & 0xFFFFFFFFL} for
     * the unsigned magnitude. Throws if it is not a uint32.
     */
    public int asU32() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsU32(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as an {@code int64}. Throws if it is not an int64. */
    public long asI64() {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueAsI64(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * Reads this value as a {@code uint64}, its raw 64 bits. A value at or
     * above 2^63 reads back negative. Throws if it is not a uint64.
     */
    public long asU64() {
        long v = handle();
        long[] out = new long[1];
        int rc = FfiAccess.dynamicValueAsU64(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as a {@code float32}. Throws if it is not a float32. */
    public float asF32() {
        long v = handle();
        float[] out = new float[1];
        int rc = FfiAccess.dynamicValueAsF32(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as a {@code float64}. Throws if it is not a float64. */
    public double asF64() {
        long v = handle();
        double[] out = new double[1];
        int rc = FfiAccess.dynamicValueAsF64(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as a {@code char8}, its unsigned 0-255 byte value. Throws if it is not a char8. */
    public int asChar8() {
        long v = handle();
        int[] out = new int[1];
        int rc = FfiAccess.dynamicValueAsChar8(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /** Reads this value as a {@code string}/{@code wstring}. Throws if it is not one. */
    public String asString() {
        long v = handle();
        byte[][] out = new byte[1][];
        int rc = FfiAccess.dynamicValueAsString(v, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new String(out[0], UTF8);
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
     * Inserts {@code key}/{@code value} into this map value.
     *
     * <p>Consumes BOTH {@code key} and {@code value}: on success their
     * handles have been moved into this map, so neither must be used, pushed
     * or inserted elsewhere, or closed again -- this method marks both
     * consumed itself, making their {@link #close()} a no-op. Throws
     * (without consuming either) if this value is not a map, or if either
     * argument has already been consumed.
     */
    public void insert(DynamicValue key, DynamicValue value) {
        if (key == null) {
            throw new NullPointerException("key");
        }
        if (value == null) {
            throw new NullPointerException("value");
        }
        if (key.consumed.get()) {
            throw new IllegalStateException("key has already been consumed");
        }
        if (value.consumed.get()) {
            throw new IllegalStateException("value has already been consumed");
        }
        long m = handle();
        long k = key.handle();
        long v = value.handle();
        int rc = FfiAccess.dynamicValueMapInsert(m, k, v);
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(key);
        NativeKeepAlive.keepAlive(value);
        ReturnCodes.check(rc);
        key.consumed.set(true);
        value.consumed.set(true);
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
