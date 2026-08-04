package com.intellectus.int2dds.internal;

import com.intellectus.int2dds.internal.ffi.FfiAccess;
import com.intellectus.int2dds.qos.DataReaderQos;
import com.intellectus.int2dds.qos.DataRepresentation;
import com.intellectus.int2dds.qos.DataRepresentationKind;
import com.intellectus.int2dds.qos.DataWriterQos;
import com.intellectus.int2dds.qos.Deadline;
import com.intellectus.int2dds.qos.DestinationOrder;
import com.intellectus.int2dds.qos.DestinationOrderKind;
import com.intellectus.int2dds.qos.Durability;
import com.intellectus.int2dds.qos.DurabilityKind;
import com.intellectus.int2dds.qos.History;
import com.intellectus.int2dds.qos.HistoryKind;
import com.intellectus.int2dds.qos.LatencyBudget;
import com.intellectus.int2dds.qos.Lifespan;
import com.intellectus.int2dds.qos.Liveliness;
import com.intellectus.int2dds.qos.LivelinessKind;
import com.intellectus.int2dds.qos.Ownership;
import com.intellectus.int2dds.qos.OwnershipKind;
import com.intellectus.int2dds.qos.OwnershipStrength;
import com.intellectus.int2dds.qos.ParticipantQos;
import com.intellectus.int2dds.qos.Partition;
import com.intellectus.int2dds.qos.PropertyEntry;
import com.intellectus.int2dds.qos.PublisherQos;
import com.intellectus.int2dds.qos.ReaderDataLifecycle;
import com.intellectus.int2dds.qos.Reliability;
import com.intellectus.int2dds.qos.ReliabilityKind;
import com.intellectus.int2dds.qos.ResourceLimits;
import com.intellectus.int2dds.qos.SubscriberQos;
import com.intellectus.int2dds.qos.TimeBasedFilter;
import com.intellectus.int2dds.qos.TopicQos;
import com.intellectus.int2dds.qos.TransportPriority;
import com.intellectus.int2dds.qos.UserData;
import com.intellectus.int2dds.qos.WriterDataLifecycle;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.Charset;
import java.time.Duration;

