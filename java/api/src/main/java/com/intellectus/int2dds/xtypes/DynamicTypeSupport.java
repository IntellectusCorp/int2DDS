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

    /**
     * Wraps an already-created native DynamicTypeSupport handle. Public, in
     * the same style as {@link DynamicData#fromHandle}, so callers outside
     * this package (namely {@code DomainParticipantFactory}) can hand back a
     * handle produced by a bridge such as {@link FfiAccess#getDynamicTypeSupport}.
     */
    public static DynamicTypeSupport fromHandle(long rawHandle) {
        return new DynamicTypeSupport(rawHandle);
    }

    private static int deleteVoid(long h) {
        FfiAccess.dynamicTypeSupportDestroy(h);
        return 0;
    }

    /**
     * The native pointer. Public -- unlike the package-private {@code
     * handle()} convention elsewhere -- because {@link
     * com.intellectus.int2dds.core.DomainParticipant#createDynamicTopic},
     * {@link com.intellectus.int2dds.core.Publisher#createDynamicDataWriter}
     * and {@link com.intellectus.int2dds.core.Subscriber#createDynamicDataReader}
     * live in a different package and need it to call the matching
     * {@code FfiAccess} bridge, the same reasoning as {@link TypeObject#handle()}.
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
