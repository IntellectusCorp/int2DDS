package com.intellectus.int2dds.xtypes;

import com.intellectus.int2dds.internal.NativeCleaner;
import com.intellectus.int2dds.internal.NativeHandle;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A topic backed by a {@link DynamicTypeSupport} rather than a generated
 * {@link com.intellectus.int2dds.types.IDdsType}. Produced by {@link
 * com.intellectus.int2dds.core.DomainParticipant#createDynamicTopic}.
 * NativeCleaner-managed like {@link com.intellectus.int2dds.core.Topic}; a
 * dynamic topic is still a topic on the native side, so this reuses the same
 * {@code int2dds_delete_topic} deleter through {@link FfiAccess#deleteTopic}.
 */
public final class DynamicTopic implements AutoCloseable {

    private final NativeHandle handle;

    private DynamicTopic(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, FfiAccess::deleteTopic);
    }

    /**
     * Wraps an already-created native dynamic-topic handle. Public, the same
     * style as {@link DynamicData#fromHandle}, so {@link
     * com.intellectus.int2dds.core.DomainParticipant#createDynamicTopic} --
     * outside this package -- can hand back a handle produced by {@link
     * FfiAccess#createTopicDynamic}.
     */
    public static DynamicTopic fromHandle(long rawHandle) {
        return new DynamicTopic(rawHandle);
    }

    /**
     * The native pointer. Public -- unlike the package-private {@code
     * handle()} convention elsewhere -- because {@link
     * com.intellectus.int2dds.core.Publisher#createDynamicDataWriter} and
     * {@link com.intellectus.int2dds.core.Subscriber#createDynamicDataReader}
     * live in a different package and need it to call their matching
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
