package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.cdr.Extensibility;
import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;

/**
 * A built XTypes TypeObject — the type description
 * {@link com.intellectus.int2dds.core.DomainParticipant#dynamicDataFromSample}
 * decodes a serialized sample against. Produced by {@link
 * TypeInfo#toTypeObject()}; a distinct native allocation from the builder
 * that produced it, so both must be released independently.
 */
public final class TypeObject implements AutoCloseable {

    private final NativeHandle handle;

    TypeObject(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, TypeObject::deleteVoid);
    }

    private static int deleteVoid(long h) {
        FfiAccess.typeObjectDestroy(h);
        return 0;
    }

    /**
     * The native pointer. Public — unlike the package-private {@code handle()}
     * convention elsewhere — because {@link
     * com.intellectus.int2dds.core.DomainParticipant#dynamicDataFromSample}
     * lives in a different package and needs it to call {@link
     * FfiAccess#dynamicDataFromSample}.
     */
    public long handle() {
        return handle.value();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }

    private static final Charset UTF8 = Charset.forName("UTF-8");

    /**
     * The number of struct members. Requires a struct TypeObject -- throws
     * {@code DdsException} (unsupported-type code) for any other kind.
     */
    public int memberCount() {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.typeObjectMemberCount(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * The member name at {@code index}. Requires a struct TypeObject; throws
     * for an out-of-range index or a non-struct TypeObject.
     */
    public String memberName(int index) {
        long h = handle();
        byte[][] out = new byte[1][];
        int rc = FfiAccess.typeObjectMemberName(h, index, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return new String(out[0], UTF8);
    }

    /**
     * The index of the member named {@code name}. Requires a struct
     * TypeObject. Throws {@code DdsException} for an unknown name -- the
     * native {@code RET_DYNAMIC_FIELD_NOT_FOUND} code, surfaced generically
     * through {@link ReturnCodes#check} since that family has no dedicated
     * exception type yet -- so callers wanting a non-throwing lookup should
     * catch {@code DdsException} rather than compare against a sentinel.
     */
    public int findMember(String name) {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.typeObjectFindMember(h, name.getBytes(UTF8), out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return out[0];
    }

    /**
     * Per-member info (member id, {@link FieldType} kind, flags) at {@code
     * index}. Requires a struct TypeObject; throws for an out-of-range index
     * or a non-struct TypeObject.
     */
    public MemberInfo memberInfo(int index) {
        long h = handle();
        ByteBuffer buf = ByteBuffer.allocateDirect(12).order(ByteOrder.nativeOrder());
        int rc = FfiAccess.typeObjectMemberInfo(h, index, FfiAccess.directBufferAddress(buf));
        NativeKeepAlive.keepAlive(this);
        NativeKeepAlive.keepAlive(buf);
        ReturnCodes.check(rc);
        int memberId = buf.getInt(0);
        int kind = buf.getInt(4);
        int flags = buf.getInt(8);
        return new MemberInfo(memberId, kind, flags);
    }

    /**
     * The struct's extensibility. Requires a struct TypeObject -- throws for
     * any other kind.
     */
    public Extensibility extensibility() {
        long h = handle();
        int[] out = new int[1];
        int rc = FfiAccess.typeObjectExtensibility(h, out);
        NativeKeepAlive.keepAlive(this);
        ReturnCodes.check(rc);
        return Extensibility.fromValue(out[0]);
    }
}
