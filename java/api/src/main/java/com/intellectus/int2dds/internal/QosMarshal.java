package com.intellectus.int2dds.internal;

import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.ParticipantQos;
import com.intellectus.int2dds.qos.Partition;
import com.intellectus.int2dds.qos.PropertyEntry;
import com.intellectus.int2dds.qos.PublisherQos;
import com.intellectus.int2dds.qos.ResourceLimits;
import com.intellectus.int2dds.qos.SubscriberQos;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.qos.UserData;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;

/**
 * Writes QoS objects onto native QoS handles.
 *
 * <p>Only non-null policies are applied. A null means "leave it to the core",
 * which keeps the DDS defaults defined in exactly one place — the Rust core —
 * instead of duplicating them here where they could drift.
 *
 * <p>Every policy named in the container field table has a matching native
 * setter and is marshalled here; none are omitted. {@code Property} is the
 * one exception to the "set_" naming pattern — the FFI exposes it as {@code
 * int2dds_participant_qos_add_property}, called once per entry, rather than a
 * single "set" call.
 *
 * <p>Calls go through {@link FfiAccess} rather than the generated {@code Ffi}
 * class directly: {@code Ffi}'s native declarations are package-visible to
 * {@code com.intellectus.int2dds.internal.ffi} only, and this class lives one
 * package up, so {@link FfiAccess} is the only way in.
 */
public final class QosMarshal {

    private static final Charset UTF8 = Charset.forName("UTF-8");

    private QosMarshal() {}

    public static void applyParticipantQos(long h, ParticipantQos q) {
        if (q.getUserData() != null) {
            applyUserData(q.getUserData(), (addr, len) -> FfiAccess.participantQosSetUserData(h, addr, len));
        }
        if (q.getProperty() != null) {
            for (PropertyEntry e : q.getProperty().getEntries()) {
                ReturnCodes.check(FfiAccess.participantQosAddProperty(
                        h, utf8(e.getName()), utf8(e.getValue()), e.isPropagate()));
            }
        }
    }

    public static void applyPublisherQos(long h, PublisherQos q) {
        if (q.getPartition() != null) {
            applyPartition(q.getPartition(), (names, count) ->
                    FfiAccess.publisherQosSetPartition(h, names, count));
        }
    }

    public static void applySubscriberQos(long h, SubscriberQos q) {
        if (q.getPartition() != null) {
            applyPartition(q.getPartition(), (names, count) ->
                    FfiAccess.subscriberQosSetPartition(h, names, count));
        }
    }

