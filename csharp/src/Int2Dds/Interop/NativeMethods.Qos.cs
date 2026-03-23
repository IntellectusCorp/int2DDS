using System.Runtime.InteropServices;

namespace Int2Dds.Interop;

internal static partial class NativeMethods
{
    // ── DataWriter QoS ──────────────────────────────────────────────────

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_create_default(out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_reliability(nint qos, int kind, long max_blocking_time_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_durability(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_history(nint qos, int kind, int depth);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_data_representation(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_ownership(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_ownership_strength(nint qos, int value);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_resource_limits(nint qos, int max_samples, int max_instances, int max_per_instance);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_lifespan(nint qos, long duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_destination_order(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_latency_budget(nint qos, long duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_transport_priority(nint qos, int priority);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datawriter_qos_set_user_data(nint qos, byte* data, nuint data_len);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_writer_data_lifecycle(nint qos, [MarshalAs(UnmanagedType.U1)] bool autodispose);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_deadline(nint qos, long period_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_set_liveliness(nint qos, int kind, long lease_duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datawriter_qos_destroy(nint qos);

    // ── DataReader QoS ──────────────────────────────────────────────────

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_create_default(out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_reliability(nint qos, int kind, long max_blocking_time_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_durability(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_history(nint qos, int kind, int depth);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_data_representation(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_ownership(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_resource_limits(nint qos, int max_samples, int max_instances, int max_per_instance);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_destination_order(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_time_based_filter(nint qos, long minimum_separation_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_latency_budget(nint qos, long duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_datareader_qos_set_user_data(nint qos, byte* data, nuint data_len);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_reader_data_lifecycle(nint qos, long autopurge_nowriter_ns, long autopurge_disposed_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_deadline(nint qos, long period_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_set_liveliness(nint qos, int kind, long lease_duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_datareader_qos_destroy(nint qos);

    // ── Topic QoS ───────────────────────────────────────────────────────

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_create_default(out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_reliability(nint qos, int kind, long max_blocking_time_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_durability(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_history(nint qos, int kind, int depth);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_deadline(nint qos, long period_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_liveliness(nint qos, int kind, long lease_duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_destination_order(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_resource_limits(nint qos, int max_samples, int max_instances, int max_per_instance);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_transport_priority(nint qos, int priority);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_lifespan(nint qos, long duration_ns);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_ownership(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_set_data_representation(nint qos, int kind);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_topic_qos_destroy(nint qos);

    // ── Participant QoS ─────────────────────────────────────────────────

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_participant_qos_create_default(out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_participant_qos_set_user_data(nint qos, byte* data, nuint data_len);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_participant_qos_destroy(nint qos);

    // ── Publisher QoS ───────────────────────────────────────────────────

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publisher_qos_create_default(out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_publisher_qos_set_partition(nint qos, byte** partitions, nuint partition_count);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_publisher_qos_destroy(nint qos);

    // ── Subscriber QoS ──────────────────────────────────────────────────

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_subscriber_qos_create_default(out nint qos_out);

    [LibraryImport("int2dds_ffi")]
    internal static unsafe partial int int2dds_subscriber_qos_set_partition(nint qos, byte** partitions, nuint partition_count);

    [LibraryImport("int2dds_ffi")]
    internal static partial int int2dds_subscriber_qos_destroy(nint qos);
}
