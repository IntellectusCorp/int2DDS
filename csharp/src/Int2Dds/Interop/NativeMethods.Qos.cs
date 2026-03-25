using System;
using System.Runtime.InteropServices;

namespace Int2Dds.Interop
{
    internal static partial class NativeMethods
    {
        // ── DataWriter QoS ──────────────────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_create_default(out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_reliability(IntPtr qos, int kind, long max_blocking_time_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_durability(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_history(IntPtr qos, int kind, int depth);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_data_representation(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_ownership(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_ownership_strength(IntPtr qos, int value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_resource_limits(IntPtr qos, int max_samples, int max_instances, int max_per_instance);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_lifespan(IntPtr qos, long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_destination_order(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_latency_budget(IntPtr qos, long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_transport_priority(IntPtr qos, int priority);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datawriter_qos_set_user_data(IntPtr qos, byte* data, UIntPtr data_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_writer_data_lifecycle(IntPtr qos, [MarshalAs(UnmanagedType.U1)] bool autodispose);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_deadline(IntPtr qos, long period_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_set_liveliness(IntPtr qos, int kind, long lease_duration_ns);

        // DataWriter QoS Getters

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_reliability(IntPtr qos, out int kind, out long max_blocking_time_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_durability(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_history(IntPtr qos, out int kind, out int depth);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_ownership(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_ownership_strength(IntPtr qos, out int value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_resource_limits(IntPtr qos, out int max_samples, out int max_instances, out int max_per_instance);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_lifespan(IntPtr qos, out long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_destination_order(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_deadline(IntPtr qos, out long period_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_liveliness(IntPtr qos, out int kind, out long lease_duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_data_representation(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_transport_priority(IntPtr qos, out int value);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_latency_budget(IntPtr qos, out long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_get_writer_data_lifecycle(IntPtr qos, [MarshalAs(UnmanagedType.U1)] out bool autodispose);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datawriter_qos_destroy(IntPtr qos);

        // ── DataReader QoS ──────────────────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_create_default(out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_reliability(IntPtr qos, int kind, long max_blocking_time_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_durability(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_history(IntPtr qos, int kind, int depth);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_data_representation(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_ownership(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_resource_limits(IntPtr qos, int max_samples, int max_instances, int max_per_instance);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_destination_order(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_time_based_filter(IntPtr qos, long minimum_separation_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_latency_budget(IntPtr qos, long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_datareader_qos_set_user_data(IntPtr qos, byte* data, UIntPtr data_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_reader_data_lifecycle(IntPtr qos, long autopurge_nowriter_ns, long autopurge_disposed_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_deadline(IntPtr qos, long period_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_set_liveliness(IntPtr qos, int kind, long lease_duration_ns);

        // DataReader QoS Getters

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_reliability(IntPtr qos, out int kind, out long max_blocking_time_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_durability(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_history(IntPtr qos, out int kind, out int depth);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_ownership(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_resource_limits(IntPtr qos, out int max_samples, out int max_instances, out int max_per_instance);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_destination_order(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_deadline(IntPtr qos, out long period_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_liveliness(IntPtr qos, out int kind, out long lease_duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_data_representation(IntPtr qos, out int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_latency_budget(IntPtr qos, out long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_time_based_filter(IntPtr qos, out long min_separation_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_get_reader_data_lifecycle(IntPtr qos, out long autopurge_nowriter_ns, out long autopurge_disposed_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_datareader_qos_destroy(IntPtr qos);

        // ── Topic QoS ───────────────────────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_create_default(out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_reliability(IntPtr qos, int kind, long max_blocking_time_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_durability(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_history(IntPtr qos, int kind, int depth);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_deadline(IntPtr qos, long period_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_liveliness(IntPtr qos, int kind, long lease_duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_destination_order(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_resource_limits(IntPtr qos, int max_samples, int max_instances, int max_per_instance);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_transport_priority(IntPtr qos, int priority);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_lifespan(IntPtr qos, long duration_ns);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_ownership(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_set_data_representation(IntPtr qos, int kind);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_topic_qos_destroy(IntPtr qos);

        // ── Participant QoS ─────────────────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_qos_create_default(out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_participant_qos_set_user_data(IntPtr qos, byte* data, UIntPtr data_len);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_participant_qos_destroy(IntPtr qos);

        // ── Publisher QoS ───────────────────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_qos_create_default(out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_publisher_qos_set_partition(IntPtr qos, byte** partitions, UIntPtr partition_count);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_publisher_qos_destroy(IntPtr qos);

        // ── Subscriber QoS ──────────────────────────────────────────────────

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_qos_create_default(out IntPtr qos_out);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static unsafe extern int int2dds_subscriber_qos_set_partition(IntPtr qos, byte** partitions, UIntPtr partition_count);

        [DllImport("int2dds_ffi", CallingConvention = CallingConvention.Cdecl)]
        internal static extern int int2dds_subscriber_qos_destroy(IntPtr qos);
    }
}
