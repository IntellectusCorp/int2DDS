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

    /** Wraps a raw builder handle already produced natively (e.g. by {@link #createEnum}). */
    TypeInfo(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, TypeInfo::deleteVoid);
    }

    /**
     * Creates an enum type-info builder. {@code bitBound} is the discriminant bit width
     * (IDL enums use 32). Populate literals with {@link #addEnumLiteral}, then reference
     * this from a struct builder with {@link #addNestedField} so the enum resolves at
     * decode time.
     */
    public static TypeInfo createEnum(String name, int bitBound) {
        long[] out = new long[1];
        int rc = FfiAccess.typeInfoCreateEnum(name.getBytes(UTF8), bitBound, out);
        ReturnCodes.check(rc);
        return new TypeInfo(out[0]);
    }

    /**
     * Creates a bitmask type-info builder. {@code bitBound} is the flag storage bit
     * width. Populate flags with {@link #addBitmaskFlag}.
     */
    public static TypeInfo createBitmask(String name, int bitBound) {
        long[] out = new long[1];
        int rc = FfiAccess.typeInfoCreateBitmask(name.getBytes(UTF8), bitBound, out);
        ReturnCodes.check(rc);
        return new TypeInfo(out[0]);
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

    /** Appends a sequence field ({@code bound == 0} means unbounded). */
    public void addSequenceField(String name, int elementType, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddSequenceField(
                handle(), name.getBytes(UTF8), elementType, bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a nested struct-typed field referencing {@code nested}'s own builder.
     * {@code nested} is borrowed, not consumed -- the caller still owns it.
     */
    public void addNestedField(String name, TypeInfo nested, int flags) {
        int rc = FfiAccess.typeInfoAddNestedField(handle(), name.getBytes(UTF8), nested.handle(), flags);
        // Same fence as addField, for both this builder and the borrowed nested one --
        // the native call dereferences both handles.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(nested);
        ReturnCodes.check(rc);
    }

    /** Appends a bounded string field ({@code bound == 0} means unbounded). */
    public void addStringField(String name, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddStringField(handle(), name.getBytes(UTF8), bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Appends a wide-string (wstring) field ({@code bound == 0} means unbounded). */
    public void addWstringField(String name, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddWstringField(handle(), name.getBytes(UTF8), bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Appends a fixed-size array field of a primitive {@link FieldType}. */
    public void addArrayField(String name, int elementType, int arraySize, int flags) {
        int rc = FfiAccess.typeInfoAddArrayField(
                handle(), name.getBytes(UTF8), elementType, arraySize, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a fixed-size array field whose element is a nested struct, referencing
     * {@code element}'s own builder. {@code element} is borrowed, not consumed -- the
     * caller still owns it.
     */
    public void addArrayOfNestedField(String name, TypeInfo element, int arraySize, int flags) {
        int rc = FfiAccess.typeInfoAddArrayOfNestedField(
                handle(), name.getBytes(UTF8), element.handle(), arraySize, flags);
        // Same fence as addNestedField, for both this builder and the borrowed element one --
        // the native call dereferences both handles.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(element);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a sequence field whose element is a nested struct ({@code bound == 0}
     * means unbounded), referencing {@code element}'s own builder. {@code element} is
     * borrowed, not consumed -- the caller still owns it.
     */
    public void addSequenceOfNestedField(String name, TypeInfo element, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddSequenceOfNestedField(
                handle(), name.getBytes(UTF8), element.handle(), bound, flags);
        // Same fence as addNestedField, for both this builder and the borrowed element one --
        // the native call dereferences both handles.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(element);
        ReturnCodes.check(rc);
    }

    /** Appends a literal to an enum builder. No-op if this builder is not an enum. */
    public void addEnumLiteral(String name, int value, boolean isDefault) {
        int rc = FfiAccess.typeInfoAddEnumLiteral(
                handle(), name.getBytes(UTF8), value, isDefault ? 1 : 0);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /** Appends a flag to a bitmask builder. No-op if this builder is not a bitmask. */
    public void addBitmaskFlag(String name, int position) {
        int rc = FfiAccess.typeInfoAddBitmaskFlag(handle(), name.getBytes(UTF8), position);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a field referencing another type by name (e.g. an enum built with {@link
     * #createEnum}), rather than by a borrowed builder handle. Unlike {@link
     * #addNestedField}, this does not record the referenced type's {@link TypeObject} into
     * this builder's dependency closure, so the referenced type must resolve some other
     * way (e.g. discovery) to decode.
     */
    public void addNamedTypeField(String fieldName, String typeName, int flags) {
        int rc = FfiAccess.typeInfoAddNamedTypeField(
                handle(), fieldName.getBytes(UTF8), typeName.getBytes(UTF8), flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a sequence field whose element is a type referenced by name (e.g. a
     * struct built elsewhere), rather than a borrowed builder handle ({@code bound == 0}
     * means unbounded). Like {@link #addNamedTypeField}, this does not record the
     * referenced type's {@link TypeObject} into this builder's dependency closure, so the
     * element type must resolve some other way (e.g. discovery) to decode.
     */
    public void addSequenceOfNamedField(String fieldName, String elementTypeName, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddSequenceOfNamedField(
                handle(), fieldName.getBytes(UTF8), elementTypeName.getBytes(UTF8), bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a fixed-size array field whose element is a type referenced by name,
     * rather than a borrowed builder handle. Same discovery-resolution caveat as {@link
     * #addSequenceOfNamedField}.
     */
    public void addArrayOfNamedField(String fieldName, String elementTypeName, int arraySize, int flags) {
        int rc = FfiAccess.typeInfoAddArrayOfNamedField(
                handle(), fieldName.getBytes(UTF8), elementTypeName.getBytes(UTF8), arraySize, flags);
        // Same fence as addField.
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
