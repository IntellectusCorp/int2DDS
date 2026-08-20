package com.intellectus.int2dds.core;

import com.intellectus.int2dds.cdr.CdrReader;
import com.intellectus.int2dds.exceptions.DdsErrorException;
import com.intellectus.int2dds.exceptions.DdsException;
import com.intellectus.int2dds.internal.NativeKeepAlive;
import com.intellectus.int2dds.internal.QosMarshal;
import com.intellectus.int2dds.internal.ReturnCodes;
import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.types.IDdsType;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.util.Objects;
import java.util.function.Supplier;

/**
 * Receives samples of type {@code T} from a {@link Topic}. Structural twin of
 * {@link DataWriter}: created through {@link Subscriber#createDataReader}, it
 * takes/reads one serialized sample into a pooled direct buffer and rebuilds
 * {@code T} with the CDR layer — no loan, no copy beyond the core's own.
 *
 * <p><b>Listener defect (deferred):</b> {@code int2dds_delete_datareader}
 * runs {@code set_listener(None)} before its rejectable delete without
 * restoring on failure. This binding never installs a listener, so that path
 * is dormant here; a future listener branch owns the fix.
 *
 * @param <T> the DDS data type this reader receives.
 */
public final class DataReader<T extends IDdsType> extends NativeEntity {

    private static final boolean LITTLE_ENDIAN_HOST =
            ByteOrder.nativeOrder() == ByteOrder.LITTLE_ENDIAN;
    private static final int DEFAULT_CAPACITY = 4096;

    private final Topic<T> topic;
    private final Supplier<T> factory;
    private final boolean xcdr2;

    // Reused across take/read on this reader. Not thread-safe by design: a
    // DataReader is used from one consumer thread at a time, as in the C#
    // reference. Grown on BUFFER_TOO_SMALL, never shrunk.
    private ByteBuffer payload =
            ByteBuffer.allocateDirect(DEFAULT_CAPACITY).order(ByteOrder.nativeOrder());
    private final ByteBuffer sizeSlot =
            ByteBuffer.allocateDirect(8).order(ByteOrder.nativeOrder());
    private final ByteBuffer infoSlot =
            ByteBuffer.allocateDirect(SampleInfo.STRUCT_SIZE).order(ByteOrder.nativeOrder());

    DataReader(Subscriber subscriber, Topic<T> topic, Supplier<T> factory, DataReaderQos qos) {
        super(Objects.requireNonNull(subscriber, "subscriber"),
                create(subscriber, Objects.requireNonNull(topic, "topic"), qos),
                FfiAccess::deleteDataReader);
        this.topic = topic;
        this.factory = Objects.requireNonNull(factory, "factory");
        this.xcdr2 = resolveXcdr2(qos);
    }

    /** The topic this reader receives from — the same instance passed to {@code createDataReader}. */
    public Topic<T> topic() {
        return topic;
    }

    /** Takes (removes) the next sample, or null if the cache is empty. */
    public Sample<T> take() {
        return next(true);
    }

    /** Reads (without removing) the next sample, or null if the cache is empty. */
    public Sample<T> read() {
        return next(false);
    }

    private Sample<T> next(boolean take) {
        long h = handle();
        // The core removes the sample only when it fits, so on BUFFER_TOO_SMALL
        // we can grow to the required size and retry without losing data.
        while (true) {
            int rc = take
                    ? FfiAccess.datareaderTakeSerializedWInfo(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(infoSlot))
                    : FfiAccess.datareaderReadSerializedWInfo(h, addr(payload), payload.capacity(),
                            addr(sizeSlot), addr(infoSlot));
            // handle() and the three buffer addresses were consumed by the
            // native call above; keep them all reachable across it.
            NativeKeepAlive.keepAlive(this);
            NativeKeepAlive.keepAlive(payload);
            NativeKeepAlive.keepAlive(sizeSlot);
            NativeKeepAlive.keepAlive(infoSlot);
            if (rc == DdsException.RET_BUFFER_TOO_SMALL) {
                growPayload((int) sizeSlot.getLong(0));
                continue;
            }
            if (!ReturnCodes.checkOrNoData(rc)) {
                return null;   // NO_DATA: nothing in the cache.
            }
            return decodeSample();
        }
    }

    private Sample<T> decodeSample() {
        SampleInfo info = SampleInfo.decode(infoSlot);
        if (!info.validData()) {
            return new Sample<T>(null, info);
        }
        int n = (int) sizeSlot.getLong(0);
        ((java.nio.Buffer) payload).position(0);
        ((java.nio.Buffer) payload).limit(n);
        CdrReader reader = CdrReader.ofRaw(payload, LITTLE_ENDIAN_HOST, xcdr2);
        T data = factory.get();
        data.deserializeCdr(reader);
        ((java.nio.Buffer) payload).clear();
        return new Sample<T>(data, info);
    }

    private void growPayload(int required) {
        int cap = payload.capacity();
        while (cap < required) {
            cap <<= 1;
        }
        payload = ByteBuffer.allocateDirect(cap).order(ByteOrder.nativeOrder());
    }

    private static long addr(ByteBuffer b) {
        return FfiAccess.directBufferAddress(b);
    }

    /** {@code true} for XCDR2. Mirrors DataWriter.resolveXcdr2 on the reader QoS. */
    private static boolean resolveXcdr2(DataReaderQos qos) {
        DataRepresentationKind kind = (qos != null && qos.getDataRepresentation() != null)
                ? qos.getDataRepresentation().getKind()
                : DataRepresentationKind.fromValue(FfiAccess.defaultDataRepresentation());
        return kind == DataRepresentationKind.XCDR2;
    }

    private static long create(Subscriber subscriber, Topic<?> topic, DataReaderQos qos) {
        if (qos == null) {
            return createNative(subscriber, topic, 0L);
        }
        long qosHandle = FfiAccess.createDataReaderQos();
        if (qosHandle == 0L) {
            throw new DdsErrorException(
                    "failed to allocate a native DataReaderQos handle for datareader creation");
        }
        try {
            QosMarshal.applyReaderQos(qosHandle, qos);
            return createNative(subscriber, topic, qosHandle);
        } finally {
            FfiAccess.destroyDataReaderQos(qosHandle);
        }
    }

    private static long createNative(Subscriber subscriber, Topic<?> topic, long qos) {
        long[] handleOut = new long[1];
        int rc = FfiAccess.createDataReader(
                subscriber.handle(), topic.handle(), qos, 0L, 0, handleOut);
        NativeKeepAlive.keepAlive(subscriber);
        NativeKeepAlive.keepAlive(topic);
        ReturnCodes.check(rc);
        return handleOut[0];
    }
}