    public static void applyTopicQos(long h, TopicQos q) {
        if (q.getReliability() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetReliability(
                    h, q.getReliability().getKind().value(), q.getReliability().maxBlockingTimeNs()));
        }
        if (q.getDurability() != null) {
            ReturnCodes.check(
                    FfiAccess.topicQosSetDurability(h, q.getDurability().getKind().value()));
        }
        if (q.getHistory() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetHistory(
                    h, q.getHistory().getKind().value(), q.getHistory().getDepth()));
        }
        if (q.getDeadline() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetDeadline(h, q.getDeadline().periodNs()));
        }
        if (q.getLiveliness() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetLiveliness(
                    h, q.getLiveliness().getKind().value(), q.getLiveliness().leaseDurationNs()));
        }
        if (q.getDestinationOrder() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetDestinationOrder(
                    h, q.getDestinationOrder().getKind().value()));
        }
        if (q.getResourceLimits() != null) {
            ResourceLimits rl = q.getResourceLimits();
            ReturnCodes.check(FfiAccess.topicQosSetResourceLimits(
                    h, rl.getMaxSamples(), rl.getMaxInstances(), rl.getMaxSamplesPerInstance()));
        }
        if (q.getTransportPriority() != null) {
            ReturnCodes.check(
                    FfiAccess.topicQosSetTransportPriority(h, q.getTransportPriority().getValue()));
        }
        if (q.getLifespan() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetLifespan(h, q.getLifespan().durationNs()));
        }
        if (q.getOwnership() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetOwnership(h, q.getOwnership().getKind().value()));
        }
        if (q.getDataRepresentation() != null) {
            ReturnCodes.check(FfiAccess.topicQosSetDataRepresentation(
                    h, q.getDataRepresentation().getKind().value()));
        }
    }

    public static void applyWriterQos(long h, DataWriterQos q) {
        if (q.getReliability() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetReliability(
                    h, q.getReliability().getKind().value(), q.getReliability().maxBlockingTimeNs()));
        }
        if (q.getDurability() != null) {
            ReturnCodes.check(
                    FfiAccess.writerQosSetDurability(h, q.getDurability().getKind().value()));
        }
        if (q.getHistory() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetHistory(
                    h, q.getHistory().getKind().value(), q.getHistory().getDepth()));
        }
        if (q.getOwnership() != null) {
            ReturnCodes.check(
                    FfiAccess.writerQosSetOwnership(h, q.getOwnership().getKind().value()));
        }
        if (q.getOwnershipStrength() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetOwnershipStrength(
                    h, q.getOwnershipStrength().getValue()));
        }
        if (q.getResourceLimits() != null) {
            ResourceLimits rl = q.getResourceLimits();
            ReturnCodes.check(FfiAccess.writerQosSetResourceLimits(
                    h, rl.getMaxSamples(), rl.getMaxInstances(), rl.getMaxSamplesPerInstance()));
        }
        if (q.getLifespan() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetLifespan(h, q.getLifespan().durationNs()));
        }
        if (q.getDestinationOrder() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetDestinationOrder(
                    h, q.getDestinationOrder().getKind().value()));
        }
        if (q.getLatencyBudget() != null) {
            ReturnCodes.check(
                    FfiAccess.writerQosSetLatencyBudget(h, q.getLatencyBudget().durationNs()));
        }
        if (q.getTransportPriority() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetTransportPriority(
                    h, q.getTransportPriority().getValue()));
        }
        if (q.getUserData() != null) {
            applyUserData(q.getUserData(), (addr, len) -> FfiAccess.writerQosSetUserData(h, addr, len));
        }
        if (q.getWriterDataLifecycle() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetWriterDataLifecycle(
                    h, q.getWriterDataLifecycle().isAutodisposeUnregisteredInstances()));
        }
        if (q.getDataRepresentation() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetDataRepresentation(
                    h, q.getDataRepresentation().getKind().value()));
        }
        if (q.getDeadline() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetDeadline(h, q.getDeadline().periodNs()));
        }
        if (q.getLiveliness() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetLiveliness(
                    h, q.getLiveliness().getKind().value(), q.getLiveliness().leaseDurationNs()));
        }
        if (q.getDataFrag() != null) {
            ReturnCodes.check(FfiAccess.writerQosSetDataFrag(h, q.getDataFrag()));
        }
    }

    public static void applyReaderQos(long h, DataReaderQos q) {
        if (q.getReliability() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetReliability(
                    h, q.getReliability().getKind().value(), q.getReliability().maxBlockingTimeNs()));
        }
        if (q.getDurability() != null) {
            ReturnCodes.check(
                    FfiAccess.readerQosSetDurability(h, q.getDurability().getKind().value()));
        }
        if (q.getHistory() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetHistory(
                    h, q.getHistory().getKind().value(), q.getHistory().getDepth()));
        }
        if (q.getOwnership() != null) {
            ReturnCodes.check(
                    FfiAccess.readerQosSetOwnership(h, q.getOwnership().getKind().value()));
        }
        if (q.getResourceLimits() != null) {
            ResourceLimits rl = q.getResourceLimits();
            ReturnCodes.check(FfiAccess.readerQosSetResourceLimits(
                    h, rl.getMaxSamples(), rl.getMaxInstances(), rl.getMaxSamplesPerInstance()));
        }
        if (q.getDestinationOrder() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetDestinationOrder(
                    h, q.getDestinationOrder().getKind().value()));
        }
        if (q.getTimeBasedFilter() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetTimeBasedFilter(
                    h, q.getTimeBasedFilter().minimumSeparationNs()));
        }
        if (q.getLatencyBudget() != null) {
            ReturnCodes.check(
                    FfiAccess.readerQosSetLatencyBudget(h, q.getLatencyBudget().durationNs()));
        }
        if (q.getUserData() != null) {
            applyUserData(q.getUserData(), (addr, len) -> FfiAccess.readerQosSetUserData(h, addr, len));
        }
        if (q.getReaderDataLifecycle() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetReaderDataLifecycle(
                    h,
                    q.getReaderDataLifecycle().autopurgeNowriterNs(),
                    q.getReaderDataLifecycle().autopurgeDisposedNs()));
        }
        if (q.getDataRepresentation() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetDataRepresentation(
                    h, q.getDataRepresentation().getKind().value()));
        }
        if (q.getDeadline() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetDeadline(h, q.getDeadline().periodNs()));
        }
        if (q.getLiveliness() != null) {
            ReturnCodes.check(FfiAccess.readerQosSetLiveliness(
                    h, q.getLiveliness().getKind().value(), q.getLiveliness().leaseDurationNs()));
        }
    }

    /** A native setter taking the address and length of a direct byte buffer. */
    private interface ByteBufferSetter {
        int set(long dataAddr, long dataLen);
    }

    /** A native setter taking a UTF-8 name array and its count. */
    private interface NamesSetter {
        int set(byte[][] names, long count);
    }

    /**
     * Copies {@code ud}'s bytes into a direct buffer and hands its address to
     * {@code setter}. The buffer stays reachable for the duration of the call,
     * so it cannot be collected out from under the native side.
     */
    private static void applyUserData(UserData ud, ByteBufferSetter setter) {
        byte[] data = ud.getData();
        ByteBuffer buf = ByteBuffer.allocateDirect(data.length).order(ByteOrder.nativeOrder());
        buf.put(data);
        long addr = FfiAccess.directBufferAddress(buf);
        ReturnCodes.check(setter.set(addr, data.length));
    }

    /** Converts a partition's names to UTF-8 and hands them to {@code setter}. */
    private static void applyPartition(Partition partition, NamesSetter setter) {
        String[] names = partition.getNames();
        byte[][] raw = new byte[names.length][];
        for (int i = 0; i < names.length; i++) {
            raw[i] = utf8(names[i]);
        }
        ReturnCodes.check(setter.set(raw, raw.length));
    }

    /** UTF-8 bytes for a name that crosses as byte[]. Never a String. */
    private static byte[] utf8(String s) {
        return s == null ? new byte[0] : s.getBytes(UTF8);
    }
}
