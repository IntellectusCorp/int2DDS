package kr.co.intellectus.int2dds.xtypes;

import java.nio.charset.Charset;
import kr.co.intellectus.int2dds.exceptions.DdsErrorException;
import kr.co.intellectus.int2dds.exceptions.DdsException;
import kr.co.intellectus.int2dds.internal.NativeCleaner;
import kr.co.intellectus.int2dds.internal.NativeHandle;
import kr.co.intellectus.int2dds.internal.NativeKeepAlive;
import kr.co.intellectus.int2dds.internal.ReturnCodes;
import kr.co.intellectus.int2dds.internal.ffi.FfiAccess;

/**
 * A topic backed by a {@link DynamicTypeSupport} rather than a generated {@link
 * kr.co.intellectus.int2dds.types.IDdsType}. Produced by {@link
 * kr.co.intellectus.int2dds.core.DomainParticipant#createDynamicTopic}. NativeCleaner-managed like
 * {@link kr.co.intellectus.int2dds.core.Topic}; a dynamic topic is still a topic on the native
 * side, so this reuses the same {@code int2dds_delete_topic} deleter through {@link
 * FfiAccess#deleteTopic}.
 */
public final class DynamicTopic implements AutoCloseable {

    private final NativeHandle handle;

    private DynamicTopic(long rawHandle) {
        this.handle = NativeCleaner.register(this, rawHandle, FfiAccess::deleteTopic);
    }

    /**
     * Wraps an already-created native dynamic-topic handle. Public, the same style as {@link
     * DynamicData#fromHandle}, so {@link
     * kr.co.intellectus.int2dds.core.DomainParticipant#createDynamicTopic} -- outside this package
     * -- can hand back a handle produced by {@link FfiAccess#createTopicDynamic}.
     */
    public static DynamicTopic fromHandle(long rawHandle) {
        return new DynamicTopic(rawHandle);
    }

    /**
     * The native pointer. Public -- unlike the package-private {@code handle()} convention
     * elsewhere -- because {@link kr.co.intellectus.int2dds.core.Publisher#createDynamicDataWriter}
     * and {@link kr.co.intellectus.int2dds.core.Subscriber#createDynamicDataReader} live in a
     * different package and need it to call their matching {@code FfiAccess} bridge, the same
     * reasoning as {@link TypeObject#handle()}.
     */
    public long handle() {
        return handle.value();
    }

    public boolean isClosed() {
        return handle.isClosed();
    }

    /** The topic name, read from the native topic. */
    public String name() {
        return readName(true);
    }

    /** The DDS type name the topic registered, read from the native topic. */
    public String typeName() {
        return readName(false);
    }

    private String readName(boolean topicName) {
        long h = handle();
        int cap = 256;
        while (true) {
            byte[] buf = new byte[cap];
            int rc =
                    topicName ? FfiAccess.topicGetName(h, buf) : FfiAccess.topicGetTypeName(h, buf);
            NativeKeepAlive.keepAlive(this);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                if (cap >= MAX_NAME_BYTES) {
                    throw new DdsErrorException(
                            "topic name longer than " + MAX_NAME_BYTES + " bytes");
                }
                cap <<= 1;
                continue;
            }
            ReturnCodes.check(rc);
            int n = 0;
            while (n < buf.length && buf[n] != 0) {
                n++;
            }
            return new String(buf, 0, n, UTF8);
        }
    }

    private static final Charset UTF8 = Charset.forName("UTF-8");
    private static final int MAX_NAME_BYTES = 1 << 20;

    @Override
    public void close() {
        ReturnCodes.check(handle.close());
    }
}
