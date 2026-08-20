package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;

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
}
