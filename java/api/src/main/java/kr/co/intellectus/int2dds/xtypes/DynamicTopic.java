package kr.co.intellectus.int2dds.xtypes;

import kr.co.intellectus.int2dds.internal.NativeCleaner;
import kr.co.intellectus.int2dds.internal.NativeHandle;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A topic backed by a {@link DynamicTypeSupport} rather than a generated
 * {@link kr.co.intellectus.int2dds.types.IDdsType}. Produced by {@link
 * kr.co.intellectus.int2dds.core.DomainParticipant#createDynamicTopic}.
 * NativeCleaner-managed like {@link kr.co.intellectus.int2dds.core.Topic}; a
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
     * kr.co.intellectus.int2dds.core.DomainParticipant#createDynamicTopic} --
     * outside this package -- can hand back a handle produced by {@link
     * FfiAccess#createTopicDynamic}.
     */
    public static DynamicTopic fromHandle(long rawHandle) {
        return new DynamicTopic(rawHandle);
    }

    /**
     * The native pointer. Public -- unlike the package-private {@code
     * handle()} convention elsewhere -- because {@link
     * kr.co.intellectus.int2dds.core.Publisher#createDynamicDataWriter} and
     * {@link kr.co.intellectus.int2dds.core.Subscriber#createDynamicDataReader}
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
