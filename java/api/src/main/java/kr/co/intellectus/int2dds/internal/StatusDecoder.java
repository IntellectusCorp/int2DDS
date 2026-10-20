package kr.co.intellectus.int2dds.internal;

import java.nio.ByteBuffer;
import kr.co.intellectus.int2dds.status.LivelinessChangedStatus;
import kr.co.intellectus.int2dds.status.LivelinessLostStatus;
import kr.co.intellectus.int2dds.status.OfferedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.OfferedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.PublicationMatchedStatus;
import kr.co.intellectus.int2dds.status.RequestedDeadlineMissedStatus;
import kr.co.intellectus.int2dds.status.RequestedIncompatibleQosStatus;
import kr.co.intellectus.int2dds.status.SampleLostStatus;
import kr.co.intellectus.int2dds.status.SampleRejectedStatus;
import kr.co.intellectus.int2dds.status.SubscriptionMatchedStatus;

/**
 * Decodes the C ABI's {@code #[repr(C)]} status structs ({@code ffi/src/status.rs}) out of a
 * native-order {@link ByteBuffer}.
 *
 * <p>The single offset table for these structs: the status getters read a buffer the C ABI filled
 * in, and the JDK 22+ FFM listener upcalls read the struct the core passes to a callback. Both go
 * through here so the two can never disagree on a layout.
 *
 * <p>Every read is absolute, so the buffer's position and limit are ignored.
 */
public final class StatusDecoder {

    /** {@code Int2DdsSubscriptionMatchedStatus}: four {@code i32} and a 16-byte handle. */
    public static final int SUBSCRIPTION_MATCHED_SIZE = 32;

    /** {@code Int2DdsPublicationMatchedStatus}: four {@code i32} and a 16-byte handle. */
    public static final int PUBLICATION_MATCHED_SIZE = 32;

    /** {@code Int2DdsLivelinessChangedStatus}: four {@code i32} and a 16-byte handle. */
    public static final int LIVELINESS_CHANGED_SIZE = 32;

    /** {@code Int2DdsSampleRejectedStatus}: three {@code i32} and a 16-byte handle. */
    public static final int SAMPLE_REJECTED_SIZE = 28;

    /** {@code Int2Dds{Requested,Offered}DeadlineMissedStatus}: two {@code i32} and a handle. */
    public static final int DEADLINE_MISSED_SIZE = 24;

    /** {@code Int2Dds{Requested,Offered}IncompatibleQosStatus}: four 32-bit fields. */
    public static final int INCOMPATIBLE_QOS_SIZE = 16;

    /** {@code Int2DdsSampleLostStatus}, {@code Int2DdsLivelinessLostStatus}: two {@code i32}. */
    public static final int COUNT_PAIR_SIZE = 8;

    private static final int HANDLE_SIZE = 16;

    private StatusDecoder() {}

    public static SubscriptionMatchedStatus subscriptionMatched(ByteBuffer b) {
        return new SubscriptionMatchedStatus(
                b.getInt(0), b.getInt(4), b.getInt(8), b.getInt(12), handle(b, 16));
    }

    public static PublicationMatchedStatus publicationMatched(ByteBuffer b) {
        return new PublicationMatchedStatus(
                b.getInt(0), b.getInt(4), b.getInt(8), b.getInt(12), handle(b, 16));
    }

    public static LivelinessChangedStatus livelinessChanged(ByteBuffer b) {
        return new LivelinessChangedStatus(
                b.getInt(0), b.getInt(4), b.getInt(8), b.getInt(12), handle(b, 16));
    }

    public static SampleRejectedStatus sampleRejected(ByteBuffer b) {
        return new SampleRejectedStatus(b.getInt(0), b.getInt(4), b.getInt(8), handle(b, 12));
    }

    public static RequestedDeadlineMissedStatus requestedDeadlineMissed(ByteBuffer b) {
        return new RequestedDeadlineMissedStatus(b.getInt(0), b.getInt(4), handle(b, 8));
    }

    public static OfferedDeadlineMissedStatus offeredDeadlineMissed(ByteBuffer b) {
        return new OfferedDeadlineMissedStatus(b.getInt(0), b.getInt(4), handle(b, 8));
    }

    public static RequestedIncompatibleQosStatus requestedIncompatibleQos(ByteBuffer b) {
        return new RequestedIncompatibleQosStatus(
                b.getInt(0), b.getInt(4), b.getInt(8), b.getInt(12));
    }

    public static OfferedIncompatibleQosStatus offeredIncompatibleQos(ByteBuffer b) {
        return new OfferedIncompatibleQosStatus(b.getInt(0), b.getInt(4), b.getInt(8), b.getInt(12));
    }

    public static SampleLostStatus sampleLost(ByteBuffer b) {
        return new SampleLostStatus(b.getInt(0), b.getInt(4));
    }

    public static LivelinessLostStatus livelinessLost(ByteBuffer b) {
        return new LivelinessLostStatus(b.getInt(0), b.getInt(4));
    }

    private static byte[] handle(ByteBuffer b, int offset) {
        byte[] out = new byte[HANDLE_SIZE];
        for (int i = 0; i < HANDLE_SIZE; i++) {
            out[i] = b.get(offset + i);
        }
        return out;
    }
}
