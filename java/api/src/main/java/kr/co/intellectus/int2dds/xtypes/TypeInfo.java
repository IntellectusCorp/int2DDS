package kr.co.intellectus.int2dds.xtypes;

import java.nio.charset.Charset;
import kr.co.intellectus.int2dds.cdr.Extensibility;
import kr.co.intellectus.int2dds.exceptions.DdsErrorException;
import kr.co.intellectus.int2dds.internal.NativeCleaner;
import kr.co.intellectus.int2dds.internal.NativeHandle;
import kr.co.intellectus.int2dds.internal.NativeKeepAlive;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A builder for a runtime XTypes type description: append fields with {@link #addField}, then bake
 * it into an immutable {@link TypeObject} with {@link #toTypeObject()}. Mirrors {@link
 * kr.co.intellectus.int2dds.conditions.Condition}'s handle-object pattern.
 */
public final class TypeInfo implements AutoCloseable {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    /** Member flag bits for the {@code flags} argument of the {@code add*Field} methods. */
    public static final int MEMBER_KEY = 1;

    public static final int MEMBER_OPTIONAL = 1 << 1;
    public static final int MEMBER_MUST_UNDERSTAND = 1 << 2;
    public static final int MEMBER_EXTERNAL = 1 << 3;

    /** Marks the {@code default:} member of a union built with {@link #createUnion}. */
    public static final int MEMBER_DEFAULT = 1 << 4;

    private final NativeHandle handle;
    private boolean hasKey;

    public TypeInfo(String name, Extensibility extensibility) {
        this(name.getBytes(UTF8), extensibility);
    }

    public TypeInfo(byte[] name, Extensibility extensibility) {
        this.handle =
                NativeCleaner.register(this, create(name, extensibility), TypeInfo::deleteVoid);
    }

    /** Wraps a raw builder handle already produced natively (e.g. by {@link #createEnum}). */
    TypeInfo(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, TypeInfo::deleteVoid);
    }

    /**
     * Creates an enum type-info builder. {@code bitBound} is the discriminant bit width (IDL enums
     * use 32). Populate literals with {@link #addEnumLiteral}, then reference this from a struct
     * builder with {@link #addNestedField} so the enum resolves at decode time.
     */
    public static TypeInfo createEnum(String name, int bitBound) {
        long[] out = new long[1];
        int rc = FfiAccess.typeInfoCreateEnum(name.getBytes(UTF8), bitBound, out);
        ReturnCodes.check(rc);
        return new TypeInfo(out[0]);
    }

    /**
     * Creates a bitmask type-info builder. {@code bitBound} is the flag storage bit width. Populate
     * flags with {@link #addBitmaskFlag}.
     */
    public static TypeInfo createBitmask(String name, int bitBound) {
        long[] out = new long[1];
        int rc = FfiAccess.typeInfoCreateBitmask(name.getBytes(UTF8), bitBound, out);
        ReturnCodes.check(rc);
        return new TypeInfo(out[0]);
    }

    /**
     * Creates a union type-info builder switching on {@code discriminatorType}, a scalar {@link
     * FieldType} (string kinds are rejected). Add each case member with the same {@code add*Field}
     * calls a struct uses, giving the {@code default:} member {@link #MEMBER_DEFAULT}, then attach
     * its labels with {@link #addUnionLabel}.
     */
    public static TypeInfo createUnion(
            String name, Extensibility extensibility, int discriminatorType) {
        long[] out = new long[1];
        int rc =
                FfiAccess.typeInfoCreateUnion(
                        name.getBytes(UTF8), extensibility.value(), discriminatorType, out);
        ReturnCodes.check(rc);
        return new TypeInfo(out[0]);
    }

    /**
     * Creates a bitset type-info builder. Populate it with {@link #addBitfield} in declaration
     * order, then reference it from a struct builder with {@link #addNestedField}.
     */
    public static TypeInfo createBitset(String name) {
        long[] out = new long[1];
        int rc = FfiAccess.typeInfoCreateBitset(name.getBytes(UTF8), out);
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

    /** True once a field carrying {@link #MEMBER_KEY} has been appended. */
    public boolean hasKey() {
        return hasKey;
    }

    /**
     * Creates a native topic advertising this type; the bridge {@link
     * kr.co.intellectus.int2dds.core.Topic} creates itself through. This builder is borrowed.
     * {@code qos} may be {@code 0L}. Returns the C ABI status code and, on success, the topic
     * handle in {@code handleOut[0]}.
     */
    public int createTopic(long participant, byte[] topicName, long qos, long[] handleOut) {
        int rc =
                FfiAccess.createTopicWithTypeInfo(participant, topicName, handle(), qos, handleOut);
        NativeKeepAlive.keepAlive(this);
        return rc;
    }

    private void fieldAdded(int rc, int flags) {
        ReturnCodes.check(rc);
        hasKey |= (flags & MEMBER_KEY) != 0;
    }

    /** Appends one field. {@code name} crosses as UTF-8, never modified UTF-8. */
    public void addField(String name, int fieldType, int flags) {
        int rc = FfiAccess.typeInfoAddField(handle(), name.getBytes(UTF8), fieldType, flags);
        // Without this fence the reaper could destroy this TypeInfo while the
        // native call above is still dereferencing the handle it was handed.
        // Same hazard FfiAccess's own bridges guard against -- see
        // NativeKeepAlive's doc for the full argument.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /** Appends a sequence field ({@code bound == 0} means unbounded). */
    public void addSequenceField(String name, int elementType, int bound, int flags) {
        int rc =
                FfiAccess.typeInfoAddSequenceField(
                        handle(), name.getBytes(UTF8), elementType, bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a nested struct-typed field referencing {@code nested}'s own builder. {@code nested}
     * is borrowed, not consumed -- the caller still owns it.
     */
    public void addNestedField(String name, TypeInfo nested, int flags) {
        int rc =
                FfiAccess.typeInfoAddNestedField(
                        handle(), name.getBytes(UTF8), nested.handle(), flags);
        // Same fence as addField, for both this builder and the borrowed nested one --
        // the native call dereferences both handles.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(nested);
        fieldAdded(rc, flags);
    }

    /** Appends a bounded string field ({@code bound == 0} means unbounded). */
    public void addStringField(String name, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddStringField(handle(), name.getBytes(UTF8), bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /** Appends a wide-string (wstring) field ({@code bound == 0} means unbounded). */
    public void addWstringField(String name, int bound, int flags) {
        int rc = FfiAccess.typeInfoAddWstringField(handle(), name.getBytes(UTF8), bound, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /** Appends a fixed-size array field of a primitive {@link FieldType}. */
    public void addArrayField(String name, int elementType, int arraySize, int flags) {
        int rc =
                FfiAccess.typeInfoAddArrayField(
                        handle(), name.getBytes(UTF8), elementType, arraySize, flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a fixed-size array field whose element is a nested struct, referencing {@code
     * element}'s own builder. {@code element} is borrowed, not consumed -- the caller still owns
     * it.
     */
    public void addArrayOfNestedField(String name, TypeInfo element, int arraySize, int flags) {
        int rc =
                FfiAccess.typeInfoAddArrayOfNestedField(
                        handle(), name.getBytes(UTF8), element.handle(), arraySize, flags);
        // Same fence as addNestedField, for both this builder and the borrowed element one --
        // the native call dereferences both handles.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(element);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a sequence field whose element is a nested struct ({@code bound == 0} means
     * unbounded), referencing {@code element}'s own builder. {@code element} is borrowed, not
     * consumed -- the caller still owns it.
     */
    public void addSequenceOfNestedField(String name, TypeInfo element, int bound, int flags) {
        int rc =
                FfiAccess.typeInfoAddSequenceOfNestedField(
                        handle(), name.getBytes(UTF8), element.handle(), bound, flags);
        // Same fence as addNestedField, for both this builder and the borrowed element one --
        // the native call dereferences both handles.
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(element);
        fieldAdded(rc, flags);
    }

    /** Appends a literal to an enum builder. No-op if this builder is not an enum. */
    public void addEnumLiteral(String name, int value, boolean isDefault) {
        int rc =
                FfiAccess.typeInfoAddEnumLiteral(
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
     * #createEnum}), rather than by a borrowed builder handle. Unlike {@link #addNestedField}, this
     * does not record the referenced type's {@link TypeObject} into this builder's dependency
     * closure, so the referenced type must resolve some other way (e.g. discovery) to decode.
     */
    public void addNamedTypeField(String fieldName, String typeName, int flags) {
        int rc =
                FfiAccess.typeInfoAddNamedTypeField(
                        handle(), fieldName.getBytes(UTF8), typeName.getBytes(UTF8), flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a sequence field whose element is a type referenced by name (e.g. a struct built
     * elsewhere), rather than a borrowed builder handle ({@code bound == 0} means unbounded). Like
     * {@link #addNamedTypeField}, this does not record the referenced type's {@link TypeObject}
     * into this builder's dependency closure, so the element type must resolve some other way (e.g.
     * discovery) to decode.
     */
    public void addSequenceOfNamedField(
            String fieldName, String elementTypeName, int bound, int flags) {
        int rc =
                FfiAccess.typeInfoAddSequenceOfNamedField(
                        handle(),
                        fieldName.getBytes(UTF8),
                        elementTypeName.getBytes(UTF8),
                        bound,
                        flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a fixed-size array field whose element is a type referenced by name, rather than a
     * borrowed builder handle. Same discovery-resolution caveat as {@link
     * #addSequenceOfNamedField}.
     */
    public void addArrayOfNamedField(
            String fieldName, String elementTypeName, int arraySize, int flags) {
        int rc =
                FfiAccess.typeInfoAddArrayOfNamedField(
                        handle(),
                        fieldName.getBytes(UTF8),
                        elementTypeName.getBytes(UTF8),
                        arraySize,
                        flags);
        // Same fence as addField.
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a {@code bitfield<bitcount>} to a bitset builder. {@code holderType} is the integer
     * {@link FieldType} that holds it ({@code BYTE} up to 8 bits, then {@code UINT16}/{@code
     * UINT32}/{@code UINT64}); the bit position follows the previous bitfield.
     */
    public void addBitfield(String name, int bitcount, int holderType) {
        int rc = FfiAccess.typeInfoAddBitfield(handle(), name.getBytes(UTF8), bitcount, holderType);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Attaches the case label {@code label} to the union member {@code memberName}, which must
     * already have been added. Boolean labels are {@code 1}/{@code 0}; enum labels are their
     * literal value.
     */
    public void addUnionLabel(String memberName, int label) {
        int rc = FfiAccess.typeInfoAddUnionLabel(handle(), memberName.getBytes(UTF8), label);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
    }

    /**
     * Appends a {@code map<K, V>} field of scalar {@link FieldType} kinds. {@code keyBound}/{@code
     * valueBound} apply to string kinds only and {@code bound} is the map's own; {@code 0} means
     * unbounded throughout.
     */
    public void addMapField(
            String name,
            int keyType,
            int keyBound,
            int valueType,
            int valueBound,
            int bound,
            int flags) {
        int rc =
                FfiAccess.typeInfoAddMapField(
                        handle(),
                        name.getBytes(UTF8),
                        keyType,
                        keyBound,
                        valueType,
                        valueBound,
                        bound,
                        flags);
        NativeKeepAlive.keepAlive(this);
        fieldAdded(rc, flags);
    }

    /**
     * Appends a {@code map<K, Nested>} field whose values are described by {@code value}'s builder.
     * {@code value} is borrowed, not consumed.
     */
    public void addMapOfNestedField(
            String name, int keyType, int keyBound, TypeInfo value, int bound, int flags) {
        int rc =
                FfiAccess.typeInfoAddMapOfNestedField(
                        handle(),
                        name.getBytes(UTF8),
                        keyType,
                        keyBound,
                        value.handle(),
                        bound,
                        flags);
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(value);
        fieldAdded(rc, flags);
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