/**
 * Writes QoS objects onto native QoS handles, and reads writer/reader QoS
 * back off them.
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
 *
 * <h2>Null means opposite things going in versus coming out</h2>
 *
 * <p>On the way in (the {@code apply*} methods), null means "leave it to the
 * core" — the field is simply not sent. On the way out ({@link
 * #readWriterQos} / {@link #readReaderQos}), null means "no getter exists for
 * this policy" — the FFI cannot report what was set. A caller who reads a QoS
 * back and passes it forward unchanged would silently drop every set-only
 * policy below, because each one reads back as null regardless of what was
 * actually configured on the handle.
 *
 * <h2>Set-only, no getter — not round-trip verifiable</h2>
 *
 * <p>The FFI has 44 QoS setters and 28 getters (not the same shape: one
 * getter, {@code int2dds_participant_qos_get_properties_with_prefix}, has no
 * corresponding setter, so the gap below is 17 entries rather than the 44-28
 * arithmetic difference of 16). Output of:
 *
 * <pre>{@code
 * comm -23 \
 *   <(grep -oE 'int2dds_[a-z]+_qos_set_[a-z_]+' ffi/src/qos.rs | sed 's/_set_/_/' | sort -u) \
 *   <(grep -oE 'int2dds_[a-z]+_qos_get_[a-z_]+' ffi/src/qos.rs | sed 's/_get_/_/' | sort -u)
 * }</pre>
 *
 * <pre>
 * int2dds_datareader_qos_user_data
 * int2dds_datawriter_qos_user_data
 * int2dds_participant_qos_multicast_ttl
 * int2dds_participant_qos_user_data
 * int2dds_publisher_qos_partition
 * int2dds_subscriber_qos_partition
 * int2dds_topic_qos_data_representation
 * int2dds_topic_qos_deadline
 * int2dds_topic_qos_destination_order
 * int2dds_topic_qos_durability
 * int2dds_topic_qos_history
 * int2dds_topic_qos_lifespan
 * int2dds_topic_qos_liveliness
 * int2dds_topic_qos_ownership
 * int2dds_topic_qos_reliability
 * int2dds_topic_qos_resource_limits
 * int2dds_topic_qos_transport_priority
 * </pre>
 *
 * <p>None of these seventeen policies can be round-trip verified; a test can
 * only assert the setter call returned OK. If the FFI ever grows a getter for
 * one of them, this list — and {@link #readWriterQos} / {@link
 * #readReaderQos} — should be updated to match.
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

    /**
     * Reads back the DataWriter policies the FFI exposes getters for.
     *
     * <p>Policies with no getter — see the set-only list in this class's
     * javadoc — stay null in the result. A null here means "not readable, no
     * getter exists," which is the opposite of what null means in {@link
     * #applyWriterQos}, where it means "leave it to the core." A caller that
     * reads a QoS back and passes it forward unchanged would silently drop
     * every set-only policy.
     */
    public static DataWriterQos readWriterQos(long h) {
        DataWriterQos q = new DataWriterQos();

        ByteBuffer slot = ByteBuffer.allocateDirect(16).order(ByteOrder.nativeOrder());
        long addr = FfiAccess.directBufferAddress(slot);

        ReturnCodes.check(FfiAccess.writerQosGetReliability(h, addr, addr + 8));
        q.setReliability(new Reliability(
                ReliabilityKind.fromValue(slot.getInt(0)), Duration.ofNanos(slot.getLong(8))));

        ReturnCodes.check(FfiAccess.writerQosGetDurability(h, addr));
        q.setDurability(new Durability(DurabilityKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.writerQosGetHistory(h, addr, addr + 4));
        q.setHistory(new History(HistoryKind.fromValue(slot.getInt(0)), slot.getInt(4)));

        ReturnCodes.check(FfiAccess.writerQosGetOwnership(h, addr));
        q.setOwnership(new Ownership(OwnershipKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.writerQosGetOwnershipStrength(h, addr));
        q.setOwnershipStrength(new OwnershipStrength(slot.getInt(0)));

        ReturnCodes.check(FfiAccess.writerQosGetResourceLimits(h, addr, addr + 4, addr + 8));
        q.setResourceLimits(
                new ResourceLimits(slot.getInt(0), slot.getInt(4), slot.getInt(8)));

        ReturnCodes.check(FfiAccess.writerQosGetLifespan(h, addr));
        q.setLifespan(new Lifespan(Duration.ofNanos(slot.getLong(0))));

        ReturnCodes.check(FfiAccess.writerQosGetDestinationOrder(h, addr));
        q.setDestinationOrder(new DestinationOrder(DestinationOrderKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.writerQosGetLatencyBudget(h, addr));
        q.setLatencyBudget(new LatencyBudget(Duration.ofNanos(slot.getLong(0))));

        ReturnCodes.check(FfiAccess.writerQosGetTransportPriority(h, addr));
        q.setTransportPriority(new TransportPriority(slot.getInt(0)));

        ReturnCodes.check(FfiAccess.writerQosGetWriterDataLifecycle(h, addr));
        q.setWriterDataLifecycle(new WriterDataLifecycle(slot.get(0) != 0));

        ReturnCodes.check(FfiAccess.writerQosGetDataRepresentation(h, addr));
        q.setDataRepresentation(
                new DataRepresentation(DataRepresentationKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.writerQosGetDeadline(h, addr));
        q.setDeadline(new Deadline(Duration.ofNanos(slot.getLong(0))));

        ReturnCodes.check(FfiAccess.writerQosGetLiveliness(h, addr, addr + 8));
        q.setLiveliness(new Liveliness(
                LivelinessKind.fromValue(slot.getInt(0)), Duration.ofNanos(slot.getLong(8))));

        ReturnCodes.check(FfiAccess.writerQosGetDataFrag(h, addr));
        q.setDataFrag(slot.getInt(0));

        return q;
    }

    /**
     * Reads back the DataReader policies the FFI exposes getters for.
     *
     * <p>Policies with no getter — see the set-only list in this class's
     * javadoc — stay null in the result. A null here means "not readable, no
     * getter exists," which is the opposite of what null means in {@link
     * #applyReaderQos}, where it means "leave it to the core." A caller that
     * reads a QoS back and passes it forward unchanged would silently drop
     * every set-only policy.
     */
    public static DataReaderQos readReaderQos(long h) {
        DataReaderQos q = new DataReaderQos();

        ByteBuffer slot = ByteBuffer.allocateDirect(16).order(ByteOrder.nativeOrder());
        long addr = FfiAccess.directBufferAddress(slot);

        ReturnCodes.check(FfiAccess.readerQosGetReliability(h, addr, addr + 8));
        q.setReliability(new Reliability(
                ReliabilityKind.fromValue(slot.getInt(0)), Duration.ofNanos(slot.getLong(8))));

        ReturnCodes.check(FfiAccess.readerQosGetDurability(h, addr));
        q.setDurability(new Durability(DurabilityKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.readerQosGetHistory(h, addr, addr + 4));
        q.setHistory(new History(HistoryKind.fromValue(slot.getInt(0)), slot.getInt(4)));

        ReturnCodes.check(FfiAccess.readerQosGetOwnership(h, addr));
        q.setOwnership(new Ownership(OwnershipKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.readerQosGetResourceLimits(h, addr, addr + 4, addr + 8));
        q.setResourceLimits(
                new ResourceLimits(slot.getInt(0), slot.getInt(4), slot.getInt(8)));

        ReturnCodes.check(FfiAccess.readerQosGetDestinationOrder(h, addr));
        q.setDestinationOrder(new DestinationOrder(DestinationOrderKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.readerQosGetTimeBasedFilter(h, addr));
        q.setTimeBasedFilter(new TimeBasedFilter(Duration.ofNanos(slot.getLong(0))));

        ReturnCodes.check(FfiAccess.readerQosGetLatencyBudget(h, addr));
        q.setLatencyBudget(new LatencyBudget(Duration.ofNanos(slot.getLong(0))));

        ReturnCodes.check(FfiAccess.readerQosGetReaderDataLifecycle(h, addr, addr + 8));
        q.setReaderDataLifecycle(new ReaderDataLifecycle(
                Duration.ofNanos(slot.getLong(0)), Duration.ofNanos(slot.getLong(8))));

        ReturnCodes.check(FfiAccess.readerQosGetDataRepresentation(h, addr));
        q.setDataRepresentation(
                new DataRepresentation(DataRepresentationKind.fromValue(slot.getInt(0))));

        ReturnCodes.check(FfiAccess.readerQosGetDeadline(h, addr));
        q.setDeadline(new Deadline(Duration.ofNanos(slot.getLong(0))));

        ReturnCodes.check(FfiAccess.readerQosGetLiveliness(h, addr, addr + 8));
        q.setLiveliness(new Liveliness(
                LivelinessKind.fromValue(slot.getInt(0)), Duration.ofNanos(slot.getLong(8))));

        return q;
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
