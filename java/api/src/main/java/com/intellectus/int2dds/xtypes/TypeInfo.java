package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.charset.Charset;

/**
 * A builder for a runtime XTypes type description: append fields with
 * {@link #addField}, then bake it into an immutable {@link TypeObject} with
 * {@link #toTypeObject()}. Mirrors {@link
 * com.intellectus.int2dds.conditions.Condition}'s handle-object pattern.
 */
public final class TypeInfo implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private final NativeHandle handle;

    public TypeInfo(String name, Extensibility extensibility) {
        this(name.getBytes(UTF8), extensibility);
    }

    public TypeInfo(byte[] name, Extensibility extensibility) {
        this.handle = NativeCleaner.register(this, create(name, extensibility), TypeInfo::deleteVoid);
    }

    private static long create(byte[] name, Extensibility extensibility) {
        long h = FfiAccess.typeInfoCreate(name, extensibility.value());
        if (h == 0L) {
            throw new DdsErrorException("failed to create a native TypeInfo builder");
        }
        return h;
    }

    private static int deleteVoid(long h) {
        FfiAccess.typeInfoDestroy(h);
        return 0;
    }

    long handle() {
        return handle.value();
    }

    /** Appends one field. {@code name} crosses as UTF-8, never modified UTF-8. */
    public void addField(String name, int fieldType, int flags) {
        int rc = FfiAccess.typeInfoAddField(handle(), name.getBytes(UTF8), fieldType, flags);
        // Without this fence the reaper could destroy this TypeInfo while the
        // native call above is still dereferencing the handle it was handed.
        // Same hazard FfiAccess's own bridges guard against -- see
        // NativeKeepAlive's doc for the full argument.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Bakes the fields appended so far into an immutable {@link TypeObject}. */
    public TypeObject toTypeObject() {
        long h = handle();
        long typeObj = FfiAccess.typeInfoToTypeObject(h);
        // Same fence as addField: this TypeInfo must stay reachable until the
        // native call that dereferenced its handle has returned.
        NativeKeepAlive.keepAlive(this);
        if (typeObj == 0L) {
            throw new DdsErrorException("failed to build a TypeObject from this TypeInfo");
        }
        return new TypeObject(typeObj);
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
