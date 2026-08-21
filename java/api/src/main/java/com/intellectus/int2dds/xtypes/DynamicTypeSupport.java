package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A type support looked up from an {@link XmlTypeRegistry}, used to create
 * writable {@link DynamicData} instances via {@link DynamicData#create}.
 * NativeCleaner-managed like {@link TypeObject}.
 */
public final class DynamicTypeSupport implements AutoCloseable {

    private final NativeHandle handle;

    DynamicTypeSupport(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, DynamicTypeSupport::deleteVoid);
    }

    private static int deleteVoid(long h) {
        FfiAccess.dynamicTypeSupportDestroy(h);
        return 0;
    }

    long handle() {
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
